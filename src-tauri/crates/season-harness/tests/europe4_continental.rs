//! EUROPE4 end to end: three league-phase European competitions played beside a generated world's
//! domestic leagues and cups, by the game's own day loop and rollover, for consecutive seasons.
//!
//! What this holds that the unit tests cannot: that the competitions are drawn, dated and settled
//! into the world's calendar; that every match is played whether or not the competition is one the
//! player follows (the dormant tier settles the others by scoreline); that an edition survives the
//! rollover and a reload mid-way; that it is archived once with its champion and renewed with a
//! fresh draw; and that no club is in two European competitions at once.

use std::collections::BTreeMap;

use domain::league::FixtureStatus;
use domain::league_phase::{CompetitionStage, KnockoutStage};
use ofm_core::game::Game;
use season_harness::calendar::{reloaded, unplayed_clashes};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::europe::{
    EUROPEAN_COMPETITIONS, all_european_editions_completed, clubs_in_two_european_competitions,
    europe_world, european,
};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;

/// Prints the day each season is rolled over, and what the user's own competitions still had open.
struct Rollovers;

impl SeasonObserver for Rollovers {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let user = game.manager.team_id.clone().unwrap_or_default();
        let open: Vec<String> = game
            .competitions
            .iter()
            .filter(|c| c.participant_ids.contains(&user))
            .map(|c| {
                let left = c
                    .fixtures
                    .iter()
                    .filter(|f| f.status == FixtureStatus::Scheduled)
                    .count();
                format!("{}:{left}", c.id)
            })
            .collect();
        eprintln!(
            "rollover of season {season} on {} (open fixtures: {open:?})",
            game.clock.current_date.date_naive()
        );
    }
}

/// What the European competitions must satisfy at the end of every day.
#[derive(Default)]
struct EuropeWatch {
    faults: Vec<String>,
    days: u32,
    stages: BTreeMap<String, CompetitionStage>,
    /// `date competition stage season`, one line per change: the timeline a failure is read from.
    timeline: Vec<String>,
}

impl EuropeWatch {
    fn flag(&mut self, game: &Game, what: impl Into<String>) {
        if self.faults.len() < 40 {
            self.faults.push(format!(
                "{}: {}",
                game.clock.current_date.date_naive(),
                what.into()
            ));
        }
    }
}

impl SeasonObserver for EuropeWatch {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        self.days += 1;
        let today = game.clock.current_date.format("%Y-%m-%d").to_string();
        for competition in european(game) {
            if let Some(state) = &competition.league_phase
                && self.stages.insert(competition.id.clone(), state.stage) != Some(state.stage)
            {
                self.timeline.push(format!(
                    "{today} {} season {} -> {:?} ({} fixtures)",
                    competition.id,
                    competition.season,
                    state.stage,
                    competition.fixtures.len()
                ));
            }
        }
        for clash in unplayed_clashes(game).into_iter().take(3) {
            self.flag(game, format!("rest window broken: {clash}"));
        }
        for fault in clubs_in_two_european_competitions(game) {
            self.flag(game, fault);
        }
        for competition in european(game) {
            // A European match left unplayed in the past was skipped: whether or not the player
            // follows the competition, every match is played.
            let stranded = competition
                .fixtures
                .iter()
                .filter(|f| f.status == FixtureStatus::Scheduled && f.date < today)
                .count();
            if stranded > 0 {
                self.flag(
                    game,
                    format!("{} left {stranded} past matches unplayed", competition.id),
                );
            }
            let Some(state) = &competition.league_phase else {
                self.flag(
                    game,
                    format!("{} lost its league-phase state", competition.id),
                );
                continue;
            };
            let expected = match state.stage {
                CompetitionStage::Created => 0,
                _ => competition
                    .rules
                    .league_phase
                    .as_ref()
                    .map_or(0, |r| r.league_fixture_count()),
            };
            let knockout: usize = state.ties.iter().map(|t| t.fixture_ids.len()).sum();
            if state.stage != CompetitionStage::Created
                && competition.fixtures.len() != expected + knockout
            {
                self.flag(
                    game,
                    format!(
                        "{} holds {} fixtures, expected {}",
                        competition.id,
                        competition.fixtures.len(),
                        expected + knockout
                    ),
                );
            }
        }
    }
}

/// Where two strings first differ, with the text around it from each.
fn window_of_difference(left: &str, right: &str) -> String {
    let at = left
        .bytes()
        .zip(right.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    let around = |text: &str| {
        let from = at.saturating_sub(160);
        let to = (at + 160).min(text.len());
        String::from_utf8_lossy(&text.as_bytes()[from..to]).into_owned()
    };
    format!(
        "byte {at} of {} / {}\n  left : …{}…\n  right: …{}…",
        left.len(),
        right.len(),
        around(left),
        around(right)
    )
}

fn season_options(seasons: u32) -> RunOptions {
    RunOptions::seasons(seasons)
}

/// Whether every European competition is in the edition of `season` and the one before it is on
/// record in the archive: finished, archived and renewed.
fn all_european_editions_renewed(game: &Game, season: u32) -> bool {
    european(game).all(|competition| {
        competition.season == season
            && game
                .edition_archive
                .iter()
                .any(|edition| edition.is_edition_of(&competition.id, season - 1))
    })
}

/// Plays on, day by day, until every European edition is finished, archived and renewed.
fn finish_european_editions(
    game: &mut Game,
    watch: &mut (&mut EuropeWatch, &mut Invariants),
    first_day: u32,
    renewed_into: u32,
) {
    for day in first_day..first_day + 400 {
        if all_european_editions_renewed(game, renewed_into) {
            return;
        }
        play_day(game, season_options(1), 99, day, watch).unwrap_or_else(|error| panic!("{error}"));
    }
    let state: Vec<String> = european(game)
        .map(|c| {
            let open = c
                .fixtures
                .iter()
                .filter(|f| f.status == FixtureStatus::Scheduled)
                .count();
            format!(
                "{} season {} {:?} open={open} draw_error={:?}",
                c.id,
                c.season,
                c.league_phase.as_ref().map(|s| s.stage),
                c.league_phase.as_ref().and_then(|s| s.draw_error.clone()),
            )
        })
        .collect();
    panic!(
        "the European editions were never renewed by {}: {state:#?}\n{:#?}",
        game.clock.current_date, watch.0.timeline
    );
}

fn assert_editions_archived(game: &Game, seasons: &[u32]) {
    for (id, _, _) in EUROPEAN_COMPETITIONS {
        for season in seasons {
            let edition = game
                .edition_archive
                .iter()
                .find(|edition| edition.is_edition_of(id, *season))
                .unwrap_or_else(|| panic!("{id} season {season} was never archived"));
            let state = edition
                .league_phase
                .as_ref()
                .expect("the archive freezes the state");
            let counts: Vec<usize> = KnockoutStage::ALL
                .iter()
                .map(|stage| state.ties_of(*stage).count())
                .collect();
            assert_eq!(counts, vec![8, 8, 4, 2, 1], "{id} {season}");
            assert_eq!(
                state.champion_id.as_deref(),
                Some(edition.champion_id.as_str())
            );
            assert_eq!(edition.standings.len(), 36);
        }
        let archived: Vec<_> = game
            .edition_archive
            .iter()
            .filter(|edition| edition.competition_id == id)
            .collect();
        assert_eq!(
            archived.len(),
            seasons.len(),
            "{id}: one record per edition, never two"
        );
    }
}

/// Plays `seasons` seasons of the three European competitions in the world of `seed`, then on to the
/// end of the last finished edition, and checks every rule on the way and at the end.
fn play_europe(seed: u64, seasons: u32) -> Game {
    let mut game = europe_world(seed).expect("the world and its European competitions build");
    for competition in european(&game) {
        let state = competition.league_phase.as_ref().unwrap();
        assert_eq!(state.stage, CompetitionStage::LeaguePhaseScheduled);
        assert_eq!(competition.participant_ids.len(), 36, "{}", competition.id);
    }
    assert!(clubs_in_two_european_competitions(&game).is_empty());
    assert!(
        unplayed_clashes(&game).is_empty(),
        "{:?}",
        unplayed_clashes(&game)
    );
    let user = game.manager.team_id.clone().unwrap();
    let followed: Vec<&str> = european(&game)
        .filter(|c| game.active_competition_ids.contains(&c.id))
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        followed,
        vec!["europe-cl"],
        "only the manager's competition is followed"
    );
    assert!(
        game.competitions
            .iter()
            .any(|c| c.id == "europe-cl" && c.participant_ids.contains(&user))
    );

    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);
    let mut watch = EuropeWatch::default();
    run_seasons(
        &mut game,
        season_options(seasons),
        &mut (
            &mut Rollovers,
            (&mut watch, (&mut invariants, &mut lifecycle)),
        ),
    )
    .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
    let days_played = watch.days;
    eprintln!(
        "seed {seed}: seasons played; the clock is at {}",
        game.clock.current_date.date_naive()
    );
    finish_european_editions(
        &mut game,
        &mut (&mut watch, &mut invariants),
        days_played + 1,
        2033 + seasons,
    );
    eprintln!(
        "seed {seed}: editions finished; the clock is at {}",
        game.clock.current_date.date_naive()
    );

    assert!(watch.faults.is_empty(), "seed {seed}: {:#?}", watch.faults);
    assert!(
        invariants.is_clean(),
        "seed {seed}: {}",
        invariants.report()
    );
    assert!(lifecycle.is_clean(), "seed {seed}: {}", lifecycle.report());
    let editions: Vec<u32> = (0..seasons).map(|offset| 2033 + offset).collect();
    assert_editions_archived(&game, &editions);
    game
}

/// Given a generated world with three drawn European competitions
/// When two consecutive seasons are played through the game's own day loop and rollover
/// Then every invariant of the harness holds, every European match is played (in the followed
///      competition by the engine, in the others by scoreline), each edition is archived once with
///      its champion and renewed with a fresh draw, and no club is ever in two of them.
#[test]
fn two_seasons_of_three_european_competitions_hold_every_rule() {
    play_europe(1, 2);
}

/// The same over three seasons and several seeds (`EUROPE4_SEEDS`, comma separated). Minutes long,
/// so it runs by hand: `cargo test -p season-harness --test europe4_continental -- --ignored`.
#[test]
#[ignore = "minutes per seed; run by hand for the audit"]
fn several_seeds_over_three_seasons_hold_every_rule() {
    let seeds = std::env::var("EUROPE4_SEEDS").unwrap_or_else(|_| "2,3".to_string());
    for seed in seeds
        .split(',')
        .filter_map(|seed| seed.trim().parse::<u64>().ok())
    {
        let game = play_europe(seed, 3);
        eprintln!(
            "seed {seed}: {} editions archived, last day {}",
            game.edition_archive.len(),
            game.clock.current_date.date_naive()
        );
    }
}

/// Given a European edition played through the game's day loop
/// When the game is saved and reloaded each time any competition moves to a new stage
/// Then every European competition is identical before and after each reload (draw, table, ties,
///      fixtures, posted prizes), the game plays on from the reloaded state, and every edition
///      finishes with a champion the strict completion proof accepts.
#[test]
fn a_save_and_reload_at_every_stage_loses_nothing() {
    let mut game = europe_world(2).unwrap();
    let mut watch = EuropeWatch::default();
    let mut invariants = Invariants::new(&game);
    let mut sink = (&mut watch, &mut invariants);
    let mut last_stages: BTreeMap<String, CompetitionStage> = BTreeMap::new();
    let mut reloads = 0;

    for day in 1..420 {
        play_day(&mut game, season_options(1), 1, day, &mut sink).unwrap();
        let moved = european(&game).any(|c| {
            let stage = c.league_phase.as_ref().unwrap().stage;
            last_stages.insert(c.id.clone(), stage) != Some(stage)
        });
        if moved {
            let before: Vec<String> = european(&game)
                .map(|c| serde_json::to_string(c).unwrap())
                .collect();
            game = reloaded(&game);
            let after: Vec<String> = european(&game)
                .map(|c| serde_json::to_string(c).unwrap())
                .collect();
            for (a, b) in before.iter().zip(&after) {
                assert!(
                    a == b,
                    "a reload changed a competition: {}",
                    window_of_difference(a, b)
                );
            }
            reloads += 1;
        }
        if all_european_editions_completed(&game) {
            break;
        }
    }

    assert!(all_european_editions_completed(&game));
    assert!(
        reloads >= 12,
        "only {reloads} stage changes were crossed with a reload"
    );
    for competition in european(&game) {
        let proof = ofm_core::competition_completion::verify_completed_edition(competition, None)
            .unwrap_or_else(|failure| {
                panic!("{} does not prove complete: {failure:?}", competition.id)
            });
        assert_eq!(
            proof.champion_id,
            competition.league_phase.as_ref().unwrap().champion_id,
            "{}",
            competition.id
        );
    }
    assert!(watch.faults.is_empty(), "{:#?}", watch.faults);
}

/// The control for the multi-season run: the same twelve-nation world and seed, played for three
/// seasons with no European competition added. A fault that shows up here is not the league
/// phase's. Run by hand with the multi-seed test.
#[test]
#[ignore = "minutes; the control for the multi-seed European run"]
fn the_same_world_without_european_competitions_for_three_seasons() {
    let seed = std::env::var("EUROPE4_SEEDS")
        .ok()
        .and_then(|seeds| {
            seeds
                .split(',')
                .next()
                .and_then(|s| s.trim().parse::<u64>().ok())
        })
        .unwrap_or(2);
    let mut game = season_harness::europe::europe_spec(seed).build().unwrap();
    let mut invariants = Invariants::new(&game);
    run_seasons(
        &mut game,
        season_options(3),
        &mut (&mut Rollovers, &mut invariants),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Diagnostic for a stranded fixture seen in the multi-seed run: replays the seed and, whenever a
/// past fixture of any non-European competition is still unplayed, says exactly what it is.
#[test]
#[ignore = "diagnostic; minutes"]
fn probe_stranded_fixtures_with_europe() {
    let seed = std::env::var("EUROPE4_SEEDS")
        .ok()
        .and_then(|seeds| {
            seeds
                .split(',')
                .next()
                .and_then(|s| s.trim().parse::<u64>().ok())
        })
        .unwrap_or(2);
    struct Probe(Vec<String>);
    impl SeasonObserver for Probe {
        fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
            let today = game.clock.current_date.format("%Y-%m-%d").to_string();
            for c in &game.competitions {
                for f in c
                    .fixtures
                    .iter()
                    .filter(|f| f.status == FixtureStatus::Scheduled && f.date < today)
                {
                    let line = format!(
                        "season {season} day {day} today {today}: {} ({:?}/{:?}) fixture {} {} v {} due {} matchday {}",
                        c.id,
                        c.kind,
                        c.scope,
                        f.id,
                        f.home_team_id,
                        f.away_team_id,
                        f.date,
                        f.matchday
                    );
                    if self.0.len() < 6 && !self.0.iter().any(|l| l.contains(&f.id)) {
                        self.0.push(line);
                    }
                }
            }
        }
    }
    let mut game = europe_world(seed).unwrap();
    let mut probe = Probe(Vec::new());
    run_seasons(&mut game, season_options(3), &mut probe).unwrap();
    for line in &probe.0 {
        eprintln!("{line}");
    }
    let wc: Vec<_> = game
        .competitions
        .iter()
        .filter(|c| c.id.starts_with("world-cup"))
        .map(|c| {
            format!(
                "{} season {} fixtures {} open {}",
                c.id,
                c.season,
                c.fixtures.len(),
                c.fixtures
                    .iter()
                    .filter(|f| f.status == FixtureStatus::Scheduled)
                    .count()
            )
        })
        .collect();
    eprintln!("world cup competitions: {wc:#?}");
    assert!(probe.0.is_empty(), "stranded fixtures: {:#?}", probe.0);
}

/// Diagnostic: which competitions end late in the European world, and how far the calendar moved them.
#[test]
#[ignore = "diagnostic; about a minute"]
fn probe_which_competitions_run_late() {
    struct Late;
    impl SeasonObserver for Late {
        fn on_before_rollover(&mut self, game: &Game, season: u32) {
            let mut rows: Vec<(String, String, usize, usize, i64)> = Vec::new();
            for c in &game.competitions {
                let Some(last) = c
                    .fixtures
                    .iter()
                    .filter(|f| f.status == FixtureStatus::Completed)
                    .map(|f| f.date.clone())
                    .max()
                else {
                    continue;
                };
                let moved = c
                    .fixtures
                    .iter()
                    .filter(|f| f.postponement.is_some())
                    .count();
                let worst = c
                    .fixtures
                    .iter()
                    .filter_map(|f| {
                        let original = f.postponement.as_ref()?.original_date.clone();
                        let a = chrono::NaiveDate::parse_from_str(&original, "%Y-%m-%d").ok()?;
                        let b = chrono::NaiveDate::parse_from_str(&f.date, "%Y-%m-%d").ok()?;
                        Some((b - a).num_days())
                    })
                    .max()
                    .unwrap_or(0);
                rows.push((last, c.id.clone(), c.fixtures.len(), moved, worst));
            }
            rows.sort();
            rows.reverse();
            eprintln!(
                "season {season} rollover on {}:",
                game.clock.current_date.date_naive()
            );
            for (last, id, total, moved, worst) in rows.iter().take(6) {
                eprintln!(
                    "  last played {last}  {id}: {moved}/{total} moved, worst displacement {worst} days"
                );
            }
        }
    }
    let mut game = europe_world(2).unwrap();
    run_seasons(&mut game, season_options(1), &mut Late).unwrap();
}

/// Diagnostic: the scope and the European fields around the first matchday of the second edition.
#[test]
#[ignore = "diagnostic; minutes"]
fn probe_scope_around_the_second_edition() {
    struct Scope;
    impl SeasonObserver for Scope {
        fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
            let today = game.clock.current_date.format("%Y-%m-%d").to_string();
            if !("2034-09-04"..="2034-09-08").contains(&today.as_str()) {
                return;
            }
            let user = game.manager.team_id.clone().unwrap();
            let fields: Vec<String> = european(game)
                .map(|c| {
                    let unreported = c
                        .fixtures
                        .iter()
                        .filter(|f| {
                            f.status == FixtureStatus::Completed
                                && f.result.as_ref().is_some_and(|r| r.report.is_none())
                        })
                        .count();
                    format!(
                        "{} season {} user_in={} unreported={unreported}",
                        c.id,
                        c.season,
                        c.participant_ids.contains(&user)
                    )
                })
                .collect();
            eprintln!(
                "{today}: active={:?} regions={:?} {fields:?}",
                game.active_competition_ids, game.active_region_ids
            );
        }
    }
    let mut game = europe_world(2).unwrap();
    run_seasons(&mut game, season_options(2), &mut Scope).unwrap();
}
