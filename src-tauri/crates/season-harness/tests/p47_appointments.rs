//! P4.7: who do clubs appoint, and how long do they stay? Measurement only (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test p47_appointments -- --ignored --nocapture`
//!
//! Every appointment of a permanent manager is recorded with how badly his identity suits the club
//! (0 a perfect fit, 1 the opposite; `ofm_core::ai_manager_appointments::appointment_mismatch`),
//! against the mean over every unemployed manager and that club on the same day, which is what a
//! club choosing by stature alone would get on average. A tenure is the days from appointment to
//! the day the club next has a different permanent manager.

use std::collections::HashMap;

use ofm_core::ai_manager_appointments::appointment_mismatch;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

#[derive(Default)]
struct Log {
    manager_of: HashMap<String, String>,
    since: HashMap<String, u32>,
    day: u32,
    chosen: Vec<f64>,
    pool_mean: Vec<f64>,
    tenures: Vec<u32>,
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        self.day = season * 450 + day;
        let permanent: std::collections::HashSet<&str> = game
            .managers
            .iter()
            .filter(|m| !m.interim)
            .map(|m| m.id.as_str())
            .collect();
        let user = game.manager.team_id.as_deref();
        for team in &game.teams {
            if Some(team.id.as_str()) == user {
                continue;
            }
            let now = team
                .manager_id
                .clone()
                .filter(|id| permanent.contains(id.as_str()));
            let before = self.manager_of.get(&team.id).cloned();
            if now != before {
                if let (Some(_), Some(since)) = (&before, self.since.get(&team.id)) {
                    self.tenures.push(self.day - since);
                }
                if let Some(manager) = &now {
                    if let Some(fit) = appointment_mismatch(game, &team.id, manager) {
                        let pool: Vec<f64> = game
                            .managers
                            .iter()
                            .filter(|m| m.team_id.is_none() && !m.interim)
                            .filter_map(|m| appointment_mismatch(game, &team.id, &m.id))
                            .collect();
                        // The pool before he left it also held him; only count a real pool.
                        if pool.len() >= 3 {
                            self.chosen.push(fit);
                            self.pool_mean
                                .push(pool.iter().sum::<f64>() / pool.len() as f64);
                        }
                    }
                    self.since.insert(team.id.clone(), self.day);
                    self.manager_of.insert(team.id.clone(), manager.clone());
                } else {
                    self.manager_of.remove(&team.id);
                }
            }
        }
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_appointments() {
    let seasons = env::first("P47_SEASONS", "10") as u32;
    let mut all = Log::default();
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let mut log = Log::default();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("plays");
        all.chosen.extend(log.chosen);
        all.pool_mean.extend(log.pool_mean);
        all.tenures.extend(log.tenures);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let n = all.chosen.len();
    let diffs: Vec<f64> = all
        .chosen
        .iter()
        .zip(&all.pool_mean)
        .map(|(c, p)| c - p)
        .collect();
    let sd = (diffs
        .iter()
        .map(|d| (d - mean(&diffs)).powi(2))
        .sum::<f64>()
        / n.max(1) as f64)
        .sqrt();
    eprintln!(
        "appointments {n} | mismatch of the manager appointed {:.3} vs the pool's mean {:.3} | difference {:+.3} (se {:.3})",
        mean(&all.chosen),
        mean(&all.pool_mean),
        mean(&diffs),
        sd / (n.max(1) as f64).sqrt()
    );
    let tenures: Vec<f64> = all.tenures.iter().map(|t| f64::from(*t)).collect();
    let mut sorted = all.tenures.clone();
    sorted.sort_unstable();
    eprintln!(
        "completed tenures {} | mean {:.0} days | median {} days",
        tenures.len(),
        mean(&tenures),
        sorted.get(sorted.len() / 2).copied().unwrap_or(0)
    );
}
