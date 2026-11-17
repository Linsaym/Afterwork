use crate::{
    achievements, game_tracking,
    model::{Activity, ActivityDetails, Data, Game, Session, Settings, Timestamp},
    CommandResult,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Start {
        activity: u64,
    },
    Pause,
    Resume,
    ToggleTimer,
    Finish {
        note: String,
        #[serde(default, rename = "completedGoals")]
        completed_goals: Vec<String>,
    },
    AddActivity {
        details: ActivityDetails,
    },
    EditActivity {
        id: u64,
        details: ActivityDetails,
    },
    ArchiveActivity {
        id: u64,
        archived: bool,
    },
    AddGame {
        path: String,
    },
    RemoveGame {
        path: String,
    },
    ExcludeGame {
        path: String,
        excluded: bool,
    },
    SaveSettings {
        settings: Settings,
    },
}

pub fn apply(data: &mut Data, action: Action, now: Timestamp) -> CommandResult<()> {
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
                completed_goals: Vec::new(),
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
        Action::Finish {
            note,
            completed_goals,
        } => {
            if note.chars().count() > 2000 {
                return Err("Заметка слишком длинная.".into());
            }
            let active = data.active.as_ref().ok_or("Текущая сессия отсутствует.")?;
            let activity = data
                .activities
                .iter_mut()
                .find(|a| a.id == active.activity)
                .ok_or("Занятие отсутствует.")?;
            let unique: std::collections::BTreeSet<_> = completed_goals.iter().collect();
            if unique.len() != completed_goals.len()
                || completed_goals
                    .iter()
                    .any(|g| !activity.details.goals_enabled || !activity.details.goals.contains(g))
            {
                return Err("Список целей изменился. Выберите цели заново.".into());
            }
            activity
                .details
                .goals
                .retain(|g| !completed_goals.contains(g));
            let mut session = data.active.take().ok_or("Текущая сессия отсутствует.")?;
            session.pause(now);
            session.note = note.trim().into();
            session.finished = Some(now);
            session.completed_goals = completed_goals;
            data.history.push(session);
        }
        Action::AddActivity { details } => {
            let details = data.validate_activity(details, None)?;
            let id = data.id();
            data.activities.push(Activity {
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
}

#[cfg(test)]
mod goal_tests {
    use super::*;
    #[test]
    fn goals_preserve_history_and_legacy_roundtrip() {
        let mut data: Data =
            serde_json::from_str(include_str!("../../fixtures/legacy-afterwork.json")).unwrap();
        let id = data.activities[0].id;
        let mut details = data.activities[0].details.clone();
        assert!(!details.goals_enabled);
        details.goals_enabled = true;
        details.goals = vec!["First".into(), "Second".into(), "Later".into()];
        apply(&mut data, Action::EditActivity { id, details }, 100).unwrap();
        data.active = None;
        apply(&mut data, Action::Start { activity: id }, 100).unwrap();
        assert!(apply(
            &mut data,
            Action::Finish {
                note: String::new(),
                completed_goals: vec!["Missing".into()]
            },
            110
        )
        .is_err());
        assert!(data.active.is_some());
        apply(
            &mut data,
            Action::Finish {
                note: String::new(),
                completed_goals: vec!["First".into(), "Second".into()],
            },
            120,
        )
        .unwrap();
        assert_eq!(data.activities[0].details.goals, vec!["Later"]);
        let mut details = data.activities[0].details.clone();
        details.goals.clear();
        details.goals_enabled = false;
        apply(&mut data, Action::EditActivity { id, details }, 130).unwrap();
        let dir = std::env::temp_dir().join(format!("afterwork-goals-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let restored: Data = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            restored.history.last().unwrap().completed_goals,
            vec!["First", "Second"]
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
