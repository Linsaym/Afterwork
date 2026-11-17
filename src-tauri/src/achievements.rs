use crate::model::{boundary, local, Data, GameSession, Timestamp};
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
    use crate::model::Session;

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

#[derive(Default)]
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
    end > start
        && coverage
            .iter()
            .any(|span| span[0] <= start && span[1] >= end)
}

#[derive(Default)]
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

impl Facts {
    fn collect(data: &Data, games: &[GameSession], now: Timestamp) -> Self {
        let mut facts = Self {
            completed: data.history.len() as i64,
            created: data
                .activities
                .iter()
                .filter(|activity| activity.id > 3)
                .count() as i64,
            noted: data
                .history
                .iter()
                .filter(|session| !session.note.trim().is_empty())
                .count() as i64,
            midnight: data
                .history
                .iter()
                .filter(|session| session.finished.is_some_and(|time| local(time).hour() < 6))
                .count() as i64,
            ..Self::default()
        };
        let mut activities_with_goals = BTreeSet::new();
        for session in &data.history {
            let count = session.completed_goals.len() as i64;
            if count > 0 {
                facts.activity_goals += count;
                facts.best_goal_session = facts.best_goal_session.max(count);
                activities_with_goals.insert(session.activity);
            }
        }
        facts.activities_with_goals = activities_with_goals.len() as i64;
        let mut days = BTreeMap::<NaiveDate, Day>::new();
        let mut by_activity = BTreeMap::<u64, Vec<Span>>::new();
        for session in data.sessions() {
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
                by_activity
                    .entry(session.activity)
                    .or_default()
                    .push([start, end]);
                each_day([start, end], |date, span| {
                    days.entry(date).or_default().work.push(span)
                });
            }
            facts.best_session = facts.best_session.max(elapsed);
        }
        facts.distinct = by_activity.len() as i64;
        for spans in by_activity.into_values().map(merge) {
            facts.best_activity = facts.best_activity.max(seconds(&spans));
            for pair in spans.windows(2) {
                facts.activity_break = facts.activity_break.max(pair[1][0] - pair[0][1]);
            }
        }
        let game_paths: BTreeMap<_, _> = data
            .games
            .iter()
            .flat_map(|game| {
                let identity = if game.catalog_id.is_empty() {
                    &game.path
                } else {
                    &game.catalog_id
                };
                game.paths().map(move |path| (path, identity.as_str()))
            })
            .collect();
        let mut distinct_games = BTreeSet::new();
        for game in games {
            let end = game.end.min(now);
            let elapsed = (end - game.start).max(0);
            facts.longest_game_session = facts.longest_game_session.max(elapsed);
            if elapsed >= 10 * 60 {
                distinct_games.insert(
                    game_paths
                        .get(game.game.as_str())
                        .copied()
                        .unwrap_or(&game.game),
                );
            }
            each_day([game.start, end], |date, span| {
                let day = days.entry(date).or_default();
                day.play.push(span);
                day.observe_game(span[0], game.launched == Some(span[0]));
            });
            if let Some(launch) = game.launched.filter(|&time| time <= now) {
                days.entry(day_of(launch))
                    .or_default()
                    .observe_game(launch, true);
            }
        }
        facts.distinct_games = distinct_games.len() as i64;

        let coverage = merge(data.game_monitoring.clone());
        let mut previous_goal: Option<NaiveDate> = None;
        let mut streak = 0;
        for (date, day) in days {
            let work = merge(day.work);
            let play = merge(day.play);
            let total = seconds(&work);
            let played = seconds(&play);
            if played >= 10 * 60 {
                facts.game_days += 1;
            }
            let start = boundary(date, DAY_START);
            let end = boundary(date.succ_opt().unwrap(), DAY_START);
            let target = data
                .achievement_goals
                .range(..=date)
                .next_back()
                .map_or(NORMAL_TARGET, |(_, target)| *target)
                .max(60);
            facts.work += total;
            facts.best_day = facts.best_day.max(total);
            facts.extra = facts.extra.max(total.saturating_sub(target));
            if let Some(goal) = crossed(&work, target) {
                facts.goals += 1;
                let gap =
                    previous_goal.map(|previous| date.signed_duration_since(previous).num_days());
                if let Some(gap) = gap {
                    facts.goal_break = facts.goal_break.max(gap - 1);
                }
                streak = if gap == Some(1) { streak + 1 } else { 1 };
                facts.streak = facts.streak.max(streak);
                previous_goal = Some(date);
                if played > 0 {
                    facts.goal_and_play += 1;
                }
                if day.first_game.is_none_or(|(time, _)| time >= goal)
                    && observed(&coverage, start, goal)
                {
                    facts.work_before_play = 1;
                }
            }
            let cutoff = boundary(date, 16 * 60) - 1;
            let early_work = work
                .iter()
                .map(|[start, end]| (end.min(&cutoff) - start).max(0))
                .sum();
            facts.early = facts.early.max(early_work);

            if let Some((launch, true)) = day.first_game {
                if crossed(&work, target / 2).is_some_and(|time| time <= launch)
                    && observed(&coverage, start, launch)
                {
                    facts.first_before_game = 1;
                }
            }
            if now >= end && total > played && observed(&coverage, start, end) {
                facts.balanced = 1;
            }
        }
        facts
    }
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub current: i64,
    pub target: i64,
}

pub fn progress(data: &Data, games: &[GameSession], now: Timestamp) -> BTreeMap<String, Progress> {
    let facts = Facts::collect(data, games, now);
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
