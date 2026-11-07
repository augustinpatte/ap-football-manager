//! Run F5.4: thirty seasons of the AI clubs' squads. Measurement only (`#[ignore]`d).
//!
//! `F54_WORLD=gate|lower|shipped F54_SEEDS=1,2,3 F54_SEASONS=30 cargo test --release -p season-harness --test f54_squad_study -- --ignored --nocapture`
//! (from `src-tauri/`, so the local M12 override applies). It reads only what the game itself
//! defines, so the same file runs unchanged on the F5.3 tree for the before figures.

use season_harness::driver::{RunOptions, run_seasons};
use season_harness::finance_study::{FinanceLog, report as finance_report};
use season_harness::squad_study::{SquadLog, report, summary};
use season_harness::world_health::{WorldHealthLog, headline};
use season_harness::worlds::WorldSpec;

fn env_list(name: &str, default: &str) -> Vec<u64> {
    std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

#[test]
#[ignore = "measurement study"]
fn f54_squads_by_standing() {
    let seasons = env_list("F54_SEASONS", "30")[0] as u32;
    for seed in env_list("F54_SEEDS", "1") {
        let spec = match std::env::var("F54_WORLD").as_deref() {
            Ok("shipped") => WorldSpec::shipped(seed),
            Ok("lower") => WorldSpec::lower_nations(seed),
            _ => WorldSpec::gate(seed),
        };
        let mut game = spec.build().expect("the world builds");
        let user = game.manager.team_id.clone();
        eprintln!("== seed {seed}: {} clubs", game.teams.len());
        let mut squads = SquadLog::starting_from(&game);
        let mut finance = FinanceLog::starting_from(&game);
        let mut health = WorldHealthLog::starting_from(&game);
        let buyers_before = ofm_core::transfers::market_buyer_evaluations();
        let first_date = game.clock.current_date.date_naive();
        let started = std::time::Instant::now();
        run_seasons(
            &mut game,
            RunOptions::seasons(seasons),
            &mut (&mut squads, (&mut finance, &mut health)),
        )
        .expect("seasons play");
        eprintln!(
            "== seed {seed}: {seasons} seasons in {:.3}s",
            started.elapsed().as_secs_f64()
        );
        eprintln!(
            "== seed {seed}: {} market buyer evaluations over {} calendar days",
            ofm_core::transfers::market_buyer_evaluations() - buyers_before,
            (game.clock.current_date.date_naive() - first_date).num_days()
        );
        let wanted: Vec<usize> = [1usize, 2, 3, 5, 10, 15, 20, 25, 30]
            .into_iter()
            .filter(|s| *s <= seasons as usize)
            .collect();
        eprintln!("{}", report(&squads, &wanted));
        eprintln!("{}", summary(&squads));
        let finance_wanted: Vec<usize> = [0usize, 1, 5, 10, 20, 30]
            .into_iter()
            .filter(|s| *s <= seasons as usize)
            .collect();
        eprintln!(
            "{}",
            finance_report(&finance, user.as_deref(), &finance_wanted)
        );
        for s in health
            .snapshots
            .iter()
            .filter(|s| finance_wanted.contains(&(s.seasons_done as usize)))
        {
            eprintln!("{}", headline(s));
        }
        eprintln!("FINISHED seed {seed}");
    }
}
