//! Run F5.1: thirty seasons of the world, measured. Measurement only (`#[ignore]`d).
//!
//! `F51_SEEDS=1,2,3 F51_SEASONS=30 cargo test --release -p season-harness --test f51_world_health -- --ignored --nocapture`
//! (from `src-tauri/`, so the local M12 override applies).

use season_harness::driver::{RunOptions, run_seasons};
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
fn f51_world_health_over_many_seasons() {
    let seasons = env_list("F51_SEASONS", "30")[0] as u32;
    for seed in env_list("F51_SEEDS", "1") {
        let mut game = WorldSpec::gate(seed).build().expect("the world builds");
        let mut log = WorldHealthLog::starting_from(&game);
        let started = std::time::Instant::now();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        eprintln!(
            "== seed {seed}: {seasons} seasons in {:.0?}",
            started.elapsed()
        );
        for s in &log.snapshots {
            eprintln!("{}", headline(s));
        }
        let last = log.last();
        eprintln!("   median by country (last): {:?}", last.median_by_country);
        let first = &log.snapshots[0];
        eprintln!(
            "   median by country (first): {:?}",
            first.median_by_country
        );
        for s in log.snapshots.iter().skip(1) {
            eprintln!(
                "   s{:>2} arrivals {:>4} (pot mean {:>4.1} p90 {:>4.1} 80+ {:>2}) departures {:>4} retired(mean age {:>4.1}, 70+ {:>4.1})",
                s.seasons_done,
                s.arrivals,
                s.arrivals_potential_mean,
                s.arrivals_potential_p90,
                s.arrivals_potential_80_plus,
                s.departures,
                s.mean_retirement_age,
                s.mean_retirement_age_good
            );
        }
        for s in log
            .snapshots
            .iter()
            .filter(|s| matches!(s.seasons_done, 0 | 1 | 5 | 10 | 20 | 30))
        {
            eprintln!(
                "   s{:>2} age bands {:?} | cash p10/p50/p90 {:.1}/{:.1}/{:.1}M min {:.1}M | deepDebt {} | wk wages p50 {:.2}M over-budget {} | rep p10/p50/p90 {:.0}/{:.0}/{:.0} | paid {} free {} rel {} fees {:.0}M renew {}",
                s.seasons_done,
                s.age_bands.map(|b| (b * 100.0).round()),
                s.cash.p10 / 1e6,
                s.cash.p50 / 1e6,
                s.cash.p90 / 1e6,
                s.cash.min / 1e6,
                s.clubs_deep_debt,
                s.weekly_wage_bill.p50 / 1e6,
                s.wage_over_budget_clubs,
                s.reputation.p10,
                s.reputation.p50,
                s.reputation.p90,
                s.paid_transfers_total,
                s.free_signings_total,
                s.releases_total,
                s.fees_paid_total as f64 / 1e6,
                s.renewals_total,
            );
        }
    }
}
