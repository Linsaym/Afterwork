use crate::model::{ActivitySummary, Data, Day, Session, Settings, Statistics, Timestamp};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Default)]
pub struct WorkHistory {
    pub positions: HashMap<u64, usize>,
    pub goals: BTreeMap<u64, usize>,
    totals: Statistics,
    activities: BTreeMap<u64, ActivityHistory>,
    last_ends: BTreeSet<(Timestamp, usize)>,
}

#[derive(Default)]
struct ActivityHistory {
    seconds: i64,
    times: BTreeSet<(Timestamp, usize)>,
}

impl WorkHistory {
    pub fn new(data: &Data) -> Self {
        let mut history = Self::default();
        for (position, session) in data.history.iter().enumerate() {
            history.add(&data.settings, position, session);
        }
        history
    }

    pub fn add(&mut self, settings: &Settings, position: usize, session: &Session) {
        self.positions.entry(session.id).or_insert(position);
        if !session.completed_goals.is_empty() {
            *self.goals.entry(session.activity).or_default() += session.completed_goals.len();
        }
        self.totals.add(settings, session, session.started, 1);
        let activity = self.activities.entry(session.activity).or_default();
        activity.seconds += session.elapsed(session.started);
        activity
            .times
            .insert((last_active(session, session.started), position));
        if let Some(span) = session.spans.last() {
            self.last_ends.insert((span[1], position));
        }
    }

    pub fn remove(&mut self, settings: &Settings, position: usize, session: &Session) {
        if !session.completed_goals.is_empty() {
            *self.goals.entry(session.activity).or_default() -= session.completed_goals.len();
        }
        self.totals.add(settings, session, session.started, -1);
        let activity = self.activities.get_mut(&session.activity).unwrap();
        activity.seconds -= session.elapsed(session.started);
        activity
            .times
            .remove(&(last_active(session, session.started), position));
        if let Some(span) = session.spans.last() {
            self.last_ends.remove(&(span[1], position));
        }
    }

    pub fn status(&self, data: &Data, now: Timestamp) -> Day {
        if self.last_ends.last().is_some_and(|(end, _)| *end > now) {
            return data.day_status(now, data.sessions());
        }
        let mut status = data.day_status(now, data.active.iter());
        status.seconds += self
            .totals
            .days
            .get(&status.day)
            .copied()
            .unwrap_or_default();
        status
    }

    pub fn summaries(&self, data: &Data, now: Timestamp) -> BTreeMap<u64, ActivitySummary> {
        let mut summaries: BTreeMap<_, _> = self
            .activities
            .iter()
            .map(|(&id, item)| {
                (
                    id,
                    ActivitySummary {
                        seconds: item.seconds,
                        last_active: item.times.last().map_or(0, |(time, _)| (*time).max(0)),
                    },
                )
            })
            .collect();
        if let Some(session) = &data.active {
            let summary = summaries.entry(session.activity).or_default();
            summary.seconds += session.elapsed(now);
            summary.last_active = summary.last_active.max(last_active(session, now));
        }
        summaries
    }

    pub fn totals(&self, data: &Data, now: Timestamp) -> Statistics {
        let mut totals = self.totals.clone();
        if let Some(session) = &data.active {
            totals.add(&data.settings, session, now, 1);
        }
        totals
    }
}

fn last_active(session: &Session, now: Timestamp) -> Timestamp {
    session
        .intervals(now)
        .fold(session.started, |latest, span| latest.max(span[1]))
}
