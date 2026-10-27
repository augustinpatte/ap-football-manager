//! Probe: build the FM24 career, list competitions, play one season with the invariants.
use std::time::Instant;

use season_harness::driver::{RunOptions, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

fn main() {
    let path = world_path_from_env().expect("APFM_WORLD_JSON");
    let club = std::env::args().nth(1).unwrap_or_else(|| "fm-c-498".into());
    let t = Instant::now();
    let mut game = build_fm24_career(&path, &club, 7).expect("career builds");
    println!(
        "built in {:.1}s; date {}",
        t.elapsed().as_secs_f32(),
        game.clock.current_date
    );
    for c in &game.competitions {
        println!(
            "{} | {} | {:?} | n={} fixtures={} season={}",
            c.id,
            c.name,
            c.kind,
            c.participant_ids.len(),
            c.fixtures.len(),
            c.season
        );
    }
    println!("active comps {:?}", game.active_competition_ids);
    let mut inv = Invariants::new(&game);
    let t = Instant::now();
    let r = run_seasons(&mut game, RunOptions::seasons(1), &mut inv);
    println!(
        "{:?} in {:.1}s",
        r.map(|s| s.days),
        t.elapsed().as_secs_f32()
    );
    println!("clean={} \n{}", inv.is_clean(), inv.report());
}
