//! OFM's own generated world, as a yardstick for the calibration of imported ratings:
//! distribution of ratings by club-reputation band, and counts per OFM tier.
//! `cargo run -p fm24-adapter --example baseline`

use ofm_core::generator::{DefinitionSources, generate_world_data_seeded};
use std::collections::BTreeMap;

fn pct(sorted: &[u8], q: f64) -> u8 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let world = generate_world_data_seeded(7, &DefinitionSources::embedded_only());
    let rep: BTreeMap<&str, u32> = world
        .teams
        .iter()
        .map(|t| (t.id.as_str(), t.reputation))
        .collect();
    let mut bands: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut all = Vec::new();
    let mut top14: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    let mut by_team: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for p in &world.players {
        if let Some(team) = p.team_id.as_deref() {
            by_team.entry(team).or_default().push(p.ovr);
            bands.entry(rep[team] / 100 * 100).or_default().push(p.ovr);
            all.push(p.ovr);
        }
    }
    for (team, mut v) in by_team {
        v.sort_unstable_by(|a, b| b.cmp(a));
        let top: Vec<f64> = v.iter().take(14).map(|x| f64::from(*x)).collect();
        top14
            .entry(rep[team] / 100 * 100)
            .or_default()
            .push(top.iter().sum::<f64>() / top.len() as f64);
    }
    all.sort_unstable();
    println!(
        "OFM generated world: {} clubs, {} players",
        world.teams.len(),
        world.players.len()
    );
    println!(
        "all: median {} p90 {} p99 {} max {} ; >=81: {} ; 77-80: {} ; 70-76: {}",
        pct(&all, 0.5),
        pct(&all, 0.9),
        pct(&all, 0.99),
        all[all.len() - 1],
        all.iter().filter(|v| **v >= 81).count(),
        all.iter().filter(|v| (77..=80).contains(*v)).count(),
        all.iter().filter(|v| (70..=76).contains(*v)).count()
    );
    println!("rep band | players | median | p90 | max | avg top-14");
    for (band, mut v) in bands {
        v.sort_unstable();
        let t = &top14[&band];
        println!(
            "{band:>4}+ | {:>5} | {} | {} | {} | {:.1}",
            v.len(),
            pct(&v, 0.5),
            pct(&v, 0.9),
            v[v.len() - 1],
            t.iter().sum::<f64>() / t.len() as f64
        );
    }
}
