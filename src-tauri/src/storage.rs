use crate::{
    model::{Data, GameSession, Session, Timestamp},
    storage_error, CommandResult,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use std::{
    fs::{self, File},
    io::BufReader,
    path::Path,
};

pub struct Storage(Connection);

impl Storage {
    pub fn open(directory: &Path) -> CommandResult<(Self, Data)> {
        fs::create_dir_all(directory).map_err(storage_error)?;
        let mut db =
            Connection::open(directory.join("afterwork.sqlite3")).map_err(storage_error)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS profile (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS focus (position INTEGER PRIMARY KEY, id INTEGER NOT NULL, activity INTEGER NOT NULL,
                 completion INTEGER, data TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS focus_goals ON focus(activity, completion DESC, id DESC)
                 WHERE completion IS NOT NULL;
             CREATE TABLE IF NOT EXISTS games (position INTEGER PRIMARY KEY, id INTEGER UNIQUE NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS monitoring (position INTEGER PRIMARY KEY, start INTEGER NOT NULL, end INTEGER NOT NULL);"
        ).map_err(storage_error)?;
        let profile: Option<String> = db
            .query_row("SELECT data FROM profile WHERE id=1", [], |row| row.get(0))
            .optional()
            .map_err(storage_error)?;
        let data: Data = if let Some(profile) = profile {
            let mut data: Data = serde_json::from_str(&profile).map_err(storage_error)?;
            data.history = read_json_rows(&db, "SELECT data FROM focus ORDER BY position")?;
            data.game_sessions = read_json_rows(&db, "SELECT data FROM games ORDER BY position")?;
            let mut query = db
                .prepare("SELECT start,end FROM monitoring ORDER BY position")
                .map_err(storage_error)?;
            data.game_monitoring = query
                .query_map([], |row| Ok([row.get(0)?, row.get(1)?]))
                .map_err(storage_error)?
                .collect::<Result<_, _>>()
                .map_err(storage_error)?;
            data
        } else {
            let (mut data, legacy): (Data, bool) =
                match File::open(directory.join("afterwork.json")) {
                    Ok(file) => (
                        serde_json::from_reader(BufReader::new(file)).map_err(storage_error)?,
                        true,
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        (Data::default(), false)
                    }
                    Err(error) => return Err(storage_error(error)),
                };
            data.settings.validate()?;
            if legacy && data.achievement_goals.is_empty() {
                crate::achievements::preserve_legacy_awards(&mut data, crate::model::now());
            }
            if data.achievement_goals.is_empty() {
                data.achievement_goals.insert(
                    crate::achievements::day_of(crate::model::now()),
                    i64::from(data.settings.minutes) * 120,
                );
            }
            let tx = db.transaction().map_err(storage_error)?;
            for (position, session) in data.history.iter().enumerate() {
                write_focus(&tx, position, session)?;
            }
            write_games(&tx, &data.game_sessions)?;
            write_monitoring(&tx, 0, &data.game_monitoring)?;
            write_profile(&tx, &data.metadata())?;
            tx.commit().map_err(storage_error)?;
            data
        };
        data.settings.validate()?;
        // Keep the imported JSON intact as a backup. Subsequent launches use the committed database.
        Ok((Self(db), data))
    }

    pub fn save(
        &mut self,
        metadata: &Data,
        focus: Option<(usize, &Session)>,
        games: &[GameSession],
        monitoring: (usize, &[[Timestamp; 2]]),
    ) -> CommandResult<()> {
        let tx = self.0.transaction().map_err(storage_error)?;
        write_profile(&tx, metadata)?;
        if let Some((position, session)) = focus {
            write_focus(&tx, position, session)?;
        }
        write_games(&tx, games)?;
        write_monitoring(&tx, monitoring.0, monitoring.1)?;
        tx.commit().map_err(storage_error)
    }

    pub fn focus_page(
        &self,
        activity: Option<u64>,
        offset: u32,
        limit: u32,
    ) -> CommandResult<Vec<Session>> {
        let sql = if activity.is_some() {
            "SELECT data FROM focus WHERE activity=?3 AND completion IS NOT NULL ORDER BY completion DESC, id DESC LIMIT ?1 OFFSET ?2"
        } else {
            "SELECT data FROM focus WHERE ?3 IS NULL ORDER BY position DESC LIMIT ?1 OFFSET ?2"
        };
        let mut query = self.0.prepare_cached(sql).map_err(storage_error)?;
        let rows = query
            .query_map(params![limit, offset, activity.map(ordered_id)], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?;
        rows.map(|row| serde_json::from_str(&row.map_err(storage_error)?).map_err(storage_error))
            .collect()
    }
}

fn read_json_rows<T: DeserializeOwned>(db: &Connection, sql: &str) -> CommandResult<Vec<T>> {
    let mut query = db.prepare(sql).map_err(storage_error)?;
    let rows = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    rows.map(|row| serde_json::from_str(&row.map_err(storage_error)?).map_err(storage_error))
        .collect()
}

fn write_profile(tx: &Transaction<'_>, metadata: &Data) -> CommandResult<()> {
    let json = serde_json::to_string(metadata).map_err(storage_error)?;
    tx.execute("REPLACE INTO profile(id,data) VALUES (1,?1)", [json])
        .map_err(storage_error)?;
    Ok(())
}

fn write_focus(tx: &Transaction<'_>, position: usize, session: &Session) -> CommandResult<()> {
    let completion = (!session.completed_goals.is_empty()).then(|| session.completion_time());
    let json = serde_json::to_string(session).map_err(storage_error)?;
    tx.execute(
        "REPLACE INTO focus(position,activity,completion,id,data) VALUES (?1,?2,?3,?4,?5)",
        params![
            position as i64,
            ordered_id(session.activity),
            completion,
            ordered_id(session.id),
            json
        ],
    )
    .map_err(storage_error)?;
    Ok(())
}

fn write_games(tx: &Transaction<'_>, games: &[GameSession]) -> CommandResult<()> {
    let mut write = tx.prepare_cached("INSERT INTO games(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data").map_err(storage_error)?;
    for session in games {
        let json = serde_json::to_string(session).map_err(storage_error)?;
        write
            .execute(params![ordered_id(session.id), json])
            .map_err(storage_error)?;
    }
    Ok(())
}

fn write_monitoring(
    tx: &Transaction<'_>,
    position: usize,
    spans: &[[Timestamp; 2]],
) -> CommandResult<()> {
    let mut write = tx
        .prepare_cached("REPLACE INTO monitoring(position,start,end) VALUES (?1,?2,?3)")
        .map_err(storage_error)?;
    for (offset, [start, end]) in spans.iter().enumerate() {
        write
            .execute(params![(position + offset) as i64, start, end])
            .map_err(storage_error)?;
    }
    Ok(())
}

fn ordered_id(id: u64) -> i64 {
    (id ^ (1 << 63)) as i64
}
