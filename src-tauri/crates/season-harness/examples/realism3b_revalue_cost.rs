//! REALISM3B: what the seasonal revaluation costs, and what it adds to the state.
//!
//! `realism3b_revalue_cost <world.json|--generated> [--repeat 20]`
//!
//! Builds the career, anchors every player, then times one full revaluation pass over every player
//! of the world (the same call the season's end makes per player), and reports the serialized size
//! of the value bases and of the scale.

use std::time::Instant;

use ofm_core::revaluation::{anchor_missing_bases, revalue_after_season};
use season_harness::fm24::build_fm24_career;
use season_harness::worlds::WorldSpec;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repeat: usize = args
        .iter()
        .position(|a| a == "--repeat")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(20);
    let mut game = if args.iter().any(|a| a == "--generated") {
        WorldSpec::shipped(1).build()?
    } else {
        let world = args.first().ok_or("usage: <world.json> | --generated")?;
        build_fm24_career(std::path::Path::new(world), "fm-c-498", 1)?
    };
    let on = game.clock.current_date.date_naive();
    let started = Instant::now();
    let anchored = anchor_missing_bases(&mut game, on);
    println!("anchoring {anchored} players: {:?}", started.elapsed());

    let scale = game.value_scale.clone();
    let mut times = Vec::new();
    for season in 0..repeat {
        let started = Instant::now();
        let mut changed = 0usize;
        for player in game.players.iter_mut() {
            // A year on, so every player is carried and none is skipped by the marker.
            changed += usize::from(revalue_after_season(
                player,
                &scale,
                on + chrono::Duration::days(365 * (season as i64 + 1)),
                2027 + season as u32,
            ));
        }
        times.push((started.elapsed(), changed));
    }
    times.sort();
    println!(
        "players {}, calibrated scale {}, revaluation pass median {:?} (min {:?}, max {:?})",
        game.players.len(),
        scale.is_calibrated(),
        times[times.len() / 2].0,
        times[0].0,
        times[times.len() - 1].0
    );
    let bases: usize = game
        .players
        .iter()
        .filter_map(|p| p.value_basis.as_ref())
        .map(|b| serde_json::to_string(b).map(|s| s.len()).unwrap_or(0))
        .sum();
    println!(
        "value bases: {} players, {} bytes of JSON ({:.0} per player), scale {} bytes, in-memory {} bytes per basis",
        game.players
            .iter()
            .filter(|p| p.value_basis.is_some())
            .count(),
        bases,
        bases as f64 / game.players.len().max(1) as f64,
        serde_json::to_string(&scale).map(|s| s.len()).unwrap_or(0),
        std::mem::size_of::<domain::player::ValueBasis>()
    );
    Ok(())
}
