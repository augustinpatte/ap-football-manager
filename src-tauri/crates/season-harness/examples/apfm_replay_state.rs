//! Mission 11: plays days from a saved career and times them, to tell whether a slowdown lives in
//! the state or in the process that holds it. `apfm_replay_state <saves dir> <save id> <days>`
use std::time::Instant;

use ofm_core::game::Game;
use season_harness::driver::{DayPath, RunOptions, SeasonObserver, play_day};

struct Quiet;
impl SeasonObserver for Quiet {}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("saves dir"));
    let id = args.next().expect("save id");
    let days: u32 = args.next().expect("days").parse().expect("number");
    let mut manager = db::save_manager::SaveManager::init(&dir).expect("save manager");
    manager.load_saves().expect("list");
    let t = Instant::now();
    let mut game: Game = manager.load_game(&id).expect("load");
    println!(
        "loaded in {:.2}s, date {}",
        t.elapsed().as_secs_f64(),
        game.clock.current_date
    );
    let mut options = RunOptions::seasons(1);
    options.day_path = DayPath::UserMatchLive;
    let mut ms = Vec::new();
    for day in 1..=days {
        let t = Instant::now();
        play_day(&mut game, options, 1, day, &mut Quiet).expect("day");
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = Instant::now();
    let _copy = game.clone();
    let clone_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{} days, mean {:.1} ms, max {:.1} ms, clone {:.1} ms, players {}",
        days,
        ms.iter().sum::<f64>() / ms.len() as f64,
        ms.iter().cloned().fold(0.0, f64::max),
        clone_ms,
        game.players.len()
    );
}
