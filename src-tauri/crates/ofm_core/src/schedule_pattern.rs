//! Domestic league calendars that fall across the week the way a real league's do.
//!
//! The ordinary cadence puts every fixture of a round on one day, seven days after the last, so a
//! club keeps the weekday of the opening round all season: a Ligue 1 side that opens on a Friday
//! plays every Friday. A [`MatchdayPattern`] changes only *when* the existing pairings are played:
//!
//! * a round is **ordinary** (spread over the weekend days the pattern weights, a club's weekday
//!   varying from round to round) or **midweek** (a Wednesday, say);
//! * midweek rounds are added only as the season needs them: the pattern's minimum, and as many more
//!   as the season's window lacks weeks for (a 46-round division in a 40-week window);
//! * the minimum rest between two rounds is guaranteed by construction: a round's earliest day is
//!   never less than [`MIN_REST_DAYS`] after the previous round's latest, so no club, whatever
//!   days it draws, plays twice inside the rest window.
//!
//! The dates are generated. They follow the shape of a football week, not any official fixture list.

use chrono::{Datelike, Duration, NaiveDate};
use domain::competition_calendar::{CalendarDate, MatchdayPattern};
use std::collections::BTreeSet;

/// The rest the world calendar demands between two fixtures of one club
/// (`calendar_booking::MIN_GAP_DAYS`); the plan keeps rounds this far apart.
pub(crate) const MIN_REST_DAYS: i64 = crate::calendar_booking::MIN_GAP_DAYS;

/// The date a season's window ends on: the first `end` on or after the opener (an end before the
/// opener's month and day belongs to the following year).
pub(crate) fn window_end_date(start: NaiveDate, end: CalendarDate) -> Option<NaiveDate> {
    // 29 February in a year that has none is 28 February, as the opener's own date is clamped.
    let in_year = |year: i32| {
        NaiveDate::from_ymd_opt(year, end.month.into(), end.day.into()).or_else(|| {
            (end.month == 2 && end.day == 29)
                .then(|| NaiveDate::from_ymd_opt(year, 2, 28))
                .flatten()
        })
    };
    in_year(start.year())
        .filter(|d| *d >= start)
        .or_else(|| in_year(start.year() + 1))
}

/// Where one round may fall: the days it may use and their weights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoundPlan {
    pub midweek: bool,
    pub days: Vec<(NaiveDate, u8)>,
}

/// The matchdays (1-based) played midweek: spread evenly, never among the first two or last two
/// rounds, never next to each other.
fn midweek_positions(rounds: u32, wanted: u32) -> BTreeSet<u32> {
    let mut positions = BTreeSet::new();
    if rounds < 8 || wanted == 0 {
        return positions;
    }
    let count = wanted.min(rounds / 3);
    for k in 1..=count {
        let mut index = (u64::from(k) * u64::from(rounds) / u64::from(count + 1)) as u32;
        index = index.clamp(3, rounds - 2);
        while positions.contains(&index)
            || positions.contains(&(index - 1))
            || positions.contains(&(index + 1))
        {
            index += 1;
        }
        if index <= rounds - 2 {
            positions.insert(index);
        }
    }
    positions
}

/// The most international weekends a season pauses for: the windows of a real calendar.
const MAX_BREAKS: u32 = 6;

/// The matchdays that start after a pause, spread evenly, never the first three rounds or the last
/// three, and never a midweek round (which shares its week with the one after).
fn break_positions(rounds: u32, wanted: u32, midweek: &BTreeSet<u32>) -> BTreeSet<u32> {
    let mut positions = BTreeSet::new();
    if rounds < 10 || wanted == 0 {
        return positions;
    }
    for k in 1..=wanted {
        let mut index = (u64::from(k) * u64::from(rounds) / u64::from(wanted + 1)) as u32;
        index = index.clamp(4, rounds - 3);
        while positions.contains(&index) || midweek.contains(&index) {
            index += 1;
        }
        if index <= rounds - 3 {
            positions.insert(index);
        }
    }
    positions
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// The weeks between `start` and `end` that offer an ordinary round a day: a window that ends on a
/// Saturday has no Sunday in its last week, and one that ends on a Tuesday has no weekend at all.
fn weekend_weeks(start: NaiveDate, end: NaiveDate, pattern: &MatchdayPattern) -> u32 {
    let first_monday = monday_of(start);
    let mut weeks = 0;
    let mut monday = first_monday;
    while monday <= end {
        let offered = (0..7).any(|d| {
            let date = monday + Duration::days(d);
            pattern.weekend[d as usize] > 0 && date >= start && date <= end
        });
        weeks += u32::from(offered);
        monday += Duration::days(7);
    }
    weeks
}

/// The days each round may use. `window_end` is the date the season should be over by, if known.
pub(crate) fn plan_rounds(
    rounds: u32,
    start: NaiveDate,
    window_end: Option<NaiveDate>,
    pattern: &MatchdayPattern,
) -> Vec<RoundPlan> {
    let first_monday = monday_of(start);
    let weeks_available = window_end.map(|end| weekend_weeks(start, end, pattern));
    let needed = weeks_available.map_or(0, |weeks| rounds.saturating_sub(weeks));
    let midweek_rounds = midweek_positions(rounds, pattern.midweek_rounds.max(needed));
    // Weeks the window has beyond what the rounds use pause the season for an international weekend.
    let spare = weeks_available.map_or(0, |weeks| {
        weeks.saturating_sub(rounds.saturating_sub(midweek_rounds.len() as u32))
    });
    let breaks = break_positions(rounds, spare.min(MAX_BREAKS), &midweek_rounds);

    // The days a midweek round must not use: a day either side of a European matchday, for a league
    // that asks to keep clear of them. Built only for such a league.
    let european: BTreeSet<NaiveDate> = if pattern.avoid_european_weeks {
        let season_year = if start.month() >= 6 {
            start.year()
        } else {
            start.year() - 1
        };
        crate::league_phase::calendar::european_matchday_dates(season_year)
    } else {
        BTreeSet::new()
    };
    let clashes_with_europe = |date: NaiveDate| {
        (-1..=1).any(|offset| european.contains(&(date + Duration::days(offset))))
    };

    let mut plans = Vec::with_capacity(rounds as usize);
    let mut week: i64 = 0;
    let mut latest_before: Option<NaiveDate> = None;
    for matchday in 1..=rounds {
        let midweek = midweek_rounds.contains(&matchday);
        if breaks.contains(&matchday) {
            week += 1;
        }
        let weights = if midweek && pattern.midweek.iter().any(|w| *w > 0) {
            &pattern.midweek
        } else {
            &pattern.weekend
        };
        let earliest = [
            Some(start),
            latest_before.map(|d| d + Duration::days(MIN_REST_DAYS)),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(start);
        // The first week (from `week`) that offers a weighted day at or after `earliest`.
        let (days, used_week) = (0..52)
            .find_map(|extra| {
                let monday = first_monday + Duration::days((week + extra) * 7);
                let days: Vec<(NaiveDate, u8)> = (0..7)
                    .map(|d| (monday + Duration::days(d), weights[d as usize]))
                    .filter(|(date, weight)| {
                        *weight > 0
                            && *date >= earliest
                            && window_end.is_none_or(|end| *date <= end)
                            && !(midweek && clashes_with_europe(*date))
                    })
                    .collect();
                (!days.is_empty()).then_some((days, week + extra))
            })
            .unwrap_or_else(|| (vec![(earliest, 1)], week));
        latest_before = days.iter().map(|(date, _)| *date).max();
        // An ordinary round uses up its week; a midweek one shares the week of the round after it.
        if !midweek {
            week = used_week + 1;
        } else {
            week = used_week;
        }
        plans.push(RoundPlan { midweek, days });
    }
    plans
}

/// One date for each fixture of a round: the round's days in proportion to their weights, which
/// fixture gets which by a stable hash of its key, so that the same fixture list always gets the same
/// dates and one club's weekday varies from round to round.
pub(crate) fn allocate_dates(round: &RoundPlan, keys: &[u64]) -> Vec<NaiveDate> {
    let n = keys.len();
    let total: u64 = round.days.iter().map(|(_, w)| u64::from(*w)).sum();
    if n == 0 || total == 0 {
        return vec![round.days.first().map_or(NaiveDate::MIN, |d| d.0); n];
    }
    // Largest-remainder quotas.
    let mut quotas: Vec<(usize, usize, u64)> = round
        .days
        .iter()
        .enumerate()
        .map(|(i, (_, w))| {
            let exact = n as u64 * u64::from(*w);
            (i, (exact / total) as usize, exact % total)
        })
        .collect();
    let unassigned = n.saturating_sub(quotas.iter().map(|q| q.1).sum::<usize>());
    let mut by_remainder: Vec<usize> = (0..quotas.len()).collect();
    by_remainder.sort_by(|a, b| quotas[*b].2.cmp(&quotas[*a].2).then(a.cmp(b)));
    for index in by_remainder.into_iter().take(unassigned) {
        quotas[index].1 += 1;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|i| (keys[*i], *i));
    let mut dates = vec![NaiveDate::MIN; n];
    let mut cursor = 0;
    for (day, quota, _) in quotas {
        for fixture in order.iter().skip(cursor).take(quota) {
            dates[*fixture] = round.days[day].0;
        }
        cursor += quota;
    }
    dates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{build_patterned_fixtures, build_round_robin_fixtures_with};
    use chrono::TimeZone;
    use domain::league::{Fixture, FixtureCompetition};
    use std::collections::{BTreeMap, HashMap};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn teams(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("club-{i:02}")).collect()
    }

    /// Friday 1, Saturday 3, Sunday 5: a Ligue 1 weekend. Midweek on Wednesday.
    fn weekend_pattern() -> MatchdayPattern {
        MatchdayPattern {
            weekend: [0, 0, 0, 0, 1, 3, 5],
            midweek: [0, 0, 1, 0, 0, 0, 0],
            midweek_rounds: 4,
            ..Default::default()
        }
    }

    fn build(n: usize, pattern: &MatchdayPattern, end: Option<NaiveDate>) -> Vec<Fixture> {
        build_from(day(2026, 8, 21), n, pattern, end)
    }

    fn build_from(
        opener: NaiveDate,
        n: usize,
        pattern: &MatchdayPattern,
        end: Option<NaiveDate>,
    ) -> Vec<Fixture> {
        let start = chrono::Utc.from_utc_datetime(&opener.and_hms_opt(0, 0, 0).unwrap());
        build_patterned_fixtures(
            "league",
            &teams(n),
            start,
            FixtureCompetition::League,
            2,
            end,
            pattern,
        )
    }

    /// Everything about a fixture a calendar decides: `Fixture` itself is not comparable.
    fn shape(fixtures: &[Fixture]) -> Vec<(String, u32, String, String, String)> {
        fixtures
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    f.matchday,
                    f.date.clone(),
                    f.home_team_id.clone(),
                    f.away_team_id.clone(),
                )
            })
            .collect()
    }

    fn date_of(fixture: &Fixture) -> NaiveDate {
        NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").unwrap()
    }

    fn weekday_counts(fixtures: &[Fixture]) -> BTreeMap<u32, usize> {
        let mut counts = BTreeMap::new();
        for f in fixtures {
            *counts
                .entry(date_of(f).weekday().num_days_from_monday())
                .or_default() += 1;
        }
        counts
    }

    fn club_dates(fixtures: &[Fixture]) -> HashMap<String, Vec<NaiveDate>> {
        let mut by_club: HashMap<String, Vec<NaiveDate>> = HashMap::new();
        for f in fixtures {
            by_club
                .entry(f.home_team_id.clone())
                .or_default()
                .push(date_of(f));
            by_club
                .entry(f.away_team_id.clone())
                .or_default()
                .push(date_of(f));
        }
        by_club.values_mut().for_each(|d| d.sort());
        by_club
    }

    /// Given a pattern that asks to keep clear of the European weeks
    /// When a league's calendar is planned for several seasons
    /// Then no midweek round falls within a day of a European matchday, the same pairings are
    ///      played, and a pattern that does not ask is planned exactly as before.
    #[test]
    fn a_league_can_keep_its_midweek_rounds_off_the_european_weeks() {
        let careful = MatchdayPattern {
            avoid_european_weeks: true,
            ..weekend_pattern()
        };
        for year in 2026..=2031 {
            let opener = day(year, 8, 21);
            let end = Some(day(year + 1, 5, 17));
            let european = crate::league_phase::calendar::european_matchday_dates(year);
            assert!(european.len() > 40, "{year}: the three calendars are known");
            let plans = plan_rounds(38, opener, end, &careful);
            let midweek: Vec<NaiveDate> = plans
                .iter()
                .filter(|plan| plan.midweek)
                .flat_map(|plan| plan.days.iter().map(|(date, _)| *date))
                .collect();
            assert!(
                midweek.len() >= 4,
                "{year}: the midweek rounds are still played"
            );
            for date in midweek {
                for offset in -1..=1 {
                    assert!(
                        !european.contains(&(date + Duration::days(offset))),
                        "{year}: a midweek round on {date} meets a European matchday"
                    );
                }
            }
            assert_eq!(plans.len(), 38);
        }
        // Not asking changes nothing: byte-identical plans to the plain pattern.
        let plain = plan_rounds(
            38,
            day(2026, 8, 21),
            Some(day(2027, 5, 17)),
            &weekend_pattern(),
        );
        let unflagged = MatchdayPattern {
            avoid_european_weeks: false,
            ..weekend_pattern()
        };
        assert_eq!(
            plain,
            plan_rounds(38, day(2026, 8, 21), Some(day(2027, 5, 17)), &unflagged)
        );
    }

    /// Given a league with no pattern,
    /// When its fixtures are built the patterned way,
    /// Then they are exactly the legacy ones: same pairings, dates and ids.
    #[test]
    fn no_pattern_is_the_legacy_calendar_exactly() {
        let start = chrono::Utc.from_utc_datetime(&day(2026, 8, 21).and_hms_opt(0, 0, 0).unwrap());
        let legacy = build_round_robin_fixtures_with(
            "league",
            &teams(18),
            start,
            FixtureCompetition::League,
            2,
            7,
        );

        let patterned = build_patterned_fixtures(
            "league",
            &teams(18),
            start,
            FixtureCompetition::League,
            2,
            None,
            &MatchdayPattern::default(),
        );

        assert_eq!(shape(&patterned), shape(&legacy));
    }

    /// Given a Ligue 1 weekend pattern (Friday 1, Saturday 3, Sunday 5),
    /// When a season is built,
    /// Then the fixtures fall on Friday, Saturday and Sunday in about those proportions, with the
    /// midweek rounds on Wednesdays and nothing on Monday, Tuesday or Thursday.
    #[test]
    fn a_weekend_pattern_spreads_fixtures_over_friday_saturday_and_sunday() {
        let fixtures = build(18, &weekend_pattern(), None);
        let counts = weekday_counts(&fixtures);
        let total = fixtures.len() as f64;

        let share = |d: u32| *counts.get(&d).unwrap_or(&0) as f64 / total;
        assert!(share(4) > 0.06 && share(4) < 0.16, "Fri {}", share(4));
        assert!(share(5) > 0.22 && share(5) < 0.36, "Sat {}", share(5));
        assert!(share(6) > 0.40 && share(6) < 0.58, "Sun {}", share(6));
        assert!(share(2) > 0.05 && share(2) < 0.20, "Wed {}", share(2));
        assert_eq!(
            [0, 1, 3]
                .iter()
                .map(|d| counts.get(d).unwrap_or(&0))
                .sum::<usize>(),
            0
        );
    }

    /// Given the same season,
    /// When each club's league weekdays are read,
    /// Then no club plays on one weekday all season, and none on a Friday in most rounds: the
    /// opening round's weekday is not kept.
    #[test]
    fn no_club_keeps_the_weekday_of_the_opening_round() {
        let fixtures = build(18, &weekend_pattern(), None);
        assert_eq!(club_dates(&fixtures).len(), 18);

        for (club, dates) in club_dates(&fixtures) {
            let weekdays: BTreeSet<u32> = dates
                .iter()
                .map(|d| d.weekday().num_days_from_monday())
                .collect();
            let fridays = dates
                .iter()
                .filter(|d| d.weekday().num_days_from_monday() == 4)
                .count();
            assert!(weekdays.len() >= 3, "{club} plays on {weekdays:?}");
            assert!(
                fridays * 2 < dates.len(),
                "{club} plays {fridays} of {} on Fridays",
                dates.len()
            );
        }
    }

    /// Given a season built with a pattern,
    /// When its pairings are counted,
    /// Then every ordered pair of clubs meets exactly once, each round has every club once, and home
    /// and away are level: the round-robin algorithm is unchanged.
    #[test]
    fn the_pairings_are_exactly_a_double_round_robin() {
        let fixtures = build(18, &weekend_pattern(), None);

        let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
        let mut home: BTreeMap<String, i64> = BTreeMap::new();
        for f in &fixtures {
            *pairs
                .entry((f.home_team_id.clone(), f.away_team_id.clone()))
                .or_default() += 1;
            *home.entry(f.home_team_id.clone()).or_default() += 1;
            *home.entry(f.away_team_id.clone()).or_default() -= 1;
        }
        assert_eq!(fixtures.len(), 18 * 17);
        assert_eq!(pairs.len(), 18 * 17);
        assert!(pairs.values().all(|n| *n == 1));
        assert!(home.values().all(|v| v.abs() <= 1), "{home:?}");
        let rounds: BTreeSet<u32> = fixtures.iter().map(|f| f.matchday).collect();
        assert_eq!(rounds.len(), 34);
        for round in rounds {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for f in fixtures.iter().filter(|f| f.matchday == round) {
                assert!(
                    seen.insert(&f.home_team_id) && seen.insert(&f.away_team_id),
                    "round {round}"
                );
            }
        }
    }

    /// Given a season built with a pattern,
    /// When the gaps between each club's consecutive fixtures are read,
    /// Then none is under the world's minimum rest and no club plays twice on a day.
    #[test]
    fn every_club_has_the_minimum_rest_between_fixtures() {
        for (teams_n, end) in [(18, None), (24, Some(day(2027, 5, 8)))] {
            let fixtures = build(teams_n, &weekend_pattern(), end);
            assert_eq!(club_dates(&fixtures).len(), teams_n);

            for (club, dates) in club_dates(&fixtures) {
                for pair in dates.windows(2) {
                    assert!(
                        (pair[1] - pair[0]).num_days() >= MIN_REST_DAYS,
                        "{club}: {} then {}",
                        pair[0],
                        pair[1]
                    );
                }
            }
        }
    }

    /// Given a 24-club division (46 rounds) and a window with 38 weekend weeks, and no midweek round
    /// asked for,
    /// When its calendar is built,
    /// Then the season fits the window by playing the eight rounds it lacks weeks for midweek: all
    /// 8 x 12 fixtures of those rounds are on Wednesdays and nothing else is.
    #[test]
    fn a_long_division_plays_midweek_rounds_to_fit_its_window() {
        let end = day(2027, 5, 8);
        let fixtures = build(
            24,
            &MatchdayPattern {
                midweek_rounds: 0,
                ..weekend_pattern()
            },
            Some(end),
        );

        let last = fixtures.iter().map(date_of).max().unwrap();
        let wednesdays = weekday_counts(&fixtures).get(&2).copied().unwrap_or(0);

        assert!(last <= end, "ends {last}");
        assert_eq!(wednesdays, 8 * 12);
        let rounds: BTreeSet<u32> = fixtures.iter().map(|f| f.matchday).collect();
        assert_eq!(rounds.len(), 46);
    }

    /// Given an 18-club league and a window with eight weeks to spare,
    /// When its calendar is built,
    /// Then the season pauses for a few international weekends (weeks with no round at all) and still
    /// ends inside the window; with no window known it does not pause.
    #[test]
    fn spare_weeks_become_international_breaks() {
        let weeks_without_a_round = |fixtures: &[Fixture]| -> usize {
            let weeks: BTreeSet<NaiveDate> =
                fixtures.iter().map(|f| monday_of(date_of(f))).collect();
            let (first, last) = (*weeks.first().unwrap(), *weeks.last().unwrap());
            ((last - first).num_days() / 7 + 1) as usize - weeks.len()
        };
        let end = day(2027, 5, 17);

        let bounded = build(18, &weekend_pattern(), Some(end));
        let open = build(18, &weekend_pattern(), None);

        // Six pauses are planned; one that falls next to a midweek round shares that round's week
        // instead of leaving it empty.
        let paused = weeks_without_a_round(&bounded);
        assert!(
            (MAX_BREAKS as usize - 1..=MAX_BREAKS as usize).contains(&paused),
            "{paused}"
        );
        assert_eq!(weeks_without_a_round(&open), 0);
        assert!(bounded.iter().map(date_of).max().unwrap() <= end);
    }

    /// Given the same inputs,
    /// When the calendar is built twice,
    /// Then the fixtures, ids and dates are identical.
    #[test]
    fn the_calendar_is_deterministic() {
        assert_eq!(
            shape(&build(18, &weekend_pattern(), None)),
            shape(&build(18, &weekend_pattern(), None))
        );
    }

    /// Given a season built with a pattern,
    /// When fixture ids are read,
    /// Then they are unique.
    #[test]
    fn fixture_ids_are_unique() {
        let fixtures = build(20, &weekend_pattern(), None);

        let ids: BTreeSet<&str> = fixtures.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids.len(), fixtures.len());
    }

    /// Given an opening on a Sunday, with a Friday-Saturday-Sunday pattern,
    /// When the first round is read,
    /// Then it is played on the opener alone: the Friday and Saturday before it are not offered.
    #[test]
    fn the_first_round_is_not_before_the_opening_date() {
        let opener = day(2026, 8, 23);

        let fixtures = build_from(opener, 18, &weekend_pattern(), None);

        let first_round: BTreeSet<NaiveDate> = fixtures
            .iter()
            .filter(|f| f.matchday == 1)
            .map(date_of)
            .collect();
        assert_eq!(first_round, BTreeSet::from([opener]));
        assert!(fixtures.iter().map(date_of).min().unwrap() >= opener);
    }

    /// Given an opener in August and a window ending 31 May (before the opener's month and day),
    /// When the end date is resolved,
    /// Then it is the following year's; one ending after the opener stays in the opener's year.
    #[test]
    fn a_window_end_before_the_opener_belongs_to_the_next_year() {
        let may = CalendarDate { month: 5, day: 31 };
        let december = CalendarDate { month: 12, day: 31 };

        assert_eq!(
            window_end_date(day(2026, 8, 21), may),
            Some(day(2027, 5, 31))
        );
        assert_eq!(
            window_end_date(day(2026, 8, 21), december),
            Some(day(2026, 12, 31))
        );
        assert_eq!(
            window_end_date(day(2026, 8, 21), CalendarDate { month: 2, day: 30 }),
            None
        );
        // 29 February in a year without one is 28 February, not no window at all.
        assert_eq!(
            window_end_date(day(2026, 8, 21), CalendarDate { month: 2, day: 29 }),
            Some(day(2027, 2, 28))
        );
    }

    fn regenerated(pattern: Option<MatchdayPattern>, end: Option<CalendarDate>) -> Vec<Fixture> {
        use domain::competition_calendar::{CalendarMetadata, SeasonCalendar};
        let start = chrono::Utc.from_utc_datetime(&day(2026, 8, 21).and_hms_opt(0, 0, 0).unwrap());
        let mut league = crate::schedule::generate_league("L", 2026, &teams(18), start);
        league.calendar = Some(CalendarMetadata {
            season: SeasonCalendar {
                window_end: end,
                matchday_pattern: pattern,
                ..Default::default()
            },
            ..Default::default()
        });
        let next = chrono::Utc.from_utc_datetime(&day(2027, 8, 20).and_hms_opt(0, 0, 0).unwrap());
        crate::schedule::regenerate_league_for_season(&mut league, 2027, next);
        league.fixtures
    }

    /// Given a league whose stored calendar carries a weekday pattern,
    /// When it is renewed for the next season,
    /// Then the new season's fixtures follow the pattern (so the calendar survives rollover), and a
    /// league without one is renewed on the legacy cadence: all on one weekday.
    #[test]
    fn a_renewed_league_keeps_its_stored_pattern() {
        // A window tighter than the season's natural end (mid-March), so that ignoring it shows.
        let window = CalendarDate { month: 3, day: 1 };
        let patterned = regenerated(Some(weekend_pattern()), Some(window));
        let unbounded = regenerated(Some(weekend_pattern()), None);
        let legacy = regenerated(None, None);

        assert!(weekday_counts(&patterned).len() >= 4);
        assert_eq!(weekday_counts(&legacy).len(), 1);
        assert!(patterned.iter().map(date_of).min().unwrap() >= day(2027, 8, 20));
        assert!(patterned.iter().map(date_of).max().unwrap() <= day(2028, 3, 1));
        assert!(unbounded.iter().map(date_of).max().unwrap() > day(2028, 3, 1));
    }

    /// Given a pattern whose weights sit on one weekday only,
    /// When rounds are allocated,
    /// Then every fixture of the round is on that day.
    #[test]
    fn a_single_weighted_day_takes_the_whole_round() {
        let round = RoundPlan {
            midweek: false,
            days: vec![(day(2026, 8, 22), 7)],
        };

        let dates = allocate_dates(&round, &[5, 3, 9, 1]);

        assert!(dates.iter().all(|d| *d == day(2026, 8, 22)));
    }

    /// Given weights 1 : 3 : 5 and eight fixtures (exact shares 0.9, 2.7 and 4.4),
    /// When they are allocated,
    /// Then the days get 1, 3 and 4: the two fixtures the floors leave over go to the largest
    /// remainders, not to chance.
    #[test]
    fn a_round_is_allocated_by_exact_quotas() {
        let round = RoundPlan {
            midweek: false,
            days: vec![
                (day(2026, 8, 21), 1),
                (day(2026, 8, 22), 3),
                (day(2026, 8, 23), 5),
            ],
        };

        let dates = allocate_dates(
            &round,
            &(0..8u64).map(|i| i * 7919 % 13).collect::<Vec<_>>(),
        );

        let mut counts: BTreeMap<NaiveDate, usize> = BTreeMap::new();
        dates
            .iter()
            .for_each(|d| *counts.entry(*d).or_default() += 1);
        assert_eq!(counts.values().copied().collect::<Vec<_>>(), vec![1, 3, 4]);
    }
}
