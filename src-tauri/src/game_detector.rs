use crate::{
    game_catalog::{path_key, Catalog, DetectedGame},
    model::Timestamp,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, RECT, WAIT_TIMEOUT},
    System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible,
    },
};

pub struct Sample {
    pub discovered: Vec<DetectedGame>,
    pub running: Vec<(PathBuf, Timestamp, bool)>,
    pub exited: Vec<(PathBuf, Timestamp)>,
    pub observed: bool,
}

struct Process {
    handle: HANDLE,
    created: Timestamp,
    path: PathBuf,
    known: bool,
    last_rejection: Option<&'static str>,
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

fn unix_time(time: FILETIME) -> Timestamp {
    (((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)) / 10_000_000)
        as Timestamp
        - 11_644_473_600
}

fn process_times(handle: HANDLE) -> Option<(Timestamp, Option<Timestamp>)> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    (ok != 0).then(|| {
        (
            unix_time(creation),
            (exit.dwHighDateTime != 0 || exit.dwLowDateTime != 0).then(|| unix_time(exit)),
        )
    })
}

impl Process {
    fn open(pid: u32) -> Option<Self> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            return None;
        }
        let mut process = Self {
            handle,
            created: 0,
            path: PathBuf::new(),
            known: false,
            last_rejection: None,
        };
        if unsafe { WaitForSingleObject(handle, 0) } != WAIT_TIMEOUT {
            return None;
        }
        let (created, _) = process_times(handle)?;
        let mut buffer = vec![0u16; 32768];
        let mut len = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) } == 0 {
            return None;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len as usize]));
        // Canonical paths match the launcher catalog and the blocker, including junctions.
        process.path = fs::canonicalize(&path).unwrap_or(path);
        process.created = created;
        Some(process)
    }
}

#[derive(Default)]
struct Windows {
    foreground: u32,
    visible: HashSet<u32>,
    observed: bool,
}

unsafe extern "system" fn window_proc(window: HWND, context: LPARAM) -> i32 {
    if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
        return 1;
    }
    let mut rect = RECT::default();
    if GetWindowRect(window, &mut rect) == 0
        || rect.right - rect.left < 560
        || rect.bottom - rect.top < 320
    {
        return 1;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid != 0 && pid != std::process::id() {
        (*(context as *mut Windows)).visible.insert(pid);
    }
    1
}

fn windows() -> Windows {
    let mut windows = Windows::default();
    // Both calls run synchronously. The callback borrows this stack value only during EnumWindows.
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut windows.foreground);
        windows.observed = EnumWindows(Some(window_proc), &mut windows as *mut Windows as LPARAM)
            != 0
            && windows.foreground != 0;
    }
    windows
}

#[derive(Default)]
pub struct Detector {
    catalog: Catalog,
    next_catalog: Option<Timestamp>,
    processes: HashMap<u32, Process>,
}

impl Detector {
    pub fn sample(&mut self, now: Timestamp, registered: &HashSet<String>) -> Sample {
        let mut discovered = Vec::new();
        if self
            .next_catalog
            .is_none_or(|scan| now >= scan || now < scan - 120)
        {
            self.catalog = Catalog::discover();
            discovered = self.catalog.launch_targets();
            self.next_catalog = Some(now + 120);
        }

        let mut running = Vec::new();
        let mut exited = Vec::new();
        self.processes.retain(|_, process| {
            if unsafe { WaitForSingleObject(process.handle, 0) } == WAIT_TIMEOUT {
                return true;
            }
            if process.known {
                let stop = process_times(process.handle)
                    .and_then(|(_, stop)| stop)
                    .unwrap_or(now);
                exited.push((process.path.clone(), stop));
            }
            false
        });
        let windows = windows();
        for &pid in &windows.visible {
            if let std::collections::hash_map::Entry::Vacant(entry) = self.processes.entry(pid) {
                if let Some(process) = Process::open(pid) {
                    entry.insert(process);
                }
            }
        }
        for (&pid, process) in &mut self.processes {
            let was_known = process.known;
            if !process.known && windows.visible.contains(&pid) {
                if registered.contains(&path_key(&process.path)) {
                    process.known = true;
                } else {
                    match self
                        .catalog
                        .identify(&process.path, pid == windows.foreground)
                    {
                        Ok(game) => {
                            process.known = true;
                            discovered.push(game);
                        }
                        Err(reason)
                            if crate::game_diagnostics()
                                && process.last_rejection != Some(reason) =>
                        {
                            process.last_rejection = Some(reason);
                            eprintln!("[Afterwork] {}: {reason}", process.path.display());
                        }
                        Err(_) => {}
                    }
                }
            }
            if process.known {
                running.push((process.path.clone(), process.created, !was_known));
            }
        }
        Sample {
            discovered,
            running,
            exited,
            observed: windows.observed,
        }
    }
}

pub fn registered_path(path: &str) -> String {
    path_key(Path::new(path))
}
