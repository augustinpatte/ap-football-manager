//! Run F5.0: does every fixture of the world leave the same kind of consequences behind?
//!
//! The world is played with every competition active, so each club fixture goes through the one
//! post-match pipeline (`turn::apply_match_report_in`). An observer watches every day and every
//! stats capture and reports, per rule, how often it was broken. Nothing here changes how the game
//! plays.
//!
//! `cargo test --release -p season-harness --test f50_consequences -- --ignored --nocapture`

use std::collections::{BTreeMap, BTreeSet};

use domain::league::FixtureStatus;
use domain::stats::StatsState;
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::{Scope, WorldSpec};

/// What one club's players looked like at the start of the day.
#[derive(Default, Clone)]
struct Before {
    injured: BTreeSet<String>,
}

#[derive(Default)]
struct Study {
    /// Captures exist for the full path only, so season totals reconcile against them only when
    /// every competition is played in full.
    reconcile: bool,
    before: Before,
    /// Per fixture id: how many times a capture was delivered.
    captures_per_fixture: BTreeMap<String, u32>,
    /// Per player: minutes the captures credit them with this season.
    capture_appearances: BTreeMap<String, u32>,
    capture_minutes: BTreeMap<String, u32>,
    capture_goals: BTreeMap<String, u32>,
    capture_yellows: BTreeMap<String, u32>,
    capture_reds: BTreeMap<String, u32>,
    played_injured: Vec<String>,
    played_total: u64,
    zero_minute_records: u64,
    /// (club is the player's, injuries began that day).
    user_club: Option<String>,
    injuries_began: BTreeMap<String, u32>,
    bans_set: BTreeMap<String, u32>,
    season_reconciliation: Vec<String>,
    matches: u64,
    prev_injured: BTreeSet<String>,
    prev_suspended: BTreeMap<String, u8>,
    /// Season totals by tier ("active" / "dormant"): minutes, goals, assists, yellows, reds.
    tiers: BTreeMap<&'static str, [u64; 5]>,
    active_clubs: BTreeSet<String>,
    /// Each player's condition at the end of the previous day, i.e. as a match finds him.
    condition_before: BTreeMap<String, u8>,
    /// (players with minutes, of whom below 50, sum of condition) for the user's club and the rest.
    kickoff: BTreeMap<bool, (u64, u64, u64)>,
}

impl SeasonObserver for Study {
    fn on_stats_captured(&mut self, stats: &StatsState) {
        let mut fixtures: BTreeSet<&str> = BTreeSet::new();
        for team in &stats.team_matches {
            fixtures.insert(team.fixture_id.as_str());
        }
        for fixture in fixtures {
            *self
                .captures_per_fixture
                .entry(fixture.to_string())
                .or_default() += 1;
        }
        self.matches += 1;
        for record in &stats.player_matches {
            self.played_total += 1;
            if record.minutes_played >= 45
                && let Some(&condition) = self.condition_before.get(&record.player_id)
            {
                let is_user = self.user_club.as_deref() == Some(record.team_id.as_str());
                let entry = self.kickoff.entry(is_user).or_default();
                entry.0 += 1;
                entry.1 += u64::from(condition < 50);
                entry.2 += u64::from(condition);
            }
            if record.minutes_played == 0 {
                self.zero_minute_records += 1;
            }
            *self
                .capture_appearances
                .entry(record.player_id.clone())
                .or_default() += 1;
            *self
                .capture_minutes
                .entry(record.player_id.clone())
                .or_default() += u32::from(record.minutes_played);
            *self
                .capture_goals
                .entry(record.player_id.clone())
                .or_default() += u32::from(record.goals);
            *self
                .capture_yellows
                .entry(record.player_id.clone())
                .or_default() += u32::from(record.yellow_cards);
            *self
                .capture_reds
                .entry(record.player_id.clone())
                .or_default() += u32::from(record.red_cards);
            if record.minutes_played > 0 && self.before.injured.contains(&record.player_id) {
                self.played_injured.push(format!(
                    "{} played {} while injured",
                    record.player_id, record.fixture_id
                ));
            }
        }
    }

    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        // State "before the next day" is the state at the end of this one.
        self.user_club = game.manager.team_id.clone();
        self.condition_before = game
            .players
            .iter()
            .map(|p| (p.id.clone(), p.condition))
            .collect();
        let injured: BTreeSet<String> = game
            .players
            .iter()
            .filter(|p| p.injury.is_some())
            .map(|p| p.id.clone())
            .collect();
        for id in injured.difference(&self.prev_injured) {
            let club = game
                .players
                .iter()
                .find(|p| &p.id == id)
                .and_then(|p| p.team_id.clone())
                .unwrap_or_default();
            *self.injuries_began.entry(club).or_default() += 1;
        }
        self.prev_injured = injured.clone();
        let suspended: BTreeMap<String, u8> = game
            .players
            .iter()
            .filter(|p| p.suspended_matches > 0)
            .map(|p| (p.id.clone(), p.suspended_matches))
            .collect();
        for id in suspended.keys() {
            if !self.prev_suspended.contains_key(id) {
                let club = game
                    .players
                    .iter()
                    .find(|p| &p.id == id)
                    .and_then(|p| p.team_id.clone())
                    .unwrap_or_default();
                *self.bans_set.entry(club).or_default() += 1;
            }
        }
        self.prev_suspended = suspended.clone();
        self.before = Before { injured };
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let active: BTreeSet<String> = game
            .competitions
            .iter()
            .filter(|c| game.competition_in_active_scope(c))
            .flat_map(|c| c.participant_ids.iter().cloned())
            .collect();
        self.active_clubs.extend(active.iter().cloned());
        for p in &game.players {
            let Some(team) = p.team_id.as_deref() else {
                continue;
            };
            let tier = if active.contains(team) {
                "active"
            } else {
                "dormant"
            };
            let totals = self.tiers.entry(tier).or_default();
            totals[0] += u64::from(p.stats.minutes_played);
            totals[1] += u64::from(p.stats.goals);
            totals[2] += u64::from(p.stats.assists);
            totals[3] += u64::from(p.stats.yellow_cards);
            totals[4] += u64::from(p.stats.red_cards);
        }
        // Season totals against the captured match records.
        let mut faults = Vec::new();
        for p in game.players.iter().filter(|_| self.reconcile) {
            let c = |m: &BTreeMap<String, u32>| m.get(&p.id).copied().unwrap_or(0);
            if p.stats.appearances != c(&self.capture_appearances) && p.team_id.is_some() {
                faults.push(format!(
                    "{} appearances {} vs captured {}",
                    p.id,
                    p.stats.appearances,
                    c(&self.capture_appearances)
                ));
            }
            if p.stats.minutes_played != c(&self.capture_minutes) && p.team_id.is_some() {
                faults.push(format!(
                    "{} minutes {} vs captured {}",
                    p.id,
                    p.stats.minutes_played,
                    c(&self.capture_minutes)
                ));
            }
            if p.stats.goals != c(&self.capture_goals) && p.team_id.is_some() {
                faults.push(format!(
                    "{} goals {} vs captured {}",
                    p.id,
                    p.stats.goals,
                    c(&self.capture_goals)
                ));
            }
            if p.stats.yellow_cards != c(&self.capture_yellows) && p.team_id.is_some() {
                faults.push(format!(
                    "{} yellows {} vs captured {}",
                    p.id,
                    p.stats.yellow_cards,
                    c(&self.capture_yellows)
                ));
            }
        }
        // Completed fixtures against captures: exactly once.
        let mut completed = 0usize;
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                if fixture.status == FixtureStatus::Completed && fixture.result.is_some() {
                    completed += 1;
                    let n = self
                        .captures_per_fixture
                        .get(&fixture.id)
                        .copied()
                        .unwrap_or(0);
                    if n > 1 {
                        faults.push(format!("fixture {} captured {n} times", fixture.id));
                    }
                }
            }
        }
        let mut uncaptured: BTreeMap<String, u32> = BTreeMap::new();
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                if fixture.status == FixtureStatus::Completed
                    && !self.captures_per_fixture.contains_key(&fixture.id)
                {
                    *uncaptured
                        .entry(format!(
                            "{:?}/{}/report={}",
                            competition.kind,
                            competition.id,
                            fixture.result.as_ref().is_some_and(|r| r.report.is_some())
                        ))
                        .or_default() += 1;
                }
            }
        }
        self.season_reconciliation.push(format!(
            "  uncaptured completed fixtures by kind: {uncaptured:?}"
        ));
        self.season_reconciliation.push(format!(
            "season {season}: {completed} completed fixtures, {} captured, {} reconciliation faults",
            self.captures_per_fixture.len(),
            faults.len()
        ));
        self.season_reconciliation
            .extend(faults.into_iter().take(8));
        // Season counters reset at rollover; so do the capture tallies.
        self.capture_appearances.clear();
        self.capture_minutes.clear();
        self.capture_goals.clear();
        self.capture_yellows.clear();
        self.capture_reds.clear();
        self.captures_per_fixture.clear();
    }

    fn on_season_end(&mut self, _game: &Game, _summary: &EndOfSeasonSummary, _season: u32) {}
}

fn run(seed: u64, seasons: u32, scope: Scope) -> Study {
    let mut game = WorldSpec::gate(seed)
        .with_scope(scope)
        .build()
        .expect("world");
    let mut study = Study {
        reconcile: scope == Scope::Everything,
        ..Study::default()
    };
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut study).expect("run");
    study
}

fn report(study: &Study) {
    println!("matches captured: {}", study.matches);
    println!(
        "player records: {} of which with 0 minutes: {}",
        study.played_total, study.zero_minute_records
    );
    for line in &study.season_reconciliation {
        println!("{line}");
    }
    println!(
        "players who played while injured: {} (suspended: see the harness rule banned-player-never-plays)",
        study.played_injured.len()
    );
    for line in study.played_injured.iter().take(5) {
        println!("  {line}");
    }
    let user = study.user_club.clone().unwrap_or_default();
    let others: Vec<(&String, &u32)> = study
        .injuries_began
        .iter()
        .filter(|(club, _)| **club != user)
        .collect();
    let ai_total: u32 = others.iter().map(|(_, n)| **n).sum();
    println!(
        "injuries began: user {} | AI clubs {} across {} clubs (mean {:.1})",
        study.injuries_began.get(&user).copied().unwrap_or(0),
        ai_total,
        others.len(),
        f64::from(ai_total) / others.len().max(1) as f64
    );
    let bans_ai: u32 = study
        .bans_set
        .iter()
        .filter(|(c, _)| **c != user)
        .map(|(_, n)| *n)
        .sum();
    println!(
        "bans began: user {} | AI {}",
        study.bans_set.get(&user).copied().unwrap_or(0),
        bans_ai
    );
    for (is_user, (n, tired, sum)) in &study.kickoff {
        println!(
            "{:>8} players with 45+ minutes: {n}, mean condition at kick-off {:.1}, share below 50: {:.1}%",
            if *is_user { "user" } else { "AI" },
            *sum as f64 / (*n).max(1) as f64,
            100.0 * *tired as f64 / (*n).max(1) as f64
        );
    }
    for (tier, t) in &study.tiers {
        let team_matches = t[0] as f64 / 990.0;
        let injuries: u64 = study
            .injuries_began
            .iter()
            .filter(|(club, _)| (study.active_clubs.contains(*club)) == (*tier == "active"))
            .map(|(_, n)| u64::from(*n))
            .sum();
        println!(
            "{tier:>8}: team-matches {team_matches:>7.0} | goals {:.2} assists {:.2} yellow {:.2} red {:.3} injuries {:.3} per team-match",
            t[1] as f64 / team_matches,
            t[2] as f64 / team_matches,
            t[3] as f64 / team_matches,
            t[4] as f64 / team_matches,
            injuries as f64 / team_matches,
        );
    }
}

/// Given a world played in full for two seasons,
/// When every stats capture and every day is observed,
/// Then the study prints the consequence report.
#[test]
#[ignore = "plays seasons; run in release with --ignored --nocapture"]
fn f50_every_fixture_leaves_consequences() {
    for scope in [Scope::Everything, Scope::Followed] {
        println!("== scope {scope:?}");
        let study = run(11, 2, scope);
        report(&study);
    }
}

/// Given a world and a player serving a ban,
/// When a match hands the stats store minutes for him,
/// Then the invariants name him: the gate can fail on a suspended player in a lineup.
#[test]
fn a_banned_player_with_minutes_in_a_match_is_reported() {
    use season_harness::invariants::{BANNED_PLAYER_PLAYED, Invariants};
    let mut game = WorldSpec::gate(3).build().expect("world");
    let mut invariants = Invariants::new(&game);
    let id = game
        .players
        .iter()
        .find(|p| p.team_id.is_some())
        .expect("a player")
        .id
        .clone();
    game.players
        .iter_mut()
        .find(|p| p.id == id)
        .expect("player")
        .suspended_matches = 2;
    invariants.on_day_end(&game, 1, 1);

    let mut record = domain::stats::PlayerMatchStatsRecord {
        fixture_id: "fx".into(),
        season: 2033,
        matchday: 1,
        date: "2033-08-01".into(),
        competition: domain::league::FixtureCompetition::League,
        player_id: id.clone(),
        team_id: "a".into(),
        opponent_team_id: "b".into(),
        home_team_id: "a".into(),
        away_team_id: "b".into(),
        home_goals: 0,
        away_goals: 0,
        minutes_played: 90,
        goals: 0,
        assists: 0,
        shots: 0,
        shots_on_target: 0,
        passes_completed: 0,
        passes_attempted: 0,
        tackles_won: 0,
        interceptions: 0,
        fouls_committed: 0,
        yellow_cards: 0,
        red_cards: 0,
        rating: 6.0,
    };
    invariants.on_stats_captured(&StatsState::with_lines(vec![record.clone()], vec![]));
    assert!(invariants.report_of(BANNED_PLAYER_PLAYED).contains(&id));

    // An unused substitute with no minutes is no breach.
    let mut clean = Invariants::new(&game);
    clean.on_day_end(&game, 1, 1);
    record.minutes_played = 0;
    clean.on_stats_captured(&StatsState::with_lines(vec![record], vec![]));
    assert!(clean.report_of(BANNED_PLAYER_PLAYED).is_empty());
}

/// Given a generated world played in full for a season,
/// When every stats capture is read against the day before,
/// Then no banned player had minutes, and the season's totals equal what the captures say.
#[test]
fn a_season_of_full_matches_keeps_availability_and_reconciles() {
    let mut game = WorldSpec::gate(5)
        .with_scope(Scope::Everything)
        .build()
        .expect("world");
    let mut study = Study {
        reconcile: true,
        ..Study::default()
    };
    let mut invariants = season_harness::invariants::Invariants::new(&game);
    run_seasons(
        &mut game,
        RunOptions::seasons(1),
        &mut (&mut study, &mut invariants),
    )
    .expect("run");
    assert!(
        invariants
            .report_of(season_harness::invariants::BANNED_PLAYER_PLAYED)
            .is_empty(),
        "{}",
        invariants.report()
    );
    assert!(
        study
            .season_reconciliation
            .iter()
            .all(|line| !line.contains("vs captured")),
        "{:?}",
        study.season_reconciliation
    );
}
