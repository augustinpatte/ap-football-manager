//! P4 Phase C: do manager dispositions change who wins? Measurement only (`#[ignore]`d).
//!
//! `P4_SEEDS=1,2,3 P4_SEASONS=2 cargo test --release -p season-harness --test p4_results --
//! --ignored --nocapture` (from `src-tauri/`).
//!
//! At every season's end, for each AI club in a league: points per game, the strength of its best
//! fourteen, and the dispositions of its manager. Points per game are compared with what the club's
//! strength predicts (a straight-line fit within its league), and the residual is averaged by
//! disposition band. A band that wins beyond its strength is a disposition that dominates the others.
//!
//! The dispositions are re-derived here from the manager id with a copy of the derivation, so the
//! same file also runs on the tree before identities existed, where it is a placebo: nothing there
//! reads them, so every band must sit on zero. League scoring and the share of home wins and draws
//! are reported for the same runs.

use std::collections::HashMap;

use domain::league::CompetitionType;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

fn draw(manager_id: &str, salt: u64) -> f64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ salt;
    for byte in manager_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^= hash >> 33;
    (hash >> 11) as f64 / (1u64 << 53) as f64
}

const RISK: u64 = 0x6d67_7220_7269_736b;
const ADAPTABILITY: u64 = 0x6d67_7220_6164_6170;
const ROTATION: u64 = 0x6d67_7220_726f_7461;

struct Row {
    residual: f64,
    risk: f64,
    adaptability: f64,
    rotation: f64,
}

#[derive(Default)]
struct Log {
    rows: Vec<Row>,
    goals: f64,
    matches: f64,
    home_wins: f64,
    draws: f64,
}

impl SeasonObserver for Log {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let user = game.manager.team_id.as_deref();
        let mut by_club: HashMap<&str, Vec<u8>> = HashMap::new();
        for player in &game.players {
            if let Some(team) = player.team_id.as_deref() {
                by_club.entry(team).or_default().push(player.ovr);
            }
        }
        for league in &game.competitions {
            if league.kind != CompetitionType::League || league.standings.len() < 8 {
                continue;
            }
            let mut points = Vec::new();
            for entry in &league.standings {
                self.goals += f64::from(entry.goals_for);
                self.matches += f64::from(entry.played) / 2.0;
                self.draws += f64::from(entry.drawn) / 2.0;
                let Some(team) = game.teams.iter().find(|t| t.id == entry.team_id) else {
                    continue;
                };
                let mut ovr = by_club
                    .get(entry.team_id.as_str())
                    .cloned()
                    .unwrap_or_default();
                ovr.sort_unstable_by(|a, b| b.cmp(a));
                ovr.truncate(14);
                if ovr.is_empty() || entry.played == 0 || Some(entry.team_id.as_str()) == user {
                    continue;
                }
                let strength = ovr.iter().map(|o| f64::from(*o)).sum::<f64>() / ovr.len() as f64;
                let Some(manager_id) = team.manager_id.as_deref() else {
                    continue;
                };
                points.push((
                    f64::from(entry.points) / f64::from(entry.played),
                    strength,
                    manager_id.to_string(),
                ));
            }
            let n = points.len() as f64;
            let (mean_p, mean_s) = (
                points.iter().map(|p| p.0).sum::<f64>() / n,
                points.iter().map(|p| p.1).sum::<f64>() / n,
            );
            let var_s = points.iter().map(|p| (p.1 - mean_s).powi(2)).sum::<f64>();
            let cov = points
                .iter()
                .map(|p| (p.1 - mean_s) * (p.0 - mean_p))
                .sum::<f64>();
            let slope = if var_s > 0.0 { cov / var_s } else { 0.0 };
            for (ppg, strength, manager_id) in points {
                self.rows.push(Row {
                    residual: ppg - (mean_p + slope * (strength - mean_s)),
                    risk: draw(&manager_id, RISK),
                    adaptability: draw(&manager_id, ADAPTABILITY),
                    rotation: draw(&manager_id, ROTATION),
                });
            }
        }
        for league in &game.competitions {
            if league.kind != CompetitionType::League {
                continue;
            }
            for fixture in &league.fixtures {
                if let Some(result) = &fixture.result
                    && result.home_goals > result.away_goals
                {
                    self.home_wins += 1.0;
                }
            }
        }
    }
}

fn band(rows: &[Row], value: &dyn Fn(&Row) -> f64, low: bool) -> String {
    let picked: Vec<f64> = rows
        .iter()
        .filter(|r| {
            if low {
                value(r) <= 0.25
            } else {
                value(r) >= 0.75
            }
        })
        .map(|r| r.residual)
        .collect();
    let n = picked.len() as f64;
    let mean = picked.iter().sum::<f64>() / n;
    let sd = (picked.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n).sqrt();
    format!("{mean:+.3} (+-{:.3}, n {})", sd / n.sqrt(), picked.len())
}

#[test]
#[ignore = "measurement study"]
fn p4_results_by_disposition() {
    let list = |name: &str, default: &str| -> Vec<u64> {
        std::env::var(name)
            .unwrap_or_else(|_| default.to_string())
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    };
    let seasons = list("P4_SEASONS", "2")[0] as u32;
    let mut log = Log::default();
    for seed in list("P4_SEEDS", "1") {
        let mut game = WorldSpec::gate(seed).build().expect("the world builds");
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
    }
    eprintln!(
        "club-seasons {} | goals per match {:.3} | home win share {:.1}% | draw share {:.1}%",
        log.rows.len(),
        log.goals / log.matches,
        100.0 * log.home_wins / log.matches,
        100.0 * log.draws / log.matches
    );
    eprintln!("points per game beyond what strength predicts, by band (mean, standard error):");
    for (name, f) in [
        ("risk", (&|r: &Row| r.risk) as &dyn Fn(&Row) -> f64),
        ("adaptability", &|r: &Row| r.adaptability),
        ("rotation", &|r: &Row| r.rotation),
    ] {
        eprintln!(
            "  {name:<13} bold/high {} | cautious/low {}",
            band(&log.rows, f, false),
            band(&log.rows, f, true)
        );
    }
}
