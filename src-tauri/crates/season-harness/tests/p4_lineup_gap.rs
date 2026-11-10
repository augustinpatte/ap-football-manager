//! P4 Phase A: how far is the squad model's optimal side from the side the lineup picker fields?
//! Measurement only (`#[ignore]`d).
//!
//! `P4_WORLD=gate|shipped P4_SEEDS=1,2,3 P4_SEASONS=2 cargo test --release -p season-harness
//! --test p4_lineup_gap -- --ignored --nocapture` (from `src-tauri/`).
//!
//! Every AI club is read at the opening and on day 100 of each season (squads settled after the
//! window), from the same healthy, non-loaned senior pool: the optimal assignment the market values
//! signings by, against the greedy slot walk the AI manager actually plays. Strength is the game's
//! raw slot rating, so neither side's rounding can bias it.

use ofm_core::game::Game;
use ofm_core::{LineupGap, lineup_gaps};
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

fn env_list(name: &str, default: &str) -> Vec<u64> {
    std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

struct Sample {
    label: String,
    gaps: Vec<LineupGap>,
    clubs: usize,
}

#[derive(Default)]
struct GapLog {
    samples: Vec<Sample>,
}

fn anchor_offsets(game: &Game) -> String {
    let offsets: Vec<f64> = game
        .players
        .iter()
        .filter(|p| p.team_id.is_some() && p.ovr > 0)
        .map(|p| f64::from(p.ovr) - ofm_core::player_rating::natural_ovr(p))
        .collect();
    let n = offsets.len().max(1) as f64;
    let mean_abs = offsets.iter().map(|o| o.abs()).sum::<f64>() / n;
    let over = offsets.iter().filter(|o| o.abs() > 0.5).count() as f64 / n * 100.0;
    format!(
        "stored ovr - natural_ovr over {} rostered players: mean |offset| {mean_abs:.2}, {over:.1}% beyond rounding",
        offsets.len()
    )
}

impl GapLog {
    fn take(&mut self, game: &Game, label: String) {
        eprintln!("[{label}] {}", anchor_offsets(game));
        let user = game.manager.team_id.as_deref();
        let gaps: Vec<LineupGap> = lineup_gaps(game)
            .into_iter()
            .filter(|gap| Some(gap.team_id.as_str()) != user)
            .collect();
        let clubs = game.teams.len() - usize::from(user.is_some());
        self.samples.push(Sample { label, gaps, clubs });
    }
}

impl SeasonObserver for GapLog {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        if day == 100 {
            self.take(game, format!("season {season} day 100"));
        }
    }
}

/// The lineup gain over the old slot walk, by the club's own strength: is it spread evenly?
fn gain_by_strength(sample: &Sample) -> String {
    let mut gaps: Vec<&LineupGap> = sample.gaps.iter().collect();
    gaps.sort_by(|a, b| a.picker_raw.total_cmp(&b.picker_raw));
    let quarter = (gaps.len() / 4).max(1);
    let gains: Vec<String> = gaps
        .chunks(quarter)
        .take(4)
        .map(|chunk| {
            let gain = chunk
                .iter()
                .map(|g| f64::from(g.picker_raw - g.legacy_picker_raw))
                .sum::<f64>()
                / chunk.len() as f64;
            format!("{gain:+.3}")
        })
        .collect();
    format!(
        "{:<18} lineup gain over the slot walk by club strength (weakest..strongest quarter): {}",
        "",
        gains.join(" ")
    )
}

fn describe(sample: &Sample) -> String {
    let n = sample.gaps.len().max(1) as f64;
    let mean = |f: &dyn Fn(&LineupGap) -> f64| sample.gaps.iter().map(f).sum::<f64>() / n;
    let share = |f: &dyn Fn(&LineupGap) -> bool| {
        100.0 * sample.gaps.iter().filter(|g| f(g)).count() as f64 / n
    };
    let mut losses: Vec<f64> = sample
        .gaps
        .iter()
        .map(|g| f64::from(g.optimal_raw - g.picker_raw))
        .collect();
    losses.sort_by(f64::total_cmp);
    let pct = |p: f64| {
        losses
            .get(((losses.len().max(1) - 1) as f64 * p).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    format!(
        "{:<18} clubs read {:>3}/{:<3} | picker's side is not the optimal side {:>5.1}% | a model \
         starter is left out {:>5.1}% (mean {:.2}) | moved {:.2}\n{:<18} raw rating per slot, \
         optimal - picker: mean {:.3} p50 {:.3} p90 {:.3} max {:.3} | optimal - mirror {:.3} | \
         modelled optimal - picker {:.3}\n{:<18} loaned-in healthy per club {:.2} | clubs fielding a \
         loaned-in starter {:>5.1}% | mirror disagrees with the real \
         picker in {:.2} slots",
        sample.label,
        sample.gaps.len(),
        sample.clubs,
        share(&|g| g.changed_starters > 0 || g.moved_starters > 0),
        share(&|g| g.changed_starters > 0),
        mean(&|g| f64::from(g.changed_starters)),
        mean(&|g| f64::from(g.moved_starters)),
        "",
        mean(&|g| f64::from(g.optimal_raw - g.picker_raw)),
        pct(0.5),
        pct(0.9),
        losses.last().copied().unwrap_or(0.0),
        mean(&|g| f64::from(g.optimal_raw - g.mirror_raw)),
        mean(&|g| f64::from(g.optimal_modelled - g.picker_modelled)),
        "",
        mean(&|g| g.loaned_in as f64),
        share(&|g| g.loaned_in_starters > 0),
        mean(&|g| f64::from(g.mirror_disagreements)),
    )
}

#[test]
#[ignore = "measurement study"]
fn p4_model_versus_picker() {
    let seasons = env_list("P4_SEASONS", "2")[0] as u32;
    for seed in env_list("P4_SEEDS", "1") {
        let spec = match std::env::var("P4_WORLD").as_deref() {
            Ok("shipped") => WorldSpec::shipped(seed),
            _ => WorldSpec::gate(seed),
        };
        let mut game = spec.build().expect("the world builds");
        let mut log = GapLog::default();
        log.take(&game, "opening".to_string());
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        eprintln!("== seed {seed}");
        for sample in &log.samples {
            eprintln!("{}", describe(sample));
            eprintln!("{}", gain_by_strength(sample));
        }
    }
}
