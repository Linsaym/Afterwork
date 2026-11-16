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
