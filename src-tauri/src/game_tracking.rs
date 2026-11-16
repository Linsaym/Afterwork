use crate::{
    game_catalog::DetectedGame,
    game_detector::{registered_path, Detector, Sample},
    model::{self, Game, GameSession, Timestamp},
    Engine,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
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
        });
        true
    }
}

impl Engine {
    pub fn registered_games(&self) -> HashSet<String> {
        self.data
            .games
            .iter()
            .flat_map(Game::paths)
            .map(registered_path)
            .collect()
    }

    pub fn observe_games(&mut self, sample: Sample, now: Timestamp) {
        let mut games = self.data.games.clone();
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
            if enroll(&mut games, entry, path.to_owned()) {
                changed = true;
                if crate::game_diagnostics() {
                    eprintln!("[Afterwork] {path}: распознано, источник — {source}");
                }
            }
        }
        if changed
            && self
                .update(|data| {
                    data.games = games;
                    Ok(())
                })
                .is_ok()
        {
            self.sync_locks(now, true);
        }

        let paths: HashMap<_, _> = self
            .data
            .games
            .iter()
            .filter(|game| !game.excluded)
            .flat_map(|game| {
                game.paths()
                    .map(move |path| (registered_path(path), game.path.clone()))
            })
            .collect();
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
        }
        tracker.last_observed = sample.observed.then_some(now);
        let continuous = tracker
            .last_poll
            .filter(|last| now >= *last && now - *last <= 12);
        let mut transitioned = false;
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
        if transitioned
            || !tracker.finished.is_empty()
            || (sample.observed || !tracker.live.is_empty())
                && (now >= tracker.checkpoint_at || now < tracker.checkpoint_at - 60)
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
            false
        });
        if !tracker.finished.is_empty() {
            let _ = self.checkpoint_games(now);
        }
    }

    pub fn checkpoint_games(&mut self, now: Timestamp) -> crate::CommandResult<()> {
        let tracker = &self.game_tracking;
        if tracker.live.is_empty()
            && tracker.finished.is_empty()
            && self.data.game_monitoring.is_empty()
        {
            return Ok(());
        }
        let sessions = tracker
            .finished
            .iter()
            .chain(tracker.live.values())
            .cloned()
            .collect::<Vec<_>>();
        self.update(|data| {
            merge_sessions(&mut data.game_sessions, sessions);
            Ok(())
        })?;
        self.game_tracking.finished.clear();
        self.game_tracking.checkpoint_at = now
            + if self.game_tracking.live.is_empty() {
                60
            } else {
                20
            };
        Ok(())
    }

    pub fn game_report(&self) -> Vec<GameSession> {
        let mut history = self.data.game_sessions.clone();
        merge_sessions(
            &mut history,
            self.game_tracking
                .finished
                .iter()
                .chain(self.game_tracking.live.values())
                .cloned(),
        );
        history
    }
}

fn merge_sessions(history: &mut Vec<GameSession>, sessions: impl IntoIterator<Item = GameSession>) {
    for session in sessions {
        if let Some(saved) = history
            .iter_mut()
            .rev()
            .find(|saved| saved.id == session.id)
        {
            *saved = session;
        } else if session.end > session.start || session.launched.is_some() {
            history.push(session);
        }
    }
}

pub fn track(app: AppHandle) {
    let mut detector = Detector::default();
    loop {
        let state = app.state::<Mutex<Engine>>();
        let registered = match state.lock() {
            Ok(engine) => engine.registered_games(),
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
        assert_eq!(engine.data.game_sessions.len(), 1);
        assert_eq!(
            engine.data.game_sessions[0].end - engine.data.game_sessions[0].start,
            2
        );
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
        assert_eq!(reopened.game_report().len(), 1);
    }
}
