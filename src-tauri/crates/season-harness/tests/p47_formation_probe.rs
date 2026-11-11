//! P4.7: what the formation review sees in a real world. Measurement only (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=3 cargo test --release -p season-harness --test p47_formation_probe -- --ignored --nocapture`
//!
//! At the opening and after each season, for every AI club: the spread of the squad score across the
//! eight formations, the spread of taste, the margin, and how many clubs the review would move.

use ofm_core::ai_formation::read_all_clubs;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

#[derive(Default)]
struct Probe {
    label: String,
}

fn report(game: &Game, label: &str) {
    let started = std::time::Instant::now();
    let readings = read_all_clubs(game);
    eprintln!(
        "   read_all_clubs: {} clubs x 8 formations in {:.1} ms ({:.0} us per model build)",
        readings.len(),
        started.elapsed().as_secs_f64() * 1e3,
        started.elapsed().as_secs_f64() * 1e6 / (readings.len().max(1) as f64 * 8.0)
    );
    let n = readings.len() as f64;
    let mut squad_spread = Vec::new();
    let mut taste_spread = Vec::new();
    let mut gain_over_current = Vec::new();
    let mut margins = Vec::new();
    let mut moves = 0;
    let mut best_squad_is_current = 0;
    for reading in &readings {
        let squad: Vec<f64> = reading.scores.iter().map(|s| s.squad).collect();
        let taste: Vec<f64> = reading.scores.iter().map(|s| s.taste).collect();
        let span = |v: &[f64]| {
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        squad_spread.push(span(&squad));
        taste_spread.push(span(&taste));
        let current = reading
            .scores
            .iter()
            .find(|s| s.formation == reading.current)
            .map(|s| s.total());
        if let Some(current) = current {
            let best = reading
                .scores
                .iter()
                .map(|s| s.total())
                .fold(f64::MIN, f64::max);
            gain_over_current.push(best - current);
        }
        let best_squad = reading
            .scores
            .iter()
            .max_by(|a, b| a.squad.total_cmp(&b.squad))
            .map(|s| s.formation);
        if best_squad == Some(reading.current.as_str()) {
            best_squad_is_current += 1;
        }
        margins.push(reading.margin);
        if reading.would_move_to.is_some() {
            moves += 1;
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let pct = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(|a, b| a.total_cmp(b));
        v.get(((v.len() as f64 - 1.0) * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let mut gains = gain_over_current.clone();
    eprintln!(
        "{label}: clubs {n} | squad spread mean {:.2} | taste spread mean {:.2} | margin {:.2} | gain over current p25/50/75/90 {:.2}/{:.2}/{:.2}/{:.2} | squad-best is current {best_squad_is_current} | would move {moves}",
        mean(&squad_spread),
        mean(&taste_spread),
        mean(&margins),
        pct(&mut gains, 0.25),
        pct(&mut gains, 0.5),
        pct(&mut gains, 0.75),
        pct(&mut gains, 0.9),
    );
    let mut best_counts = std::collections::BTreeMap::new();
    for reading in &readings {
        let best = reading
            .scores
            .iter()
            .max_by(|a, b| a.total().total_cmp(&b.total()))
            .map(|s| s.formation)
            .unwrap_or("?");
        *best_counts.entry(best).or_insert(0) += 1;
    }
    eprintln!("   best total formation across clubs: {best_counts:?}");
}

impl SeasonObserver for Probe {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        report(game, &format!("{} end of season {season}", self.label));
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_formation_probe() {
    let seasons = env::first("P47_SEASONS", "3") as u32;
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        report(&game, &format!("seed {seed} opening"));
        let mut probe = Probe {
            label: format!("seed {seed}"),
        };
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut probe).expect("seasons play");
    }
}
