//! EUROPE4 draw and schedule study: how reliably and how fast the league-phase draw is made.
//!
//! For each competition profile and each association distribution, every seed in a range is drawn
//! and put on matchdays, and each draw is checked by the independent `draw::verify`. Prints the
//! success rate, the restarts the solver needed, the home/away pattern violations left and the
//! slowest draw. Run in release mode:
//!
//! `cargo run --release -p season-harness --example europe4_study -- 1000`

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ofm_core::league_phase::draw::{DrawInput, draw, verify};
use ofm_core::league_phase::matchdays::schedule;
use ofm_core::league_phase::profiles;

/// 36 clubs dealt to associations of the given sizes round-robin, strongest first.
fn field(pot_size: usize, sizes: &[usize]) -> (Vec<Vec<String>>, BTreeMap<String, String>) {
    let mut left = sizes.to_vec();
    let mut next = 0;
    let mut map = BTreeMap::new();
    let clubs: Vec<String> = (0..36).map(|i| format!("club-{i:02}")).collect();
    for club in &clubs {
        while left[next % sizes.len()] == 0 {
            next += 1;
        }
        let association = next % sizes.len();
        left[association] -= 1;
        map.insert(club.clone(), format!("assoc-{association}"));
        next += 1;
    }
    (
        clubs.chunks(pot_size).map(<[String]>::to_vec).collect(),
        map,
    )
}

/// Seeds per cell that are also put on matchdays.
const SCHEDULED_SEEDS: u64 = 12;

fn main() {
    let seeds: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(200);
    let distributions: [(&str, Vec<usize>); 4] = [
        (
            "realistic skew (15 associations, max 4)",
            vec![4, 4, 4, 4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1],
        ),
        ("balanced (18 associations of 2)", vec![2; 18]),
        ("tight (12 associations of 3)", vec![3; 12]),
        (
            "top-heavy (9 associations, max 5)",
            vec![5, 5, 5, 5, 4, 4, 3, 3, 2],
        ),
    ];
    let profiles = [
        (
            "Champions League (4x9, 8 matches)",
            profiles::champions_league_2026(),
        ),
        (
            "Europa League (4x9, 8 matches)",
            profiles::europa_league_2026(),
        ),
        (
            "Conference League (6x6, 6 matches)",
            profiles::conference_league_2026(),
        ),
    ];
    println!("seeds per cell: {seeds}\n");
    for (name, rules) in &profiles {
        for (shape, sizes) in &distributions {
            let (pots, associations) = field(rules.pot_size(), sizes);
            let (mut ok, mut failed, mut worst) = (0u64, 0u64, Duration::ZERO);
            let (mut total, mut restarts, mut max_restarts) = (Duration::ZERO, 0u64, 0u32);
            let (mut tidy, mut violations, mut max_violations) = (0u64, 0usize, 0usize);
            let mut scheduled = 0u64;
            let mut first_error = None;
            for seed in 0..seeds {
                let started = Instant::now();
                let input = DrawInput {
                    pots: &pots,
                    associations: &associations,
                    rules,
                    seed,
                    tag: "study",
                };
                match draw(&input) {
                    Ok(graph) => {
                        assert_eq!(verify(&graph, &input), Ok(()), "{name} {shape} seed {seed}");
                        restarts += u64::from(graph.restarts);
                        max_restarts = max_restarts.max(graph.restarts);
                        ok += 1;
                        // The matchday search anneals for about a second, so it is sampled.
                        if seed < SCHEDULED_SEEDS {
                            let plan =
                                schedule(&graph, 36, rules.matches_per_club(), seed, "study")
                                    .unwrap_or_else(|e| {
                                        panic!("{name} {shape} seed {seed}: {e:?}")
                                    });
                            scheduled += 1;
                            tidy += u64::from(plan.pattern_violations == 0);
                            violations += plan.pattern_violations;
                            max_violations = max_violations.max(plan.pattern_violations);
                        }
                    }
                    Err(error) => {
                        failed += 1;
                        first_error.get_or_insert(format!("{error:?}"));
                    }
                }
                let elapsed = started.elapsed();
                total += elapsed;
                worst = worst.max(elapsed);
            }
            println!("{name} / {shape}");
            if failed > 0 && ok == 0 {
                println!("  every seed refused: {}", first_error.unwrap_or_default());
            } else {
                println!(
                    "  drawn {ok}/{seeds} ({:.1}%), restarts mean {:.2} max {max_restarts}; \
                     matchdays for the first {scheduled}: pattern clean {tidy}/{scheduled}, \
                     violations mean {:.2} max {max_violations}",
                    100.0 * ok as f64 / seeds as f64,
                    restarts as f64 / ok.max(1) as f64,
                    violations as f64 / scheduled.max(1) as f64,
                );
                println!(
                    "  time per draw+schedule: mean {:?}, worst {:?}{}",
                    total / seeds as u32,
                    worst,
                    first_error
                        .map(|e| format!("; first refusal: {e}"))
                        .unwrap_or_default()
                );
            }
        }
    }
}
