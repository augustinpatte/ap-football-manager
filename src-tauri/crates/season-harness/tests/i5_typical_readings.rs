//! I5: what a typical squad's eleven reads in each shape, as the engine now reads it.
//!
//! `ofm_core::formation_fit` scores a club's shape by the engine's own reading of its eleven: the
//! back line's hold, the midfield on the ball and the forwards' finishing. I5 gave the engine a
//! depth-aware reading of a group short of a full one (`engine::group_depth`), so those means are not
//! the plain means they were: a back three's hold is filled to four at the side's own standard, and a
//! lone forward's finishing is filled to two. This study measures the eight triples on the same
//! worlds and clubs P6.1 used.
//!
//! `P6_WORLD=gate P6_SEEDS=1,2,3,4,5,6,7,8,9,10,11,12 P61_CLUBS=16 cargo test --release -p season-harness --test i5_typical_readings -- --ignored --nocapture`
//!
//! The numbers are evidence for two claims in `docs/APFM_I5_VALIDATION.md`: that the level of a shape
//! is a real property of the engine and not of the eleven (so the fit reads the level), and that the
//! depth-aware reading moved the shapes whose group counts are short of a full group (a back three
//! and a one-forward shape) and no others. The uncertainty printed is the standard error over clubs.
//! The test asserts that a sample was actually read, so a zero-club run is not reported as a
//! measurement.

use ofm_core::ai_formation::{freeze_reviews_for_study, squads_by_ai_club};
use ofm_core::formation_fit::group_readings;
use ofm_core::player_rating::FORMATIONS;
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::env;

struct Quiet;
impl SeasonObserver for Quiet {}

fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (variance / n).sqrt())
}

/// One shape's readings over the sample: one value a club for each of the engine's three groups.
struct ShapeReadings {
    formation: &'static str,
    back_lines: Vec<f64>,
    midfields: Vec<f64>,
    finishings: Vec<f64>,
}

#[test]
#[ignore = "a measurement: see the module documentation for the command"]
fn what_a_typical_squad_reads_in_each_shape() {
    let seeds = env::list("P6_SEEDS", "1,2,3,4,5,6,7,8,9,10,11,12");
    let clubs_per_world = env::first("P61_CLUBS", "16") as usize;
    let mut sample: Vec<ShapeReadings> = FORMATIONS
        .iter()
        .map(|formation| ShapeReadings {
            formation,
            back_lines: Vec::new(),
            midfields: Vec::new(),
            finishings: Vec::new(),
        })
        .collect();
    for &seed in &seeds {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=env::first("P6_WARMUP", "60") as u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let _frozen = freeze_reviews_for_study();
        let squads = squads_by_ai_club(&game);
        let step = (squads.len() / clubs_per_world).max(1);
        for (_, squad) in squads.iter().step_by(step).take(clubs_per_world) {
            for shape in sample.iter_mut() {
                let readings = group_readings(squad, shape.formation);
                shape.back_lines.push(readings.back_line);
                shape.midfields.push(readings.midfield);
                shape.finishings.push(readings.finishing);
            }
        }
    }
    let mut clubs = 0usize;
    println!(
        "{:<10}{:>18}{:>18}{:>18}",
        "shape", "back line", "midfield", "finishing"
    );
    for shape in &sample {
        let (back_line, back_se) = mean_se(&shape.back_lines);
        let (midfield, mid_se) = mean_se(&shape.midfields);
        let (finishing, finish_se) = mean_se(&shape.finishings);
        clubs = clubs.max(shape.back_lines.len());
        println!(
            "{:<10}{back_line:>10.3} ({back_se:.2}){midfield:>10.3} ({mid_se:.2}){finishing:>10.3} ({finish_se:.2})",
            shape.formation
        );
    }
    println!("{clubs} clubs a shape");
    assert!(clubs >= 8, "only {clubs} clubs read: not a sample");
}
