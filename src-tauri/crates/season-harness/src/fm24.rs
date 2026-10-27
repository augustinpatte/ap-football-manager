//! A career in the FM24 world, built the way `start_new_game` + `select_team` build one.
//!
//! The FM24 world is a file the user builds on their own machine (`apfm-import`), so it is never
//! in the repository: callers read its path from `APFM_WORLD_JSON`, and a test without it skips.
//! Nothing here is game logic. It repeats the app's three steps in order — load the world file,
//! make it ready for a career (`build_game_from_imported_world`), take charge of a club
//! (`begin_career`) — and fixes the one thing the app leaves to chance: the seed. An imported
//! world carries no seed, so the app draws a fresh one; a scenario that has to replay sets it.

use std::path::Path;

use domain::manager::Manager;
use domain::stats::StatsState;
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::generator::{ReputationBasis, load_world_from_path};
use ofm_core::world::build::{PastHistory, build_game_from_imported_world};
use ofm_core::world::start_date_for_year;

/// The year the FM24 world's squads are dated for (the 2026/27 season).
pub const FM24_START_YEAR: i32 = 2026;

/// Environment variable naming the world file a test or a scenario plays in.
pub const WORLD_ENV: &str = "APFM_WORLD_JSON";

/// The manager the scenarios play as: born in 1980, so the age rule is never what is being tested.
pub fn scenario_manager() -> Manager {
    Manager::new(
        "mgr_user".to_string(),
        "Scenario".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "France".to_string(),
    )
}

/// The world file named by [`WORLD_ENV`], or `None` when it is not set or not there.
pub fn world_path_from_env() -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(std::env::var_os(WORLD_ENV)?);
    path.is_file().then_some(path)
}

/// The game before any club is chosen: what the new-game screen holds while the player picks a team.
pub fn build_fm24_world(path: &Path, seed: u64) -> Result<(Game, StatsState), String> {
    let mut world = load_world_from_path(path)?;
    world.generation_seed = Some(seed);
    let clock = GameClock::new(start_date_for_year(FM24_START_YEAR)?);
    let history = PastHistory {
        start_year: FM24_START_YEAR,
        depth_years: 0,
    };
    Ok(build_game_from_imported_world(
        clock,
        scenario_manager(),
        history,
        world,
        ReputationBasis::Authored(&Default::default()),
    ))
}

/// The game with the manager in charge of `club_id`, as `select_team` leaves it.
pub fn build_fm24_career(path: &Path, club_id: &str, seed: u64) -> Result<Game, String> {
    let (mut game, stats) = build_fm24_world(path, seed)?;
    begin_career(&mut game, club_id, CareerScope::default(), stats)?;
    Ok(game)
}
