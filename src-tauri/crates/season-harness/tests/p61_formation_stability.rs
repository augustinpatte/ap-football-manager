//! P6.1: how steady is a formation rule? With every club's formation held, the weekly review's
//! scores are read for a season's worth of weeks, and for each rule (the squad model's score, the
//! engine's fit) the study counts how often a club's best formation changes from one week to the
//! next, how far the best is above the current one, and how often that gap clears the margin a
//! move needs. A rule that flips its mind weekly will have clubs changing shape all season.
//!
//! `P6_SEEDS=1,2,3 P61_WEEKS=17 cargo test --release -p season-harness --test p61_formation_stability -- --ignored --nocapture`

use std::collections::HashMap;

use ofm_core::ai_formation::{ClubFormationReading, freeze_reviews_for_study, read_all_clubs};
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::env;
use season_harness::match_study::mean_se;

struct Quiet;
impl SeasonObserver for Quiet {}

/// The index of the formation with the best total under `total`, ties to the earlier.
fn best(
    reading: &ClubFormationReading,
    total: fn(&ofm_core::ai_formation::FormationScore) -> f64,
) -> usize {
    let mut best = 0;
    for (index, score) in reading.scores.iter().enumerate() {
        if total(score) > total(&reading.scores[best]) {
            best = index;
        }
    }
    best
}

/// How far the best formation is above the club's current one, in the rule's own points.
fn gap(
    reading: &ClubFormationReading,
    total: fn(&ofm_core::ai_formation::FormationScore) -> f64,
) -> f64 {
    let top = reading
        .scores
        .iter()
        .map(total)
        .fold(f64::NEG_INFINITY, f64::max);
    let current = reading
        .scores
        .iter()
        .find(|s| s.formation == reading.current)
        .map_or(top, total);
    top - current
}

#[test]
#[ignore = "measurement study"]
fn p61_formation_stability() {
    let weeks = env::first("P61_WEEKS", "17");
    type Total = fn(&ofm_core::ai_formation::FormationScore) -> f64;
    let rules: [(&str, Total); 2] = [
        ("squad model + taste", |s| s.squad + s.taste),
        ("engine fit + taste", |s| s.fit + s.taste),
    ];
    // Per rule: weekly flips, gaps, and club-weeks clearing the margin.
    let mut flips: Vec<Vec<f64>> = vec![Vec::new(); rules.len()];
    let mut gaps: Vec<Vec<f64>> = vec![Vec::new(); rules.len()];
    let mut clears: Vec<Vec<f64>> = vec![Vec::new(); rules.len()];
    let mut margins: Vec<f64> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        let _frozen = freeze_reviews_for_study();
        for day in 1..=60u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let mut previous: HashMap<(usize, String), usize> = HashMap::new();
        for week in 0..weeks {
            for day in 0..7u32 {
                let d = 61 + week as u32 * 7 + day;
                play_day(&mut game, RunOptions::seasons(1), 1, d, &mut Quiet)
                    .expect("the day plays");
            }
            for reading in read_all_clubs(&game) {
                margins.push(reading.margin);
                for (r, (_, total)) in rules.iter().enumerate() {
                    let top = best(&reading, *total);
                    if let Some(before) = previous.insert((r, reading.team_id.clone()), top) {
                        flips[r].push(f64::from(u8::from(before != top)));
                    }
                    let g = gap(&reading, *total);
                    gaps[r].push(g);
                    clears[r].push(f64::from(u8::from(g > reading.margin)));
                }
            }
        }
    }
    let (margin, _) = mean_se(&margins);
    eprintln!("\n== weekly formation reviews, formations held, mean margin {margin:.3} ==");
    for (r, (name, _)) in rules.iter().enumerate() {
        let (flip, _) = mean_se(&flips[r]);
        let (g, _) = mean_se(&gaps[r]);
        let (clear, _) = mean_se(&clears[r]);
        let mut sorted = gaps[r].clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        let p90 = sorted[sorted.len() * 9 / 10];
        eprintln!(
            "{name:<22} best shape changes week to week {:>5.1} % | gap best - current: mean {g:.2}, median {median:.2}, p90 {p90:.2} | gap clears the margin {:>5.1} % of club-weeks",
            flip * 100.0,
            clear * 100.0
        );
    }
}
