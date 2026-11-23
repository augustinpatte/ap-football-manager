//! REALISM3: the domestic calendars of the FM24 world, division by division.
//!
//! `realism3_calendar_audit <world.json> <out.json> [--seasons 2] [--seed 1] [--club fm-c-498]`
//!
//! Builds the career as the app does, then reads every league table's fixtures at the start and
//! after each of the next `--seasons` rollovers (the new season's calendar). For each division:
//! rounds, fixtures, matches by weekday, midweek share, the dates the season spans, whether every
//! ordered pairing appears exactly once and every club plays as often at home as away, and, across
//! *all* competitions, double bookings, rest shorter than three days and the most matches any club
//! plays in a week. For the chosen club the weekday of each league fixture. Aggregates only, and the
//! dates of one club's league fixtures: no opponents, no ids.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use domain::league::{
    CompetitionFormat, CompetitionScope, FixtureCompetition, FixtureStatus, League,
};
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::build_fm24_career;
use serde_json::{Value, json};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn is_table(league: &League) -> bool {
    league.scope == CompetitionScope::Domestic
        && league.rules.format == CompetitionFormat::LeagueTable
        && league.participant_ids.len() >= 2
}

fn division(game: &Game, league: &League, club: &str) -> Value {
    let fixtures: Vec<(NaiveDate, &str, &str, u32)> = league
        .fixtures
        .iter()
        // Friendlies are appended to a league's list; they are not its rounds.
        .filter(|f| f.competition == FixtureCompetition::League)
        .filter_map(|f| {
            Some((
                date(&f.date)?,
                f.home_team_id.as_str(),
                f.away_team_id.as_str(),
                f.matchday,
            ))
        })
        .collect();
    let teams = league.participant_ids.len();
    let rounds = fixtures.iter().map(|f| f.3).max().unwrap_or(0);
    let mut by_weekday = [0usize; 7];
    for f in &fixtures {
        by_weekday[f.0.weekday().num_days_from_monday() as usize] += 1;
    }
    let total = fixtures.len().max(1);
    let midweek = (1..=3).map(|d| by_weekday[d]).sum::<usize>();
    let mut pairings: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut home: BTreeMap<&str, i64> = BTreeMap::new();
    for f in &fixtures {
        *pairings.entry((f.1, f.2)).or_default() += 1;
        *home.entry(f.1).or_default() += 1;
        *home.entry(f.2).or_default() -= 1;
    }
    let expected_pairs = teams * teams.saturating_sub(1);
    let pairing_ok = pairings.len() == expected_pairs && pairings.values().all(|n| *n == 1);
    let imbalance = home.values().map(|v| v.abs()).max().unwrap_or(0);
    let mut per_month: BTreeMap<String, usize> = BTreeMap::new();
    for f in &fixtures {
        *per_month
            .entry(f.0.format("%Y-%m").to_string())
            .or_default() += 1;
    }
    let dates: Vec<NaiveDate> = fixtures.iter().map(|f| f.0).collect();
    let span = (dates.iter().min().copied(), dates.iter().max().copied());
    let mut dates_per_round: BTreeMap<u32, BTreeSet<NaiveDate>> = BTreeMap::new();
    for f in &fixtures {
        dates_per_round.entry(f.3).or_default().insert(f.0);
    }
    let club_days: Vec<String> = fixtures
        .iter()
        .filter(|f| f.1 == club || f.2 == club)
        .map(|f| f.0.format("%a %Y-%m-%d").to_string())
        .collect();
    let club_weekdays: Vec<&str> = fixtures
        .iter()
        .filter(|f| f.1 == club || f.2 == club)
        .map(|f| WEEKDAYS[f.0.weekday().num_days_from_monday() as usize])
        .collect();
    let median_dates_per_round = {
        let mut counts: Vec<usize> = dates_per_round.values().map(BTreeSet::len).collect();
        counts.sort_unstable();
        counts.get(counts.len() / 2).copied().unwrap_or(0)
    };
    // Per club: how many distinct weekdays its league fixtures use, and how much of its season falls
    // on its most common one (1.0 = the same weekday all season).
    let mut by_club: BTreeMap<&str, [usize; 7]> = BTreeMap::new();
    for f in &fixtures {
        let weekday = f.0.weekday().num_days_from_monday() as usize;
        by_club.entry(f.1).or_default()[weekday] += 1;
        by_club.entry(f.2).or_default()[weekday] += 1;
    }
    let shares: Vec<f64> = by_club
        .values()
        .map(|d| *d.iter().max().unwrap_or(&0) as f64 / d.iter().sum::<usize>().max(1) as f64)
        .collect();
    let weekdays_used: Vec<usize> = by_club
        .values()
        .map(|d| d.iter().filter(|n| **n > 0).count())
        .collect();
    let postponed = league
        .fixtures
        .iter()
        .filter(|f| f.postponement.is_some())
        .count();
    let _ = game;
    json!({
        "name": league.name, "teams": teams, "fixtures": total, "rounds": rounds,
        "expected_fixtures": expected_pairs,
        "pairings_exactly_once": pairing_ok,
        "max_home_away_imbalance": imbalance,
        "weekdays_mon_to_sun": by_weekday,
        "club_top_weekday_share_mean": shares.iter().sum::<f64>() / shares.len().max(1) as f64,
        "club_top_weekday_share_max": shares.iter().copied().fold(0.0, f64::max),
        "club_min_weekdays_used": weekdays_used.iter().min(),
        "clubs_on_one_weekday": weekdays_used.iter().filter(|n| **n == 1).count(),
        "midweek_share": midweek as f64 / total as f64,
        "weekend_share": (by_weekday[4] + by_weekday[5] + by_weekday[6]) as f64 / total as f64,
        "first": span.0.map(|d| d.to_string()), "last": span.1.map(|d| d.to_string()),
        "median_dates_per_round": median_dates_per_round,
        "per_month": per_month,
        "postponed": postponed,
        "club_weekday_counts": club_weekdays.iter().fold(BTreeMap::<&str, usize>::new(), |mut m, d| { *m.entry(*d).or_default() += 1; m }),
        "club_first_dates": club_days.iter().take(12).collect::<Vec<_>>(),
    })
}

/// A competition's kind, for attributing a clash to what played it.
fn label(league: &League) -> String {
    format!("{:?}/{:?}", league.scope, league.rules.format)
}

/// Across every competition: clubs booked twice on a day, rest under three days, busiest week. The
/// shorter rests are attributed by the pair of competitions involved and by whether the side is a
/// club (`fm-c-`) or something else (a national team).
fn world_checks(game: &Game) -> Value {
    let mut by_club: BTreeMap<&str, Vec<(NaiveDate, String)>> = BTreeMap::new();
    for league in &game.competitions {
        let kind = label(league);
        for f in league
            .fixtures
            .iter()
            .filter(|f| f.status != FixtureStatus::Completed)
        {
            let Some(d) = date(&f.date) else { continue };
            by_club
                .entry(f.home_team_id.as_str())
                .or_default()
                .push((d, kind.clone()));
            by_club
                .entry(f.away_team_id.as_str())
                .or_default()
                .push((d, kind.clone()));
        }
    }
    let (mut double, mut short, mut busiest) = (0usize, 0usize, 0usize);
    let mut shortest: i64 = i64::MAX;
    let mut short_by_pair: BTreeMap<String, usize> = BTreeMap::new();
    for (side, days) in by_club.iter_mut() {
        days.sort();
        for pair in days.windows(2) {
            let gap = (pair[1].0 - pair[0].0).num_days();
            if gap == 0 {
                double += 1;
            } else if gap < ofm_core::calendar_booking::MIN_GAP_DAYS {
                short += 1;
            }
            if gap < ofm_core::calendar_booking::MIN_GAP_DAYS {
                let who = if side.starts_with("fm-c-") {
                    "club"
                } else {
                    "other"
                };
                let mut kinds = [pair[0].1.clone(), pair[1].1.clone()];
                kinds.sort();
                *short_by_pair
                    .entry(format!("{who}: {} + {}", kinds[0], kinds[1]))
                    .or_default() += 1;
            }
            shortest = shortest.min(gap);
        }
        for (i, d) in days.iter().enumerate() {
            let in_week = days[i..]
                .iter()
                .take_while(|x| (x.0 - d.0).num_days() < 7)
                .count();
            busiest = busiest.max(in_week);
        }
    }
    json!({"clubs": by_club.len(), "double_bookings": double, "rest_under_3_days": short,
           "rest_under_3_days_by_competitions": short_by_pair,
           "shortest_gap_days": if shortest == i64::MAX { Value::Null } else { json!(shortest) },
           "most_matches_in_7_days": busiest})
}

fn snapshot(game: &Game, club: &str) -> Value {
    let divisions: Vec<Value> = game
        .competitions
        .iter()
        .filter(|l| is_table(l))
        .map(|l| division(game, l, club))
        .collect();
    let census: BTreeMap<String, usize> = game
        .competitions
        .iter()
        .map(|l| {
            format!(
                "{:?}/{:?}/{}",
                l.scope,
                l.rules.format,
                l.participant_ids.len().min(2)
            )
        })
        .fold(BTreeMap::new(), |mut m, k| {
            *m.entry(k).or_default() += 1;
            m
        });
    json!({"divisions": divisions, "census": census, "world": world_checks(game),
           "date": game.clock.current_date.format("%Y-%m-%d").to_string()})
}

struct Audit {
    club: String,
    snaps: Vec<(String, Value)>,
}

impl SeasonObserver for Audit {
    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        self.snaps
            .push((format!("rollover{season}"), snapshot(game, &self.club)));
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(world), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: realism3_calendar_audit <world.json> <out.json> [--seasons 2] [--seed 1] [--club fm-c-498]".into());
    };
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seasons: u32 = flag("--seasons").and_then(|s| s.parse().ok()).unwrap_or(2);
    let seed: u64 = flag("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let club = flag("--club").unwrap_or_else(|| "fm-c-498".to_string());
    let mut game = build_fm24_career(std::path::Path::new(world), &club, seed)?;
    // Every division of the world, not only the player's league.
    game.active_region_ids.clear();
    game.active_competition_ids.clear();
    let mut audit = Audit {
        club: club.clone(),
        snaps: vec![("start".to_string(), snapshot(&game, &club))],
    };
    let started = std::time::Instant::now();
    if seasons > 0 {
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut audit)
            .map_err(|e| e.to_string())?;
    }
    let report: serde_json::Map<String, Value> = audit.snaps.into_iter().collect();
    std::fs::write(
        out,
        serde_json::to_string_pretty(&json!({"seed": seed, "club": club, "seconds": started.elapsed().as_secs_f64(), "snapshots": report}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
