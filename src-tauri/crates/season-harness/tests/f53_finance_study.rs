//! Run F5.3: thirty seasons of the world's money, by club standing. Measurement only (`#[ignore]`d).
//!
//! `F53_WORLD=gate|lower|shipped F53_SEEDS=1,2,3 F53_SEASONS=30 cargo test --release -p season-harness --test f53_finance_study -- --ignored --nocapture`
//! (from `src-tauri/`, so the local M12 override applies). The same seeds give the same world, so a
//! before/after pair is a like-for-like comparison.

use season_harness::driver::{RunOptions, run_seasons};
use season_harness::finance_study::{FinanceLog, report};
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
fn f53_finances_by_standing() {
    let seasons = env_list("F53_SEASONS", "30")[0] as u32;
    for seed in env_list("F53_SEEDS", "1") {
        let spec = match std::env::var("F53_WORLD").as_deref() {
            Ok("shipped") => WorldSpec::shipped(seed),
            Ok("lower") => WorldSpec::lower_nations(seed),
            _ => WorldSpec::gate(seed),
        };
        let mut game = spec.build().expect("the world builds");
        let user = game.manager.team_id.clone();
        eprintln!("== seed {seed}: {} clubs", game.teams.len());
        let mut finance = FinanceLog::starting_from(&game);
        let mut health = WorldHealthLog::starting_from(&game);
        let started = std::time::Instant::now();
        run_seasons(
            &mut game,
            RunOptions::seasons(seasons),
            &mut (&mut finance, &mut health),
        )
        .expect("seasons play");
        eprintln!(
            "== seed {seed}: {seasons} seasons in {:.0?}",
            started.elapsed()
        );
        let wanted: Vec<usize> = [0usize, 1, 5, 10, 15, 20, 25, 30]
            .into_iter()
            .filter(|s| *s <= seasons as usize)
            .collect();
        eprintln!("{}", report(&finance, user.as_deref(), &wanted));
        for s in health
            .snapshots
            .iter()
            .filter(|s| wanted.contains(&(s.seasons_done as usize)))
        {
            eprintln!("{}", headline(s));
        }
    }
}
