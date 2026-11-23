//! Revaluing the players of the active world once a season, after they have developed.
//!
//! A player's market value is *carried* ([`ValueScale::carry`]): it keeps its place on the world's
//! scale and moves by the scale's ratio between the state he was valued in and the state he is in.
//! To do that the player has to remember the state he was valued in, his
//! [`ValueBasis`](domain::player::ValueBasis), and the world has to remember its scale
//! ([`Game::value_scale`]); neither may depend on a file outside the save, or a career would be
//! read differently after a world is reinstalled.
//!
//! The lifecycle is three moves:
//!
//! 1. **Anchor.** A player whose value was set by a creator (the generator, a registry
//!    activation, a legacy save) is given a basis that is exactly his stored value and the rating,
//!    ceiling and age he has at that moment. Nothing is repriced: an old career opens with every
//!    value it had.
//! 2. **Develop.** The season turns; ratings, ceilings and ages move.
//! 3. **Revalue.** At the season's end the value is carried from the basis to the new state and the
//!    basis is replaced, so the same change is never paid for twice.
//!
//! A value is an estimate of what a player is worth, not money. Nothing here touches a contract, a
//! wage, an offer already made, a club's cash or a transfer budget: those keep the terms they were
//! agreed on, and only a negotiation begun after the revaluation reads the new number.

use crate::game::Game;
use crate::value_scale::{Standing, ValueScale};
use chrono::NaiveDate;
use domain::player::{Player, ValueBasis};

/// The model identity a basis carries: the scale's version when it is calibrated, 0 for the native
/// formula. A scale of another version is not the one the basis was taken under.
fn model_of(scale: &ValueScale) -> u32 {
    if scale.is_calibrated() {
        scale.version
    } else {
        0
    }
}

/// What a player is, as far as his value is concerned, on `on`.
fn standing_of(player: &Player, on: NaiveDate) -> Standing {
    Standing {
        ovr: player.ovr,
        potential: player.potential.max(player.ovr),
        age: domain::age::age_on_or_unknown(on, &player.date_of_birth),
    }
}

fn basis_standing(basis: &ValueBasis) -> Standing {
    Standing {
        ovr: basis.ovr,
        potential: basis.potential.max(basis.ovr),
        age: basis.age,
    }
}

fn basis_in(player: &Player, scale: &ValueScale, on: NaiveDate, revalued_year: u32) -> ValueBasis {
    let now = standing_of(player, on);
    ValueBasis {
        value: player.market_value,
        ovr: now.ovr,
        potential: now.potential,
        age: now.age,
        revalued_year,
        model: model_of(scale),
    }
}

/// Records the state `player`'s current value stands for. The value is not changed.
pub(crate) fn anchor_value(player: &mut Player, scale: &ValueScale, on: NaiveDate) {
    player.value_basis = Some(basis_in(player, scale, on, 0));
}

/// A newcomer from a generator that prices on the native formula takes the price of players like
/// him on the world's own scale, then is anchored on it. A world on the native formula is left as
/// it is: the generator's price is already on its scale.
pub(crate) fn enter_the_market(player: &mut Player, scale: &ValueScale, on: NaiveDate) {
    if scale.is_calibrated() {
        player.market_value = scale.value(standing_of(player, on));
    }
    anchor_value(player, scale, on);
}

/// Gives a basis to every player who has none, at the state he is in now. A creator that did not
/// anchor its player, a legacy save and a career's opening all pass through here, so a value is
/// never revalued from a state it was not set in. Idempotent: a player who has a basis keeps it.
/// Returns how many were anchored.
pub fn anchor_missing_bases(game: &mut Game, on: NaiveDate) -> usize {
    let scale = &game.value_scale;
    let mut anchored = 0;
    for player in game.players.iter_mut().filter(|p| p.value_basis.is_none()) {
        anchor_value(player, scale, on);
        anchored += 1;
    }
    anchored
}

/// Revalues `player` at the end of the season of `year` (the calendar year it ends in), after his rating, ceiling and age have moved. Returns
/// whether his value changed.
///
/// * no basis: he is anchored where he stands (nothing to carry from);
/// * already revalued at this season's end: nothing, so a retried rollover cannot pay twice;
/// * a basis of another model, or one his value no longer matches (something priced him since):
///   re-anchored without a ratio, because the value he has is for the state he is in;
/// * otherwise: the value is carried from the basis to now, and the basis replaced.
///
/// A player with no value yet, or one who has retired, is left alone.
pub fn revalue_after_season(
    player: &mut Player,
    scale: &ValueScale,
    on: NaiveDate,
    year: u32,
) -> bool {
    if player.retired || player.market_value == 0 {
        return false;
    }
    let Some(basis) = player.value_basis else {
        player.value_basis = Some(basis_in(player, scale, on, year));
        return false;
    };
    if basis.revalued_year == year {
        return false;
    }
    let before = player.market_value;
    if basis.model == model_of(scale) && basis.value == before {
        player.market_value = scale.carry(before, basis_standing(&basis), standing_of(player, on));
    }
    player.value_basis = Some(basis_in(player, scale, on, year));
    player.market_value != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;
    use crate::value_scale::sample_imported_scale;
    use domain::player::Position;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2027, 5, 31).unwrap()
    }

    fn player(born: &str, ovr: u8, potential: u8, value: u64) -> Player {
        let mut player = Player::new(
            "p".to_string(),
            "P".to_string(),
            "P P".to_string(),
            born.to_string(),
            "England".to_string(),
            Position::Midfielder,
            uniform_attributes(ovr),
        );
        player.ovr = ovr;
        player.potential = potential;
        player.market_value = value;
        player
    }

    /// Given a player valued at 90 M as an 87-rated 32-year-old on a calibrated scale,
    /// When a season takes him to 77 at 33 and the season is revalued,
    /// Then his value falls by the scale's own ratio: it is neither frozen nor hardcoded.
    #[test]
    fn a_player_who_declines_loses_value_by_the_scales_ratio() {
        let scale = sample_imported_scale();
        let mut star = player("1994-03-01", 87, 87, 90_000_000);
        anchor_value(
            &mut star,
            &scale,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        let then = star.value_basis.expect("anchored");
        // Born 1 March 1994, anchored on 31 May 2026: thirty-two, under the calibrated model.
        assert_eq!((then.age, then.model), (32, scale.version));
        star.ovr = 77;
        star.potential = 77;

        assert!(revalue_after_season(&mut star, &scale, day(), 2027));

        let expected = scale.carry(
            90_000_000,
            basis_standing(&then),
            Standing {
                ovr: 77,
                potential: 77,
                age: 33,
            },
        );
        assert_eq!(star.market_value, expected);
        assert!(star.market_value < 30_000_000, "{}", star.market_value);
    }

    /// Given a 19-year-old who gains rating,
    /// When the season is revalued,
    /// Then he is worth more.
    #[test]
    fn a_player_who_improves_gains_value() {
        let scale = sample_imported_scale();
        let mut prospect = player("2008-01-01", 60, 80, 1_000_000);
        anchor_value(
            &mut prospect,
            &scale,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        prospect.ovr = 68;

        assert!(revalue_after_season(&mut prospect, &scale, day(), 2027));

        assert!(
            prospect.market_value > 1_000_000,
            "{}",
            prospect.market_value
        );
    }

    /// Given a season already revalued,
    /// When the same season's end is run again (a retried rollover) after his rating has moved
    /// further,
    /// Then nothing is paid a second time: the season's marker, not the unchanged state, is what
    /// stops it (the new rating is picked up by the next season's end).
    #[test]
    fn a_season_is_revalued_once() {
        let scale = sample_imported_scale();
        let mut star = player("1994-03-01", 87, 87, 90_000_000);
        anchor_value(
            &mut star,
            &scale,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        star.ovr = 80;
        revalue_after_season(&mut star, &scale, day(), 2027);
        let once = star.clone();
        star.ovr = 74;

        let changed = revalue_after_season(&mut star, &scale, day(), 2027);

        assert!(!changed);
        assert_eq!(star.market_value, once.market_value);
        assert_eq!(star.value_basis, once.value_basis);
        // The next season's end does price the further decline, from the basis kept.
        assert!(revalue_after_season(&mut star, &scale, day(), 2028));
        assert!(star.market_value < once.market_value);
    }

    /// Given a player with no basis (a legacy save),
    /// When the season is revalued,
    /// Then he is anchored where he stands and keeps the value he had.
    #[test]
    fn a_player_with_no_basis_is_anchored_not_repriced() {
        let scale = sample_imported_scale();
        let mut legacy = player("1994-03-01", 87, 87, 90_000_000);

        let changed = revalue_after_season(&mut legacy, &scale, day(), 2027);

        assert!(!changed);
        assert_eq!(legacy.market_value, 90_000_000);
        let basis = legacy.value_basis.expect("anchored");
        assert_eq!(
            (basis.value, basis.ovr, basis.revalued_year),
            (90_000_000, 87, 2027)
        );
    }

    /// Given a player whose value was set after his basis was taken (something priced him),
    /// When the season is revalued,
    /// Then the value he has is kept and the basis moves to the present: no ratio is paid on top of
    /// a value that is already for his current state.
    #[test]
    fn a_value_set_since_the_basis_is_not_carried_again() {
        let scale = sample_imported_scale();
        let mut player = player("1994-03-01", 87, 87, 90_000_000);
        anchor_value(
            &mut player,
            &scale,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        player.ovr = 80;
        player.market_value = 25_000_000;

        let changed = revalue_after_season(&mut player, &scale, day(), 2027);

        assert!(!changed);
        assert_eq!(player.market_value, 25_000_000);
        assert_eq!(player.value_basis.expect("re-anchored").ovr, 80);
    }

    /// Given a basis taken under the native formula and a world that now carries a calibrated scale,
    /// When the season is revalued,
    /// Then the value is kept and re-anchored: a ratio of one model is never applied to a value of
    /// another.
    #[test]
    fn a_basis_of_another_model_is_re_anchored() {
        let native = ValueScale::native();
        let calibrated = sample_imported_scale();
        let mut player = player("1994-03-01", 87, 87, 90_000_000);
        anchor_value(
            &mut player,
            &native,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        player.ovr = 70;

        assert!(!revalue_after_season(&mut player, &calibrated, day(), 2027));

        assert_eq!(player.market_value, 90_000_000);
        assert_eq!(
            player.value_basis.expect("re-anchored").model,
            calibrated.version
        );
    }

    /// Given a player who has no value, or has retired,
    /// When the season is revalued,
    /// Then he is left alone.
    #[test]
    fn a_valueless_or_retired_player_is_left_alone() {
        let scale = sample_imported_scale();
        let mut valueless = player("1994-03-01", 87, 87, 0);
        let mut retired = player("1990-03-01", 70, 70, 1_000_000);
        retired.retired = true;

        assert!(!revalue_after_season(&mut valueless, &scale, day(), 2027));
        assert!(!revalue_after_season(&mut retired, &scale, day(), 2027));

        assert_eq!(valueless.value_basis, None);
        assert_eq!(retired.value_basis, None);
    }

    fn game_of(players: Vec<Player>) -> Game {
        use chrono::TimeZone;
        let clock = crate::clock::GameClock::new(
            chrono::Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap(),
        );
        let manager = domain::manager::Manager::new(
            "mgr".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        Game::new(clock, manager, vec![], players, vec![], vec![])
    }

    /// Given a roster in which one player already has a basis and two have none,
    /// When the missing bases are anchored,
    /// Then two are anchored (and counted), the one that had a basis keeps exactly his own, and a
    /// second pass anchors nobody.
    #[test]
    fn anchoring_the_missing_bases_leaves_an_existing_one_alone() {
        let scale = sample_imported_scale();
        let mut kept = player("1994-03-01", 80, 80, 20_000_000);
        kept.id = "kept".to_string();
        anchor_value(
            &mut kept,
            &scale,
            NaiveDate::from_ymd_opt(2024, 5, 31).unwrap(),
        );
        let before = kept.value_basis;
        let (mut a, mut b) = (
            player("2000-03-01", 70, 75, 3_000_000),
            player("2001-03-01", 60, 70, 900_000),
        );
        (a.id, b.id) = ("a".to_string(), "b".to_string());
        let mut game = game_of(vec![kept, a, b]);
        game.value_scale = scale;
        let on = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        assert_eq!(anchor_missing_bases(&mut game, on), 2);

        assert_eq!(game.players[0].value_basis, before);
        let anchored = game.players[1].value_basis.expect("anchored");
        assert_eq!(
            (
                anchored.value,
                anchored.ovr,
                anchored.potential,
                anchored.age,
                anchored.model
            ),
            (3_000_000, 70, 75, 26, game.value_scale.version)
        );
        assert_eq!(anchor_missing_bases(&mut game, on), 0);
    }

    /// Given a basis taken under a calibrated scale and a world that is native, and the reverse,
    /// When the season is revalued,
    /// Then in both directions the value is kept and re-anchored on the model now in force.
    #[test]
    fn a_basis_is_never_carried_across_models_in_either_direction() {
        let (native, calibrated) = (ValueScale::native(), sample_imported_scale());
        let mut player = player("1994-03-01", 87, 87, 90_000_000);
        anchor_value(
            &mut player,
            &calibrated,
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        );
        player.ovr = 70;

        assert!(!revalue_after_season(&mut player, &native, day(), 2027));

        assert_eq!(player.market_value, 90_000_000);
        assert_eq!(player.value_basis.expect("re-anchored").model, 0);
    }
}
