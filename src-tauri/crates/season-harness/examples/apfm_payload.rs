//! Size of the game state sent to the screen, before (`Game` as is) and after (`Game::ui_clone`),
//! on the FM24 world. `APFM_WORLD_JSON=<world> apfm_payload <club id> [seasons]`. Prints sizes only.

use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

fn main() {
    let club = std::env::args().nth(1).expect("club id");
    let path = world_path_from_env().expect("APFM_WORLD_JSON is not set");
    let seasons: u32 = std::env::args()
        .nth(2)
        .map_or(0, |n| n.parse().expect("seasons"));
    let mut game = build_fm24_career(&path, &club, 1).expect("career builds");
    struct Quiet;
    impl SeasonObserver for Quiet {}
    if seasons > 0 {
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut Quiet).expect("seasons play");
    }
    print!("after {seasons} season(s): ");

    let full = serde_json::to_vec(&game).unwrap().len();
    let light = serde_json::to_vec(&game.ui_clone()).unwrap().len();
    let history: usize = game
        .players
        .iter()
        .map(|p| {
            serde_json::to_vec(&p.career).unwrap().len()
                + serde_json::to_vec(&p.movement_history.to_vec())
                    .unwrap()
                    .len()
        })
        .sum();
    let one = serde_json::to_vec(&season_harness_history(&game))
        .unwrap()
        .len();
    println!(
        "players={} full_bytes={full} light_bytes={light} saved_bytes={} ({:.1}%) history_bytes={history} one_profile_history_bytes={one}",
        game.players.len(),
        full - light,
        100.0 * (full - light) as f64 / full as f64
    );
}

fn season_harness_history(
    game: &ofm_core::game::Game,
) -> ofm_core::slices::player_history::PlayerHistory {
    ofm_core::slices::player_history::query_player_history(game, &game.players[0].id).unwrap()
}
