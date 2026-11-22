//! REALISM3: how market values and wages of the external registry move from one season to the next.
//!
//! `cargo run --release -p fm24-adapter --example realism3_value_continuity -- <world.db> <scratch-dir> <out.json>`
//!
//! Builds the registry from the source (read-only) into `<scratch-dir>`, builds a career cache at the
//! start season and at each of the next five, and compares every player present in two snapshots.
//! It prints percentile tables by ability band and, for six probe players (by source person id),
//! their value and wage season by season. Nothing but aggregates and the probes is written, and the
//! output belongs in a scratch directory, never in the repository.

use chrono::NaiveDate;
use fm24_adapter::career::ensure_career_cache;
use fm24_adapter::registry::build_registry;
use ofm_core::external_world::{projected_market_value, projected_wage};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const SEED: u64 = 7;
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
    potential: u8,
    birth: String,
    value: u64,
    wage: u64,
    club_reputation: i64,
    status: String,
    has_club: bool,
}

fn load(path: &Path) -> Result<BTreeMap<i64, Row>, String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT person_id, ovr, potential, COALESCE(birth_date,''), market_value_eur, wage_eur_week,
                    club_reputation, status, club_id IS NOT NULL FROM registry_player",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Row {
                    ovr: r.get::<_, i64>(1)?.clamp(0, 99) as u8,
                    potential: r.get::<_, i64>(2)?.clamp(0, 99) as u8,
                    birth: r.get(3)?,
                    value: r.get::<_, i64>(4)?.max(0) as u64,
                    wage: r.get::<_, i64>(5)?.max(0) as u64,
                    club_reputation: r.get(6)?,
                    status: r.get(7)?,
                    has_club: r.get::<_, i64>(8)? != 0,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|e| e.to_string())
}

fn pct(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn band(ovr: u8) -> String {
    let low = (ovr / 10) * 10;
    format!("{low}-{}", low + 9)
}

fn age_on(birth: &str, on: NaiveDate) -> i32 {
    domain::age::age_on(on, birth).unwrap_or(27)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [world_db, scratch, out] = args.as_slice() else {
        return Err("usage: realism3_value_continuity <world.db> <scratch-dir> <out.json>".into());
    };
    if Path::new(out) == Path::new(world_db) || Path::new(scratch) == Path::new(world_db) {
        return Err("the output and scratch paths must not be the source database".into());
    }
    let scratch = Path::new(scratch);
    let registry = scratch.join("registry.db");
    let cache_dir = scratch.join("cache");
    build_registry(Path::new(world_db), &registry)?;

    let dates = [
        ("s0", "2026-06-01", 2026),
        ("s1", "2027-07-01", 2027),
        ("s2", "2028-07-01", 2028),
        ("s3", "2029-07-01", 2029),
        ("s4", "2030-07-01", 2030),
        ("s5", "2031-07-01", 2031),
    ];
    let mut snapshots: Vec<(&str, i32, BTreeMap<i64, Row>)> = Vec::new();
    for (label, date, season) in dates {
        let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| e.to_string())?;
        let report = ensure_career_cache(&registry, &cache_dir, SEED, today)?;
        snapshots.push((label, season, load(&report.path)?));
    }
    let start = &snapshots[0].2;
    let on_start = NaiveDate::from_ymd_opt(2026, 6, 1).ok_or("date")?;

    // The instantaneous scale ratio: what the native formula says of a player *as he is at the start*
    // against the value the import gave him. This is the jump a projection that does not advance the
    // player at all would apply.
    let mut jump: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut wage_jump: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for row in start
        .values()
        .filter(|r| r.status == "eligible" && r.value > 0)
    {
        let age = age_on(&row.birth, on_start);
        let native = projected_market_value(row.ovr, row.potential, age) as f64;
        jump.entry(band(row.ovr))
            .or_default()
            .push(native / row.value as f64);
        if row.wage > 0 && row.has_club {
            let native_wage = f64::from(projected_wage(row.ovr, age, row.club_reputation));
            wage_jump
                .entry(band(row.ovr))
                .or_default()
                .push(native_wage / row.wage as f64);
        }
    }
    let summarize = |m: BTreeMap<String, Vec<f64>>| -> Value {
        let mut o = serde_json::Map::new();
        for (k, mut v) in m {
            v.sort_by(f64::total_cmp);
            o.insert(
                k,
                json!({"n": v.len(), "p10": pct(&v, 0.1), "p50": pct(&v, 0.5), "p90": pct(&v, 0.9)}),
            );
        }
        Value::Object(o)
    };

    // Season by season: value and wage ratio against the start, by start ability band (externals
    // present in both snapshots with a positive value then and now).
    let mut seasons = serde_json::Map::new();
    for (label, season, snap) in snapshots.iter().skip(1) {
        let mut value_ratio: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut wage_ratio: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut elite = Vec::new();
        let (mut halved, mut doubled, mut n) = (0usize, 0usize, 0usize);
        for (id, then) in start
            .iter()
            .filter(|(_, r)| r.status == "eligible" && r.value > 0)
        {
            let Some(now) = snap.get(id) else { continue };
            if now.value == 0 {
                continue;
            }
            let ratio = now.value as f64 / then.value as f64;
            n += 1;
            halved += usize::from(ratio < 0.5);
            doubled += usize::from(ratio > 2.0);
            value_ratio.entry(band(then.ovr)).or_default().push(ratio);
            if then.value >= 20_000_000 {
                elite.push(ratio);
            }
            if then.wage > 0 && now.wage > 0 {
                wage_ratio
                    .entry(band(then.ovr))
                    .or_default()
                    .push(now.wage as f64 / then.wage as f64);
            }
        }
        elite.sort_by(f64::total_cmp);
        seasons.insert(
            (*label).to_string(),
            json!({
                "season": season, "compared": n,
                "share_value_more_than_halved": halved as f64 / n.max(1) as f64,
                "share_value_more_than_doubled": doubled as f64 / n.max(1) as f64,
                "elite_value_ge_20m_ratio": {"n": elite.len(), "p10": pct(&elite, 0.1), "p50": pct(&elite, 0.5), "p90": pct(&elite, 0.9)},
                "value_ratio_by_start_ovr_band": summarize(value_ratio),
                "wage_ratio_by_start_ovr_band": summarize(wage_ratio),
            }),
        );
    }

    // Distribution of values across the external population, season by season.
    let mut distributions = serde_json::Map::new();
    for (label, _, snap) in &snapshots {
        let mut values: Vec<f64> = snap
            .values()
            .filter(|r| r.status == "eligible" && r.value > 0)
            .map(|r| r.value as f64)
            .collect();
        values.sort_by(f64::total_cmp);
        let mut wages: Vec<f64> = snap
            .values()
            .filter(|r| r.status == "eligible" && r.wage > 0)
            .map(|r| r.wage as f64)
            .collect();
        wages.sort_by(f64::total_cmp);
        distributions.insert(
            (*label).to_string(),
            json!({
                "n": values.len(),
                "value": {"p10": pct(&values,0.1), "p50": pct(&values,0.5), "p90": pct(&values,0.9), "p99": pct(&values,0.99), "max": values.last()},
                "wage": {"p10": pct(&wages,0.1), "p50": pct(&wages,0.5), "p90": pct(&wages,0.9), "p99": pct(&wages,0.99), "max": wages.last()},
            }),
        );
    }

    let probes: Vec<Value> = PROBES
        .iter()
        .map(|(id, label)| {
            let by_season: Vec<Value> = snapshots
                .iter()
                .map(|(l, season, snap)| match snap.get(id) {
                    Some(r) => json!({"s": l, "season": season, "ovr": r.ovr, "pot": r.potential, "value": r.value, "wage": r.wage, "status": r.status}),
                    None => json!({"s": l, "season": season, "absent": true}),
                })
                .collect();
            json!({"label": label, "person_id": id, "seasons": by_season})
        })
        .collect();

    let report = json!({
        "instant_value_jump_native_over_imported_by_ovr_band": summarize(jump),
        "instant_wage_jump_native_over_imported_by_ovr_band": summarize(wage_jump),
        "seasons": seasons,
        "distributions": distributions,
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
