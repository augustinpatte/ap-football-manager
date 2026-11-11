//! P4.7: does the match engine price a formation beyond what the squad model says it is worth?
//! A controlled experiment, measurement only (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=3 cargo test --release -p season-harness --test p47_formation_engine -- --ignored --nocapture`
//!
//! Club reviews are frozen and every AI club is given a formation by a hash of its id, whatever its
//! squad. At each season's end, within each league, points a game are regressed on the strength of
//! the club's best fourteen and on the squad model's score for the formation it actually played. A
//! formation that still sits above or below zero beyond both is one the engine rewards or punishes
//! for the shape itself.

use std::collections::{BTreeMap, HashMap};

use domain::league::CompetitionType;
use ofm_core::ai_formation::{freeze_reviews_for_study, read_all_clubs};
use ofm_core::game::Game;
use ofm_core::player_rating::FORMATIONS;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

struct Row {
    formation: String,
    ppg: f64,
    gd: f64,
    strength: f64,
    own_score: f64,
    best_score: f64,
}

#[derive(Default)]
struct Log {
    rows: Vec<Row>,
    league_of_row: Vec<usize>,
    next_league: usize,
}

impl SeasonObserver for Log {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let readings: HashMap<String, _> = read_all_clubs(game)
            .into_iter()
            .map(|r| (r.team_id.clone(), r))
            .collect();
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
            self.next_league += 1;
            for entry in &league.standings {
                let Some(reading) = readings.get(&entry.team_id) else {
                    continue;
                };
                if entry.played == 0 {
                    continue;
                }
                let mut ovr = by_club
                    .get(entry.team_id.as_str())
                    .cloned()
                    .unwrap_or_default();
                ovr.sort_unstable_by(|a, b| b.cmp(a));
                ovr.truncate(14);
                if ovr.is_empty() {
                    continue;
                }
                let own = reading
                    .scores
                    .iter()
                    .find(|s| s.formation == reading.current)
                    .map(|s| s.squad);
                let Some(own) = own else { continue };
                let best = reading
                    .scores
                    .iter()
                    .map(|s| s.squad)
                    .fold(f64::MIN, f64::max);
                self.rows.push(Row {
                    formation: reading.current.clone(),
                    ppg: f64::from(entry.points) / f64::from(entry.played),
                    gd: (f64::from(entry.goals_for) - f64::from(entry.goals_against))
                        / f64::from(entry.played),
                    strength: ovr.iter().map(|o| f64::from(*o)).sum::<f64>() / ovr.len() as f64,
                    own_score: own,
                    best_score: best,
                });
                self.league_of_row.push(self.next_league);
            }
        }
    }
}

/// Subtracts each league's mean from `values`.
fn within(values: &[f64], leagues: &[usize]) -> Vec<f64> {
    let mut sums: HashMap<usize, (f64, f64)> = HashMap::new();
    for (v, l) in values.iter().zip(leagues) {
        let e = sums.entry(*l).or_default();
        e.0 += v;
        e.1 += 1.0;
    }
    values
        .iter()
        .zip(leagues)
        .map(|(v, l)| v - sums[l].0 / sums[l].1)
        .collect()
}

/// Least squares of `y` on two covariates (no intercept: everything is demeaned), returning the
/// coefficients and the residuals.
fn ols2(y: &[f64], a: &[f64], b: &[f64]) -> ((f64, f64), Vec<f64>) {
    let dot = |x: &[f64], z: &[f64]| x.iter().zip(z).map(|(p, q)| p * q).sum::<f64>();
    let (aa, ab, bb, ay, by) = (dot(a, a), dot(a, b), dot(b, b), dot(a, y), dot(b, y));
    let det = aa * bb - ab * ab;
    let ca = (ay * bb - by * ab) / det;
    let cb = (by * aa - ay * ab) / det;
    let resid = (0..y.len()).map(|i| y[i] - ca * a[i] - cb * b[i]).collect();
    ((ca, cb), resid)
}

#[test]
#[ignore = "measurement study"]
fn p47_formation_engine() {
    let seasons = env::first("P47_SEASONS", "3") as u32;
    let mut log = Log::default();
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let frozen = freeze_reviews_for_study();
        let user = game.manager.team_id.clone();
        let ids: Vec<String> = game.teams.iter().map(|t| t.id.clone()).collect();
        for (index, id) in ids.iter().enumerate() {
            if Some(id) == user.as_ref() {
                continue;
            }
            let pick = (index * 7 + seed as usize * 3 + index / 8) % FORMATIONS.len();
            let formation = FORMATIONS[pick];
            if let Some(team) = game.teams.iter_mut().find(|t| &t.id == id) {
                team.formation = formation.to_string();
            }
        }
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        drop(frozen);
    }
    let leagues = &log.league_of_row;
    let ppg = within(&log.rows.iter().map(|r| r.ppg).collect::<Vec<_>>(), leagues);
    let gd = within(&log.rows.iter().map(|r| r.gd).collect::<Vec<_>>(), leagues);
    let strength = within(
        &log.rows.iter().map(|r| r.strength).collect::<Vec<_>>(),
        leagues,
    );
    let own = within(
        &log.rows.iter().map(|r| r.own_score).collect::<Vec<_>>(),
        leagues,
    );
    let deficit = within(
        &log.rows
            .iter()
            .map(|r| r.best_score - r.own_score)
            .collect::<Vec<_>>(),
        leagues,
    );
    let ((c_strength, c_own), resid) = ols2(&ppg, &strength, &own);
    eprintln!(
        "club-seasons {} | ppg ~ {c_strength:.3} strength + {c_own:.3} formation score (per rating point)",
        log.rows.len()
    );
    let ((_, c_deficit), _) = ols2(&ppg, &strength, &deficit);
    eprintln!(
        "ppg on strength and squad-model deficit vs the club's best formation: {c_deficit:.3} per point of deficit"
    );
    let ((_, _), resid_gd) = ols2(&gd, &strength, &own);
    let mut table: BTreeMap<&str, (f64, f64, f64, f64)> = BTreeMap::new();
    for (index, row) in log.rows.iter().enumerate() {
        let e = table.entry(row.formation.as_str()).or_default();
        e.0 += 1.0;
        e.1 += resid[index];
        e.2 += resid_gd[index];
        e.3 += row.best_score - row.own_score;
    }
    eprintln!(
        "formation | club-seasons | ppg beyond strength+score (se) | goal diff/game beyond | mean model deficit"
    );
    let sd = (resid.iter().map(|r| r * r).sum::<f64>() / resid.len() as f64).sqrt();
    for (formation, (n, r, g, d)) in table {
        eprintln!(
            "{formation:<8} | {n:>4} | {:+.3} (±{:.3}) | {:+.3} | {:.2}",
            r / n,
            sd / n.sqrt(),
            g / n,
            d / n
        );
    }
}
