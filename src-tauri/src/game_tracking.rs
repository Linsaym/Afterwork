use crate::{
    game_catalog::DetectedGame,
    game_detector::{registered_path, Detector, Sample},
    model::{self, Game, GameSession, Timestamp},
    Engine,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct GameTracking {
    pub live: HashMap<String, GameSession>,
    finished: Vec<GameSession>,
    last_poll: Option<Timestamp>,
    last_observed: Option<Timestamp>,
    checkpoint_at: Timestamp,
    pub revision: u64,
    registered: Arc<HashSet<String>>,
    paths: HashMap<String, String>,
}

fn enroll(games: &mut Vec<Game>, entry: DetectedGame, path: String) -> bool {
    let target = registered_path(&path);
    let existing = games
        .iter()
        .position(|game| game.paths().any(|path| registered_path(path) == target))
        .or_else(|| games.iter().position(|game| game.catalog_id == entry.id));
    if let Some(index) = existing {
        let game = &mut games[index];
        let mut changed = false;
        if game.catalog_id.is_empty() {
            game.catalog_id = entry.id;
            game.source = entry.source.into();
            game.name = entry.name;
            changed = true;
        }
        if !game.paths().any(|path| registered_path(path) == target) {
            game.aliases.push(path);
            changed = true;
        }
        changed
    } else {
        games.push(Game {
            name: entry.name,
            path,
            source: entry.source.into(),
            catalog_id: entry.id,
            aliases: Vec::new(),
            excluded: false,
            shortcut: None,
        });
        true
    }
}

impl Engine {
    pub fn refresh_game_paths(&mut self) {
        self.game_tracking.registered = Arc::new(
            self.data
                .games
                .iter()
                .flat_map(Game::paths)
                .map(registered_path)
                .collect(),
        );
        self.game_tracking.paths = self
            .data
            .games
            .iter()
            .filter(|game| !game.excluded)
            .flat_map(|game| {
                game.paths()
                    .map(move |path| (registered_path(path), game.path.clone()))
            })
            .collect();
    }

    pub fn observe_games(&mut self, sample: Sample, now: Timestamp) {
        let mut games = None;
        let mut changed = false;
        for entry in sample.discovered {
            let path = match self.game_path(&entry.path) {
                Ok(path) => path,
                Err(error) => {
                    if crate::game_diagnostics() {
                        eprintln!("[Afterwork] {}: {error}", entry.path.display());
                    }
                    continue;
                }
            };
            let Some(path) = path.to_str() else { continue };
            let source = entry.source;
            if enroll(
                games.get_or_insert_with(|| self.data.games.clone()),
                entry,
                path.to_owned(),
            ) {
                changed = true;
                if crate::game_diagnostics() {
                    eprintln!("[Afterwork] {path}: распознано, источник — {source}");
                }
            }
        }
        if changed
            && self
                .update(|data| {
                    data.games = games.unwrap();
                    Ok(())
                })
                .is_ok()
        {
            self.achievement_history = None;
            self.achievements_dirty = true;
            self.refresh_game_paths();
            self.sync_locks(now, true);
        }

        let paths = &self.game_tracking.paths;
        let mut running = HashMap::<String, (Timestamp, bool)>::new();
        for (path, created, discovered) in sample.running {
            if let Some(game) = paths.get(&registered_path(&path.to_string_lossy())) {
                let first = running.entry(game.clone()).or_insert((created, discovered));
                if created < first.0 {
                    *first = (created, discovered);
                }
            }
        }
        let mut exits = HashMap::<String, Timestamp>::new();
        for (path, stopped) in sample.exited {
            if let Some(game) = paths.get(&registered_path(&path.to_string_lossy())) {
                let last = exits.entry(game.clone()).or_insert(stopped);
                *last = (*last).max(stopped);
            }
        }

        let tracker = &mut self.game_tracking;
        if sample.observed {
            let joined = tracker
                .last_observed
                .is_some_and(|last| now >= last && now - last <= 6);
            if let Some(span) = self.data.game_monitoring.last_mut().filter(|_| joined) {
                span[1] = now;
            } else {
                self.data.game_monitoring.push([now, now]);
            }
            let position = self.data.game_monitoring.len() - 1;
            self.monitoring_dirty = Some(
                self.monitoring_dirty
                    .map_or(position, |dirty| dirty.min(position)),
            );
        }
        let observation_changed = sample.observed != tracker.last_observed.is_some();
        tracker.last_observed = sample.observed.then_some(now);
        let continuous = tracker
            .last_poll
            .filter(|last| now >= *last && now - *last <= 12);
        let mut transitioned = false;
        let observed_changed = !tracker.live.is_empty();
        tracker.live.retain(|game, session| {
            if continuous.is_some()
                && running.get(game).is_some_and(|(created, _)| {
                    *created <= session.end || exits.get(game).is_some_and(|stop| created <= stop)
                })
            {
                session.end = now.max(session.end);
                return true;
            }
            if let Some(end) = continuous.and_then(|_| exits.get(game)) {
                session.end = (*end).clamp(session.end, now.max(session.end));
            }
            tracker.finished.push(session.clone());
            if session.end > session.start || session.launched.is_some() {
                self.game_history.push(session.clone());
                if let Some(history) = &mut self.achievement_history {
                    history.append_game(&self.data, session, now);
                }
            }
            transitioned = true;
            false
        });

        // The path and start time of a new segment are unique within this tracker.
        // Keep allocating IDs from the same persisted counter as focus sessions.
        for (game, (created, discovered)) in running {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                tracker.live.entry(game.clone())
            {
                let launched = (discovered
                    && sample.observed
                    && self
                        .data
                        .game_monitoring
                        .last()
                        .is_some_and(|[start, end]| *start <= created && created <= *end))
                .then_some(created);
                let start =
                    launched.unwrap_or_else(|| created.clamp(continuous.unwrap_or(now), now));
                let id = self.data.id();
                entry.insert(GameSession {
                    id,
                    game,
                    start,
                    end: now,
                    launched,
                });
                transitioned = true;
            }
        }
        tracker.last_poll = Some(now);
        if observed_changed || transitioned {
            tracker.revision += 1;
        }
        self.achievements_dirty |= transitioned || observation_changed;
        if transitioned
            || (sample.observed || !tracker.live.is_empty())
                && (now >= tracker.checkpoint_at || now < tracker.checkpoint_at - 60)
            || !tracker.finished.is_empty() && now >= tracker.checkpoint_at
        {
            let _ = self.checkpoint_games(now);
        }
    }

    pub fn close_untracked_games(&mut self, now: Timestamp) {
        let tracked: HashSet<&str> = self
            .data
            .games
            .iter()
            .filter(|game| !game.excluded)
            .map(|game| game.path.as_str())
            .collect();
        let tracker = &mut self.game_tracking;
        tracker.live.retain(|path, session| {
            if tracked.contains(path.as_str()) {
                return true;
            }
            tracker.finished.push(session.clone());
            if session.end > session.start || session.launched.is_some() {
                self.game_history.push(session.clone());
                if let Some(history) = &mut self.achievement_history {
                    history.append_game(&self.data, session, now);
                }
            }
            tracker.revision += 1;
            false
        });
        if !tracker.finished.is_empty() {
            let _ = self.checkpoint_games(now);
        }
    }

    pub fn checkpoint_games(&mut self, now: Timestamp) -> crate::CommandResult<()> {
        let tracker = &self.game_tracking;
        if tracker.live.is_empty() && tracker.finished.is_empty() && self.monitoring_dirty.is_none()
        {
            return Ok(());
        }
        let sessions: Vec<_> = tracker
            .finished
            .iter()
            .chain(tracker.live.values())
            .filter(|session| session.end > session.start || session.launched.is_some())
            .cloned()
            .collect();
        let metadata = self.data.metadata();
        let result = self.persist(&metadata, None, &sessions);
        if result.is_ok() {
            self.achievements_dirty = true;
            self.game_tracking.finished.clear();
        }
        self.game_tracking.checkpoint_at = now
            + if result.is_err() {
                10
            } else if self.game_tracking.live.is_empty() {
                60
            } else {
                20
            };
        result
    }
}

pub fn track(app: AppHandle) {
    let mut detector = Detector::default();
    loop {
        let state = app.state::<Mutex<Engine>>();
        let registered = match state.lock() {
            Ok(engine) => Arc::clone(&engine.game_tracking.registered),
            Err(_) => return,
        };
        let sample = detector.sample(model::now(), &registered);
        match state.lock() {
            Ok(mut engine) => engine.observe_games(sample, model::now()),
            Err(_) => return,
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

pub fn contains_path(game: &Game, path: &Path) -> bool {
    let target = registered_path(&path.to_string_lossy());
    game.paths().any(|value| registered_path(value) == target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::{Command, Stdio},
    };

    #[test]
    fn enrollment_blocks_new_launches_and_exclusion_keeps_history() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::load(directory.path().join("data")).unwrap();
        let source = std::env::var_os("SystemRoot").unwrap();
        let exe = directory.path().join("GameProbe.exe");
        fs::copy(Path::new(&source).join("System32/ping.exe"), &exe).unwrap();
        // The detector reports canonical paths, including when TEMP is a junction.
        let exe = fs::canonicalize(exe).unwrap();
        let command = |count: &str| {
            let mut cmd = Command::new(&exe);
            cmd.args(["127.0.0.1", "-n", count])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            cmd
        };
        let mut running = command("3").spawn().unwrap();
        let now = model::now();
        engine.observe_games(
            Sample {
                discovered: vec![DetectedGame {
                    id: "epic:fixture".into(),
                    name: "Game Probe".into(),
                    source: "Epic Games",
                    path: exe.clone(),
                }],
                running: vec![(exe.clone(), now, true)],
                exited: Vec::new(),
                observed: true,
            },
            now,
        );
        assert_eq!(engine.data.games.len(), 1);
        assert_eq!(engine.locks.len(), 1);
        assert_eq!(engine.game_tracking.live.len(), 1);
        assert!(running.try_wait().unwrap().is_none());
        assert!(command("1").spawn().is_err());
        engine.observe_games(
            Sample {
                discovered: Vec::new(),
                running: Vec::new(),
                exited: vec![(exe.clone(), now + 2)],
                observed: true,
            },
            now + 3,
        );
        assert_eq!(engine.game_history.iter().count(), 1);
        let session = engine.game_history.iter().next().unwrap();
        assert_eq!(session.end - session.start, 2);
        engine
            .update(|data| {
                data.games[0].excluded = true;
                Ok(())
            })
            .unwrap();
        engine.close_untracked_games(now + 3);
        engine.sync_locks(now + 3, true);
        assert!(command("1").status().unwrap().success());
        assert!(running.wait().unwrap().success());
        let reopened = Engine::load(directory.path().join("data")).unwrap();
        assert!(reopened.data.games[0].excluded);
        assert_eq!(reopened.game_history.iter().count(), 1);
    }
}
