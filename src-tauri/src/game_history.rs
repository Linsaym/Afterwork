use crate::{
    model::{self, GameSession, Timestamp},
    CommandResult,
};
use serde::Serialize;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap},
};

type Ordered = (Timestamp, Reverse<usize>);

#[derive(Default)]
pub struct GameHistory {
    sessions: Vec<GameSession>,
    all: Series,
    games: HashMap<String, Series>,
    earliest: Option<Timestamp>,
    indexed: usize,
}

#[derive(Default)]
struct Series {
    rows: BTreeSet<Ordered>,
    events: BTreeMap<Timestamp, i64>,
    integrals: Vec<(Timestamp, i64, i64)>,
    dirty: bool,
    totals: GameTotals,
}

#[derive(Clone, Default, Serialize)]
pub struct GameTotals {
    count: usize,
    seconds: i64,
    latest: Timestamp,
    buckets: [i64; 4],
}

#[derive(Serialize)]
pub struct GameReport {
    pub totals: HashMap<String, GameTotals>,
    pub buckets: Vec<i64>,
    pub sessions: Vec<GameSession>,
    pub more: bool,
}

impl GameHistory {
    pub fn new(sessions: Vec<GameSession>) -> Self {
        let earliest = sessions.iter().map(|session| session.start).min();
        Self {
            sessions,
            earliest,
            ..Self::default()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &GameSession> {
        self.sessions.iter()
    }

    pub fn earliest<'a>(&self, live: impl Iterator<Item = &'a GameSession>) -> Option<Timestamp> {
        self.earliest
            .into_iter()
            .chain(live.map(|session| session.start))
            .min()
    }

    pub fn push(&mut self, session: GameSession) {
        self.earliest = Some(
            self.earliest
                .map_or(session.start, |old| old.min(session.start)),
        );
        self.sessions.push(session);
    }

    fn index_new_sessions(&mut self) {
        for (offset, session) in self.sessions[self.indexed..].iter().enumerate() {
            let position = self.indexed + offset;
            add_to_series(&mut self.all, session, position);
            let game = self.games.entry(session.game.clone()).or_default();
            add_to_series(game, session, position);
            game.totals.add(session);
        }
        self.indexed = self.sessions.len();
    }

    pub fn report(
        &mut self,
        live: &[&GameSession],
        filtered: Option<&str>,
        ranges: &[[Timestamp; 2]],
        offset: u32,
        limit: u32,
    ) -> CommandResult<GameReport> {
        if ranges.len() > 12 || ranges.iter().any(|[start, end]| start >= end) {
            return Err("Некорректный период игровой статистики.".into());
        }
        self.index_new_sessions();
        let mut totals: HashMap<_, _> = self
            .games
            .iter()
            .map(|(path, game)| (path.clone(), game.totals.clone()))
            .collect();
        for session in live {
            totals.entry(session.game.clone()).or_default().add(session);
        }
        let series = match filtered {
            Some(path) => self.games.get_mut(path),
            None => Some(&mut self.all),
        };
        let mut series = series;
        let mut buckets = vec![0; ranges.len()];
        let mut sessions = Vec::new();
        let mut more = false;
        if let (Some(first), Some(last)) = (ranges.first(), ranges.last()) {
            let live = live
                .iter()
                .copied()
                .filter(|item| filtered.is_none_or(|path| item.game == path));
            if let Some(series) = series.as_deref_mut() {
                series.prepare();
                for (bucket, [start, end]) in buckets.iter_mut().zip(ranges) {
                    *bucket = series.integral(*end) - series.integral(*start);
                }
            }
            for item in live.clone() {
                for (bucket, [start, end]) in buckets.iter_mut().zip(ranges) {
                    *bucket += overlap(item, *start, *end);
                }
            }
            let mut active: Vec<_> = live
                .filter(|item| overlap(item, first[0], last[1]) > 0)
                .collect();
            active.sort_by_key(|item| Reverse(item.end));
            let mut active = active.into_iter().peekable();
            let mut saved = series
                .iter()
                .flat_map(|series| series.rows.range((first[0], Reverse(0))..).rev())
                .map(|(_, Reverse(position))| &self.sessions[*position])
                .filter(|item| overlap(item, first[0], last[1]) > 0)
                .peekable();
            let ordered = std::iter::from_fn(|| {
                if active.peek().is_some_and(|current| {
                    saved.peek().is_none_or(|history| current.end > history.end)
                }) {
                    active.next()
                } else {
                    saved.next()
                }
            });
            let limit = limit.clamp(1, 100) as usize;
            sessions = ordered
                .skip(offset as usize)
                .take(limit + 1)
                .cloned()
                .collect();
            more = sessions.len() > limit;
            sessions.truncate(limit);
        }
        Ok(GameReport {
            totals,
            buckets,
            sessions,
            more,
        })
    }
}

fn overlap(session: &GameSession, start: Timestamp, end: Timestamp) -> i64 {
    (session.end.min(end) - session.start.max(start)).max(0)
}

fn add_to_series(series: &mut Series, session: &GameSession, position: usize) {
    if session.end <= session.start {
        return;
    }
    series.rows.insert((session.end, Reverse(position)));
    *series.events.entry(session.start).or_default() += 1;
    *series.events.entry(session.end).or_default() -= 1;
    series.dirty = true;
}

impl Series {
    fn prepare(&mut self) {
        if !self.dirty {
            return;
        }
        let (mut previous, mut count, mut total) = (0, 0, 0);
        self.integrals.clear();
        for (&time, &delta) in &self.events {
            total += (time - previous) * count;
            count += delta;
            self.integrals.push((time, count, total));
            previous = time;
        }
        self.dirty = false;
    }

    fn integral(&self, end: Timestamp) -> i64 {
        let position = self.integrals.partition_point(|row| row.0 <= end);
        if position == 0 {
            return 0;
        }
        let (time, count, total) = self.integrals[position - 1];
        total + (end - time) * count
    }
}

impl GameTotals {
    fn add(&mut self, session: &GameSession) {
        use chrono::Timelike;
        self.latest = self.latest.max(session.end);
        self.seconds += session.end - session.start;
        if session.end <= session.start {
            return;
        }
        self.count += 1;
        let mut cursor = session.start;
        while cursor < session.end {
            let local = model::local(cursor);
            let bucket = local.hour() / 6;
            let next = if bucket == 3 {
                model::boundary(local.date_naive().succ_opt().unwrap(), 0)
            } else {
                model::boundary(local.date_naive(), (bucket + 1) * 360)
            };
            if next <= cursor {
                break;
            }
            let stop = next.min(session.end);
            self.buckets[bucket as usize] += stop - cursor;
            cursor = stop;
        }
    }
}
