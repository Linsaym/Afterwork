use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Timestamp = i64;
pub const MINECRAFT_JAVA: &str = "minecraft:java";

pub fn now() -> Timestamp {
    Utc::now().timestamp()
}

pub fn local(time: Timestamp) -> DateTime<Local> {
    Utc.timestamp_opt(time, 0)
        .single()
        .expect("valid timestamp")
        .with_timezone(&Local)
}

pub fn boundary(date: NaiveDate, minute: u32) -> Timestamp {
    let mut time = date.and_hms_opt(minute / 60, minute % 60, 0).unwrap();
    // A skipped local time (DST) takes effect at the next valid minute.
    loop {
        if let Some(value) = Local.from_local_datetime(&time).earliest() {
            return value.timestamp();
        }
        time += chrono::TimeDelta::minutes(1);
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub first: u32,
    pub second: u32,
    pub minutes: u32,
    pub notifications: bool,
    #[serde(default = "default_show_priority_goals")]
    pub show_priority_goals: bool,
}

fn default_show_priority_goals() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first: 360,
            second: 960,
            minutes: 45,
            notifications: true,
            show_priority_goals: default_show_priority_goals(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.first >= self.second || self.second >= 1440 || !(1..=240).contains(&self.minutes) {
            return Err(
                "Второй этап должен начинаться позже первого. Длительность — от 1 до 240 минут."
                    .into(),
            );
        }
        Ok(())
    }

    pub fn day(&self, time: Timestamp) -> NaiveDate {
        let date = local(time).date_naive();
        if time < boundary(date, self.first) {
            date.pred_opt().unwrap()
        } else {
            date
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: u64,
    #[serde(flatten)]
    pub details: ActivityDetails,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ActivityDetails {
    #[serde(default, rename = "goalsEnabled")]
    pub goals_enabled: bool,
    #[serde(default)]
    pub goals: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub icon: ActivityIcon,
    #[serde(default)]
    pub color: ActivityColor,
}

pub const PRIORITY_GOAL_LIMIT: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityGoal {
    pub activity: u64,
    pub goal: String,
}

#[derive(Serialize, Deserialize)]
pub struct GoalRename {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityIcon {
    #[default]
    Sparkles,
    Briefcase,
    Book,
    Code,
    Pencil,
    Palette,
    Music,
    Dumbbell,
    Languages,
    Brain,
    Target,
    Heart,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityColor {
    #[default]
    Violet,
    Blue,
    Cyan,
    Green,
    Amber,
    Orange,
    Rose,
    Slate,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub seconds: i64,
    pub last_active: Timestamp,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Game {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub catalog_id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub excluded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

impl Game {
    pub fn can_block(&self) -> bool {
        self.catalog_id != MINECRAFT_JAVA
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.path.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GameSession {
    pub id: u64,
    pub game: String,
    pub start: Timestamp,
    pub end: Timestamp,
    #[serde(default)]
    pub launched: Option<Timestamp>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default, rename = "completedGoals")]
    pub completed_goals: Vec<String>,
    pub id: u64,
    pub activity: u64,
    pub started: Timestamp,
    pub running: Option<Timestamp>,
    pub spans: Vec<[Timestamp; 2]>,
    pub note: String,
    #[serde(default)]
    pub finished: Option<Timestamp>,
}

impl Session {
    pub fn completion_time(&self) -> Timestamp {
        self.finished.unwrap_or_else(|| {
            self.spans
                .iter()
                .fold(self.started, |last, span| last.max(span[1]))
        })
    }

    pub fn intervals(&self, time: Timestamp) -> impl Iterator<Item = [Timestamp; 2]> + '_ {
        self.spans
            .iter()
            .copied()
            .chain(self.running.map(move |start| [start, time.max(start)]))
    }

    pub fn elapsed(&self, time: Timestamp) -> i64 {
        self.intervals(time).map(|[start, end]| end - start).sum()
    }

    pub fn pause(&mut self, time: Timestamp) {
        if let Some(start) = self.running.take() {
            if time > start {
                self.spans.push([start, time]);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Data {
    pub settings: Settings,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    pub activities: Vec<Activity>,
    #[serde(default)]
    pub priority_goals: Vec<PriorityGoal>,
    pub games: Vec<Game>,
    #[serde(default)]
    pub game_sessions: Vec<GameSession>,
    #[serde(default)]
    pub game_monitoring: Vec<[Timestamp; 2]>,
    #[serde(default)]
    pub achievement_goals: BTreeMap<NaiveDate, i64>,
    #[serde(default)]
    pub awards: BTreeMap<String, Timestamp>,
    pub active: Option<Session>,
    pub history: Vec<Session>,
    pub reminder: Option<Timestamp>,
    pub next_id: u64,
}

fn default_auto_update() -> bool {
    true
}

impl Default for Data {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            auto_update: default_auto_update(),
            activities: [
                ("Работа", ActivityIcon::Briefcase, ActivityColor::Violet),
                ("Учёба", ActivityIcon::Book, ActivityColor::Blue),
                ("Личный проект", ActivityIcon::Target, ActivityColor::Green),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (name, icon, color))| Activity {
                id: i as u64 + 1,
                details: ActivityDetails {
                    goals_enabled: false,
                    goals: Vec::new(),
                    name: name.into(),
                    icon,
                    color,
                },
                archived: false,
            })
            .collect(),
            priority_goals: Vec::new(),
            games: Vec::new(),
            game_sessions: Vec::new(),
            game_monitoring: Vec::new(),
            achievement_goals: BTreeMap::new(),
            awards: BTreeMap::new(),
            active: None,
            history: Vec::new(),
            reminder: None,
            next_id: 4,
        }
    }
}

impl Data {
    pub fn metadata(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            auto_update: self.auto_update,
            activities: self.activities.clone(),
            priority_goals: self.priority_goals.clone(),
            games: self.games.clone(),
            game_sessions: Vec::new(),
            game_monitoring: Vec::new(),
            achievement_goals: self.achievement_goals.clone(),
            awards: self.awards.clone(),
            active: self.active.clone(),
            history: Vec::new(),
            reminder: self.reminder,
            next_id: self.next_id,
        }
    }

    pub fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.history.iter().chain(self.active.iter())
    }

    pub fn activity_mut(&mut self, id: u64) -> Result<&mut Activity, String> {
        self.activities
            .iter_mut()
            .find(|activity| activity.id == id)
            .ok_or_else(|| "Занятие не найдено.".into())
    }

    pub fn has_active_goal(activities: &[Activity], priority: &PriorityGoal) -> bool {
        activities.iter().any(|activity| {
            activity.id == priority.activity
                && !activity.archived
                && activity.details.goals_enabled
                && activity.details.goals.contains(&priority.goal)
        })
    }

    pub fn retain_active_priorities(&mut self) {
        self.priority_goals
            .retain(|priority| Self::has_active_goal(&self.activities, priority));
    }

    pub fn validate_activity(
        &self,
        mut details: ActivityDetails,
        editing: Option<u64>,
    ) -> Result<ActivityDetails, String> {
        details.name = details.name.trim().to_owned();
        if details.name.is_empty() || details.name.chars().count() > 60 {
            return Err("Введите название длиной от 1 до 60 символов.".into());
        }
        for goal in &mut details.goals {
            *goal = goal.trim().to_owned();
        }
        if details.goals.len() > 100
            || details
                .goals
                .iter()
                .any(|g| g.is_empty() || g.chars().count() > 120)
        {
            return Err("Введите до 100 целей длиной от 1 до 120 символов.".into());
        }
        let unique: std::collections::BTreeSet<_> = details.goals.iter().collect();
        if unique.len() != details.goals.len() {
            return Err("Названия целей должны различаться.".into());
        }
        let lower = details.name.to_lowercase();
        if self
            .activities
            .iter()
            .any(|item| Some(item.id) != editing && item.details.name.to_lowercase() == lower)
        {
            return Err("Занятие с таким названием уже есть. Проверьте также архив.".into());
        }
        Ok(details)
    }

    #[cfg(test)]
    pub fn status(&self, time: Timestamp) -> Day {
        self.day_status(time, self.sessions())
    }

    pub fn day_status<'a>(
        &self,
        time: Timestamp,
        sessions: impl Iterator<Item = &'a Session>,
    ) -> Day {
        let day = self.settings.day(time);
        let start = boundary(day, self.settings.first);
        let phase = if time >= boundary(day, self.settings.second) {
            2
        } else {
            1
        };
        let seconds = sessions
            .filter(|session| {
                session.running.is_some()
                    || session.spans.last().is_some_and(|span| span[1] > start)
            })
            .flat_map(|session| session.intervals(time))
            .map(|[from, to]| (to.min(time) - from.max(start)).max(0))
            .sum();
        Day {
            day,
            start,
            phase,
            seconds,
            target: i64::from(self.settings.minutes) * 60,
        }
    }

    pub fn statistics(&self, time: Timestamp) -> Statistics {
        let mut totals = Statistics::default();
        for session in self.sessions() {
            totals.add(&self.settings, session, time, 1);
        }
        totals
    }

    #[cfg(test)]
    pub fn activity_summaries(&self, now: Timestamp) -> BTreeMap<u64, ActivitySummary> {
        crate::work_history::WorkHistory::new(self).summaries(self, now)
    }
}

pub struct Day {
    pub day: NaiveDate,
    pub start: Timestamp,
    pub phase: i64,
    pub seconds: i64,
    pub target: i64,
}

impl Day {
    pub fn locked(&self) -> bool {
        self.seconds < self.target * self.phase
    }

    pub fn reminder_slot(&self, now: Timestamp) -> Option<Timestamp> {
        let hours = (now - self.start) / 3600;
        (hours >= 1 && self.seconds < self.target * 2).then_some(self.start + hours * 3600)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Statistics {
    pub days: BTreeMap<NaiveDate, i64>,
    pub activities: BTreeMap<u64, i64>,
}

impl Statistics {
    pub fn add(&mut self, settings: &Settings, session: &Session, now: Timestamp, direction: i64) {
        for [start, end] in session.intervals(now).filter(|[start, end]| end > start) {
            adjust(
                &mut self.activities,
                session.activity,
                direction * (end - start),
            );
            let mut cursor = start;
            let mut day = settings.day(start);
            while cursor < end {
                let next = day.succ_opt().unwrap();
                let stop = end.min(boundary(next, settings.first));
                adjust(&mut self.days, day, direction * (stop - cursor));
                cursor = stop;
                day = next;
            }
        }
    }
}

fn adjust<K: Ord>(map: &mut BTreeMap<K, i64>, key: K, delta: i64) {
    use std::collections::btree_map::Entry;
    match map.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(delta);
        }
        Entry::Occupied(mut entry) => {
            *entry.get_mut() += delta;
            if *entry.get() == 0 {
                entry.remove();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_activity_loads_and_archive_preserves_session_summary() {
        let activity: Activity = serde_json::from_str(r#"{"id":7,"name":"Проект"}"#).unwrap();
        assert!(!activity.archived);
        assert!(matches!(activity.details.icon, ActivityIcon::Sparkles));
        assert!(matches!(activity.details.color, ActivityColor::Violet));
        let mut data = Data {
            activities: vec![activity],
            history: vec![Session {
                completed_goals: Vec::new(),
                id: 8,
                activity: 7,
                started: 100,
                running: None,
                spans: vec![[100, 160], [180, 200]],
                note: "Старая заметка".into(),
                finished: None,
            }],
            ..Data::default()
        };
        let activity = data.activity_mut(7).unwrap();
        activity.archived = true;
        activity.details.name = "Проект в архиве".into();
        let persisted: Data = serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(persisted.activities[0].archived);
        assert_eq!(persisted.history[0].note, "Старая заметка");
        let summary = &persisted.activity_summaries(240)[&7];
        assert_eq!(summary.seconds, 80);
        assert_eq!(summary.last_active, 200);
        assert_eq!(persisted.statistics(240).activities[&7], 80);
    }

    #[test]
    fn stages_and_advance_completion() {
        let mut data = Data::default();
        let date = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let morning = boundary(date, 360);
        let evening = boundary(date, 960);
        data.active = Some(Session {
            completed_goals: Vec::new(),
            id: 4,
            activity: 1,
            started: morning,
            running: None,
            spans: vec![[morning, morning + 2700]],
            note: String::new(),
            finished: None,
        });
        assert!(data.status(morning + 2699).locked());
        assert!(!data.status(morning + 2700).locked());
        assert!(data.status(evening).locked());
        let active = data.active.as_mut().unwrap();
        active.spans.push([morning + 3000, morning + 5700]);
        assert!(!data.status(evening).locked());
        assert_eq!(data.status(evening).reminder_slot(evening), None);
        assert!(data
            .status(boundary(date.succ_opt().unwrap(), 360))
            .locked());
    }

    #[test]
    fn running_session_is_split_at_rollover() {
        let mut data = Data::default();
        let date = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let start = boundary(date, 360);
        data.active = Some(Session {
            completed_goals: Vec::new(),
            id: 4,
            activity: 1,
            started: start - 1800,
            running: Some(start - 1800),
            spans: vec![],
            note: String::new(),
            finished: None,
        });
        assert_eq!(data.status(start + 900).seconds, 900);
        let stats = data.statistics(start + 900);
        assert_eq!(stats.days[&date.pred_opt().unwrap()], 1800);
        assert_eq!(stats.days[&date], 900);
        assert_eq!(stats.activities[&1], 2700);
    }
}
