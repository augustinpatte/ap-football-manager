//! Mission 7: careers over several seasons, held to the rules of the competitions.
//!
//! `apfm_competitions <club id> <seasons> <out.json> [--seed N] [--live]`
//!
//! Plays the career through the production day and rollover entry points with the harness
//! invariants attached, and before every rollover reads every competition of the world: leagues
//! (full double round-robin, no pair twice, every match played, final order), cups (every tie
//! played, one champion, one final, no level tie left without a winner, entrants equal to the
//! feeding divisions, the final before the rollover). After every rollover it reads what moved.
//! A failed check is printed as `FAIL` and counted; the JSON keeps everything a report needs.

use std::collections::{BTreeMap, BTreeSet};

use domain::league::{CompetitionFormat, CompetitionType, FixtureStatus, League};
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use serde_json::{Value, json};

#[derive(Default)]
struct Recorder {
    seasons: Vec<Value>,
    failures: Vec<String>,
    checks: usize,
    user_team: String,
    user_cup_matches: BTreeMap<u32, usize>,
    user_cup_ties: BTreeMap<u32, Vec<Value>>,
    current: Vec<Value>,
}

impl Recorder {
    fn check(&mut self, season: u32, ok: bool, message: impl FnOnce() -> String) {
        self.checks += 1;
        if !ok {
            let line = format!("[season {season}] {}", message());
            println!("FAIL {line}");
            self.failures.push(line);
        }
    }

    fn league(&mut self, season: u32, league: &League) -> Value {
        let n = league.participant_ids.len();
        let league_fixtures: Vec<_> = league
            .fixtures
            .iter()
            .filter(|f| f.counts_for_league_standings())
            .collect();
        let expected = n * (n - 1);
        self.check(season, league_fixtures.len() == expected, || {
            format!(
                "{}: {} league fixtures, expected {expected}",
                league.id,
                league_fixtures.len()
            )
        });
        let pairs: BTreeSet<(&str, &str)> = league_fixtures
            .iter()
            .map(|f| (f.home_team_id.as_str(), f.away_team_id.as_str()))
            .collect();
        self.check(season, pairs.len() == league_fixtures.len(), || {
            format!("{}: a home/away pair is scheduled twice", league.id)
        });
        let unplayed = league_fixtures
            .iter()
            .filter(|f| f.status != FixtureStatus::Completed)
            .count();
        self.check(season, unplayed == 0, || {
            format!(
                "{}: {unplayed} league fixtures unplayed at the rollover",
                league.id
            )
        });
        let table = league.sorted_standings();
        let played_ok = table.iter().all(|row| row.played as usize == 2 * (n - 1));
        self.check(season, played_ok, || {
            format!("{}: a club did not play {} matches", league.id, 2 * (n - 1))
        });
        let by_club: BTreeMap<&str, usize> = league_fixtures
            .iter()
            .flat_map(|f| [f.home_team_id.as_str(), f.away_team_id.as_str()])
            .fold(BTreeMap::new(), |mut acc, club| {
                *acc.entry(club).or_default() += 1;
                acc
            });
        self.check(season, by_club.values().all(|c| *c == 2 * (n - 1)), || {
            format!("{}: uneven number of matches per club", league.id)
        });
        // Clubs level on points that the tiebreakers had to separate.
        let level_pairs = table
            .windows(2)
            .filter(|w| w[0].points == w[1].points)
            .count();
        json!({
            "id": league.id, "name": league.name, "clubs": n, "fixtures": league_fixtures.len(),
            "last_fixture": league_fixtures.iter().map(|f| f.date.clone()).max(),
            "champion": table.first().map(|r| r.team_id.clone()),
            "table": table.iter().map(|r| json!([r.team_id, r.points, r.goals_for as i64 - r.goals_against as i64, r.goals_for])).collect::<Vec<_>>(),
            "adjacent_level_on_points": level_pairs,
            "withdrawn": league.rules.withdrawn_sides.iter().map(|w| w.team_id.clone()).collect::<Vec<_>>(),
        })
    }

    fn cup(&mut self, season: u32, cup: &League, rollover: &str) -> Value {
        let n = cup.participant_ids.len();
        let fixtures = &cup.fixtures;
        let unplayed = fixtures
            .iter()
            .filter(|f| f.status != FixtureStatus::Completed)
            .count();
        self.check(season, unplayed == 0, || {
            format!("{}: {unplayed} ties unplayed at the rollover", cup.id)
        });
        self.check(season, fixtures.len() == n.saturating_sub(1), || {
            format!(
                "{}: {} ties for {n} entrants, expected {}",
                cup.id,
                fixtures.len(),
                n - 1
            )
        });
        let all_rounds_closed = cup.knockout_rounds.iter().all(|r| r.completed)
            || cup
                .knockout_rounds
                .last()
                .is_some_and(|r| r.fixture_ids.len() == 1);
        self.check(season, all_rounds_closed, || {
            format!("{}: a round is not closed", cup.id)
        });
        let last_round = cup.knockout_rounds.last();
        let final_ok = last_round.is_some_and(|r| r.name == "Final" && r.fixture_ids.len() == 1);
        self.check(season, final_ok, || {
            format!("{}: the last round is not a one-tie final", cup.id)
        });
        let final_fixture = last_round
            .and_then(|r| r.fixture_ids.first())
            .and_then(|id| fixtures.iter().find(|f| &f.id == id));
        let champion = final_fixture
            .and_then(|f| f.advancing_team_id())
            .map(str::to_string);
        self.check(season, champion.is_some(), || {
            format!("{}: no champion", cup.id)
        });
        // A level score needs a shootout; a shootout needs a level score.
        let mut shootouts = 0;
        let mut bad_level = 0;
        for f in fixtures {
            if let Some(r) = &f.result {
                match (r.home_penalties, r.away_penalties) {
                    (Some(h), Some(a)) => {
                        shootouts += 1;
                        if r.home_goals != r.away_goals || h == a {
                            bad_level += 1;
                        }
                    }
                    _ => {
                        if r.home_goals == r.away_goals {
                            bad_level += 1;
                        }
                    }
                }
            }
        }
        self.check(season, bad_level == 0, || {
            format!(
                "{}: {bad_level} ties left level or shootouts out of place",
                cup.id
            )
        });
        // Nobody appears twice in a round; every round's winners are the next round's field.
        for round in &cup.knockout_rounds {
            let mut seen = BTreeSet::new();
            let mut twice = false;
            for id in &round.fixture_ids {
                if let Some(f) = fixtures.iter().find(|f| &f.id == id) {
                    twice |= !seen.insert(f.home_team_id.as_str())
                        | !seen.insert(f.away_team_id.as_str());
                }
            }
            self.check(season, !twice, || {
                format!("{}: a club plays twice in {}", cup.id, round.name)
            });
        }
        // The final is played before the season turns over, and after the cup's own start.
        let last = fixtures.iter().map(|f| f.date.as_str()).max().unwrap_or("");
        self.check(season, last <= rollover, || {
            format!("{}: last tie {last} after the rollover {rollover}", cup.id)
        });
        // Before the rollover the cup still holds the entrants it was drawn with, the divisions
        // having moved since it was made: the entrants are compared with the divisions after the
        // rollover (see `on_season_end`).
        let user_ties = fixtures
            .iter()
            .filter(|f| f.home_team_id == self.user_team || f.away_team_id == self.user_team)
            .count();
        if cup.participant_ids.contains(&self.user_team) {
            self.user_cup_matches.insert(season, user_ties);
            let ties: Vec<Value> = fixtures
                .iter()
                .filter(|f| f.home_team_id == self.user_team || f.away_team_id == self.user_team)
                .map(|f| json!({"date": f.date, "home": f.home_team_id, "away": f.away_team_id, "result": f.result.as_ref().map(|r| json!([r.home_goals, r.away_goals, r.home_penalties, r.away_penalties])), "minutes": f.result.as_ref().and_then(|r| r.report.as_ref()).map(|r| r.total_minutes)}))
                .collect();
            self.user_cup_ties.insert(season, ties);
        }
        json!({
            "id": cup.id, "name": cup.name, "entrants": n, "ties": fixtures.len(),
            "rounds": cup.knockout_rounds.iter().map(|r| json!({"name": r.name, "ties": r.fixture_ids.len(), "byes": r.bye_team_ids.len(), "first_date": r.fixture_ids.first().and_then(|id| fixtures.iter().find(|f| &f.id == id)).map(|f| f.date.clone()), "last_date": r.fixture_ids.iter().filter_map(|id| fixtures.iter().find(|f| &f.id == id)).map(|f| f.date.clone()).max()})).collect::<Vec<_>>(),
            "champion": champion, "shootouts": shootouts,
            "final": final_fixture.map(|f| json!({"date": f.date, "home": f.home_team_id, "away": f.away_team_id, "result": f.result.as_ref().map(|r| json!([r.home_goals, r.away_goals, r.home_penalties, r.away_penalties]))})),
            "first_tie": fixtures.iter().map(|f| f.date.clone()).min(), "last_tie": last,
            "user_ties": user_ties,
        })
    }
}

impl SeasonObserver for Recorder {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let rollover = game.clock.current_date.format("%Y-%m-%d").to_string();
        let mut leagues = Vec::new();
        let mut cups = Vec::new();
        for competition in &game.competitions {
            match (&competition.kind, &competition.rules.format) {
                (CompetitionType::League, CompetitionFormat::LeagueTable) => {
                    leagues.push(self.league(season, competition))
                }
                (CompetitionType::Cup, CompetitionFormat::Knockout) => {
                    cups.push(self.cup(season, competition, &rollover))
                }
                _ => {}
            }
        }
        self.current =
            vec![json!({"season": season, "rollover": rollover, "leagues": leagues, "cups": cups})];
    }

    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        let mut record = self
            .current
            .pop()
            .unwrap_or_else(|| json!({"season": season}));
        // After the rollover: cups are renewed with the divisions as they now stand.
        let mut cup_checks = Vec::new();
        for cup in game
            .competitions
            .iter()
            .filter(|c| c.kind == CompetitionType::Cup && c.rules.cup.is_authored())
        {
            let barred: BTreeSet<&str> = if cup.rules.cup.exclude_reserve_sides {
                cup.rules
                    .reserve_sides
                    .iter()
                    .map(|s| s.team_id.as_str())
                    .collect()
            } else {
                BTreeSet::new()
            };
            let feeders: BTreeSet<&str> = cup
                .rules
                .cup
                .feeder_ids
                .iter()
                .filter_map(|id| game.competitions.iter().find(|c| &c.id == id))
                .flat_map(|c| c.participant_ids.iter().map(String::as_str))
                .filter(|club| !barred.contains(club))
                .collect();
            let entrants: BTreeSet<&str> = cup.participant_ids.iter().map(String::as_str).collect();
            let renewed = !cup.fixtures.is_empty()
                && cup
                    .fixtures
                    .iter()
                    .all(|f| f.status == FixtureStatus::Scheduled);
            self.check(season, renewed, || {
                format!("{}: not renewed after the rollover", cup.id)
            });
            self.check(season, feeders == entrants, || {
                format!(
                    "{}: entrants differ from the divisions ({} vs {})",
                    cup.id,
                    entrants.len(),
                    feeders.len()
                )
            });
            cup_checks.push(json!({"id": cup.id, "season": cup.season, "entrants": cup.participant_ids.len(), "first_round_ties": cup.fixtures.len()}));
        }
        // What the boundaries moved this season, as the game's own archive remembers it.
        let moves: Vec<Value> = game
            .edition_archive
            .iter()
            .filter_map(|e| {
                e.movements
                    .as_ref()
                    .filter(|m| m.upper_competition_id == e.competition_id)
            })
            .map(|m| serde_json::to_value(m).unwrap_or_default())
            .collect();
        record["renewed_cups"] = json!(cup_checks);
        record["movements_recorded_so_far"] = json!(moves.len());
        record["season_number"] = json!(season);
        self.seasons.push(record);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let club = args.next().expect("club id");
    let seasons: u32 = args.next().expect("seasons").parse().expect("number");
    let out = args.next().expect("output path");
    let rest: Vec<String> = args.collect();
    let seed: u64 = rest
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| rest.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    let path = world_path_from_env().expect("APFM_WORLD_JSON is not set");
    let mut game = build_fm24_career(&path, &club, seed).expect("career builds");
    let mut options = RunOptions::seasons(seasons);
    if rest.iter().any(|a| a == "--live") {
        // The player's own matches go through the live engine, extra time and penalties included.
        options.day_path = season_harness::driver::DayPath::UserMatchLive;
    }
    let mut invariants = Invariants::new(&game);
    let mut recorder = Recorder {
        user_team: club.clone(),
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let result = {
        let mut both = (&mut invariants, &mut recorder);
        run_seasons(&mut game, options, &mut both)
    };
    let elapsed = started.elapsed().as_secs_f64();
    let (days, error) = match &result {
        Ok(summary) => (summary.days, None),
        Err(e) => (0, Some(format!("{e:?}"))),
    };
    // What the game remembers at the end: the palmarès and every boundary's movements.
    let history: Vec<Value> = game
        .edition_archive
        .iter()
        .map(|e| {
            json!({
                "competition": e.competition_id, "season": e.season, "champion": e.champion_id,
                "completed_on": e.completed_on, "movements": e.movements,
            })
        })
        .collect();
    let report = invariants.report();
    println!(
        "{club}: {seasons} seasons, {days} days, {elapsed:.1}s, {} checks, {} failures, invariants: {}",
        recorder.checks,
        recorder.failures.len(),
        if report.is_empty() {
            "clean".to_string()
        } else {
            report.replace('\n', " | ")
        }
    );
    if let Some(error) = &error {
        println!("ERROR {error}");
    }
    let payload = json!({
        "club": club, "seasons_requested": seasons, "days": days, "seconds": elapsed, "error": error,
        "checks": recorder.checks, "failures": recorder.failures, "invariants": report,
        "seasons": recorder.seasons, "history": history,
        "user_cup_matches": recorder.user_cup_matches,
        "user_cup_ties": recorder.user_cup_ties,
    });
    std::fs::write(&out, serde_json::to_vec_pretty(&payload).expect("json")).expect("write");
}
