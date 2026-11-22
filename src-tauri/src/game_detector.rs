use crate::{
    game_catalog::{path_key, Catalog, DetectedGame},
    model::{Timestamp, MINECRAFT_JAVA},
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
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible,
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
    key: String,
    tracked_as: Option<PathBuf>,
    last_rejection: Option<&'static str>,
    rejected_as_foreground: Option<bool>,
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
            key: String::new(),
            tracked_as: None,
            last_rejection: None,
            rejected_as_foreground: None,
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
        process.key = path_key(&process.path);
        process.created = created;
        Some(process)
    }
}

#[derive(Default)]
struct Windows {
    foreground: u32,
    visible: HashSet<u32>,
    minecraft: HashSet<u32>,
    observed: bool,
}

fn minecraft_window_title(title: &str) -> bool {
    title.strip_prefix("Minecraft").is_some_and(|suffix| {
        let suffix = suffix.strip_prefix('*').unwrap_or(suffix);
        suffix.is_empty() || suffix.starts_with(' ')
    })
}

fn minecraft_window_class(class: &[u16]) -> bool {
    ["GLFW30", "LWJGL", "SDL_app"]
        .iter()
        .any(|name| class.iter().copied().eq(name.encode_utf16()))
}

fn java_runtime(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("java.exe") || name.eq_ignore_ascii_case("javaw.exe")
    })
}

unsafe extern "system" fn window_proc(window: HWND, context: LPARAM) -> i32 {
    if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
        return 1;
    }
    let mut rect = RECT::default();
    let large = GetWindowRect(window, &mut rect) != 0
        && rect.right - rect.left >= 560
        && rect.bottom - rect.top >= 320;
    let mut class = [0u16; 32];
    let mut title = [0u16; 64];
    let class_len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    // Compare the class before reading the window caption.
    let minecraft = minecraft_window_class(&class[..class_len as usize]) && {
        let title_len = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        minecraft_window_title(&String::from_utf16_lossy(&title[..title_len as usize]))
    };
    if !large && !minecraft {
        return 1;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid != 0 && pid != std::process::id() {
        let windows = &mut *(context as *mut Windows);
        if large {
            windows.visible.insert(pid);
        }
        if minecraft {
            windows.minecraft.insert(pid);
        }
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
            for process in self.processes.values_mut() {
                process.rejected_as_foreground = None;
            }
        }

        let mut running = Vec::new();
        let mut exited = Vec::new();
        self.processes.retain(|_, process| {
            if unsafe { WaitForSingleObject(process.handle, 0) } == WAIT_TIMEOUT {
                return true;
            }
            if let Some(path) = &process.tracked_as {
                let stop = process_times(process.handle)
                    .and_then(|(_, stop)| stop)
                    .unwrap_or(now);
                exited.push((path.clone(), stop));
            }
            false
        });
        let windows = windows();
        for &pid in windows.visible.union(&windows.minecraft) {
            if let std::collections::hash_map::Entry::Vacant(entry) = self.processes.entry(pid) {
                if let Some(process) = Process::open(pid) {
                    entry.insert(process);
                }
            }
        }
        let minecraft = Path::new(MINECRAFT_JAVA);
        let mut enroll_minecraft = !registered.contains(MINECRAFT_JAVA);
        for (&pid, process) in &mut self.processes {
            let was_known = process.tracked_as.is_some();
            if !was_known && windows.minecraft.contains(&pid) && java_runtime(&process.path) {
                process.tracked_as = Some(minecraft.to_owned());
            } else if !was_known && windows.visible.contains(&pid) {
                if registered.contains(&process.key) {
                    process.tracked_as = Some(process.path.clone());
                } else if process.rejected_as_foreground != Some(pid == windows.foreground) {
                    match self
                        .catalog
                        .identify(&process.path, pid == windows.foreground)
                    {
                        Ok(game) => {
                            process.tracked_as = Some(process.path.clone());
                            discovered.push(game);
                        }
                        Err(reason) => {
                            process.rejected_as_foreground = Some(pid == windows.foreground);
                            if crate::game_diagnostics() && process.last_rejection != Some(reason) {
                                process.last_rejection = Some(reason);
                                eprintln!("[Afterwork] {}: {reason}", process.path.display());
                            }
                        }
                    }
                }
            }
            if let Some(path) = &process.tracked_as {
                if path == minecraft && enroll_minecraft {
                    discovered.push(DetectedGame {
                        id: MINECRAFT_JAVA.into(),
                        name: "Minecraft: Java Edition".into(),
                        source: "Распознано по окну",
                        path: path.clone(),
                    });
                    enroll_minecraft = false;
                }
                running.push((path.clone(), process.created, !was_known));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_detection_requires_java_and_the_game_window() {
        assert!(java_runtime(Path::new(r"D:\Java\bin\JAVAW.exe")));
        assert!(java_runtime(Path::new("java.exe")));
        assert!(!java_runtime(Path::new("TLauncher.exe")));
        assert!(!java_runtime(Path::new("javaws.exe")));

        for class in ["GLFW30", "LWJGL", "SDL_app"] {
            assert!(minecraft_window_class(
                &class.encode_utf16().collect::<Vec<_>>()
            ));
        }
        for class in ["SunAwtFrame", "Chrome_WidgetWin_1", "GLFW30Helper"] {
            assert!(!minecraft_window_class(
                &class.encode_utf16().collect::<Vec<_>>()
            ));
        }
        for title in [
            "Minecraft",
            "Minecraft 1.20.1",
            "Minecraft* 1.21.1 - Multiplayer",
            "Minecraft NeoForge* 26.3 - Одиночная игра",
        ] {
            assert!(minecraft_window_title(title));
        }
        for title in ["", "TLauncher", "MinecraftLauncher", "A guide to Minecraft"] {
            assert!(!minecraft_window_title(title));
        }
    }
}
