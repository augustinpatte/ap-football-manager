//! The rating a player is shown against the rating his attributes give him, at the opening.
//!
//! `Player::ovr` is a stored number (screens, wages, market values and the squad model read it);
//! `natural_ovr` recomputes it from the attributes at the player's natural position (the match
//! engine reads the attributes). They agree to rounding. P4.5 found they did not at the opening of
//! a generated career: `begin_career` made positions granular without refreshing `ovr`. D6 fixed
//! that in `upgrade_player_identity`; the tests here keep it fixed, and pin what the fix must
//! leave alone (identity, ceilings).

use chrono::NaiveDate;
use domain::manager::Manager;
use domain::player::Player;
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

/// Given a career that has just begun
/// When every rostered player's stored rating is set against his attributes
/// Then they agree to rounding, so a player's rating, wage and market value on day one are the
/// ones his attributes earn.
///
/// Was the P4.5 specification of a known gap; D6 (`player_identity::upgrade_player_identity`
/// re-derives `ovr` when it refines a position) closed it.
#[test]
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

/// Given a generated world with every player's id and ceiling recorded
/// When a career begins in it
/// Then no player was renamed or re-identified, every ceiling is at least the rating, and a
/// ceiling moved only where the corrected rating overtook it (it was kept, not shifted).
#[test]
fn beginning_a_career_keeps_identities_and_only_lifts_a_ceiling_the_rating_overtakes() {
    let (mut game, club) = generated();
    let before: std::collections::HashMap<String, (String, String, String, u8)> = game
        .players
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                (
                    p.full_name.clone(),
                    p.date_of_birth.clone(),
                    p.nationality.clone(),
                    p.potential,
                ),
            )
        })
        .collect();
    begin_career(&mut game, &club, CareerScope::default(), Default::default())
        .expect("the career begins");
    let mut lifted = 0;
    for player in &game.players {
        let Some((name, born, nation, ceiling)) = before.get(&player.id) else {
            continue; // a player the career added (youth repair): nothing to compare
        };
        assert_eq!(&player.full_name, name, "{} was renamed", player.id);
        assert_eq!(
            &player.date_of_birth, born,
            "{} changed birth date",
            player.id
        );
        assert_eq!(
            &player.nationality, nation,
            "{} changed nationality",
            player.id
        );
        assert!(
            player.potential >= player.ovr,
            "{} ceiling below rating",
            player.id
        );
        assert_eq!(
            player.potential,
            (*ceiling).max(player.ovr),
            "{}: a ceiling may rise only to meet the corrected rating",
            player.id
        );
        if player.potential > *ceiling {
            lifted += 1;
        }
    }
    assert!(
        lifted > 0,
        "the fixture must exercise a lifted ceiling, or the assertion above proves nothing"
    );
}

/// Given a career that has begun
/// When the identity upgrade runs again (as it does on every load)
/// Then nothing changes: the repair is idempotent.
#[test]
fn the_identity_upgrade_is_idempotent_once_a_career_has_begun() {
    let (mut game, club) = generated();
    begin_career(&mut game, &club, CareerScope::default(), Default::default())
        .expect("the career begins");
    let snapshot: Vec<(String, u8, u8)> = game
        .players
        .iter()
        .map(|p| (p.id.clone(), p.ovr, p.potential))
        .collect();
    assert!(!ofm_core::player_identity::upgrade_game_player_identities(
        &mut game
    ));
    let after: Vec<(String, u8, u8)> = game
        .players
        .iter()
        .map(|p| (p.id.clone(), p.ovr, p.potential))
        .collect();
    assert_eq!(snapshot, after);
}

/// Measurement, not a gate: how far the generated market value and wage sit from what the
/// corrected rating would price, by position. Value and wage are sized at generation from the
/// rating of the coarse group the player was generated as (`500 * ovr^2 * age factor`; wage =
/// value / 200), and D6 corrects `ovr` afterwards without re-pricing, so a winger is now rated
/// above the rating his price was set from. Printed for `docs/APFM_P46_INTEGRATION_AND_PERSISTENCE.md`.
///
/// `cargo test -p ofm_core --test opening_ratings economy -- --ignored --nocapture`
#[test]
#[ignore = "measurement"]
fn economy_priced_before_the_rating_correction() {
    use std::collections::BTreeMap;
    let (mut game, club) = generated();
    let before: std::collections::HashMap<String, u8> =
        game.players.iter().map(|p| (p.id.clone(), p.ovr)).collect();
    begin_career(&mut game, &club, CareerScope::default(), Default::default())
        .expect("the career begins");
    let on = game.clock.current_date.date_naive();
    // position -> (n, sum ovr before, sum ovr after, sum value, sum value at corrected rating, sum wage)
    let mut rows: BTreeMap<String, (u32, f64, f64, f64, f64, f64)> = BTreeMap::new();
    for player in game.players.iter().filter(|p| p.team_id.is_some()) {
        let Some(&old) = before.get(&player.id) else {
            continue;
        };
        let factor = match domain::age::playing_age(on, &player.date_of_birth) {
            0..=23 => 1.5,
            24..=28 => 1.2,
            29..=32 => 0.8,
            _ => 0.4,
        };
        let value_now = 500.0 * f64::from(player.ovr).powi(2) * factor;
        let row = rows
            .entry(format!("{:?}", player.natural_position))
            .or_default();
        row.0 += 1;
        row.1 += f64::from(old);
        row.2 += f64::from(player.ovr);
        row.3 += player.market_value as f64;
        row.4 += value_now;
        row.5 += f64::from(player.wage());
    }
    eprintln!(
        "position | n | ovr before | ovr after | value / value at corrected rating | wage / (value at corrected rating / 200)"
    );
    for (position, (n, old, new, value, value_now, wage)) in &rows {
        let n = f64::from(*n);
        eprintln!(
            "{position} | {n} | {:.1} | {:.1} | {:.3} | {:.3}",
            old / n,
            new / n,
            value / value_now,
            wage / (value_now / 200.0)
        );
    }
}
