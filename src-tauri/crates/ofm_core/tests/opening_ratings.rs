//! The rating a player is shown against the rating his attributes give him, at the opening.
//!
//! `Player::ovr` is a stored number (screens, wages, market values and the squad model read it);
//! `natural_ovr` recomputes it from the attributes at the player's natural position (the match
//! engine reads the attributes). They should agree to rounding. P4.5 found they do not at the
//! opening of a generated career, for the players whose position is not centre back, central
//! midfielder, striker or keeper, and that they agree again within the first weeks.

use chrono::NaiveDate;
use domain::manager::Manager;
use domain::player::{Player, Position};
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::generator::{DefinitionSources, WorldGenConfig, generate_world_data_seeded_with};
use ofm_core::player_rating::natural_ovr;

fn disagreeing(players: &[Player]) -> Vec<&Player> {
    players
        .iter()
        .filter(|player| player.team_id.is_some() && player.ovr > 0)
        .filter(|player| (f64::from(player.ovr) - natural_ovr(player)).abs() > 0.5)
        .collect()
}

/// A small generated world, before any career has begun.
fn generated() -> (Game, String) {
    let sources = DefinitionSources::embedded_only();
    let mut config = WorldGenConfig::standard_from(&sources);
    config
        .nations
        .retain(|nation| nation.code == "ENG" || nation.code == "ES");
    config.opening_year = Some(2033);
    let world = generate_world_data_seeded_with(2, &config, &sources);
    let club = world.teams[0].id.clone();
    let clock = GameClock::new(
        NaiveDate::from_ymd_opt(2033, 7, 1)
            .expect("a date")
            .and_hms_opt(12, 0, 0)
            .expect("a time")
            .and_utc(),
    );
    let manager = Manager::new(
        "opening-ratings".to_string(),
        "Rating".to_string(),
        "Probe".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    let mut game = Game::new(
        clock,
        manager,
        world.teams,
        world.players,
        world.staff,
        vec![],
    );
    game.seed = 2;
    (game, club)
}

/// Given a generated world that no career has begun in
/// When every rostered player's stored rating is set against his attributes
/// Then they agree to rounding: the generator is consistent, so the opening mismatch is introduced
/// afterwards.
#[test]
fn a_generated_world_rates_every_player_as_his_attributes_do() {
    let (game, _) = generated();
    let off = disagreeing(&game.players);
    assert!(
        off.is_empty(),
        "{} players disagree before the career begins",
        off.len()
    );
}

/// Given the same world once a career has begun in it
/// When the same comparison is made
/// Then the players who disagree are exactly those whose coarse position (Defender, Midfielder,
/// Forward) became a granular one that is not centre back, central midfielder or striker; the
/// others (keepers, centre backs, central midfielders, strikers) still agree.
///
/// This characterises the cause: `begin_career` ends by making positions granular
/// (`upgrade_game_player_identities`) after `ovr` was computed for the coarse bucket, and does not
/// refresh `ovr`.
#[test]
fn only_players_who_became_wingers_midfielders_or_full_backs_disagree_after_the_career_begins() {
    let (mut game, club) = generated();
    begin_career(&mut game, &club, CareerScope::default(), Default::default())
        .expect("the career begins");
    let off = disagreeing(&game.players);
    assert!(
        !off.is_empty(),
        "the mismatch this test characterises is gone: retire the spec below"
    );
    let unaffected = [
        Position::Goalkeeper,
        Position::CenterBack,
        Position::CentralMidfielder,
        Position::Striker,
    ];
    assert!(
        off.iter()
            .all(|player| !unaffected.contains(&player.natural_position)),
        "a keeper, centre back, central midfielder or striker disagrees: the cause is not what \
         this test says"
    );
}

/// Given a career that has just begun
/// When every rostered player's stored rating is set against his attributes
/// Then they agree to rounding, so a player's rating, wage and market value on day one are the
/// ones his attributes earn.
///
/// Known gap, not fixed (P4.5): refreshing `ovr` here would move the rating of about two fifths of
/// outfield players, by up to ten points for wingers, and so rebalance the opening world, which
/// needs an owner's decision (see docs/APFM_P45_VALIDATION_AND_FIXES.md §6).
#[test]
#[ignore = "known gap: begin_career makes positions granular without refreshing ovr; the fix rebalances the opening world"]
fn a_new_career_opens_with_every_rating_agreeing_with_the_attributes() {
    let (mut game, club) = generated();
    begin_career(&mut game, &club, CareerScope::default(), Default::default())
        .expect("the career begins");
    let off = disagreeing(&game.players);
    assert!(
        off.is_empty(),
        "{} players disagree at the opening",
        off.len()
    );
}
