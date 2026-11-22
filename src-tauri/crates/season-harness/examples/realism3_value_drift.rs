//! REALISM3: how the market values and wages of the *active world's* players move over seasons.
//!
//! `realism3_value_drift <world.json> <out.json> [--seasons 5] [--seed 1] [--club fm-c-498]`
//!
//! Plays the career through the production day and rollover path and, at the start and after every
//! rollover, reads each club player's rating, age, value and wage: the share whose value moved at
//! all, the rank correlation between value and rating, and the median value by rating band. It also
//! follows six probe players (by source person id). Aggregates only.

use std::collections::BTreeMap;

use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::build_fm24_career;
use serde_json::{Value, json};

const PROBES: [(i64, &str); 6] = [
    (12275, "Marcus Rashford"),
    (40368, "Kylian Mbappé"),
    (55635, "Kobbie Mainoo"),
    (6606, "Youri Tielemans"),
    (53728, "Mika Godts"),
    (63609, "Gabriel Moscardo"),
];

#[derive(Clone)]
struct Row {
    ovr: u8,
    age: i32,
    value: u64,
    wage: u32,
}

fn snapshot(game: &Game) -> BTreeMap<String, Row> {
    let on = game.clock.current_date.date_naive();
    game.players
        .iter()
        .filter(|p| p.team_id.is_some() && !p.retired && p.id.starts_with("fm-p-"))
        .map(|p| {
            (
                p.id.clone(),
                Row {
                    ovr: p.ovr,
                    age: domain::age::age_on_or_unknown(on, &p.date_of_birth),
                    value: p.market_value,
                    wage: p.wage(),
                },
            )
        })
        .collect()
}

fn rank(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut ranks = vec![0.0; values.len()];
    for (position, index) in order.into_iter().enumerate() {
        ranks[index] = position as f64;
    }
    ranks
}

fn spearman(pairs: &[(f64, f64)]) -> f64 {
    let (a, b): (Vec<f64>, Vec<f64>) = pairs.iter().copied().unzip();
    let (ra, rb) = (rank(&a), rank(&b));
    let n = pairs.len() as f64;
    let (ma, mb) = (ra.iter().sum::<f64>() / n, rb.iter().sum::<f64>() / n);
    let cov: f64 = ra.iter().zip(&rb).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let (va, vb): (f64, f64) = (
        ra.iter().map(|x| (x - ma).powi(2)).sum(),
        rb.iter().map(|y| (y - mb).powi(2)).sum(),
    );
    cov / (va.sqrt() * vb.sqrt()).max(1e-9)
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
}

struct Drift {
    snaps: Vec<(String, BTreeMap<String, Row>)>,
}

impl SeasonObserver for Drift {
    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        self.snaps.push((format!("s{season}"), snapshot(game)));
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(world), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: realism3_value_drift <world.json> <out.json> [--seasons 5] [--seed 1] [--club fm-c-498]".into());
    };
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seasons: u32 = flag("--seasons").and_then(|s| s.parse().ok()).unwrap_or(5);
    let seed: u64 = flag("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let club = flag("--club").unwrap_or_else(|| "fm-c-498".to_string());

    let mut game = build_fm24_career(std::path::Path::new(world), &club, seed)?;
    let mut drift = Drift {
        snaps: vec![("s0".to_string(), snapshot(&game))],
    };
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut drift).map_err(|e| e.to_string())?;

    let start = drift.snaps[0].1.clone();
    let mut seasons_out = serde_json::Map::new();
    for (label, snap) in drift.snaps.iter().skip(1) {
        let (mut moved, mut n, mut halved, mut doubled) = (0usize, 0usize, 0usize, 0usize);
        let mut by_band: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
        let mut wage_by_band: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
        let mut rating_gain: BTreeMap<i32, Vec<f64>> = BTreeMap::new();
        for (id, now) in snap {
            let Some(then) = start.get(id) else { continue };
            n += 1;
            moved += usize::from(now.value != then.value);
            if then.value > 0 {
                halved += usize::from((now.value as f64) < 0.5 * then.value as f64);
                doubled += usize::from(now.value as f64 > 2.0 * then.value as f64);
            }
            by_band
                .entry((now.ovr / 10) * 10)
                .or_default()
                .push(now.value as f64);
            wage_by_band
                .entry((now.ovr / 10) * 10)
                .or_default()
                .push(f64::from(now.wage));
            // Value ratio against rating change, to see whether a player who improved is worth more.
            rating_gain
                .entry(i32::from(now.ovr) - i32::from(then.ovr))
                .or_default()
                .push(now.value as f64 / (then.value.max(1)) as f64);
        }
        let pairs: Vec<(f64, f64)> = snap
            .values()
            .map(|r| (f64::from(r.ovr), r.value as f64))
            .collect();
        seasons_out.insert(
            label.clone(),
            json!({
                "players": n,
                "share_value_changed": moved as f64 / n.max(1) as f64,
                "share_halved": halved as f64 / n.max(1) as f64,
                "share_doubled": doubled as f64 / n.max(1) as f64,
                "spearman_value_vs_ovr": spearman(&pairs),
                "median_value_by_ovr_band": by_band.into_iter().map(|(k, v)| (k.to_string(), median(v))).collect::<BTreeMap<_, _>>(),
                "median_wage_by_ovr_band": wage_by_band.into_iter().map(|(k, v)| (k.to_string(), median(v))).collect::<BTreeMap<_, _>>(),
                "median_value_ratio_by_rating_change": rating_gain.into_iter().filter(|(_, v)| v.len() >= 30).map(|(k, v)| (k.to_string(), median(v))).collect::<BTreeMap<_, _>>(),
            }),
        );
    }
    let start_pairs: Vec<(f64, f64)> = start
        .values()
        .map(|r| (f64::from(r.ovr), r.value as f64))
        .collect();
    let probes: Vec<Value> = PROBES
        .iter()
        .map(|(person, label)| {
            let id = format!("fm-p-{person}");
            let path: Vec<Value> = drift
                .snaps
                .iter()
                .map(|(l, s)| match s.get(&id) {
                    Some(r) => json!({"s": l, "ovr": r.ovr, "age": r.age, "value": r.value, "wage": r.wage}),
                    None => json!({"s": l, "absent": true}),
                })
                .collect();
            json!({"label": label, "person_id": person, "path": path})
        })
        .collect();
    let report = json!({
        "start_spearman_value_vs_ovr": spearman(&start_pairs),
        "seasons": seasons_out,
        "probes": probes,
    });
    std::fs::write(
        out,
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
