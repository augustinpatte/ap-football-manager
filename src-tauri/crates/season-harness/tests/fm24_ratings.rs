//! The level of FM24 players in the world a career is played in (Run F2).
//!
//! A player's rating follows his FM Current Ability. The world is the player's own file
//! (`APFM_WORLD_JSON`), never in the repository: each test returns at once when it is not set.
//! The ids below are FM `person.id` values read from the player's own `world.db`
//! (CA 150 and CA 127); no rating is written down here, only the order the abilities imply.
//!
//! The world must be revision 6 or later: a world installed under revision 4 rated both near
//! 66-69 from the mean of their attributes, which is what these tests would catch.

use ofm_core::game::Game;
use ofm_core::player_rating::natural_ovr;
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const LENS: &str = "fm-c-501";
/// CA 150 / PA 168, a left-back of a top club.
const HIGHER_ABILITY: &str = "fm-p-9453";
/// CA 127 / PA 148, a centre-back of a second-division club.
const LOWER_ABILITY: &str = "fm-p-29715";

fn career() -> Option<Game> {
    let path = world_path_from_env()?;
    Some(build_fm24_career(&path, LENS, 7).expect("the career builds"))
}

fn ovr_of(game: &Game, id: &str) -> u8 {
    game.players
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("{id} is in the world"))
        .ovr
}

/// Given two players of the FM24 world whose Current Ability differs by more than twenty,
/// When a career opens and both are read from the player model,
/// Then the one with the higher ability is rated clearly higher (not the 66 / 69 of an attribute mean).
#[test]
fn the_player_with_the_higher_current_ability_is_rated_higher_in_a_new_career() {
    let Some(game) = career() else {
        return;
    };
    let (high, low) = (ovr_of(&game, HIGHER_ABILITY), ovr_of(&game, LOWER_ABILITY));
    assert!(
        high >= low + 4,
        "{HIGHER_ABILITY} is rated {high}, {LOWER_ABILITY} {low}: is the installed world older than revision 6?"
    );
}

/// Given every player of the opening world,
/// When the rating the game recomputes from his attributes (training, ageing and the daily refresh
///      all do) is compared with the one the import stored,
/// Then they agree within a point: the anchor does not drift away on the first refresh.
#[test]
fn the_stored_rating_survives_the_games_own_recomputation_from_attributes() {
    let Some(game) = career() else {
        return;
    };
    let drifted: Vec<String> = game
        .players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-"))
        .filter(|p| (natural_ovr(p) - f64::from(p.ovr)).abs() > 1.0)
        .map(|p| format!("{} stored {} recomputed {:.1}", p.id, p.ovr, natural_ovr(p)))
        .collect();
    assert!(
        drifted.is_empty(),
        "{} players drift on refresh, e.g. {:?}",
        drifted.len(),
        &drifted[..drifted.len().min(5)]
    );
}

/// Given the whole imported population,
/// When ratings are read,
/// Then the hierarchy spans the scale: stars in the high 80s or more, and a real spread between
///      the median and the best (the 55-70 band an attribute mean gave is gone).
#[test]
fn the_imported_population_spans_the_scale() {
    let Some(game) = career() else {
        return;
    };
    let mut ratings: Vec<u8> = game
        .players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-") && p.team_id.is_some())
        .map(|p| p.ovr)
        .collect();
    ratings.sort_unstable();
    let best = ratings[ratings.len() - 1];
    let median = ratings[ratings.len() / 2];
    assert!(best >= 88, "best player is rated {best}");
    assert!(best - median >= 20, "best {best}, median {median}");
}
