//! P4 Phase B: do AI clubs keep a recognisable strategy, and does it move when the club does?
//! Measurement only (`#[ignore]`d).
//!
//! `P4_SEEDS=1,2,3 P4_SEASONS=3 cargo test --release -p season-harness --test p4_strategy_stability
//! -- --ignored --nocapture` (from `src-tauri/`).
//!
//! A club's profile (ambition, means, youth focus) is derived from state, never stored, so it
//! cannot drift by itself. What this asks is how often that derivation changes its answer between
//! samples taken every 50 days, and how often it changes and changes back (a flip-flop, which is
//! noise rather than a club changing).

use std::collections::HashMap;

use ofm_core::game::Game;
use ofm_core::transfers::strategy_reports;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

const SAMPLE_EVERY: u32 = 50;

#[derive(Clone, PartialEq)]
struct Reading {
    ambition: &'static str,
    means: &'static str,
    youth: f64,
}

#[derive(Default)]
struct Log {
    /// club -> readings in the order taken.
    history: HashMap<String, Vec<Reading>>,
    samples: usize,
}

impl Log {
    fn take(&mut self, game: &Game) {
        let user = game.manager.team_id.as_deref();
        for report in strategy_reports(game) {
            if Some(report.team_id.as_str()) == user {
                continue;
            }
            self.history
                .entry(report.team_id)
                .or_default()
                .push(Reading {
                    ambition: report.ambition,
                    means: report.means,
                    youth: report.youth_focus,
                });
        }
        self.samples += 1;
    }
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, _season: u32, day: u32) {
        if day.is_multiple_of(SAMPLE_EVERY) {
            self.take(game);
        }
    }
}

#[test]
#[ignore = "measurement study"]
fn p4_strategy_stability() {
    let seasons = env::first("P4_SEASONS", "3") as u32;
    for seed in env::list("P4_SEEDS", "1") {
        let mut game = env::world("P4_WORLD", seed)
            .build()
            .expect("the world builds");
        let mut log = Log::default();
        log.take(&game);
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");

        let clubs = log.history.len() as f64;
        let mut ambition_changes = 0usize;
        let mut means_changes = 0usize;
        let mut flip_flops = 0usize;
        let mut youth_moves = Vec::new();
        let mut ever_changed = 0usize;
        let mut distinct_ambitions = [0usize; 4];
        for readings in log.history.values() {
            let mut changed = false;
            for pair in readings.windows(2) {
                if pair[0].ambition != pair[1].ambition {
                    ambition_changes += 1;
                    changed = true;
                }
                if pair[0].means != pair[1].means {
                    means_changes += 1;
                    changed = true;
                }
                youth_moves.push((pair[1].youth - pair[0].youth).abs());
            }
            for triple in readings.windows(3) {
                let back = |f: &dyn Fn(&Reading) -> &'static str| {
                    f(&triple[0]) == f(&triple[2]) && f(&triple[0]) != f(&triple[1])
                };
                if back(&|r| r.ambition) || back(&|r| r.means) {
                    flip_flops += 1;
                }
            }
            if changed {
                ever_changed += 1;
            }
            let mut seen: Vec<&str> = readings.iter().map(|r| r.ambition).collect();
            seen.sort_unstable();
            seen.dedup();
            distinct_ambitions[seen.len().min(3)] += 1;
        }
        let transitions = (log.samples - 1) as f64 * clubs;
        youth_moves.sort_by(f64::total_cmp);
        let youth_p90 = youth_moves[(youth_moves.len() as f64 * 0.9) as usize];
        eprintln!(
            "== seed {seed}: {} clubs, {} samples ({} seasons, every {SAMPLE_EVERY} days)\n   \
             ambition changed in {:.2}% of club-intervals, means in {:.2}% | clubs whose ambition or \
             means ever changed {:.0}% | flip-flops (A,B,A) {} ({:.2}% of triples) | youth focus \
             move per interval p90 {:.3} | clubs that held {} / {} / {} ambitions",
            clubs as usize,
            log.samples,
            seasons,
            100.0 * ambition_changes as f64 / transitions,
            100.0 * means_changes as f64 / transitions,
            100.0 * ever_changed as f64 / clubs,
            flip_flops,
            100.0 * flip_flops as f64 / ((log.samples - 2) as f64 * clubs),
            youth_p90,
            distinct_ambitions[1],
            distinct_ambitions[2],
            distinct_ambitions[3],
        );
    }
}
