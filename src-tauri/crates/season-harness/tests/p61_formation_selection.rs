//! P6.1: which number predicts how a squad does in a formation, tested out of sample.
//!
//! P6 found the squad model's formation score has no relationship to a club's results across its own
//! eight formations (slope -0.014 +/- 0.029 points a match per rating point). This study asks what
//! does. For many clubs in many independent worlds it plays every formation against one fixed panel
//! of opponents, records what the engine fields in each (the means of exactly the attributes the
//! contests read, see `match_study::LineupFeatures`), and compares candidate scores by how well they
//! rank a club's own formations on worlds the candidate was never fitted on (leave one world out).
//!
//! `P6_SEEDS=1,2,3,4,5,6,7,8,9,10,11,12 P61_CLUBS=16 P61_REPS=8 cargo test --release -p season-harness --test p61_formation_selection -- --ignored --nocapture`
//!
//! Uncertainty is over clubs (the units that differ), never over matches. Every number a candidate
//! is judged by is a within-club contrast, so what a club is worth overall cancels out.

use ofm_core::ai_formation::{freeze_reviews_for_study, read_all_clubs};
use ofm_core::player_rating::FORMATIONS;
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::env;
use season_harness::match_study::{
    Football, LineupFeatures, lineup_features, mean_se, play, set_formation,
};

struct Quiet;
impl SeasonObserver for Quiet {}

/// One club in one formation: what it is worth to the squad model, what the engine fielded, and what
/// it did against the club's panel.
#[derive(Clone)]
struct Row {
    world: u64,
    club: String,
    formation: usize,
    squad_score: f64,
    /// What the game's formation review reads now (`FormationScore::fit`).
    engine_fit: f64,
    lineup: LineupFeatures,
    /// The panel's own lineups, averaged: the reference opponent this club's contests are read against.
    opposition: LineupFeatures,
    /// What the formation's lineup averages over every club of the study: the level a typical squad
    /// reaches in it, so `lineup - typical` is how much better this squad suits the shape than most.
    typical: LineupFeatures,
    /// Points and goal difference a match, over all panel matches, and over the even and odd
    /// repetitions separately (their agreement is the ceiling of any prediction).
    points: f64,
    goal_difference: f64,
    points_even: f64,
    points_odd: f64,
}

fn mean_features(all: &[LineupFeatures]) -> LineupFeatures {
    let n = all.len() as f64;
    let mean = |f: fn(&LineupFeatures) -> f64| all.iter().map(f).sum::<f64>() / n;
    LineupFeatures {
        defenders: mean(|l| l.defenders),
        midfielders: mean(|l| l.midfielders),
        forwards: mean(|l| l.forwards),
        mid_carry: mean(|l| l.mid_carry),
        mid_tackle: mean(|l| l.mid_tackle),
        forward_run: mean(|l| l.forward_run),
        back_hold: mean(|l| l.back_hold),
        finishing: mean(|l| l.finishing),
        back_screen: mean(|l| l.back_screen),
        keeping: mean(|l| l.keeping),
        build_up: mean(|l| l.build_up),
        press: mean(|l| l.press),
        ovr: mean(|l| l.ovr),
    }
}

fn collect(seeds: &[u64], clubs_per_world: usize, reps: u64) -> Vec<Row> {
    let panel_size = 6usize;
    let mut rows = Vec::new();
    for &seed in seeds {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=env::first("P6_WARMUP", "60") as u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let _frozen = freeze_reviews_for_study();
        let readings = read_all_clubs(&game);
        let user = game.manager.team_id.clone();
        let league = game
            .competitions
            .iter()
            .filter(|c| {
                c.kind == domain::league::CompetitionType::League && c.participant_ids.len() >= 16
            })
            .max_by_key(|c| c.participant_ids.len())
            .expect("a league")
            .clone();
        let ids: Vec<String> = league
            .participant_ids
            .iter()
            .filter(|id| Some(*id) != user.as_ref())
            .cloned()
            .collect();
        let step = (ids.len() / clubs_per_world).max(1);
        for subject in ids.iter().step_by(step).take(clubs_per_world) {
            let panel: Vec<&String> = ids
                .iter()
                .filter(|id| *id != subject)
                .skip(ids.len() / 2 - panel_size / 2)
                .take(panel_size)
                .collect();
            let opposition = mean_features(
                &panel
                    .iter()
                    .map(|opponent| {
                        lineup_features(&mut game, opponent, subject).expect("an opponent lineup")
                    })
                    .collect::<Vec<_>>(),
            );
            for (index, formation) in FORMATIONS.iter().enumerate() {
                set_formation(&mut game, subject, formation);
                let lineup = lineup_features(&mut game, subject, panel[0]).expect("a lineup");
                let reading = readings
                    .iter()
                    .find(|r| &r.team_id == subject)
                    .and_then(|r| r.scores.iter().find(|s| s.formation == *formation));
                let squad_score = reading.map_or(f64::NAN, |s| s.squad);
                let engine_fit = reading.map_or(f64::NAN, |s| s.fit);
                let (mut points, mut goal_difference) = (Vec::new(), Vec::new());
                let (mut even, mut odd) = (Vec::new(), Vec::new());
                for rep in 0..reps {
                    game.seed = seed * 100_000 + rep;
                    for opponent in &panel {
                        for home in [true, false] {
                            let result = if home {
                                play(&mut game, Football::FiveZone, subject, opponent)
                                    .expect("plays")
                                    .0
                            } else {
                                play(&mut game, Football::FiveZone, opponent, subject)
                                    .expect("plays")
                                    .1
                            };
                            points.push(result.points);
                            goal_difference.push(result.goals_for - result.goals_against);
                            if rep % 2 == 0 {
                                even.push(result.points);
                            } else {
                                odd.push(result.points);
                            }
                        }
                    }
                }
                let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
                rows.push(Row {
                    world: seed,
                    club: subject.clone(),
                    formation: index,
                    squad_score,
                    engine_fit,
                    lineup,
                    opposition,
                    typical: LineupFeatures::default(),
                    points: mean(&points),
                    goal_difference: mean(&goal_difference),
                    points_even: mean(&even),
                    points_odd: mean(&odd),
                });
            }
        }
        eprintln!("world {seed}: {} rows so far", rows.len());
    }
    fill_typical(&mut rows);
    rows
}

/// Sets each row's `typical` to the mean lineup of its formation over `rows`. (Computed over every
/// world, a few constants a model might as well know; it is not a fitted weight.)
fn fill_typical(rows: &mut [Row]) {
    let typicals: Vec<LineupFeatures> = (0..FORMATIONS.len())
        .map(|f| {
            let of_shape: Vec<LineupFeatures> = rows
                .iter()
                .filter(|r| r.formation == f)
                .map(|r| r.lineup)
                .collect();
            mean_features(&of_shape)
        })
        .collect();
    for row in rows.iter_mut() {
        row.typical = typicals[row.formation];
    }
}

// ---------------------------------------------------------------------------
// Candidate scores
// ---------------------------------------------------------------------------

/// A candidate: how it turns a row into the numbers a regression weighs.
struct Candidate {
    name: &'static str,
    predictors: fn(&Row) -> Vec<f64>,
}

/// What the engine's numbers terms are worth at the weights the game ships (`MatchConfig`):
/// `(own / opposing / level)` clamped to [0.5, 2] and raised to 0.3.
fn numbers_edge(own: f64, opposing: f64, level: f64) -> f64 {
    (own / opposing.max(1.0) / level).clamp(0.5, 2.0).powf(0.3)
}

fn shape_dummies(row: &Row) -> Vec<f64> {
    (1..FORMATIONS.len())
        .map(|f| f64::from(u8::from(row.formation == f)))
        .collect()
}

fn own_contest_means(row: &Row) -> Vec<f64> {
    let l = &row.lineup;
    vec![
        l.mid_carry,
        l.mid_tackle,
        l.forward_run,
        l.back_hold,
        l.finishing,
        l.back_screen,
        l.build_up,
        l.press,
    ]
}

/// Each contest as the engine resolves it, against the reference opponent: the log of the ratio of
/// what the two sides' players bring, with the numbers terms the game prices.
fn contest_ratios(row: &Row) -> Vec<f64> {
    let (l, o) = (&row.lineup, &row.opposition);
    let ln = f64::ln;
    vec![
        ln(l.mid_carry) - ln(o.mid_tackle) + ln(numbers_edge(l.midfielders, o.midfielders, 1.0)),
        ln(l.mid_tackle) - ln(o.mid_carry) - ln(numbers_edge(o.midfielders, l.midfielders, 1.0)),
        ln(l.forward_run) - ln(o.back_hold) + ln(numbers_edge(l.forwards, o.defenders, 0.5)),
        ln(l.back_hold) - ln(o.forward_run) - ln(numbers_edge(o.forwards, l.defenders, 0.5)),
        ln(l.finishing) - ln(o.back_screen),
        ln(l.back_screen) - ln(o.finishing),
        ln(l.build_up) - ln(o.press),
        ln(l.press) - ln(o.build_up),
    ]
}

/// The two means that carry most of the variation (P6.1 fits): the back line's contest and the
/// forwards' finishing.
fn back_line_and_finishing(row: &Row) -> Vec<f64> {
    vec![row.lineup.back_hold, row.lineup.finishing]
}

/// The back line's hold and the forwards' finishing, each less what a typical squad reaches in the
/// same shape: how much better this squad suits the shape than most.
fn back_line_and_finishing_suitability(row: &Row) -> Vec<f64> {
    vec![
        row.lineup.back_hold - row.typical.back_hold,
        row.lineup.finishing - row.typical.finishing,
    ]
}

/// The eight contest means less what a typical squad reaches in the same shape.
fn suitability_to_the_shape(row: &Row) -> Vec<f64> {
    let (l, t) = (&row.lineup, &row.typical);
    vec![
        l.mid_carry - t.mid_carry,
        l.mid_tackle - t.mid_tackle,
        l.forward_run - t.forward_run,
        l.back_hold - t.back_hold,
        l.finishing - t.finishing,
        l.back_screen - t.back_screen,
        l.build_up - t.build_up,
        l.press - t.press,
    ]
}

fn joined(parts: &[fn(&Row) -> Vec<f64>], row: &Row) -> Vec<f64> {
    parts.iter().flat_map(|part| part(row)).collect()
}

const CANDIDATES: [Candidate; 14] = [
    Candidate {
        name: "squad model's score (today's)",
        predictors: |r| vec![r.squad_score],
    },
    Candidate {
        name: "mean stored overall of the eleven",
        predictors: |r| vec![r.lineup.ovr],
    },
    Candidate {
        name: "the shape alone (7 dummies)",
        predictors: shape_dummies,
    },
    Candidate {
        name: "own contest means (8)",
        predictors: own_contest_means,
    },
    Candidate {
        name: "back line + finishing only (2)",
        predictors: back_line_and_finishing,
    },
    Candidate {
        name: "shape + back line + finishing (9)",
        predictors: |r| joined(&[shape_dummies, back_line_and_finishing], r),
    },
    Candidate {
        name: "suitability: back line + finishing (2)",
        predictors: back_line_and_finishing_suitability,
    },
    Candidate {
        name: "suitability to the shape (8)",
        predictors: suitability_to_the_shape,
    },
    Candidate {
        name: "suitability + squad score (9)",
        predictors: |r| joined(&[suitability_to_the_shape, |r| vec![r.squad_score]], r),
    },
    Candidate {
        name: "contest ratios v the panel (8)",
        predictors: contest_ratios,
    },
    Candidate {
        name: "shape + own contest means (15)",
        predictors: |r| joined(&[shape_dummies, own_contest_means], r),
    },
    Candidate {
        name: "shape + contest ratios (15)",
        predictors: |r| joined(&[shape_dummies, contest_ratios], r),
    },
    Candidate {
        name: "contest ratios + squad score (9)",
        predictors: |r| joined(&[contest_ratios, |r| vec![r.squad_score]], r),
    },
    Candidate {
        name: "shape + ratios + squad score (16)",
        predictors: |r| joined(&[shape_dummies, contest_ratios, |r| vec![r.squad_score]], r),
    },
];

// ---------------------------------------------------------------------------
// Least squares, within club
// ---------------------------------------------------------------------------

/// Solves `a x = b` by Gaussian elimination with partial pivoting; `None` for a singular system.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col].clone();
        for row in col + 1..n {
            let factor = a[row][col] / pivot_row[col];
            for (cell, above) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *cell -= factor * above;
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let tail: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    Some(x)
}

/// The rows of one club, by club.
fn by_club(rows: &[Row]) -> Vec<Vec<&Row>> {
    let mut groups: Vec<Vec<&Row>> = Vec::new();
    for row in rows {
        match groups
            .iter_mut()
            .find(|g| g[0].club == row.club && g[0].world == row.world)
        {
            Some(group) => group.push(row),
            None => groups.push(vec![row]),
        }
    }
    groups
}

/// The within-club deviation of each predictor (centred on the club's own formations' mean).
fn demeaned_predictors(group: &[&Row], candidate: &Candidate) -> Vec<Vec<f64>> {
    let all: Vec<Vec<f64>> = group.iter().map(|r| (candidate.predictors)(r)).collect();
    let k = all[0].len();
    let n = all.len() as f64;
    let means: Vec<f64> = (0..k)
        .map(|j| all.iter().map(|x| x[j]).sum::<f64>() / n)
        .collect();
    all.iter()
        .map(|x| x.iter().zip(&means).map(|(v, m)| v - m).collect())
        .collect()
}

/// Weights for the candidate's predictors that best explain a club's goal difference across its own
/// formations, over `training`: ordinary least squares on within-club deviations.
fn fit(training: &[Row], candidate: &Candidate) -> Option<Vec<f64>> {
    let mut xtx: Vec<Vec<f64>> = Vec::new();
    let mut xty: Vec<f64> = Vec::new();
    for group in by_club(training) {
        let x = demeaned_predictors(&group, candidate);
        let mean_y = group.iter().map(|r| r.goal_difference).sum::<f64>() / group.len() as f64;
        if xtx.is_empty() {
            xtx = vec![vec![0.0; x[0].len()]; x[0].len()];
            xty = vec![0.0; x[0].len()];
        }
        for (row, xi) in group.iter().zip(&x) {
            for a in 0..xi.len() {
                for b in 0..xi.len() {
                    xtx[a][b] += xi[a] * xi[b];
                }
                xty[a] += xi[a] * (row.goal_difference - mean_y);
            }
        }
    }
    for (i, row) in xtx.iter_mut().enumerate() {
        row[i] += 1e-6; // keeps a predictor that never varies from making the system singular
    }
    solve(xtx, xty)
}

fn correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let syy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    (sxx > 1e-12 && syy > 1e-12).then(|| sxy / (sxx * syy).sqrt())
}

/// What one candidate achieved on the clubs of worlds it was not fitted on.
struct Evaluation {
    correlation: (f64, f64),
    /// Points a match the chosen formation earns over the mean of the club's eight.
    gain_over_a_random_shape: (f64, f64),
    /// The same for the club's single best-predicted shape against its single worst.
    best_over_worst: (f64, f64),
    /// How many clubs the rule sends to each formation (in `FORMATIONS` order).
    choices: [usize; 8],
}

/// `frozen` scores the candidate's predictor as it is (a rule already written down), with no weights
/// fitted on other worlds.
fn evaluate(rows: &[Row], candidate: &Candidate, frozen: bool) -> Evaluation {
    let worlds: Vec<u64> = {
        let mut w: Vec<u64> = rows.iter().map(|r| r.world).collect();
        w.sort_unstable();
        w.dedup();
        w
    };
    let (mut correlations, mut gains, mut spreads) = (Vec::new(), Vec::new(), Vec::new());
    let mut choices = [0usize; 8];
    for held_out in worlds {
        let training: Vec<Row> = rows
            .iter()
            .filter(|r| r.world != held_out)
            .cloned()
            .collect();
        let weights = if frozen {
            vec![1.0; (candidate.predictors)(&rows[0]).len()]
        } else {
            match fit(&training, candidate) {
                Some(weights) => weights,
                None => continue,
            }
        };
        let testing: Vec<Row> = rows
            .iter()
            .filter(|r| r.world == held_out)
            .cloned()
            .collect();
        for group in by_club(&testing) {
            let x = demeaned_predictors(&group, candidate);
            let predicted: Vec<f64> = x
                .iter()
                .map(|xi| xi.iter().zip(&weights).map(|(a, b)| a * b).sum())
                .collect();
            let observed: Vec<f64> = group.iter().map(|r| r.points).collect();
            if let Some(c) = correlation(&predicted, &observed) {
                correlations.push(c);
            }
            let best = (0..predicted.len())
                .max_by(|&i, &j| predicted[i].total_cmp(&predicted[j]))
                .unwrap_or(0);
            let worst = (0..predicted.len())
                .min_by(|&i, &j| predicted[i].total_cmp(&predicted[j]))
                .unwrap_or(0);
            let mean_points = observed.iter().sum::<f64>() / observed.len() as f64;
            choices[group[best].formation] += 1;
            gains.push(observed[best] - mean_points);
            spreads.push(observed[best] - observed[worst]);
        }
    }
    Evaluation {
        correlation: mean_se(&correlations),
        gain_over_a_random_shape: mean_se(&gains),
        best_over_worst: mean_se(&spreads),
        choices,
    }
}

/// The agreement of two independent halves of a club's matches on how its formations rank: the
/// ceiling for any prediction, since a score cannot agree with the matches better than they agree
/// with themselves.
fn split_half_reliability(rows: &[Row]) -> (f64, f64) {
    let correlations: Vec<f64> = by_club(rows)
        .iter()
        .filter_map(|group| {
            let even: Vec<f64> = group.iter().map(|r| r.points_even).collect();
            let odd: Vec<f64> = group.iter().map(|r| r.points_odd).collect();
            correlation(&even, &odd)
        })
        .collect();
    mean_se(&correlations)
}

#[test]
#[ignore = "measurement study"]
fn p61_formation_selection() {
    let seeds = env::list("P6_SEEDS", "1,2,3,4,5,6,7,8,9,10,11,12");
    let rows = collect(
        &seeds,
        env::first("P61_CLUBS", "16") as usize,
        env::first("P61_REPS", "8"),
    );
    if let Ok(path) = std::env::var("P61_DUMP") {
        let mut out = String::from(
            "world\tclub\tformation\tsquad\tovr\tmid_carry\tmid_tackle\tforward_run\tback_hold\tfinishing\tback_screen\tbuild_up\tpress\tdef\tmid\tfwd\tpoints\tgd\n",
        );
        for r in &rows {
            let l = &r.lineup;
            out.push_str(&format!(
                "{}\t{}\t{}\t{:.4}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{:.4}\t{:.4}\n",
                r.world, r.club, FORMATIONS[r.formation], r.squad_score, l.ovr, l.mid_carry,
                l.mid_tackle, l.forward_run, l.back_hold, l.finishing, l.back_screen, l.build_up,
                l.press, l.defenders, l.midfielders, l.forwards, r.points, r.goal_difference
            ));
        }
        std::fs::write(path, out).expect("the dump is written");
    }
    let clubs = by_club(&rows).len();
    let (spread, _) = mean_se(
        &by_club(&rows)
            .iter()
            .map(|g| {
                let v: Vec<f64> = g.iter().map(|r| r.points).collect();
                let (_, se) = mean_se(&v);
                se * (v.len() as f64).sqrt()
            })
            .collect::<Vec<_>>(),
    );
    let (reliability, reliability_se) = split_half_reliability(&rows);
    eprintln!(
        "\n== which score ranks a club's own eight formations? {clubs} clubs, {} worlds, leave one world out ==",
        seeds.len()
    );
    eprintln!(
        "a club's points a match vary across its shapes with sd {spread:.3}; split-half correlation of two independent halves of its matches {reliability:+.3} (se {reliability_se:.3}): the ceiling"
    );
    eprintln!(
        "{:<36}{:>16}{:>22}{:>22}",
        "candidate", "corr (se)", "gain v random shape", "best - worst shape"
    );
    for candidate in &CANDIDATES {
        if let Some(weights) = fit(&rows, candidate) {
            let shown: Vec<String> = weights.iter().map(|w| format!("{w:+.3}")).collect();
            eprintln!(
                "  fitted on every club, goal difference a match per unit: [{}]",
                shown.join(" ")
            );
        }
        let e = evaluate(&rows, candidate, false);
        eprintln!(
            "{:<36}{:>+9.3} ({:.3}){:>+14.3} ({:.3}){:>+14.3} ({:.3})",
            candidate.name,
            e.correlation.0,
            e.correlation.1,
            e.gain_over_a_random_shape.0,
            e.gain_over_a_random_shape.1,
            e.best_over_worst.0,
            e.best_over_worst.1,
        );
        eprintln!(
            "{:<36}  chosen: {}",
            "",
            FORMATIONS
                .iter()
                .zip(e.choices)
                .map(|(f, n)| format!("{f} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let shipped = Candidate {
        name: "AS SHIPPED: the game's engine fit (frozen)",
        predictors: |r| vec![r.engine_fit],
    };
    let e = evaluate(&rows, &shipped, true);
    eprintln!(
        "{:<36}{:>+9.3} ({:.3}){:>+14.3} ({:.3}){:>+14.3} ({:.3})",
        shipped.name,
        e.correlation.0,
        e.correlation.1,
        e.gain_over_a_random_shape.0,
        e.gain_over_a_random_shape.1,
        e.best_over_worst.0,
        e.best_over_worst.1,
    );
    eprintln!(
        "{:<36}  chosen: {}",
        "",
        FORMATIONS
            .iter()
            .zip(e.choices)
            .map(|(f, n)| format!("{f} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    // What the game's AI did until now: the squad model's score, taken at face value.
    let before = Candidate {
        name: "AS IT WAS: the squad model's score (frozen)",
        predictors: |r| vec![r.squad_score],
    };
    let e = evaluate(&rows, &before, true);
    eprintln!(
        "{:<36}{:>+9.3} ({:.3}){:>+14.3} ({:.3}){:>+14.3} ({:.3})",
        before.name,
        e.correlation.0,
        e.correlation.1,
        e.gain_over_a_random_shape.0,
        e.gain_over_a_random_shape.1,
        e.best_over_worst.0,
        e.best_over_worst.1,
    );
    eprintln!(
        "{:<36}  chosen: {}",
        "",
        FORMATIONS
            .iter()
            .zip(e.choices)
            .map(|(f, n)| format!("{f} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
