mod local;
mod online;

use crate::{
    model::{self, Game, Timestamp},
    CommandResult, Engine,
};
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, UNIX_EPOCH},
};
use tauri::{ipc::Response, AppHandle, Manager, State};

const IMAGE_LIMIT: usize = 8 * 1024 * 1024;
const PNG_LIMIT: usize = 128 * 1024;

pub fn read_limited(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "image too large",
        ));
    }
    Ok(bytes)
}

fn normalize(bytes: &[u8]) -> CommandResult<Vec<u8>> {
    let unsupported = || "Выберите корректное изображение PNG, JPG или ICO.".to_owned();
    let format = image::guess_format(bytes).map_err(|_| unsupported())?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Ico
    ) {
        return Err(unsupported());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Файл повреждён или изображение больше 4096 × 4096 пикселей.".to_owned())?;
    encode(image.thumbnail(local::SIZE, local::SIZE)).ok_or_else(unsupported)
}

fn encode(image: DynamicImage) -> Option<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn import(path: &Path) -> CommandResult<Vec<u8>> {
    let file = local::local_file(path)
        .and_then(|path| File::open(path).ok())
        .ok_or("Не удалось открыть локальный файл иконки.")?;
    let bytes = read_limited(file, IMAGE_LIMIT)
        .map_err(|_| "Не удалось прочитать файл. Максимальный размер — 8 МБ.".to_owned())?;
    normalize(&bytes)
}

fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct IconStatus {
    pub revision: String,
    pub custom: bool,
}

#[derive(Default)]
struct Files {
    custom: Option<String>,
    automatic: Option<String>,
}

impl Files {
    fn slot(&mut self, custom: bool) -> &mut Option<String> {
        if custom {
            &mut self.custom
        } else {
            &mut self.automatic
        }
    }

    fn status(&self) -> Option<IconStatus> {
        let custom = self.custom.is_some();
        let revision = self.custom.as_ref().or(self.automatic.as_ref())?.clone();
        Some(IconStatus { revision, custom })
    }
}

pub struct IconStore {
    root: PathBuf,
    files: Mutex<HashMap<String, Files>>,
}

impl IconStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Mutex::default(),
        }
    }

    fn path(&self, game: &str, suffix: &str) -> PathBuf {
        self.root.join(format!("{}.{suffix}", hash(game)))
    }

    fn png(&self, game: &str, custom: bool) -> PathBuf {
        self.path(game, if custom { "custom.png" } else { "auto.png" })
    }

    fn read_png(&self, game: &str, custom: bool) -> Option<Vec<u8>> {
        let bytes = read_limited(File::open(self.png(game, custom)).ok()?, PNG_LIMIT).ok()?;
        bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
    }

    fn load(&self, game: &str) {
        if self.files.lock().map_or(true, |all| all.contains_key(game)) {
            return;
        }
        let loaded = Files {
            custom: self.read_png(game, true).map(hash),
            automatic: self.read_png(game, false).map(hash),
        };
        if let Ok(mut all) = self.files.lock() {
            all.entry(game.to_owned()).or_insert(loaded);
        }
    }

    pub fn status(&self, games: &[Game]) -> HashMap<String, IconStatus> {
        self.files
            .lock()
            .map(|all| {
                games
                    .iter()
                    .filter_map(|game| Some((game.path.clone(), all.get(&game.path)?.status()?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, game: &str, custom: bool, data: &[u8]) -> io::Result<()> {
        self.load(game);
        let mut all = self
            .files
            .lock()
            .map_err(|_| io::Error::other("game icon cache unavailable"))?;
        fs::create_dir_all(&self.root)?;
        write_atomic(&self.png(game, custom), data)?;
        *all.entry(game.to_owned()).or_default().slot(custom) = Some(hash(data));
        Ok(())
    }

    fn reset(&self, game: &str) -> io::Result<()> {
        self.load(game);
        let mut all = self
            .files
            .lock()
            .map_err(|_| io::Error::other("game icon cache unavailable"))?;
        match fs::remove_file(self.png(game, true)) {
            Ok(()) => (),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }
        all.entry(game.to_owned()).or_default().custom = None;
        Ok(())
    }

    fn active(&self, game: &str) -> Option<IconStatus> {
        self.files.lock().ok()?.get(game)?.status()
    }
}

#[tauri::command]
pub async fn choose_game(window: tauri::WebviewWindow) -> CommandResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let error = |_| "Не удалось открыть диалог выбора игры.".to_owned();
        let owner = window.hwnd().map_err(error)?;
        local::pick_game(owner).map_err(|_| "Не удалось выбрать файл игры.".to_owned())
    })
    .await
    .map_err(|_| "Не удалось открыть диалог выбора игры.".to_owned())?
}

#[tauri::command]
pub fn game_icon(
    store: State<'_, IconStore>,
    game: String,
    revision: String,
    custom: bool,
) -> Response {
    let active = store
        .active(&game)
        .is_some_and(|entry| entry.revision == revision && entry.custom == custom);
    Response::new(if active {
        store.read_png(&game, custom).unwrap_or_default()
    } else {
        Vec::new()
    })
}

#[tauri::command]
pub async fn set_game_icon(
    app: AppHandle,
    game: String,
    file: Option<String>,
) -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let exists = app
            .state::<Mutex<Engine>>()
            .lock()
            .map(|engine| engine.data.games.iter().any(|entry| entry.path == game))
            .unwrap_or(false);
        if !exists {
            return Err("Игра не найдена.".into());
        }
        let store = app.state::<IconStore>();
        let result = if let Some(file) = file {
            let image = import(Path::new(&file))?;
            store.save(&game, true, &image)
        } else {
            store.reset(&game)
        };
        result.map_err(|_| "Не удалось сохранить изменения иконки.".into())
    })
    .await
    .map_err(|_| "Не удалось обработать иконку.".to_owned())?
}

#[derive(Default, Serialize, Deserialize)]
struct CacheMeta {
    fingerprint: String,
    local: bool,
    was_locked: bool,
    local_after: Timestamp,
    online_after: Timestamp,
    #[serde(skip)]
    check_after: Timestamp,
    #[serde(skip)]
    paths: Vec<String>,
    #[serde(skip)]
    found: bool,
}

fn fingerprint(game: &Game) -> String {
    let mut digest = Sha256::new();
    digest.update(game.catalog_id.as_bytes());
    for path in game.paths().chain(game.shortcut.as_deref()) {
        digest.update([0]);
        digest.update(path.as_bytes());
        if let Ok(meta) = fs::metadata(path) {
            digest.update(meta.len().to_le_bytes());
            if let Ok(time) = meta.modified() {
                digest.update(
                    time.duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                        .to_le_bytes(),
                );
            }
        }
    }
    format!("{:x}", digest.finalize())
}

fn extracted(path: &Path, index: i32) -> Option<Vec<u8>> {
    local::extract(path, index).and_then(|image| encode(DynamicImage::ImageRgba8(image)))
}

pub fn manual_target(path: &Path) -> CommandResult<(PathBuf, Option<(PathBuf, i32)>)> {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    {
        let (target, icon, index) =
            local::selected_link(path).ok_or("Не удалось прочитать ярлык игры.")?;
        Ok((target, Some((icon, index))))
    } else {
        Ok((path.to_owned(), None))
    }
}

pub fn manual_icon(exe: &Path, icon: Option<(&Path, i32)>) -> Option<Vec<u8>> {
    icon.and_then(|(path, index)| import(path).ok().or_else(|| extracted(path, index)))
        .or_else(|| extracted(exe, 0))
}

fn local_icon(game: &Game, shortcuts: &mut Option<local::Shortcuts>) -> Option<Vec<u8>> {
    game.shortcut
        .as_deref()
        .and_then(|path| local::selected_link(Path::new(path)))
        .filter(|(target, _, _)| {
            local::local_file(target).is_some_and(|target| {
                crate::game_catalog::path_key(&target)
                    == crate::game_catalog::path_key(Path::new(&game.path))
            })
        })
        .and_then(|(_, path, index)| import(&path).ok().or_else(|| extracted(&path, index)))
        .or_else(|| game.paths().find_map(|path| extracted(Path::new(path), 0)))
        .or_else(|| local::steam_cache(game).find_map(|file| import(&file).ok()))
        .or_else(|| {
            shortcuts
                .get_or_insert_with(local::Shortcuts::read)
                .candidates(game)
                .find_map(|(path, index)| import(path).ok().or_else(|| extracted(path, index)))
        })
}

fn write_meta(store: &IconStore, game: &str, entry: &CacheMeta) {
    if let Ok(data) = serde_json::to_vec(entry) {
        if fs::create_dir_all(&store.root).is_ok() {
            let _ = write_atomic(&store.path(game, "meta.json"), &data);
        }
    }
}

pub fn worker(app: AppHandle) {
    let store = app.state::<IconStore>();
    let client = online::client();
    let mut meta = HashMap::<String, CacheMeta>::new();
    loop {
        let games = match app.state::<Mutex<Engine>>().lock() {
            Ok(engine) => engine
                .data
                .games
                .iter()
                .map(|game| {
                    let locked = game.paths().all(|path| engine.locks.contains_key(path));
                    (game.clone(), locked)
                })
                .collect::<Vec<_>>(),
            Err(_) => return,
        };
        let now = model::now();
        let mut shortcuts = None;
        let mut fetch = Vec::new();
        for (game, locked) in &games {
            store.load(&game.path);
            if store.active(&game.path).is_some_and(|icon| icon.custom) {
                continue;
            }
            let entry = meta.entry(game.path.clone()).or_insert_with(|| {
                let cached = store.path(&game.path, "meta.json");
                File::open(cached)
                    .ok()
                    .and_then(|file| read_limited(file, 8192).ok())
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default()
            });
            if now >= entry.check_after
                || now < entry.check_after - 60
                || entry.was_locked && !locked
                || !entry.paths.iter().map(String::as_str).eq(game.paths())
            {
                entry.check_after = now + 60;
                entry.paths = game.paths().map(str::to_owned).collect();
                entry.found = store.png(&game.path, false).is_file();
                let fingerprint = fingerprint(game);
                let changed = entry.fingerprint != fingerprint;
                let output_missing = entry.local && !entry.found;
                if changed
                    || output_missing
                    || !entry.local && (now >= entry.local_after || entry.was_locked && !locked)
                {
                    if changed || output_missing {
                        entry.online_after = 0;
                    }
                    entry.local = local_icon(game, &mut shortcuts)
                        .is_some_and(|image| store.save(&game.path, false, &image).is_ok());
                    entry.found |= entry.local;
                    entry.fingerprint = fingerprint;
                    entry.local_after = now + if *locked { 300 } else { 3_600 };
                    entry.was_locked = *locked;
                    write_meta(&store, &game.path, entry);
                }
                entry.was_locked = *locked;
            }
            if !entry.found && !entry.local && now >= entry.online_after {
                fetch.push(game);
            }
        }
        for game in fetch.into_iter().take(5) {
            let entry = meta
                .get_mut(&game.path)
                .expect("initialized icon cache metadata");
            if store.active(&game.path).is_some_and(|icon| icon.custom) {
                continue;
            }
            if let Some(image) = client
                .as_ref()
                .and_then(|client| online::fetch(client, game))
            {
                entry.found = store.save(&game.path, false, &image).is_ok();
            }
            entry.online_after = model::now() + 6 * 3_600;
            write_meta(&store, &game.path, entry);
        }
        std::thread::sleep(Duration::from_secs(8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn custom_cache_has_priority_and_is_reversible() {
        let root = tempfile::tempdir().unwrap();
        let key = "C:/game/fixture.exe";
        let store = IconStore::new(root.path().into());
        let red = encode(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            10,
            10,
            Rgba([255, 0, 0, 255]),
        )))
        .unwrap();
        let blue = encode(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            10,
            10,
            Rgba([0, 0, 255, 255]),
        )))
        .unwrap();
        store.save(key, true, &red).unwrap();
        store.save(key, false, &blue).unwrap();
        let reopened = IconStore::new(root.path().into());
        reopened.load(key);
        let state = reopened.active(key).unwrap();
        assert!(state.custom);
        assert_eq!(state.revision, hash(&red));
        reopened.reset(key).unwrap();
        assert_eq!(reopened.active(key).unwrap().revision, hash(&blue));
        assert!(normalize(b"<svg />").is_err());
    }

    #[test]
    fn extracts_compiled_windows_icon() {
        let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico");
        let image = local::extract(&icon, 0).unwrap();
        assert_eq!(image.dimensions(), (local::SIZE, local::SIZE));
        assert!(image.pixels().any(|pixel| pixel[3] > 0));
        assert!(image.pixels().any(|pixel| pixel[3] == 0));
    }

    #[test]
    fn imports_the_supported_formats_and_rejects_oversized_files() {
        let img =
            DynamicImage::ImageRgb8(ImageBuffer::from_pixel(140, 70, image::Rgb([12, 34, 56])));
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Ico] {
            let mut bytes = Cursor::new(Vec::new());
            if format == ImageFormat::Ico {
                DynamicImage::ImageRgba8(img.to_rgba8())
                    .write_to(&mut bytes, format)
                    .unwrap();
            } else {
                img.write_to(&mut bytes, format).unwrap();
            }
            let output =
                normalize(bytes.get_ref()).unwrap_or_else(|err| panic!("{format:?}: {err}"));
            assert_eq!(
                image::load_from_memory(&output).unwrap().width(),
                local::SIZE
            );
        }
        assert!(read_limited(&b"oversize"[..], 4).is_err());
    }
}
