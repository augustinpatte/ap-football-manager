//! Run F5.1: where a long career's load time goes. Measurement only (`#[ignore]`d).
//!
//! `F51_SEASONS=20 cargo test --release -p season-harness --test f51_load_profile -- --ignored --nocapture`

use std::time::Instant;

use domain::stats::StatsState;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

struct Quiet;
impl SeasonObserver for Quiet {}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = f();
    eprintln!(
        "  {label:<34} {:>6.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    value
}

#[test]
#[ignore = "plays many seasons; run in release with --ignored --nocapture"]
fn f51_load_profile() {
    let seasons: u32 = std::env::var("F51_SEASONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let dir = tempfile::tempdir().expect("tmp");
    let mut manager = db::save_manager::SaveManager::init(dir.path()).expect("saves");
    let mut game = WorldSpec::gate(21).build().expect("world");
    let stats = StatsState::default();
    run_seasons(&mut game, RunOptions::seasons(1), &mut Quiet).expect("season");
    let id = manager
        .create_save_with_stats(&game, &stats, "profile")
        .expect("create");
    for _ in 2..=seasons {
        run_seasons(&mut game, RunOptions::seasons(1), &mut Quiet).expect("season");
        manager
            .save_game_with_stats(&game, &stats, &id)
            .expect("save");
    }
    let path = manager.save_db_path(&id).expect("path");
    eprintln!(
        "after {seasons} seasons, file {} MB",
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) / 1_000_000
    );

    let conn = rusqlite::Connection::open(&path).expect("open");
    use db::repositories::{
        edition_archive_repo, message_repo, player_repo, retired_manager_repo, retired_player_repo,
        team_repo,
    };
    let players = timed("players", || {
        player_repo::load_all_players(&conn).expect("players")
    });
    timed("teams", || team_repo::load_all_teams(&conn).expect("teams"));
    let retired = timed("retired players", || {
        retired_player_repo::load_retired_players(&conn).expect("rp")
    });
    timed("retired managers", || {
        retired_manager_repo::load_retired_managers(&conn).expect("rm")
    });
    timed("messages", || {
        message_repo::load_all_messages(&conn).expect("messages")
    });
    timed("edition archive", || {
        edition_archive_repo::load_editions(&conn).expect("editions")
    });
    eprintln!("  ({} players, {} retired)", players.len(), retired.len());
    let mut fresh = db::save_manager::SaveManager::init(dir.path()).expect("init");
    timed("load_game (everything)", || {
        fresh.load_game(&id).expect("load")
    });
}
