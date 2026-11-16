#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod achievements;
mod game_catalog;
mod game_detector;
mod game_icons;
mod game_tracking;
mod model;

use model::{
    ActivityDetails, ActivitySummary, Data, Game, GameSession, Session, Settings, Statistics,
    Timestamp,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

type CommandResult<T> = Result<T, String>;

struct Engine {
    data: Data,
    file: PathBuf,
    locks: HashMap<String, File>,
    failures: HashMap<String, String>,
    retry_at: Timestamp,
    protected: Vec<PathBuf>,
    executable: PathBuf,
    game_tracking: game_tracking::GameTracking,
    achievement_progress: BTreeMap<String, achievements::Progress>,
    achievements_due: Timestamp,
    achievements_dirty: bool,
    achievement_day: Option<chrono::NaiveDate>,
}

impl Engine {
    fn load(directory: PathBuf) -> CommandResult<Self> {
        fs::create_dir_all(&directory).map_err(storage_error)?;
        let file = directory.join("afterwork.json");
        let mut data: Data = match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage_error)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(error) => return Err(storage_error(error)),
        };
        data.settings.validate()?;
        let migrate = file.is_file() && data.achievement_goals.is_empty();
        let now = model::now();
        if migrate {
            achievements::preserve_legacy_awards(&mut data, now);
        }
        if data.achievement_goals.is_empty() {
            data.achievement_goals.insert(
                achievements::day_of(now),
                i64::from(data.settings.minutes) * 120,
            );
        }
        let protected = ["SystemRoot", "ProgramData"]
            .iter()
            .filter_map(std::env::var_os)
            .filter_map(|path| fs::canonicalize(path).ok())
            .collect();
        let executable = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(storage_error)?;
        let mut engine = Self {
            data,
            file,
            locks: HashMap::new(),
            failures: HashMap::new(),
            retry_at: 0,
            protected,
            executable,
            game_tracking: game_tracking::GameTracking::default(),
            achievement_progress: BTreeMap::new(),
            achievements_due: 0,
            achievements_dirty: true,
            achievement_day: None,
        };
        if migrate {
            let _ = engine.update(|_| Ok(()));
        }
        Ok(engine)
    }

    fn refresh_achievements(&mut self, now: Timestamp) {
        self.achievements_due = now + 5;
        let date = achievements::day_of(now);
        if !self.achievements_dirty
            && self.achievement_day == Some(date)
            && !self
                .data
                .active
                .as_ref()
                .is_some_and(|session| session.running.is_some())
            && self.game_tracking.live.is_empty()
        {
            return;
        }
        let progress = achievements::progress(&self.data, &self.game_report(), now);
        let new = progress
            .iter()
            .filter(|(id, item)| item.current >= item.target && !self.data.awards.contains_key(*id))
            .map(|(id, _)| (id.clone(), now))
            .collect::<Vec<_>>();
        self.achievements_dirty = !new.is_empty()
            && self
                .update(|data| {
                    data.awards.extend(new);
                    Ok(())
                })
                .is_err();
        self.achievement_day = Some(date);
        self.achievement_progress = progress;
    }

    fn update(&mut self, change: impl FnOnce(&mut Data) -> CommandResult<()>) -> CommandResult<()> {
        let mut next = self.data.clone();
        change(&mut next)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(self.file.parent().unwrap()).map_err(storage_error)?;
        {
            let mut writer = BufWriter::new(temp.as_file_mut());
            serde_json::to_writer(&mut writer, &next).map_err(storage_error)?;
            writer.flush().map_err(storage_error)?;
        }
        temp.as_file().sync_all().map_err(storage_error)?;
        temp.persist(&self.file).map_err(storage_error)?;
        self.data = next;
        self.achievements_dirty = true;
        Ok(())
    }

    fn game_path(&self, input: &Path) -> CommandResult<PathBuf> {
        let path = fs::canonicalize(input).map_err(|_| "Файл игры недоступен.".to_owned())?;
        let display = path.to_string_lossy();
        let local_drive = display.strip_prefix(r"\\?\").is_some_and(|rest| {
            rest.as_bytes().get(..3).is_some_and(|prefix| {
                prefix[0].is_ascii_alphabetic() && prefix[1] == b':' && prefix[2] == b'\\'
            })
        });
        let lower = PathBuf::from(display.to_lowercase());
        if !local_drive
            || !path.is_file()
            || !path
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
            || lower == PathBuf::from(self.executable.to_string_lossy().to_lowercase())
            || self
                .protected
                .iter()
                .any(|root| lower.starts_with(root.to_string_lossy().to_lowercase()))
        {
            return Err("Выберите исполняемый файл игры вне системных папок Windows.".into());
        }
        Ok(path)
    }

    fn sync_locks(&mut self, now: Timestamp, force: bool) {
        if !self.data.status(now).locked() {
            self.locks.clear();
            self.failures.clear();
            return;
        }
        if !force && now < self.retry_at && now >= self.retry_at - 3 {
            return;
        }
        self.retry_at = now + 3;
        let targets = self
            .data
            .games
            .iter()
            .filter(|game| !game.excluded)
            .flat_map(Game::paths)
            .collect::<std::collections::HashSet<_>>();
        self.locks.retain(|path, _| targets.contains(path.as_str()));
        self.failures.clear();
        for path in targets {
            if self.locks.contains_key(path) {
                continue;
            }
            let result = self.game_path(Path::new(path)).and_then(|path| {
                // Deny new read/execute opens. Windows keeps existing mapped images alive.
                OpenOptions::new()
                    .read(true)
                    .share_mode(2)
                    .open(path)
                    .map_err(|err| {
                        if err.raw_os_error() == Some(32) {
                            "Файл занят другой программой. Afterwork повторит попытку.".into()
                        } else {
                            format!(
                                "Не удалось заблокировать файл (код ОС {}).",
                                err.raw_os_error().unwrap_or_default()
                            )
                        }
                    })
            });
            match result {
                Ok(file) => {
                    self.locks.insert(path.to_owned(), file);
                }
                Err(error) => {
                    self.failures.insert(path.to_owned(), error);
                }
            }
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> String {
    format!("Ошибка хранилища Afterwork: {error}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    date: String,
    seconds: i64,
    phase: i64,
    target: i64,
    locked: bool,
    settings: Settings,
    activities: Vec<model::Activity>,
    activity_summaries: BTreeMap<u64, ActivitySummary>,
    games: Vec<Game>,
    active: Option<Session>,
    elapsed: i64,
    failures: HashMap<String, String>,
    blocked_count: usize,
    running_games: Vec<String>,
    game_history: Option<Vec<GameSession>>,
    game_icons: Option<HashMap<String, game_icons::IconStatus>>,
    awards: BTreeMap<String, Timestamp>,
    achievement_progress: Option<BTreeMap<String, achievements::Progress>>,
    report: Option<Report>,
}

#[derive(Serialize)]
struct Report {
    totals: Statistics,
    history: Vec<Session>,
}

#[tauri::command]
fn snapshot(
    state: State<'_, Mutex<Engine>>,
    icons: State<'_, game_icons::IconStore>,
    report: bool,
    games: Option<bool>,
    achievements: Option<bool>,
) -> CommandResult<Snapshot> {
    let engine = state.lock().map_err(storage_error)?;
    let now = model::now();
    let data = &engine.data;
    let status = data.status(now);
    Ok(Snapshot {
        date: status.day.to_string(),
        seconds: status.seconds,
        phase: status.phase,
        target: status.target,
        locked: status.locked(),
        settings: data.settings.clone(),
        activities: data.activities.clone(),
        activity_summaries: data.activity_summaries(now),
        games: data.games.clone(),
        active: data.active.clone(),
        elapsed: data
            .active
            .as_ref()
            .map_or(0, |session| session.elapsed(now)),
        failures: engine.failures.clone(),
        blocked_count: data
            .games
            .iter()
            .filter(|game| {
                !game.excluded && game.paths().all(|path| engine.locks.contains_key(path))
            })
            .count(),
        running_games: engine.game_tracking.live.keys().cloned().collect(),
        game_history: games.unwrap_or(false).then(|| engine.game_report()),
        game_icons: games.unwrap_or(false).then(|| icons.status(&data.games)),
        awards: data.awards.clone(),
        achievement_progress: achievements
            .unwrap_or(false)
            .then(|| engine.achievement_progress.clone()),
        report: report.then(|| Report {
            totals: data.statistics(now),
            history: data.history.clone(),
        }),
    })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
    Start { activity: u64 },
    Pause,
    Resume,
    ToggleTimer,
    Finish { note: String },
    AddActivity { details: ActivityDetails },
    EditActivity { id: u64, details: ActivityDetails },
    ArchiveActivity { id: u64, archived: bool },
    AddGame { path: String },
    RemoveGame { path: String },
    ExcludeGame { path: String, excluded: bool },
    SaveSettings { settings: Settings },
}

#[tauri::command]
fn apply(app: AppHandle, action: Action) -> CommandResult<()> {
    apply_action(&app, action)
}

fn apply_action(app: &AppHandle, action: Action) -> CommandResult<()> {
    let state = app.state::<Mutex<Engine>>();
    let mut engine = state.lock().map_err(storage_error)?;
    let now = model::now();
    let action = match action {
        Action::AddGame { path } => {
            let path = engine.game_path(Path::new(&path))?;
            Action::AddGame {
                path: path
                    .to_str()
                    .ok_or("Имя файла содержит неподдерживаемые символы.")?
                    .into(),
            }
        }
        other => other,
    };
    engine.update(|data| {
        match action {
            Action::Start { activity } => {
                if data.active.is_some()
                    || !data
                        .activities
                        .iter()
                        .any(|item| item.id == activity && !item.archived)
                {
                    return Err("Выберите занятие и завершите текущую сессию.".into());
                }
                data.active = Some(Session {
                    id: data.id(),
                    activity,
                    started: now,
                    running: Some(now),
                    spans: Vec::new(),
                    note: String::new(),
                    finished: None,
                });
            }
            Action::Pause | Action::Resume | Action::ToggleTimer => {
                let session = data.active.as_mut().ok_or("Текущая сессия отсутствует.")?;
                if matches!(action, Action::Pause)
                    || matches!(action, Action::ToggleTimer) && session.running.is_some()
                {
                    session.pause(now);
                } else {
                    session.running.get_or_insert(now);
                }
            }
            Action::Finish { note } => {
                if note.chars().count() > 2000 {
                    return Err("Заметка слишком длинная.".into());
                }
                let mut session = data.active.take().ok_or("Текущая сессия отсутствует.")?;
                session.pause(now);
                session.note = note.trim().into();
                session.finished = Some(now);
                data.history.push(session);
            }
            Action::AddActivity { details } => {
                let details = data.validate_activity(details, None)?;
                let id = data.id();
                data.activities.push(model::Activity {
                    id,
                    details,
                    archived: false,
                });
            }
            Action::EditActivity { id, details } => {
                let details = data.validate_activity(details, Some(id))?;
                data.activity_mut(id)?.details = details;
            }
            Action::ArchiveActivity { id, archived } => data.activity_mut(id)?.archived = archived,
            Action::AddGame { path } => {
                if data
                    .games
                    .iter()
                    .any(|game| game_tracking::contains_path(game, Path::new(&path)))
                {
                    return Err("Эта игра уже добавлена.".into());
                }
                let name = Path::new(&path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                data.games.push(Game {
                    path,
                    name,
                    source: String::new(),
                    catalog_id: String::new(),
                    aliases: Vec::new(),
                    excluded: false,
                });
            }
            Action::RemoveGame { path } => data
                .games
                .retain(|game| game.path != path || !game.catalog_id.is_empty()),
            Action::ExcludeGame { path, excluded } => {
                let game = data
                    .games
                    .iter_mut()
                    .find(|game| game.path == path)
                    .ok_or("Игра не найдена.")?;
                game.excluded = excluded;
            }
            Action::SaveSettings { settings } => {
                settings.validate()?;
                if data.settings.minutes != settings.minutes {
                    data.achievement_goals
                        .insert(achievements::day_of(now), i64::from(settings.minutes) * 120);
                }
                data.settings = settings;
            }
        }
        Ok(())
    })?;
    engine.close_untracked_games(now);
    engine.sync_locks(now, true);
    engine.refresh_achievements(now);
    let timer = engine
        .data
        .active
        .as_ref()
        .map(|session| session.running.is_some());
    drop(engine);
    update_tray_timer(&app.state::<MenuItem<tauri::Wry>>(), timer);
    Ok(())
}

fn update_tray_timer(item: &MenuItem<tauri::Wry>, timer: Option<bool>) {
    let _ = item.set_enabled(timer.is_some());
    let _ = item.set_text(match timer {
        Some(true) => "Поставить на паузу",
        Some(false) => "Продолжить сессию",
        None => "Нет активной сессии",
    });
}

#[tauri::command]
fn autostart(app: AppHandle, enabled: Option<bool>) -> CommandResult<bool> {
    let manager = app.autolaunch();
    match enabled {
        Some(true) => manager.enable(),
        Some(false) => manager.disable(),
        None => Ok(()),
    }
    .map_err(|error| format!("Ошибка автозапуска: {error}"))?;
    manager
        .is_enabled()
        .map_err(|error| format!("Ошибка автозапуска: {error}"))
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn worker(app: AppHandle) {
    loop {
        let now = model::now();
        let notify = {
            let state = app.state::<Mutex<Engine>>();
            let Ok(mut engine) = state.lock() else { return };
            engine.sync_locks(now, false);
            if now >= engine.achievements_due || now < engine.achievements_due - 5 {
                engine.refresh_achievements(now);
            }
            let slot = engine.data.status(now).reminder_slot(now);
            engine.data.settings.notifications
                && slot.is_some()
                && slot > engine.data.reminder
                && engine
                    .update(|data| {
                        data.reminder = slot;
                        Ok(())
                    })
                    .is_ok()
        };
        if notify {
            let _ = app
                .notification()
                .builder()
                .title("Afterwork")
                .body("Пора улучшить жизнь :3")
                .show();
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--background")
                .build(),
        )
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            app.manage(game_icons::IconStore::new(directory.join("game-icons-v1")));
            let mut engine = Engine::load(directory).map_err(std::io::Error::other)?;
            engine.sync_locks(model::now(), true);
            engine.refresh_achievements(model::now());
            let timer_state = engine
                .data
                .active
                .as_ref()
                .map(|session| session.running.is_some());
            app.manage(Mutex::new(engine));
            let open = MenuItem::with_id(app, "open", "Открыть Afterwork", true, None::<&str>)?;
            let timer =
                MenuItem::with_id(app, "timer", "Нет активной сессии", false, None::<&str>)?;
            update_tray_timer(&timer, timer_state);
            let separator = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Выйти из Afterwork", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &timer, &separator, &quit])?;
            app.manage(timer);
            TrayIconBuilder::new()
                .tooltip("Afterwork")
                .icon(app.default_window_icon().expect("bundled icon").clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show(app),
                    "timer" => {
                        if let Err(error) = apply_action(app, Action::ToggleTimer) {
                            let _ = app
                                .notification()
                                .builder()
                                .title("Afterwork")
                                .body(error)
                                .show();
                        }
                    }
                    "quit" => {
                        if let Ok(mut engine) = app.state::<Mutex<Engine>>().lock() {
                            engine.checkpoint_games(model::now());
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show(tray.app_handle());
                    }
                })
                .build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || worker(handle));
            let handle = app.handle().clone();
            std::thread::spawn(move || game_tracking::track(handle));
            let handle = app.handle().clone();
            std::thread::spawn(move || game_icons::worker(handle));
            if !std::env::args_os().any(|arg| arg == "--background") {
                show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            apply,
            autostart,
            game_icons::game_icon,
            game_icons::set_game_icon
        ])
        .run(tauri::generate_context!())
        .expect("Не удалось запустить Afterwork");
}
