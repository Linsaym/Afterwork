#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod achievements;
mod actions;
mod game_catalog;
mod game_detector;
mod game_history;
mod game_icons;
mod game_tracking;
mod model;
mod storage;
mod updates;
mod work_history;

use actions::Action;
use model::{ActivitySummary, Data, Game, GameSession, Session, Settings, Statistics, Timestamp};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
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
const DEV_IDENTIFIER: &str = "app.afterwork.desktop.dev";

struct Engine {
    data: Data,
    storage: storage::Storage,
    work: work_history::WorkHistory,
    work_revision: u64,
    monitoring_dirty: Option<usize>,
    isolated: bool,
    locks: HashMap<String, File>,
    failures: HashMap<String, String>,
    retry_at: Timestamp,
    protected: Vec<PathBuf>,
    executable: PathBuf,
    game_tracking: game_tracking::GameTracking,
    game_history: game_history::GameHistory,
    achievement_progress: BTreeMap<String, achievements::Progress>,
    achievement_history: Option<achievements::History>,
    achievements_due: Timestamp,
    achievements_dirty: bool,
    achievement_day: Option<chrono::NaiveDate>,
}

impl Engine {
    fn load(directory: PathBuf) -> CommandResult<Self> {
        let (storage, mut data) = storage::Storage::open(&directory)?;
        let protected = ["SystemRoot", "ProgramData"]
            .iter()
            .filter_map(std::env::var_os)
            .filter_map(|path| fs::canonicalize(path).ok())
            .collect();
        let executable = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(storage_error)?;
        let work = work_history::WorkHistory::new(&data);
        let game_history = game_history::GameHistory::new(std::mem::take(&mut data.game_sessions));
        let mut engine = Self {
            data,
            storage,
            work,
            work_revision: 0,
            monitoring_dirty: None,
            isolated: false,
            locks: HashMap::new(),
            failures: HashMap::new(),
            retry_at: 0,
            protected,
            executable,
            game_tracking: game_tracking::GameTracking::default(),
            game_history,
            achievement_progress: BTreeMap::new(),
            achievement_history: None,
            achievements_due: 0,
            achievements_dirty: true,
            achievement_day: None,
        };
        engine.refresh_game_paths();
        Ok(engine)
    }

    fn refresh_achievements(&mut self, now: Timestamp) {
        self.achievements_due = now + 5;
        let date = achievements::day_of(now);
        if !self.achievements_dirty
            && self.achievement_day == Some(date)
            && self
                .data
                .active
                .as_ref()
                .is_none_or(|session| session.running.is_none())
            && self.game_tracking.live.is_empty()
        {
            return;
        }
        if self
            .achievement_history
            .as_ref()
            .is_none_or(|history| !history.is_valid(now))
        {
            self.achievement_history = Some(achievements::History::new(
                &self.data,
                self.game_history.iter(),
                now,
            ));
        }
        let progress = achievements::refresh(
            self.achievement_history.as_ref().unwrap(),
            &self.data,
            self.game_tracking.live.values(),
            now,
        );
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
        let mut next = self.data.metadata();
        change(&mut next)?;
        self.commit(next, None, &[])
    }

    fn change_action(&mut self, action: Action, now: Timestamp) -> CommandResult<()> {
        let mut next = self.data.metadata();
        let position = match &action {
            Action::ShortenSession { id, .. } => {
                let position = *self
                    .work
                    .positions
                    .get(id)
                    .ok_or("Завершённая сессия не найдена.")?;
                next.history.push(self.data.history[position].clone());
                position
            }
            _ => self.data.history.len(),
        };
        let working = matches!(
            &action,
            Action::Start { .. }
                | Action::Pause
                | Action::Resume
                | Action::ToggleTimer
                | Action::Finish { .. }
                | Action::ShortenSession { .. }
                | Action::SaveSettings { .. }
        );
        let archives = !matches!(
            &action,
            Action::Start { .. }
                | Action::Pause
                | Action::Resume
                | Action::ToggleTimer
                | Action::Finish { .. }
                | Action::EditActivity { .. }
                | Action::ArchiveActivity { .. }
                | Action::SetPriorityGoals { .. }
        );
        let finished = matches!(&action, Action::Finish { .. });
        let games_changed = matches!(
            &action,
            Action::AddGame { .. } | Action::RemoveGame { .. } | Action::ExcludeGame { .. }
        );
        actions::apply(&mut next, action, now)?;
        let changed = next.history.pop().map(|session| (position, session));
        self.commit(next, changed, &[])?;
        if games_changed {
            self.refresh_game_paths();
        }
        self.achievements_dirty = true;
        if archives {
            self.achievement_history = None;
        } else if finished {
            if let Some(history) = &mut self.achievement_history {
                history.append_work(&self.data, self.data.history.last().unwrap(), now);
            }
        }
        if working {
            self.work_revision += 1;
        }
        Ok(())
    }

    fn persist(
        &mut self,
        metadata: &Data,
        focus: Option<(usize, &Session)>,
        games: &[GameSession],
    ) -> CommandResult<()> {
        let from = self
            .monitoring_dirty
            .unwrap_or(self.data.game_monitoring.len());
        self.storage.save(
            metadata,
            focus,
            games,
            (from, &self.data.game_monitoring[from..]),
        )?;
        self.monitoring_dirty = None;
        Ok(())
    }

    fn commit(
        &mut self,
        mut next: Data,
        focus: Option<(usize, Session)>,
        games: &[GameSession],
    ) -> CommandResult<()> {
        let rebuild = next.settings.first != self.data.settings.first;
        self.persist(
            &next,
            focus.as_ref().map(|(index, session)| (*index, session)),
            games,
        )?;
        next.history = std::mem::take(&mut self.data.history);
        next.game_monitoring = std::mem::take(&mut self.data.game_monitoring);
        if let Some((index, session)) = focus {
            if index < next.history.len() {
                let old = std::mem::replace(&mut next.history[index], session);
                if !rebuild {
                    self.work.remove(&next.settings, index, &old);
                }
            } else {
                next.history.push(session);
            }
            if !rebuild {
                self.work.add(&next.settings, index, &next.history[index]);
            }
        }
        if rebuild {
            self.work = work_history::WorkHistory::new(&next);
        }
        self.data = next;
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
            || lower == Path::new(&self.executable.to_string_lossy().to_lowercase())
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
        let locked = self.work.status(&self.data, now).locked();
        self.sync_locks_for_day(now, force, locked);
    }

    fn sync_locks_for_day(&mut self, now: Timestamp, force: bool, locked: bool) {
        if self.isolated || !self.data.settings.game_blocking || !locked {
            self.locks.clear();
            self.failures.clear();
            return;
        }
        if !force
            && (now < self.retry_at && now >= self.retry_at - 3
                || !self.locks.is_empty() && self.failures.is_empty())
        {
            return;
        }
        self.retry_at = now + 3;
        let targets = self
            .data
            .games
            .iter()
            .filter(|game| !game.excluded && game.can_block())
            .flat_map(Game::paths)
            .collect::<std::collections::HashSet<_>>();
        self.locks.retain(|path, _| targets.contains(path.as_str()));
        let previous_failures = std::mem::take(&mut self.failures);
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
                    if game_diagnostics() && previous_failures.get(path) != Some(&error) {
                        eprintln!("[Afterwork] {path}: {error}");
                    }
                    self.failures.insert(path.to_owned(), error);
                }
            }
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> String {
    format!("Ошибка хранилища Afterwork: {error}")
}

fn game_diagnostics() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    cfg!(debug_assertions)
        && *ENABLED
            .get_or_init(|| std::env::var("AFTERWORK_GAME_DIAGNOSTICS").as_deref() == Ok("1"))
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
    priority_goals: Vec<model::PriorityGoal>,
    activity_summaries: BTreeMap<u64, ActivitySummary>,
    games: Vec<Game>,
    active: Option<Session>,
    elapsed: i64,
    failures: HashMap<String, String>,
    blocked_count: usize,
    running_games: Vec<String>,
    game_revision: Option<u64>,
    game_earliest: Option<Timestamp>,
    work_revision: u64,
    game_icons: Option<HashMap<String, game_icons::IconStatus>>,
    awards: BTreeMap<String, Timestamp>,
    achievement_progress: Option<BTreeMap<String, achievements::Progress>>,
    report: Option<Report>,
}

#[derive(Serialize)]
struct Report {
    totals: Statistics,
    goals: BTreeMap<u64, usize>,
}

#[derive(Serialize)]
struct FocusPage {
    sessions: Vec<Session>,
    more: bool,
}

#[tauri::command]
fn focus_page(
    state: State<'_, Mutex<Engine>>,
    activity: Option<u64>,
    offset: u32,
    limit: u32,
) -> CommandResult<FocusPage> {
    let engine = state.lock().map_err(storage_error)?;
    let limit = limit.clamp(1, 100);
    let mut sessions = engine.storage.focus_page(activity, offset, limit + 1)?;
    let more = sessions.len() > limit as usize;
    sessions.truncate(limit as usize);
    Ok(FocusPage { sessions, more })
}

#[tauri::command]
fn game_report(
    state: State<'_, Mutex<Engine>>,
    filtered: Option<String>,
    ranges: Vec<[Timestamp; 2]>,
    offset: u32,
    limit: u32,
) -> CommandResult<game_history::GameReport> {
    let mut state = state.lock().map_err(storage_error)?;
    let Engine {
        game_history,
        game_tracking,
        ..
    } = &mut *state;
    let live = game_tracking.live.values().collect::<Vec<_>>();
    game_history.report(&live, filtered.as_deref(), &ranges, offset, limit)
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
    Ok(make_snapshot(
        &engine,
        &icons,
        now,
        report,
        games,
        achievements,
    ))
}

fn make_snapshot(
    engine: &Engine,
    icons: &game_icons::IconStore,
    now: Timestamp,
    report: bool,
    games: Option<bool>,
    achievements: Option<bool>,
) -> Snapshot {
    let data = &engine.data;
    let status = engine.work.status(data, now);
    Snapshot {
        date: status.day.to_string(),
        seconds: status.seconds,
        phase: status.phase,
        target: status.target,
        locked: data.settings.game_blocking && status.locked(),
        settings: data.settings.clone(),
        activities: data.activities.clone(),
        priority_goals: data.priority_goals.clone(),
        activity_summaries: engine.work.summaries(data, now),
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
                !game.excluded
                    && game.can_block()
                    && game.paths().all(|path| engine.locks.contains_key(path))
            })
            .count(),
        running_games: engine.game_tracking.live.keys().cloned().collect(),
        game_revision: (report || games.unwrap_or(false)).then_some(engine.game_tracking.revision),
        game_earliest: games
            .unwrap_or(false)
            .then(|| {
                engine
                    .game_history
                    .earliest(engine.game_tracking.live.values())
            })
            .flatten(),
        work_revision: engine.work_revision,
        game_icons: games.unwrap_or(false).then(|| icons.status(&data.games)),
        awards: data.awards.clone(),
        achievement_progress: achievements
            .unwrap_or(false)
            .then(|| engine.achievement_progress.clone()),
        report: report.then(|| Report {
            totals: engine.work.totals(data, now),
            goals: engine.work.goals.clone(),
        }),
    }
}

#[tauri::command]
async fn apply(app: AppHandle, action: Action) -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(move || apply_action(&app, action))
        .await
        .map_err(storage_error)?
}

fn apply_action(app: &AppHandle, action: Action) -> CommandResult<()> {
    let state = app.state::<Mutex<Engine>>();
    let mut engine = state.lock().map_err(storage_error)?;
    let now = model::now();
    if let Action::AddGame { path, .. } = action {
        let (target, icon) = game_icons::manual_target(Path::new(&path))?;
        let target = engine.game_path(&target)?;
        let shortcut = icon.as_ref().map(|_| path);
        let path = target
            .to_str()
            .ok_or("Имя файла содержит неподдерживаемые символы.")?;
        engine.change_action(
            Action::AddGame {
                path: path.into(),
                shortcut,
            },
            now,
        )?;
        // The lock for a newly added game can deny the icon extractor's read access.
        if let Some(png) = game_icons::manual_icon(
            &target,
            icon.as_ref().map(|(file, index)| (file.as_path(), *index)),
        ) {
            let _ = app.state::<game_icons::IconStore>().save(path, false, &png);
        }
    } else {
        engine.change_action(action, now)?;
    }
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
    if app.config().identifier == DEV_IDENTIFIER {
        return if enabled == Some(true) {
            Err("Автозапуск отключён в Afterwork Dev.".into())
        } else {
            Ok(false)
        };
    }
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
            let day = engine.work.status(&engine.data, now);
            engine.sync_locks_for_day(now, false, day.locked());
            if now >= engine.achievements_due || now < engine.achievements_due - 5 {
                engine.refresh_achievements(now);
            }
            let slot = day.reminder_slot(now);
            !engine.isolated
                && engine.data.settings.notifications
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--background")
                .build(),
        )
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let isolated = app.config().identifier == DEV_IDENTIFIER;
            app.manage(game_icons::IconStore::new(directory.join("game-icons-v1")));
            let mut engine = Engine::load(directory).map_err(std::io::Error::other)?;
            engine.isolated = isolated;
            engine.sync_locks(model::now(), true);
            let timer_state = engine
                .data
                .active
                .as_ref()
                .map(|session| session.running.is_some());
            app.manage(Mutex::new(engine));
            app.manage(updates::Updates::new(app.handle(), isolated));
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
                            if app.config().identifier == DEV_IDENTIFIER {
                                eprintln!("{error}");
                            } else {
                                let _ = app
                                    .notification()
                                    .builder()
                                    .title("Afterwork")
                                    .body(error)
                                    .show();
                            }
                        }
                    }
                    "quit" => {
                        if let Ok(mut engine) = app.state::<Mutex<Engine>>().lock() {
                            let _ = engine.checkpoint_games(model::now());
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
            if !isolated {
                let handle = app.handle().clone();
                std::thread::spawn(move || game_tracking::track(handle));
                let handle = app.handle().clone();
                std::thread::spawn(move || game_icons::worker(handle));
            }
            if !std::env::args_os().any(|arg| arg == "--background") {
                show(app.handle());
            }
            updates::start_check(app.handle());
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
            focus_page,
            game_report,
            apply,
            autostart,
            updates::update_status,
            updates::set_auto_update,
            updates::check_update,
            updates::download_update,
            updates::install_update,
            game_icons::choose_game,
            game_icons::game_icon,
            game_icons::set_game_icon
        ])
        .run(tauri::generate_context!())
        .expect("Не удалось запустить Afterwork");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serde_json::Value;

    #[test]
    fn ipc_matches_shared_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::load(dir.path().to_owned()).unwrap();
        let icons = game_icons::IconStore::new(dir.path().join("icons"));
        let date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        let now = model::boundary(date, 8 * 60);
        let snapshot = make_snapshot(&engine, &icons, now, true, Some(true), Some(true));
        let expected: Value =
            serde_json::from_str(include_str!("../../fixtures/ipc-snapshot.json")).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), expected);
        let statistics = make_snapshot(&engine, &icons, now, true, Some(false), None);
        assert!(statistics.game_revision.is_some());
        assert!(statistics.game_icons.is_none());

        let expected: Value =
            serde_json::from_str(include_str!("../../fixtures/ipc-actions.json")).unwrap();
        let actions: Vec<Action> = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(actions).unwrap(), expected);
    }

    #[test]
    fn legacy_history_survives_saving_a_session_across_day_start() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("afterwork.json"),
            include_str!("../../fixtures/legacy-afterwork.json"),
        )
        .unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        let rollover = model::boundary(date, 360);
        let mut engine = Engine::load(dir.path().to_owned()).unwrap();
        assert!(engine.data.auto_update);
        assert!(engine.data.settings.game_blocking);
        engine
            .update(|data| {
                data.auto_update = false;
                Ok(())
            })
            .unwrap();
        engine
            .change_action(Action::Start { activity: 7 }, rollover - 60)
            .unwrap();
        let mut reopened = Engine::load(dir.path().to_owned()).unwrap();
        assert!(!reopened.data.auto_update);
        assert_eq!(
            reopened.data.active.as_ref().unwrap().running,
            Some(rollover - 60)
        );
        reopened
            .change_action(
                Action::Finish {
                    completed_goals: Vec::new(),
                    note: "Новая сессия".into(),
                },
                rollover + 120,
            )
            .unwrap();
        let reopened = Engine::load(dir.path().to_owned()).unwrap();
        assert!(reopened.data.active.is_none());
        assert!(reopened.data.settings.game_blocking);
        assert_eq!(reopened.data.history[0].note, "Старая заметка");
        assert_eq!(reopened.data.history[1].note, "Новая сессия");
        let totals = reopened.data.statistics(rollover + 120);
        assert_eq!(totals.activities[&7], 260);
        assert_eq!(totals.days[&date.pred_opt().unwrap()], 60);
        assert_eq!(totals.days[&date], 120);
    }

    #[test]
    fn priorities_keep_order_and_follow_goal_lifecycle_after_legacy_import() {
        use model::{GoalRename, PriorityGoal};

        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("afterwork.json"),
            include_str!("../../fixtures/legacy-afterwork.json"),
        )
        .unwrap();
        let mut engine = Engine::load(dir.path().to_owned()).unwrap();
        assert!(engine.data.priority_goals.is_empty());
        assert!(engine.data.settings.show_priority_goals);
        let now = model::boundary(NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(), 360);
        let mut details = engine.data.activities[0].details.clone();
        details.goals_enabled = true;
        details.goals = ["Глава", "Конспект", "Задачи"].map(str::to_owned).to_vec();
        engine
            .change_action(
                Action::EditActivity {
                    id: 7,
                    details: details.clone(),
                    goal_renames: vec![],
                },
                now,
            )
            .unwrap();
        details.name = "Чтение".into();
        engine
            .change_action(Action::AddActivity { details }, now)
            .unwrap();
        let reading = engine.data.activities.last().unwrap().id;
        let goal = |activity, name: &str| PriorityGoal {
            activity,
            goal: name.into(),
        };
        let priorities = vec![
            goal(reading, "Глава"),
            goal(7, "Конспект"),
            goal(7, "Глава"),
        ];
        engine
            .change_action(
                Action::SetPriorityGoals {
                    goals: priorities.clone(),
                },
                now,
            )
            .unwrap();
        for invalid in [
            vec![priorities[0].clone(), priorities[0].clone()],
            vec![goal(7, "Несуществующая цель")],
            [priorities.as_slice(), &[goal(7, "Задачи")]].concat(),
        ] {
            assert!(engine
                .change_action(Action::SetPriorityGoals { goals: invalid }, now)
                .is_err());
        }
        let mut settings = engine.data.settings.clone();
        settings.show_priority_goals = false;
        engine
            .change_action(Action::SaveSettings { settings }, now)
            .unwrap();
        drop(engine);
        let mut engine = Engine::load(dir.path().to_owned()).unwrap();
        assert!(!engine.data.settings.show_priority_goals);
        assert_eq!(engine.data.priority_goals, priorities);
        assert_eq!(engine.data.history[0].note, "Старая заметка");

        let mut details = engine.data.activities[0].details.clone();
        details.goals[1] = "Подробный конспект".into();
        engine
            .change_action(
                Action::EditActivity {
                    id: 7,
                    details,
                    goal_renames: vec![GoalRename {
                        from: "Конспект".into(),
                        to: "Подробный конспект".into(),
                    }],
                },
                now,
            )
            .unwrap();
        engine
            .change_action(
                Action::ArchiveActivity {
                    id: reading,
                    archived: true,
                },
                now,
            )
            .unwrap();
        assert_eq!(
            engine.data.priority_goals,
            [goal(7, "Подробный конспект"), goal(7, "Глава")]
        );

        engine
            .change_action(Action::Start { activity: 7 }, now)
            .unwrap();
        engine
            .change_action(
                Action::Finish {
                    note: String::new(),
                    completed_goals: vec!["Глава".into()],
                },
                now + 60,
            )
            .unwrap();
        assert_eq!(
            engine.data.history.last().unwrap().completed_goals,
            ["Глава"]
        );
        assert_eq!(engine.data.priority_goals, [goal(7, "Подробный конспект")]);
        let mut details = engine.data.activities[0].details.clone();
        details.goals_enabled = false;
        engine
            .change_action(
                Action::EditActivity {
                    id: 7,
                    details,
                    goal_renames: vec![],
                },
                now + 60,
            )
            .unwrap();
        assert!(engine.data.priority_goals.is_empty());
    }

    #[test]
    fn large_history_keeps_snapshots_and_pages_bounded() {
        use std::time::Instant;

        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("afterwork.json");
        let date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        let end = model::boundary(date, 12 * 60);
        let mut data = Data::default();
        data.achievement_goals.insert(date, 5400);
        for id in 1..=100_000 {
            let started = end - (100_001 - id as i64) * 1800;
            data.history.push(Session {
                id,
                activity: 1,
                started,
                running: None,
                spans: vec![[started, started + 900]],
                note: "Я".repeat(100),
                finished: Some(started + 900),
                completed_goals: vec!["Цель".into()],
            });
            if id % 5 == 0 {
                data.game_sessions.push(GameSession {
                    id: id + 100_000,
                    game: "C:/Games/Example.exe".into(),
                    start: started,
                    end: started + 900,
                    launched: Some(started),
                });
            }
        }
        data.next_id = 200_001;
        serde_json::to_writer(
            std::io::BufWriter::new(File::create(&legacy).unwrap()),
            &data,
        )
        .unwrap();
        let start = Instant::now();
        let mut engine = Engine::load(dir.path().to_owned()).unwrap();
        let import = start.elapsed();
        let icons = game_icons::IconStore::new(dir.path().join("icons"));
        let start = Instant::now();
        let snapshot = make_snapshot(&engine, &icons, end, true, Some(false), None);
        let bytes = serde_json::to_vec(&snapshot).unwrap().len();
        let snapshot_time = start.elapsed();
        assert!(bytes < 100_000, "snapshot: {bytes} bytes");
        assert_eq!(snapshot.report.unwrap().goals[&1], 100_000);
        assert_eq!(snapshot.seconds, data.status(end).seconds);
        let page = engine.storage.focus_page(None, 8, 8).unwrap();
        assert_eq!(page.first().unwrap().id, 99_992);
        assert_eq!(
            engine.storage.focus_page(Some(1), 99_999, 8).unwrap()[0].id,
            1
        );
        let games = engine
            .game_history
            .report(&[], None, &[[end - 86400, end]], 0, 2)
            .unwrap();
        assert_eq!(games.sessions.len(), 2);
        assert!(games.more);
        assert_eq!(games.buckets, vec![9_000]);

        let start = Instant::now();
        engine
            .change_action(Action::ShortenSession { id: 1, seconds: 40 }, end)
            .unwrap();
        let write = start.elapsed();
        drop(engine);
        let start = Instant::now();
        let reopened = Engine::load(dir.path().to_owned()).unwrap();
        let reload = start.elapsed();
        assert_eq!(reopened.data.history.len(), 100_000);
        assert_eq!(reopened.data.history[0].elapsed(end), 40);
        assert_eq!(
            reopened.work.totals(&reopened.data, end).activities[&1],
            99_999 * 900 + 40
        );
        assert_eq!(reopened.game_history.iter().count(), 20_000);
        let backup: Data =
            serde_json::from_reader(std::io::BufReader::new(File::open(legacy).unwrap())).unwrap();
        assert_eq!(backup.history[0].elapsed(end), 900);
        eprintln!("100k focus/20k game: import={import:?}, reload={reload:?}, snapshot={snapshot_time:?} ({bytes} bytes), edit={write:?}");
    }
}
