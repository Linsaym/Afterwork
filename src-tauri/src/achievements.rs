use crate::model::{boundary, local, Data, GameSession, Session, Timestamp};
use chrono::{Days, NaiveDate, Timelike};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const DAY_START: u32 = 6 * 60;
const NORMAL_TARGET: i64 = 90 * 60;
const HOUR: i64 = 3600;
type Span = [Timestamp; 2];

pub fn day_of(time: Timestamp) -> NaiveDate {
    let date = local(time).date_naive();
    if time < boundary(date, DAY_START) {
        date.pred_opt().unwrap()
    } else {
        date
    }
}

pub fn preserve_legacy_awards(data: &mut Data, now: Timestamp) {
    let target = i64::from(data.settings.minutes) * 120;
    let completed = data
        .statistics(now)
        .days
        .into_values()
        .filter(|&seconds| seconds >= target)
        .count();
    for (id, earned) in [
        ("first-step", !data.history.is_empty()),
        ("balanced-day", completed >= 1),
        ("good-rhythm", completed >= 7),
    ] {
        if earned {
            data.awards.entry(id.into()).or_insert(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: u64, spans: Vec<Span>) -> Session {
        Session {
            completed_goals: Vec::new(),
            id,
            activity: 1,
            started: spans[0][0],
            running: None,
            spans,
            note: String::new(),
            finished: None,
        }
    }

    #[test]
    fn day_rolls_at_six_and_distinguishes_streaks_from_total_goals() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let day = |offset| boundary(date.checked_add_days(Days::new(offset)).unwrap(), DAY_START);
        let mut data = Data::default();
        for index in [0, 1, 2, 4, 5, 6, 7, 22] {
            let time = day(index);
            data.history
                .push(session(index, vec![[time, time + NORMAL_TARGET]]));
        }
        data.history
            .push(session(30, vec![[day(23) - 2700, day(23) + 2700]]));
        let stats = progress(&data, &[], day(24));
        assert_eq!(stats["good-rhythm"].current, 7);
        assert_eq!(stats["streak-7"].current, 4);
        assert_eq!(stats["hero-returns"].current, 14);
        assert_eq!(stats["new-habit"].current, 8);
        assert_eq!(stats["night-owl"].current, 0);
    }

    #[test]
    fn game_achievements_require_evidence_beyond_a_duration() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let start = boundary(date, DAY_START);
        let end = boundary(date.succ_opt().unwrap(), DAY_START);
        let work = session(5, vec![[start + 60, start + 60 + NORMAL_TARGET]]);
        let mut data = Data {
            history: vec![work],
            ..Data::default()
        };
        let mut game = GameSession {
            id: 6,
            game: "Game".into(),
            start: start + 6000,
            end: start + 9600,
            launched: None,
        };
        let past = progress(&data, std::slice::from_ref(&game), end);
        assert_eq!(past["mindful-player"].current, 1);
        for id in ["work-first", "perfect-balance", "games-can-wait"] {
            assert_eq!(past[id].current, 0, "{id}");
        }

        game.launched = Some(game.start);
        data.game_monitoring = vec![[start, start + 3000], [start + 3001, end]];
        assert_eq!(
            progress(&data, std::slice::from_ref(&game), end)["games-can-wait"].current,
            0
        );
        data.game_monitoring = vec![[start, end]];
        let confirmed = progress(&data, &[game], end);
        for id in ["work-first", "perfect-balance", "games-can-wait"] {
            assert_eq!(confirmed[id].current, 1, "{id}");
        }
    }

    #[test]
    fn pauses_keep_session_time_but_break_continuous_work() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let start = boundary(date, DAY_START);
        let mut work = session(
            5,
            vec![[start, start + HOUR], [start + 2 * HOUR, start + 3 * HOUR]],
        );
        work.note = "Сделано".into();
        let data = Data {
            history: vec![work],
            ..Data::default()
        };
        let stats = progress(&data, &[], start + 5 * HOUR);
        assert_eq!(stats["flow"].current, 90 * 60);
        assert_eq!(stats["deep-work"].current, HOUR);
        assert_eq!(stats["result"].current, 1);
    }

    #[test]
    fn awards_use_completed_activity_goals_and_recorded_play() {
        let mut focus = session(8, vec![[1000, 1450]]);
        focus.completed_goals = vec!["Outline".into(), "Draft".into(), "Review".into()];
        let mut data = Data {
            history: vec![focus.clone()],
            active: Some(focus),
            ..Data::default()
        };
        let first = GameSession {
            id: 9,
            game: "Game".into(),
            start: 1500,
            end: 2100,
            launched: None,
        };
        let mut second = first.clone();
        second.id = 10;
        second.game = "Other".into();
        second.end -= 1;
        data.activities[0].details.goals = vec!["Unfinished".into()];
        let stats = progress(&data, &[first, second], 2100);
        assert_eq!(stats["activity-goal-first"].current, 1);
        assert_eq!(stats["activity-goal-ten"].current, 3);
        assert_eq!(stats["activity-goal-triple"].current, 3);
        assert_eq!(stats["activity-goal-variety"].current, 1);
        assert_eq!(stats["game-first-minute"].current, 60);
        assert_eq!(stats["game-variety"].current, 1);
    }
}

#[derive(Clone, Default)]
struct Day {
    work: Vec<Span>,
    play: Vec<Span>,
    first_game: Option<(Timestamp, bool)>,
}

impl Day {
    fn observe_game(&mut self, time: Timestamp, is_launch: bool) {
        match self.first_game {
            None => self.first_game = Some((time, is_launch)),
            Some((old, _)) if time < old => self.first_game = Some((time, is_launch)),
            Some((old, previous)) if time == old => {
                self.first_game = Some((time, previous && is_launch))
            }
            _ => {}
        }
    }
}

fn each_day([start, end]: Span, mut visit: impl FnMut(NaiveDate, Span)) {
    if end <= start {
        return;
    }
    let mut date = day_of(start);
    let mut from = start;
    while from < end {
        let next = date.checked_add_days(Days::new(1)).unwrap();
        let to = end.min(boundary(next, DAY_START));
        if to > from {
            visit(date, [from, to]);
        }
        from = to;
        date = next;
    }
}

fn merge(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_unstable();
    let mut joined: Vec<Span> = Vec::new();
    for span in spans {
        if span[1] <= span[0] {
            continue;
        }
        if let Some(last) = joined.last_mut().filter(|last| span[0] <= last[1]) {
            last[1] = last[1].max(span[1]);
        } else {
            joined.push(span);
        }
    }
    joined
}

fn seconds(spans: &[Span]) -> i64 {
    spans.iter().map(|[start, end]| end - start).sum()
}

fn crossed(spans: &[Span], mut remaining: i64) -> Option<Timestamp> {
    for &[start, end] in spans {
        if remaining <= end - start {
            return Some(start + remaining);
        }
        remaining -= end - start;
    }
    None
}

fn observed(coverage: &[Span], start: Timestamp, end: Timestamp) -> bool {
    let position = coverage.partition_point(|span| span[0] <= start);
    end > start && position > 0 && coverage[position - 1][1] >= end
}

#[derive(Clone, Default)]
struct Facts {
    completed: i64,
    goals: i64,
    streak: i64,
    work: i64,
    best_session: i64,
    continuous: i64,
    best_day: i64,
    created: i64,
    distinct: i64,
    activity_break: i64,
    best_activity: i64,
    noted: i64,
    activity_goals: i64,
    best_goal_session: i64,
    activities_with_goals: i64,
    longest_game_session: i64,
    distinct_games: i64,
    game_days: i64,
    first_before_game: i64,
    early: i64,
    balanced: i64,
    goal_and_play: i64,
    work_before_play: i64,
    midnight: i64,
    morning: i64,
    extra: i64,
    goal_break: i64,
}

#[derive(Default)]
struct ActivitySpans {
    spans: Vec<Span>,
    seconds: i64,
    gap: i64,
}

impl ActivitySpans {
    fn new(spans: Vec<Span>) -> Self {
        let spans = merge(spans);
        let seconds = seconds(&spans);
        let gap = spans
            .windows(2)
            .map(|pair| pair[1][0] - pair[0][1])
            .max()
            .unwrap_or(0);
        Self {
            spans,
            seconds,
            gap,
        }
    }

    fn with_active(&self, active: Vec<Span>) -> (i64, i64) {
        let active = Self::new(active);
        let Some(first) = active.spans.first() else {
            return (self.seconds, self.gap);
        };
        let Some(last) = self.spans.last() else {
            return (active.seconds, active.gap);
        };
        if first[0] >= last[1] {
            return (
                self.seconds + active.seconds,
                self.gap.max(active.gap).max(first[0] - last[1]),
            );
        }
        let joined = Self::new(self.spans.iter().chain(&active.spans).copied().collect());
        (joined.seconds, joined.gap)
    }

    fn append(&mut self, spans: Vec<Span>) {
        let pending = Self::new(spans);
        let Some(first) = pending.spans.first() else {
            return;
        };
        let Some(last) = self.spans.last_mut() else {
            *self = pending;
            return;
        };
        if first[0] < last[1] {
            *self = Self::new(self.spans.iter().chain(&pending.spans).copied().collect());
            return;
        }
        self.seconds += pending.seconds;
        self.gap = self.gap.max(pending.gap).max(first[0] - last[1]);
        if first[0] == last[1] {
            last[1] = first[1];
            self.spans.extend(pending.spans.into_iter().skip(1));
        } else {
            self.spans.extend(pending.spans);
        }
    }
}

struct DayTotals {
    work: i64,
    play: i64,
    goal: Option<Timestamp>,
    half: Option<Timestamp>,
    early: i64,
    target: i64,
    first_game: Option<(Timestamp, bool)>,
    start: Timestamp,
    end: Timestamp,
}

impl DayTotals {
    fn new(data: &Data, date: NaiveDate, day: &Day) -> Self {
        let target = data
            .achievement_goals
            .range(..=date)
            .next_back()
            .map_or(NORMAL_TARGET, |(_, target)| *target)
            .max(60);
        let cutoff = boundary(date, 16 * 60) - 1;
        Self {
            work: seconds(&day.work),
            play: seconds(&day.play),
            goal: crossed(&day.work, target),
            half: crossed(&day.work, target / 2),
            early: day
                .work
                .iter()
                .map(|[start, end]| (end.min(&cutoff) - start).max(0))
                .sum(),
            target,
            first_game: day.first_game,
            start: boundary(date, DAY_START),
            end: boundary(date.succ_opt().unwrap(), DAY_START),
        }
    }
}

pub struct History {
    facts: Facts,
    activities: BTreeMap<u64, ActivitySpans>,
    days: BTreeMap<NaiveDate, (Day, DayTotals)>,
    game_paths: BTreeMap<String, String>,
    distinct_games: BTreeSet<String>,
    activities_with_goals: BTreeSet<u64>,
    valid_from: Timestamp,
    complete: bool,
}

impl History {
    pub fn new<'a>(
        data: &Data,
        games: impl Iterator<Item = &'a GameSession>,
        now: Timestamp,
    ) -> Self {
        let mut facts = Facts {
            completed: data.history.len() as i64,
            created: data
                .activities
                .iter()
                .filter(|activity| activity.id > 3)
                .count() as i64,
            ..Facts::default()
        };
        let mut days = BTreeMap::<NaiveDate, Day>::new();
        let mut activities = BTreeMap::<u64, Vec<Span>>::new();
        let mut activities_with_goals = BTreeSet::new();
        let mut valid_from = Timestamp::MIN;
        for session in &data.history {
            facts.noted += i64::from(!session.note.trim().is_empty());
            facts.midnight +=
                i64::from(session.finished.is_some_and(|time| local(time).hour() < 6));
            let count = session.completed_goals.len() as i64;
            if count > 0 {
                facts.activity_goals += count;
                facts.best_goal_session = facts.best_goal_session.max(count);
                activities_with_goals.insert(session.activity);
            }
            let spans = activities.entry(session.activity).or_default();
            record_work(
                &mut facts,
                session,
                now,
                |date, span| days.entry(date).or_default().work.push(span),
                spans,
            );
            valid_from = valid_from.max(
                session
                    .spans
                    .iter()
                    .map(|span| span[1])
                    .max()
                    .unwrap_or(Timestamp::MIN),
            );
        }
        facts.activities_with_goals = activities_with_goals.len() as i64;
        let activities = activities
            .into_iter()
            .filter(|(_, spans)| !spans.is_empty())
            .map(|(id, spans)| (id, ActivitySpans::new(spans)))
            .collect();
        let game_paths = data
            .games
            .iter()
            .flat_map(|game| {
                let identity = if game.catalog_id.is_empty() {
                    &game.path
                } else {
                    &game.catalog_id
                };
                game.paths()
                    .map(move |path| (path.to_owned(), identity.clone()))
            })
            .collect();
        let mut distinct_games = BTreeSet::new();
        for game in games {
            record_play(
                &mut facts,
                game,
                now,
                |date, span, start, launch| {
                    update_play(days.entry(date).or_default(), span, start, launch)
                },
                &game_paths,
                &mut distinct_games,
            );
            valid_from = valid_from
                .max(game.end)
                .max(game.launched.unwrap_or(Timestamp::MIN));
        }
        let days = days
            .into_iter()
            .map(|(date, day)| {
                let day = merged_day(day);
                let totals = DayTotals::new(data, date, &day);
                (date, (day, totals))
            })
            .collect();
        Self {
            facts,
            activities,
            days,
            game_paths,
            distinct_games,
            activities_with_goals,
            valid_from,
            complete: now >= valid_from,
        }
    }

    pub fn is_valid(&self, now: Timestamp) -> bool {
        self.complete && now >= self.valid_from
    }

    pub fn append_work(&mut self, data: &Data, session: &Session, now: Timestamp) {
        let mut changed = BTreeMap::new();
        let mut spans = Vec::new();
        let facts = &mut self.facts;
        facts.completed += 1;
        facts.noted += i64::from(!session.note.trim().is_empty());
        facts.midnight += i64::from(session.finished.is_some_and(|time| local(time).hour() < 6));
        let count = session.completed_goals.len() as i64;
        if count > 0 {
            facts.activity_goals += count;
            facts.best_goal_session = facts.best_goal_session.max(count);
            self.activities_with_goals.insert(session.activity);
            facts.activities_with_goals = self.activities_with_goals.len() as i64;
        }
        record_work(
            facts,
            session,
            now,
            |date, span| {
                changed
                    .entry(date)
                    .or_insert_with(|| {
                        self.days
                            .get(&date)
                            .map(|(day, _)| day.clone())
                            .unwrap_or_default()
                    })
                    .work
                    .push(span)
            },
            &mut spans,
        );
        if !spans.is_empty() {
            self.activities
                .entry(session.activity)
                .or_default()
                .append(spans);
        }
        self.update_days(data, changed);
        self.valid_from = self.valid_from.max(
            session
                .spans
                .iter()
                .map(|span| span[1])
                .max()
                .unwrap_or(Timestamp::MIN),
        );
        self.complete &= now >= self.valid_from;
    }

    pub fn append_game(&mut self, data: &Data, game: &GameSession, now: Timestamp) {
        let mut changed = BTreeMap::new();
        record_play(
            &mut self.facts,
            game,
            now,
            |date, span, start, launch| {
                let day = changed.entry(date).or_insert_with(|| {
                    self.days
                        .get(&date)
                        .map(|(day, _)| day.clone())
                        .unwrap_or_default()
                });
                update_play(day, span, start, launch);
            },
            &self.game_paths,
            &mut self.distinct_games,
        );
        self.update_days(data, changed);
        self.valid_from = self
            .valid_from
            .max(game.end)
            .max(game.launched.unwrap_or(Timestamp::MIN));
        self.complete &= now >= self.valid_from;
    }

    fn update_days(&mut self, data: &Data, changed: BTreeMap<NaiveDate, Day>) {
        for (date, day) in changed {
            let day = merged_day(day);
            let totals = DayTotals::new(data, date, &day);
            self.days.insert(date, (day, totals));
        }
    }

    fn changed_day<'a>(
        &self,
        changed: &'a mut BTreeMap<NaiveDate, Day>,
        date: NaiveDate,
    ) -> &'a mut Day {
        changed.entry(date).or_insert_with(|| {
            self.days
                .get(&date)
                .map(|(day, _)| day.clone())
                .unwrap_or_default()
        })
    }

    fn collect<'a>(
        &self,
        data: &Data,
        games: impl Iterator<Item = &'a GameSession>,
        now: Timestamp,
        coverage: &[Span],
    ) -> Facts {
        let mut facts = self.facts.clone();
        let mut changed = BTreeMap::<NaiveDate, Day>::new();
        let mut spans = Vec::new();
        let active = data.active.as_ref();
        if let Some(session) = active {
            record_work(
                &mut facts,
                session,
                now,
                |date, span| self.changed_day(&mut changed, date).work.push(span),
                &mut spans,
            );
        }
        let mut distinct_live = BTreeSet::new();
        for game in games {
            record_play(
                &mut facts,
                game,
                now,
                |date, span, start, launch| {
                    update_play(self.changed_day(&mut changed, date), span, start, launch)
                },
                &self.game_paths,
                &mut distinct_live,
            );
        }
        facts.distinct_games = (self.distinct_games.len()
            + distinct_live.difference(&self.distinct_games).count())
            as i64;
        facts.distinct = self.activities.len() as i64;
        for (id, stored) in &self.activities {
            if active.is_some_and(|session| session.activity == *id) {
                continue;
            }
            facts.best_activity = facts.best_activity.max(stored.seconds);
            facts.activity_break = facts.activity_break.max(stored.gap);
        }
        if let Some(session) = active {
            if let Some(stored) = self.activities.get(&session.activity) {
                let (seconds, gap) = stored.with_active(spans);
                facts.best_activity = facts.best_activity.max(seconds);
                facts.activity_break = facts.activity_break.max(gap);
            } else if !spans.is_empty() {
                let active = ActivitySpans::new(spans);
                facts.distinct += 1;
                facts.best_activity = facts.best_activity.max(active.seconds);
                facts.activity_break = facts.activity_break.max(active.gap);
            }
        }
        let mut previous_goal: Option<NaiveDate> = None;
        let mut streak = 0;
        let updated = changed
            .into_iter()
            .map(|(date, day)| (date, DayTotals::new(data, date, &merged_day(day))))
            .collect::<BTreeMap<_, _>>();
        let dates: BTreeSet<_> = self.days.keys().chain(updated.keys()).copied().collect();
        for date in dates {
            let day = updated.get(&date).unwrap_or_else(|| &self.days[&date].1);
            facts.game_days += i64::from(day.play >= 10 * 60);
            facts.work += day.work;
            facts.best_day = facts.best_day.max(day.work);
            facts.extra = facts.extra.max(day.work.saturating_sub(day.target));
            if let Some(goal) = day.goal {
                facts.goals += 1;
                let gap =
                    previous_goal.map(|previous| date.signed_duration_since(previous).num_days());
                if let Some(gap) = gap {
                    facts.goal_break = facts.goal_break.max(gap - 1);
                }
                streak = if gap == Some(1) { streak + 1 } else { 1 };
                facts.streak = facts.streak.max(streak);
                previous_goal = Some(date);
                facts.goal_and_play += i64::from(day.play > 0);
                if facts.work_before_play == 0
                    && day.first_game.is_none_or(|(time, _)| time >= goal)
                    && observed(coverage, day.start, goal)
                {
                    facts.work_before_play = 1;
                }
            }
            facts.early = facts.early.max(day.early);
            if let Some((launch, true)) = day.first_game {
                if facts.first_before_game == 0
                    && day.half.is_some_and(|time| time <= launch)
                    && observed(coverage, day.start, launch)
                {
                    facts.first_before_game = 1;
                }
            }
            if facts.balanced == 0
                && now >= day.end
                && day.work > day.play
                && observed(coverage, day.start, day.end)
            {
                facts.balanced = 1;
            }
        }
        facts
    }
}

fn merged_day(day: Day) -> Day {
    Day {
        work: merge(day.work),
        play: merge(day.play),
        first_game: day.first_game,
    }
}

fn record_work(
    facts: &mut Facts,
    session: &Session,
    now: Timestamp,
    mut day: impl FnMut(NaiveDate, Span),
    spans: &mut Vec<Span>,
) {
    if (6..8).contains(&local(session.started).hour()) {
        facts.morning = 1;
    }
    let mut elapsed = 0;
    for [start, end] in session.intervals(now) {
        let end = end.min(now);
        if end <= start {
            continue;
        }
        elapsed += end - start;
        facts.continuous = facts.continuous.max(end - start);
        spans.push([start, end]);
        each_day([start, end], &mut day);
    }
    facts.best_session = facts.best_session.max(elapsed);
}

fn record_play(
    facts: &mut Facts,
    game: &GameSession,
    now: Timestamp,
    mut visit: impl FnMut(NaiveDate, Option<Span>, Timestamp, bool),
    game_paths: &BTreeMap<String, String>,
    distinct: &mut BTreeSet<String>,
) {
    let end = game.end.min(now);
    let elapsed = (end - game.start).max(0);
    facts.longest_game_session = facts.longest_game_session.max(elapsed);
    if elapsed >= 10 * 60 {
        distinct.insert(game_paths.get(&game.game).unwrap_or(&game.game).clone());
    }
    each_day([game.start, end], |date, span| {
        visit(date, Some(span), span[0], game.launched == Some(span[0]))
    });
    if let Some(launch) = game.launched.filter(|&time| time <= now) {
        visit(day_of(launch), None, launch, true);
    }
}

fn update_play(day: &mut Day, span: Option<Span>, start: Timestamp, launch: bool) {
    if let Some(span) = span {
        day.play.push(span);
    }
    day.observe_game(start, launch);
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub current: i64,
    pub target: i64,
}

#[cfg(test)]
pub fn progress(data: &Data, games: &[GameSession], now: Timestamp) -> BTreeMap<String, Progress> {
    let history = History::new(data, games.iter(), now);
    refresh(&history, data, std::iter::empty(), now)
}

pub fn refresh<'a>(
    history: &History,
    data: &Data,
    games: impl Iterator<Item = &'a GameSession>,
    now: Timestamp,
) -> BTreeMap<String, Progress> {
    let coverage = merge(data.game_monitoring.clone());
    let facts = history.collect(data, games, now, &coverage);
    macro_rules! rules {
        ($($id:literal : $fact:ident >= $target:expr),* $(,)?) => {
            BTreeMap::from([$((String::from($id), Progress {
                current: facts.$fact.clamp(0, $target),
                target: $target,
            })),*])
        };
    }
    rules! {
        "first-step": completed >= 1,
        "warm-up": work >= 45 * 60,
        "balanced-day": goals >= 1,
        "right-track": goals >= 5,
        "good-rhythm": goals >= 7,
        "new-habit": goals >= 30,
        "streak-3": streak >= 3,
        "streak-7": streak >= 7,
        "streak-14": streak >= 14,
        "streak-30": streak >= 30,
        "streak-100": streak >= 100,
        "hours-1": work >= HOUR,
        "hours-10": work >= 10 * HOUR,
        "hours-50": work >= 50 * HOUR,
        "hours-100": work >= 100 * HOUR,
        "hours-500": work >= 500 * HOUR,
        "hours-1000": work >= 1000 * HOUR,
        "flow": best_session >= 90 * 60,
        "deep-work": continuous >= 2 * HOUR,
        "one-breath": continuous >= 3 * HOUR,
        "marathon": best_day >= 5 * HOUR,
        "unstoppable": best_day >= 8 * HOUR,
        "new-start": created >= 1,
        "multitask": distinct >= 3,
        "orchestra": distinct >= 10,
        "back-to-work": activity_break >= 7 * 24 * HOUR,
        "craft-master": best_activity >= 100 * HOUR,
        "result": noted >= 1,
        "activity-goal-first": activity_goals >= 1,
        "activity-goal-ten": activity_goals >= 10,
        "activity-goal-triple": best_goal_session >= 3,
        "activity-goal-variety": activities_with_goals >= 3,
        "game-first-minute": longest_game_session >= 60,
        "game-variety": distinct_games >= 3,
        "game-days": game_days >= 3,
        "work-first": first_before_game >= 1,
        "early-freedom": early >= NORMAL_TARGET,
        "perfect-balance": balanced >= 1,
        "mindful-player": goal_and_play >= 7,
        "games-can-wait": work_before_play >= 1,
        "night-owl": midnight >= 1,
        "early-bird": morning >= 1,
        "five-more": extra >= 30 * 60,
        "hero-returns": goal_break >= 14,
        "way-of-life": goals >= 365,
    }
}
