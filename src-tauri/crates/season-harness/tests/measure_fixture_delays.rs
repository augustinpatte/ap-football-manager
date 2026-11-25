//! How far fixtures are moved from the date they were generated for, and what held them back
//! (ALPHA6.1 section 5).
//!
//! Measurement only, `#[ignore]`d, release mode, the FM24 world named by `APFM_WORLD_JSON`:
//!
//! `APFM_WORLD_JSON=… cargo test --release -p season-harness --test measure_fixture_delays -- --ignored --nocapture`
//!
//! `APFM_SEEDS` (default `1,2`) and `APFM_SEASONS` (default `2`). Per season and domestic league it
//! prints the distribution of delays (not only the maximum), the reason the game recorded for the
//! move, and, for every fixture delayed two weeks or more, a day-by-day account of what made each
//! day between its generated date and its played date unavailable to the two clubs.

use std::collections::{BTreeMap, HashMap};

use chrono::{Days, NaiveDate};
use domain::league::{Fixture, FixtureCompetition, FixtureStatus};
use ofm_core::calendar_booking::MIN_GAP_DAYS;
use ofm_core::game::Game;
use season_harness::calendar::CalendarStudy;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const LENS: &str = "fm-c-501";
const TAIL_DAYS: i64 = 14;

struct Delays {
    seed: u64,
    study: CalendarStudy,
}

/// The pairs of a club's consecutive scheduled matches that sit closer than the minimum rest in the
/// calendar as it was generated, before the first day reconciles it: by the two competitions'
/// kinds, and for two league matches by league.
fn raw_clashes(game: &Game, label: &str, seed: u64) {
    let mut by_club: HashMap<&str, Vec<(NaiveDate, FixtureCompetition, &str)>> = HashMap::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if fixture.competition == FixtureCompetition::InternationalNation
                || fixture.status != FixtureStatus::Scheduled
            {
                continue;
            }
            let Some(date) = date_of(fixture) else {
                continue;
            };
            for club in [&fixture.home_team_id, &fixture.away_team_id] {
                by_club.entry(club.as_str()).or_default().push((
                    date,
                    fixture.competition.clone(),
                    competition.id.as_str(),
                ));
            }
        }
    }
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut league_vs_league: BTreeMap<String, usize> = BTreeMap::new();
    let mut gaps: BTreeMap<i64, usize> = BTreeMap::new();
    for rows in by_club.values_mut() {
        rows.sort_by_key(|row| row.0);
        for pair in rows.windows(2) {
            let gap = (pair[1].0 - pair[0].0).num_days();
            if gap >= MIN_GAP_DAYS {
                continue;
            }
            let kind = |c: &FixtureCompetition| match c {
                FixtureCompetition::League => "league",
                FixtureCompetition::Cup => "cup",
                FixtureCompetition::ContinentalClub | FixtureCompetition::InternationalClub => {
                    "continental"
                }
                _ => "other",
            };
            let (a, b) = (kind(&pair[0].1), kind(&pair[1].1));
            let mut pair_kinds = [a, b];
            pair_kinds.sort_unstable();
            *by_kind.entry(pair_kinds.join("+")).or_default() += 1;
            *gaps.entry(gap).or_default() += 1;
            if a == "league" && b == "league" {
                *league_vs_league.entry(pair[0].2.to_string()).or_default() += 1;
            }
        }
    }
    println!(
        "RAW CLASHES seed {seed} {label}: pairs of a club's matches closer than {MIN_GAP_DAYS} days in the generated calendar, by kind {by_kind:?}, by gap in days {gaps:?}, league-vs-league by league {league_vs_league:?}"
    );
}

fn date_of(fixture: &Fixture) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").ok()
}

fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// What stands in the way of `clubs` playing on `day`, by the layer that holds the club: the highest
/// of a continental match, a cup tie or another league match inside the rest window, else a national
/// team window, else nothing the final calendar shows (a booking that has since moved, or a date a
/// later cup draw might still have claimed).
fn blocker(
    day: NaiveDate,
    clubs: [&str; 2],
    this: &str,
    matches_of: &HashMap<&str, Vec<(NaiveDate, FixtureCompetition, &str)>>,
    international_days: &std::collections::BTreeSet<NaiveDate>,
) -> &'static str {
    let reach = MIN_GAP_DAYS - 1;
    let mut found = [false; 3];
    for club in clubs {
        for (date, competition, id) in matches_of.get(club).map_or(&[][..], Vec::as_slice) {
            if *id == this || (*date - day).num_days().abs() > reach {
                continue;
            }
            match competition {
                FixtureCompetition::ContinentalClub | FixtureCompetition::InternationalClub => {
                    found[0] = true
                }
                FixtureCompetition::Cup => found[1] = true,
                FixtureCompetition::League => found[2] = true,
                _ => {}
            }
        }
    }
    if found[0] {
        "continental"
    } else if found[1] {
        "cup"
    } else if found[2] {
        "league"
    } else if international_days.contains(&day) {
        "international"
    } else {
        "nothing_visible"
    }
}

impl SeasonObserver for Delays {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let mut matches_of: HashMap<&str, Vec<(NaiveDate, FixtureCompetition, &str)>> =
            HashMap::new();
        let mut international_days = std::collections::BTreeSet::new();
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                let Some(date) = date_of(fixture) else {
                    continue;
                };
                if fixture.competition == FixtureCompetition::InternationalNation {
                    international_days.insert(date);
                    continue;
                }
                for club in [&fixture.home_team_id, &fixture.away_team_id] {
                    matches_of.entry(club.as_str()).or_default().push((
                        date,
                        fixture.competition.clone(),
                        fixture.id.as_str(),
                    ));
                }
            }
        }
        let name = |id: &str| {
            game.teams
                .iter()
                .find(|t| t.id == id)
                .map_or_else(|| id.to_string(), |t| t.name.clone())
        };
        let planned: BTreeMap<&str, NaiveDate> = self.study.generated_dates().into_iter().collect();
        let mut leagues: BTreeMap<String, Vec<(i64, &Fixture, NaiveDate, NaiveDate)>> =
            BTreeMap::new();
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                if fixture.competition != FixtureCompetition::League {
                    continue;
                }
                let (Some(actual), Some(&generated)) =
                    (date_of(fixture), planned.get(fixture.id.as_str()))
                else {
                    continue;
                };
                leagues.entry(competition.id.clone()).or_default().push((
                    (actual - generated).num_days(),
                    fixture,
                    generated,
                    actual,
                ));
            }
        }
        let league_name = |id: &str| {
            game.competitions
                .iter()
                .find(|c| c.id == id)
                .map_or_else(|| id.to_string(), |c| format!("{id} {}", c.name))
        };
        for (league_id, rows) in &leagues {
            let league = &league_name(league_id);
            let moved: Vec<i64> = {
                let mut v: Vec<i64> = rows.iter().map(|r| r.0).filter(|d| *d != 0).collect();
                v.sort_unstable();
                v
            };
            if moved.is_empty() {
                continue;
            }
            let over = |days: i64| moved.iter().filter(|d| **d >= days).count();
            let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
            for (delay, fixture, _, _) in rows {
                if *delay != 0 {
                    let reason = fixture
                        .postponement
                        .as_ref()
                        .map_or("not_shown_before".to_string(), |p| {
                            format!("{:?}", p.reason)
                        });
                    *reasons.entry(reason).or_default() += 1;
                }
            }
            println!(
                "DELAYS seed {} season {season} {league}: fixtures {} moved {} | days p50 {} p90 {} p99 {} max {} | >=7d {} >=14d {} >=30d {} >=50d {} | first reason {reasons:?}",
                self.seed,
                rows.len(),
                moved.len(),
                percentile(&moved, 0.5),
                percentile(&moved, 0.9),
                percentile(&moved, 0.99),
                moved.last().copied().unwrap_or(0),
                over(7),
                over(14),
                over(30),
                over(50),
            );
            // Cascades: a fixture whose first move the game attributed to "another league fixture"
            // was displaced by one that had itself been moved (it now sits within the rest window of
            // this fixture's generated date, and its own date is not the one it was generated for).
            let moved_ids: std::collections::HashSet<&str> = rows
                .iter()
                .filter(|r| r.0 != 0)
                .map(|r| r.1.id.as_str())
                .collect();
            let mut by_a_moved_league_fixture = 0;
            let mut by_a_fixed_league_fixture = 0;
            for (delay, fixture, generated, _) in rows {
                let is_league_clash = *delay != 0
                    && fixture
                        .postponement
                        .as_ref()
                        .is_none_or(|p| p.reason == domain::league::RescheduleReason::LeagueClash);
                if !is_league_clash {
                    continue;
                }
                let neighbours: Vec<&str> = [&fixture.home_team_id, &fixture.away_team_id]
                    .into_iter()
                    .flat_map(|club| matches_of.get(club.as_str()).into_iter().flatten())
                    .filter(|(date, competition, id)| {
                        *competition == FixtureCompetition::League
                            && *id != fixture.id.as_str()
                            && (*date - *generated).num_days().abs() < MIN_GAP_DAYS
                    })
                    .map(|(_, _, id)| *id)
                    .collect();
                if neighbours.iter().any(|id| moved_ids.contains(id)) {
                    by_a_moved_league_fixture += 1;
                } else {
                    by_a_fixed_league_fixture += 1;
                }
            }
            println!(
                "    CASCADE {league}: of the moves the game called a league clash, {by_a_moved_league_fixture} sit beside a league fixture that was itself moved, {by_a_fixed_league_fixture} beside league fixtures that were not"
            );
            let mut blocked_days: BTreeMap<&str, usize> = BTreeMap::new();
            let mut tail: Vec<&(i64, &Fixture, NaiveDate, NaiveDate)> =
                rows.iter().filter(|r| r.0 >= TAIL_DAYS).collect();
            tail.sort_by_key(|r| std::cmp::Reverse(r.0));
            for (delay, fixture, generated, actual) in tail.iter().map(|r| &**r) {
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                let mut day = *generated;
                while day < *actual {
                    let label = blocker(
                        day,
                        [&fixture.home_team_id, &fixture.away_team_id],
                        &fixture.id,
                        &matches_of,
                        &international_days,
                    );
                    *counts.entry(label).or_default() += 1;
                    *blocked_days.entry(label).or_default() += 1;
                    day = day + Days::new(1);
                }
                if tail.first().is_some_and(|first| {
                    std::ptr::eq(*first, &(*delay, *fixture, *generated, *actual))
                }) || *delay >= 40
                {
                    println!(
                        "    TAIL {league} md{} {} v {}: generated {generated} played {actual} (+{delay}d), status {:?}, days between by what held a club: {counts:?}",
                        fixture.matchday,
                        name(&fixture.home_team_id),
                        name(&fixture.away_team_id),
                        fixture.status,
                    );
                }
            }
            if !blocked_days.is_empty() {
                let total: usize = blocked_days.values().sum();
                println!(
                    "    TAIL SUMMARY {league}: {} fixtures >= {TAIL_DAYS}d, {total} days between generated and played: {blocked_days:?}",
                    tail.len()
                );
            }
        }
        let _ = FixtureStatus::Scheduled;
    }

    fn on_season_end(
        &mut self,
        game: &Game,
        _summary: &ofm_core::end_of_season::EndOfSeasonSummary,
        _season: u32,
    ) {
        // The next season's fixtures exist now, generated and not yet moved.
        raw_clashes(game, "after the rollover", self.seed);
        self.study = CalendarStudy::new(game);
    }
}

#[test]
#[ignore = "measurement: prints the distribution of fixture delays per league"]
fn domestic_fixture_delays_per_league_and_season() {
    let Some(path) = world_path_from_env() else {
        println!("SKIPPED: APFM_WORLD_JSON is not set");
        return;
    };
    let seeds: Vec<u64> = std::env::var("APFM_SEEDS")
        .unwrap_or_else(|_| "1,2".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let seasons: u32 = std::env::var("APFM_SEASONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    for seed in seeds {
        let mut game = build_fm24_career(&path, LENS, seed).expect("the career builds");
        raw_clashes(&game, "opening", seed);
        let mut delays = Delays {
            seed,
            study: CalendarStudy::new(&game),
        };
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut delays).expect("seasons play");
    }
}
