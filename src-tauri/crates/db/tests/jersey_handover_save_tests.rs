//! #498: does a day that hands a shirt number from one player to another still
//! reach the save file?
//!
//! The bug lived at this boundary. The game in memory was valid, and only the
//! row-by-row write to the `.db` collided with the club's unique shirt index,
//! so every save and every exit to the menu failed from that day on. Hence a
//! real save on disk, read back by a fresh `SaveManager`, rather than a check
//! on the in-memory game.

use chrono::{TimeZone, Utc};
use db::save_manager::SaveManager;
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position};
use domain::stats::StatsState;
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;

fn attributes() -> PlayerAttributes {
    PlayerAttributes {
        pace: 60,
        stamina: 60,
        strength: 60,
        agility: 60,
        passing: 60,
        shooting: 60,
        tackling: 60,
        dribbling: 60,
        defending: 60,
        positioning: 60,
        vision: 60,
        decisions: 60,
        composure: 60,
        aggression: 60,
        teamwork: 60,
        leadership: 60,
        handling: 30,
        reflexes: 30,
        aerial: 60,
    }
}

fn player_in_shirt(id: &str, team_id: &str, jersey: u8) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "1998-01-01".to_string(),
        "Ireland".to_string(),
        Position::Midfielder,
        attributes(),
    );
    player.team_id = Some(team_id.to_string());
    player.jersey_number = Some(jersey);
    player
}

fn club(id: &str) -> Team {
    Team::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "Ireland".to_string(),
        "Dublin".to_string(),
        "Ground".to_string(),
        10_000,
    )
}

/// A career with a newcomer in #16 at club X, listed first, and a leaver in
/// #16 at club Y, the reporter's order in `game.players`.
fn career_before_the_transfer_day() -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 16, 0, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr-user".to_string(),
        "Pat".to_string(),
        "Doyle".to_string(),
        "1985-03-01".to_string(),
        "Ireland".to_string(),
    );
    manager.hire("club-x".to_string());
    Game::new(
        clock,
        manager,
        vec![club("club-x"), club("club-y"), club("club-z")],
        vec![
            player_in_shirt("newcomer", "club-x", 16),
            player_in_shirt("leaver", "club-y", 16),
        ],
        vec![],
        vec![],
    )
}

fn shirt(game: &Game, id: &str) -> (Option<String>, Option<u8>) {
    let player = game
        .players
        .iter()
        .find(|player| player.id == id)
        .unwrap_or_else(|| panic!("{id} did not load back"));
    (player.team_id.clone(), player.jersey_number)
}

/// Given a saved career where X's newcomer and Y's leaver both wear #16; when
/// that day the leaver moves to Z and the newcomer joins Y keeping #16; then
/// saving succeeds, and the save on disk has each player at his new club in #16.
#[test]
fn a_day_that_hands_a_shirt_to_a_newcomer_saves_and_loads_back() {
    let saves = tempfile::tempdir().unwrap();
    let mut save_manager = SaveManager::init(saves.path()).unwrap();
    let mut game = career_before_the_transfer_day();
    let save_id = save_manager.create_save(&game, "Handover").unwrap();

    for player in game.players.iter_mut() {
        player.team_id = Some(
            match player.id.as_str() {
                "newcomer" => "club-y",
                _ => "club-z",
            }
            .to_string(),
        );
    }
    let saved = save_manager.save_game_with_stats(&game, &StatsState::default(), &save_id);

    assert_eq!(saved, Ok(()));
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&save_id)
        .unwrap();
    assert_eq!(
        shirt(&reloaded, "newcomer"),
        (Some("club-y".to_string()), Some(16))
    );
    assert_eq!(
        shirt(&reloaded, "leaver"),
        (Some("club-z".to_string()), Some(16))
    );
}

/// Given a saved career where club X's veteran wears #4,
/// When, at the rollover, the veteran retires (off the roster, into the archive) and the youth
///      recruit who joins X the same day is given the freed #4, and the career is saved over the
///      same file,
/// Then the save succeeds, and the file reads back with the recruit in #4 and the veteran archived.
///
/// The roster in memory never holds two #4s. The veteran's row is still in the file, though, until
/// the archive step deletes it, and the recruit's row was written first: the club's unique shirt
/// index refused it, and every save and every exit to the menu failed from the rollover on, in the
/// FM24 world (R.C. Celta Fortuna, #4), where each career saves over one file.
#[test]
fn a_retirees_shirt_given_to_a_recruit_in_the_same_save_interval_saves_and_loads_back() {
    let saves = tempfile::tempdir().unwrap();
    let mut save_manager = SaveManager::init(saves.path()).unwrap();
    let clock = GameClock::new(Utc.with_ymd_and_hms(2027, 6, 20, 0, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr-user".to_string(),
        "Pat".to_string(),
        "Doyle".to_string(),
        "1985-03-01".to_string(),
        "Ireland".to_string(),
    );
    manager.hire("club-x".to_string());
    let mut game = Game::new(
        clock,
        manager,
        vec![club("club-x")],
        vec![
            player_in_shirt("veteran", "club-x", 4),
            player_in_shirt("regular", "club-x", 7),
        ],
        vec![],
        vec![],
    );
    let save_id = save_manager.create_save(&game, "Rollover").unwrap();

    let veteran = game.players.remove(0);
    game.retired_players
        .push(domain::retired_player::RetiredPlayer::from_player(
            &veteran,
            "2027-06-27",
            2026,
        ));
    game.players.push(player_in_shirt("recruit", "club-x", 4));
    let saved = save_manager.save_game_with_stats(&game, &StatsState::default(), &save_id);

    assert_eq!(saved, Ok(()));
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&save_id)
        .unwrap();
    assert_eq!(
        shirt(&reloaded, "recruit"),
        (Some("club-x".to_string()), Some(4))
    );
    assert!(reloaded.players.iter().all(|p| p.id != "veteran"));
    assert_eq!(reloaded.retired_players.len(), 1);
}
