//! P4.5: a long career, season by season. Measurement and integrity (`#[ignore]`d).
//!
//! ```text
//! P45_WORLD=gate|shipped P45_SEEDS=1,2,3 P45_SEASONS=10 \
//!   /usr/bin/time -l cargo test --release -p season-harness --test p45_long_career -- --ignored --nocapture
//! ```
//!
//! (from `src-tauri/`; run the built test binary under `/usr/bin/time -l` for instructions and peak
//! memory, as `docs/APFM_F55_MARKET_PERFORMANCE.md` does.) `P45_INVARIANTS=0` skips the
//! rule-checking observer, which roughly doubles a run, for a world too large to check each day.
//!
//! Prints, for each seed: seconds per season, one row per season of what `career_study` reads
//! (squads, books, managers, set-up, pyramid, loans, prospects), the squad, finance and world-health
//! reports at the usual seasons, and the invariants' verdict. The second test plays a long career,
//! saves it through SQLite, reloads it, and plays the saved and the reloaded copies on side by side.

use std::time::Instant;

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use season_harness::career_study::{CareerLog, report as career_report};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::env;
use season_harness::finance_study::{FinanceLog, report as finance_report};
use season_harness::fingerprint::{fingerprint, first_difference};
use season_harness::invariants::Invariants;
use season_harness::squad_study::{SquadLog, report as squad_report, summary};
use season_harness::world_health::{WorldHealthLog, headline};

/// Seconds each season took, read at the rollover.
struct Pace {
    started: Instant,
    seasons: Vec<f64>,
}

impl SeasonObserver for Pace {
    fn on_before_rollover(&mut self, _game: &ofm_core::game::Game, _season: u32) {
        self.seasons.push(self.started.elapsed().as_secs_f64());
        self.started = Instant::now();
    }
}

#[test]
#[ignore = "long-career study"]
fn p45_long_career() {
    let seasons = env::first("P45_SEASONS", "10") as u32;
    let check = env::first("P45_INVARIANTS", "1") != 0;
    for seed in env::list("P45_SEEDS", "1") {
        let mut game = env::world("P45_WORLD", seed)
            .build()
            .expect("the world builds");
        let user = game.manager.team_id.clone();
        eprintln!(
            "== seed {seed}: {} clubs, {} players, {seasons} seasons",
            game.teams.len(),
            game.players.len()
        );
        let mut squads = SquadLog::starting_from(&game);
        let mut finance = FinanceLog::starting_from(&game);
        let mut health = WorldHealthLog::starting_from(&game);
        let mut career = CareerLog::starting_from(&game);
        let mut pace = Pace {
            started: Instant::now(),
            seasons: Vec::new(),
        };
        let mut rules = Invariants::new(&game);
        ofm_core::transfers::enable_deal_log();
        let began = Instant::now();
        let mut watched = (
            &mut squads,
            (&mut finance, (&mut health, (&mut career, &mut pace))),
        );
        let outcome = if check {
            run_seasons(
                &mut game,
                RunOptions::seasons(seasons),
                &mut (&mut rules, &mut watched),
            )
        } else {
            run_seasons(&mut game, RunOptions::seasons(seasons), &mut watched)
        };
        let stalled = outcome.as_ref().err().map(|error| error.to_string());
        eprintln!(
            "== seed {seed}: {} seasons in {:.1}s ({})",
            pace.seasons.len(),
            began.elapsed().as_secs_f64(),
            stalled.as_deref().unwrap_or("completed")
        );
        eprintln!(
            "seconds per season: {}",
            pace.seasons
                .iter()
                .map(|s| format!("{s:.1}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        eprintln!("{}", career_report(&career));
        let wanted: Vec<usize> = [1usize, 2, 3, 5, 10, 15, 20, 25, 30]
            .into_iter()
            .filter(|s| *s <= pace.seasons.len())
            .collect();
        eprintln!("{}", squad_report(&squads, &wanted));
        eprintln!("{}", summary(&squads));
        let finance_wanted: Vec<usize> = [0usize, 1, 2, 5, 10, 15, 20, 25, 30]
            .into_iter()
            .filter(|s| *s <= pace.seasons.len())
            .collect();
        eprintln!(
            "{}",
            finance_report(&finance, user.as_deref(), &finance_wanted)
        );
        for snapshot in health
            .snapshots
            .iter()
            .filter(|s| finance_wanted.contains(&(s.seasons_done as usize)))
        {
            eprintln!("{}", headline(snapshot));
        }
        if check {
            eprintln!(
                "invariants: {}",
                if rules.is_clean() {
                    "clean".to_string()
                } else {
                    rules.report()
                }
            );
        }
        assert!(stalled.is_none(), "the run stopped: {stalled:?}");
        eprintln!("FINISHED seed {seed}");
    }
}

/// Given a long career saved to SQLite and reloaded through a fresh manager
/// When the saved copy and the reloaded copy are both played on for sixty days
/// Then they are the same world: a save made many seasons in loses nothing a day's dice decide.
#[test]
#[ignore = "long-career study"]
fn p45_reload_twin_after_a_long_career() {
    let seasons = env::first("P45_SEASONS", "10") as u32;
    for seed in env::list("P45_SEEDS", "1") {
        let mut original = env::world("P45_WORLD", seed)
            .build()
            .expect("the world builds");
        let mut rules = Invariants::new(&original);
        run_seasons(&mut original, RunOptions::seasons(seasons), &mut rules).expect("plays");
        let before = fingerprint(&original);

        let dir = tempfile::tempdir().expect("a directory");
        let mut manager = SaveManager::init(dir.path()).expect("a save manager");
        let id = manager
            .create_save_with_stats(&original, &StatsState::default(), "long")
            .expect("the save");
        let mut twin = SaveManager::init(dir.path())
            .expect("a fresh manager")
            .load_game(&id)
            .expect("the load");
        let after = fingerprint(&twin);
        assert!(
            before == after,
            "seed {seed}: the reloaded world differs after {seasons} seasons: {}",
            first_difference(&before, &after)
        );

        let options = RunOptions::seasons(1);
        for day in 1..=60 {
            play_day(&mut original, options, seasons + 1, day, &mut rules).expect("plays");
            play_day(&mut twin, options, seasons + 1, day, &mut rules).expect("plays");
        }
        let (left, right) = (fingerprint(&original), fingerprint(&twin));
        assert!(
            left == right,
            "seed {seed}: the reloaded career diverged within sixty days: {}",
            first_difference(&left, &right)
        );
        eprintln!(
            "seed {seed}: reload after {seasons} seasons identical, and identical 60 days on; \
             invariants {}",
            if rules.is_clean() {
                "clean"
            } else {
                "VIOLATED"
            }
        );
    }
}
