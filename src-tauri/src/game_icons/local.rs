use crate::{
    game_catalog::{path_key, steam_root},
    model::Game,
};
use image::RgbaImage;
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows::{
    core::{Interface, PCWSTR},
    Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED, STGM_READ,
        },
        UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH},
    },
};
use windows_sys::Win32::{
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    },
    UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, PrivateExtractIconsW, DI_NORMAL, HICON},
};

pub const SIZE: u32 = 96;

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

fn from_wide(value: &[u16]) -> PathBuf {
    PathBuf::from(OsString::from_wide(
        value.split(|&part| part == 0).next().unwrap_or_default(),
    ))
}

fn local_path(path: &Path) -> Option<PathBuf> {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    // Reject UNC, device paths, alternate data streams, and junctions resolving off the drive.
    let drive = |value: &str| {
        let value = value.strip_prefix(r"\\?\").unwrap_or(value);
        let Some(bytes) = value.as_bytes().get(..3) else {
            return false;
        };
        if !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !matches!(bytes[2], b'\\' | b'/')
            || value.as_bytes()[2..].contains(&b':')
        {
            return false;
        }
        let root = [u16::from(bytes[0]), b':' as u16, b'\\' as u16, 0];
        const DRIVE_REMOTE: u32 = 4;
        let kind = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        kind > 1 && kind != DRIVE_REMOTE
    };
    if !drive(&path.to_string_lossy()) {
        return None;
    }
    let canonical = fs::canonicalize(path).ok()?;
    drive(&canonical.to_string_lossy()).then_some(canonical)
}

pub fn local_file(path: &Path) -> Option<PathBuf> {
    local_path(path).filter(|canonical| canonical.is_file())
}

pub fn extract(path: &Path, index: i32) -> Option<RgbaImage> {
    let file = local_file(path)?;
    let display = file.to_str()?;
    let regular = display
        .strip_prefix(r"\\?\")
        .filter(|name| name.encode_utf16().count() < 260)
        .unwrap_or(display);
    let mut icon = null_mut();
    // Extract a resource as image data without starting the executable.
    unsafe {
        let count = PrivateExtractIconsW(
            wide(OsStr::new(regular)).as_ptr(),
            index,
            SIZE as i32,
            SIZE as i32,
            &mut icon,
            null_mut(),
            1,
            0,
        );
        if icon.is_null() {
            return None;
        }
        let pixels = if count == 1 { render(icon) } else { None };
        DestroyIcon(icon);
        pixels
    }
}

unsafe fn render(icon: HICON) -> Option<RgbaImage> {
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return None;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE as i32,
            biHeight: -(SIZE as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buffer = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut buffer, null_mut(), 0);
    let result = if !bitmap.is_null() && !buffer.is_null() {
        let previous = SelectObject(dc, bitmap);
        let result = if previous.is_null() {
            None
        } else {
            let pixels = std::slice::from_raw_parts_mut(
                buffer.cast::<u8>(),
                SIZE as usize * SIZE as usize * 4,
            );
            let paint = |pixels: &mut [u8], value| {
                pixels.fill(value);
                (DrawIconEx(
                    dc,
                    0,
                    0,
                    icon,
                    SIZE as i32,
                    SIZE as i32,
                    0,
                    null_mut(),
                    DI_NORMAL,
                ) != 0)
                    .then(|| pixels.to_vec())
            };
            // Rendering against both backgrounds preserves alpha for legacy mask icons too.
            let result = paint(pixels, 0)
                .zip(paint(pixels, 255))
                .and_then(|(black, white)| {
                    let mut rgba = Vec::with_capacity(black.len());
                    for (dark, light) in black.chunks_exact(4).zip(white.chunks_exact(4)) {
                        let alpha = 255
                            - (0..3)
                                .map(|i| light[i].saturating_sub(dark[i]))
                                .max()
                                .unwrap();
                        for channel in [dark[2], dark[1], dark[0]] {
                            let value = if alpha == 0 {
                                0
                            } else {
                                (u32::from(channel) * 255 / u32::from(alpha)).min(255) as u8
                            };
                            rgba.push(value);
                        }
                        rgba.push(alpha);
                    }
                    rgba.chunks_exact(4)
                        .any(|pixel| pixel[3] != 0)
                        .then(|| RgbaImage::from_raw(SIZE, SIZE, rgba))
                        .flatten()
                });
            SelectObject(dc, previous);
            result
        };
        DeleteObject(bitmap);
        result
    } else {
        None
    };
    DeleteDC(dc);
    result
}

#[derive(Default)]
pub struct Shortcuts(Vec<Shortcut>);

struct Shortcut {
    game: LinkTarget,
    icon: PathBuf,
    index: i32,
}

enum LinkTarget {
    Executable(String),
    Catalog(String),
}

fn known_folder(folder: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    unsafe {
        let path = SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0.cast()));
        result
    }
}

impl Shortcuts {
    pub fn read() -> Self {
        use windows::Win32::UI::Shell::{
            FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_PublicDesktop,
        };
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
        let mut shortcuts = Vec::new();
        for root in [
            &FOLDERID_Desktop,
            &FOLDERID_PublicDesktop,
            &FOLDERID_Programs,
            &FOLDERID_CommonPrograms,
        ]
        .into_iter()
        .filter_map(known_folder)
        .filter_map(|root| local_path(&root))
        {
            for entry in walkdir::WalkDir::new(root)
                .max_depth(4)
                .into_iter()
                .filter_map(Result::ok)
                .take(1_200)
            {
                let file = entry.path();
                if !entry.file_type().is_file()
                    || entry.metadata().is_ok_and(|meta| meta.len() > 128 * 1024)
                {
                    continue;
                }
                let item = match file
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    Some("lnk") if initialized => read_lnk(file),
                    Some("url") => read_url(file),
                    _ => None,
                };
                shortcuts.extend(item);
            }
        }
        if initialized {
            unsafe { CoUninitialize() };
        }
        Self(shortcuts)
    }

    pub fn candidates<'a>(&'a self, game: &'a Game) -> impl Iterator<Item = (&'a Path, i32)> {
        self.0
            .iter()
            .filter(move |shortcut| match &shortcut.game {
                LinkTarget::Executable(path) => game
                    .paths()
                    .any(|entry| path_key(Path::new(entry)) == *path),
                LinkTarget::Catalog(id) => game.catalog_id == *id,
            })
            .map(|shortcut| (shortcut.icon.as_path(), shortcut.index))
    }
}

fn read_lnk(path: &Path) -> Option<Shortcut> {
    let file = local_file(path)?;
    let raw = wide(file.as_os_str());
    let mut target = [0u16; 1024];
    let mut icon = [0u16; 1024];
    let mut index = 0;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        persist.Load(PCWSTR(raw.as_ptr()), STGM_READ).ok()?;
        link.GetPath(&mut target, null_mut(), SLGP_RAWPATH.0 as u32)
            .ok()?;
        link.GetIconLocation(&mut icon, &mut index).ok()?;
        let target = from_wide(&target);
        if target
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("RiotClientServices.exe"))
        {
            let mut arguments = [0u16; 1024];
            link.GetArguments(&mut arguments).ok()?;
            let arguments = String::from_utf16_lossy(
                arguments
                    .split(|&part| part == 0)
                    .next()
                    .unwrap_or_default(),
            );
            let product = ["league_of_legends", "valorant"].into_iter().find(|name| {
                arguments
                    .split_whitespace()
                    .any(|arg| arg == format!("--launch-product={name}"))
            })?;
            return Some(Shortcut {
                game: LinkTarget::Catalog(format!("riot:{product}.live")),
                icon: local_file(&from_wide(&icon))?,
                index,
            });
        }
        let target = local_file(&target)?;
        Some(Shortcut {
            game: LinkTarget::Executable(path_key(&target)),
            icon: local_file(&from_wide(&icon))?,
            index,
        })
    }
}

fn read_url(path: &Path) -> Option<Shortcut> {
    let bytes = super::read_limited(fs::File::open(local_file(path)?).ok()?, 128 * 1024).ok()?;
    let text = if bytes.starts_with(&[0xff, 0xfe]) {
        let value = bytes[2..]
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&value).ok()?
    } else {
        String::from_utf8(bytes).ok()?
    };
    let value = |key: &str| {
        text.trim_start_matches('\u{feff}')
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once('=')?;
                name.trim()
                    .eq_ignore_ascii_case(key)
                    .then_some(value.trim().trim_matches('"'))
            })
    };
    let id = value("URL")?
        .strip_prefix("steam://rungameid/")?
        .parse::<u32>()
        .ok()?;
    let icon = local_file(Path::new(value("IconFile")?))?;
    let index = value("IconIndex")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Some(Shortcut {
        game: LinkTarget::Catalog(format!("steam:{id}")),
        icon,
        index,
    })
}

pub fn steam_cache(game: &Game) -> impl Iterator<Item = PathBuf> {
    let candidates = (|| {
        let id = game
            .catalog_id
            .strip_prefix("steam:")?
            .parse::<u32>()
            .ok()?;
        let cache = steam_root()?.join("appcache/librarycache");
        let app = cache.join(id.to_string());
        let mut files = vec![
            cache.join(format!("{id}_icon.jpg")),
            app.join("icon.jpg"),
            app.join("icon.png"),
            app.join("clienticon.ico"),
        ];
        // New clients keep the icon in a content-hash folder for the same App ID.
        for entry in fs::read_dir(&app)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .take(24)
        {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                files.extend(
                    ["icon.jpg", "icon.png", "clienticon.ico"].map(|name| entry.path().join(name)),
                );
            }
        }
        Some(files)
    })();
    candidates.into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_icon_location_from_a_windows_shortcut() {
        let directory = tempfile::tempdir().unwrap();
        let shortcut = directory.path().join("Game.lnk");
        let target = std::env::current_exe().unwrap();
        let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico");
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                link.SetPath(PCWSTR(wide(target.as_os_str()).as_ptr()))
                    .unwrap();
                link.SetIconLocation(PCWSTR(wide(image.as_os_str()).as_ptr()), 0)
                    .unwrap();
                let persist: IPersistFile = link.cast().unwrap();
                persist
                    .Save(PCWSTR(wide(shortcut.as_os_str()).as_ptr()), true)
                    .unwrap();
            }
            let candidate = read_lnk(&shortcut).unwrap();
            CoUninitialize();
            assert_eq!(candidate.icon, fs::canonicalize(image).unwrap());
            assert!(
                matches!(candidate.game, LinkTarget::Executable(path) if path == path_key(&target))
            );
        }
    }
}
