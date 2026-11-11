//! A formation means the same thing to both engines the game plays.
//!
//! The eleven a club names is slot-aligned: entry `i` plays slot `i` of the formation. The
//! five-zone engine is told each starter's group from that slot (`turn::squad`), and the M12
//! adapter puts starter `i` at geometry slot `i`. If the two lists of slots disagree, a player
//! chosen for one slot stands in another in the other engine.

use ofm_core::player_rating::{FORMATIONS, formation_slots};
use ofm_core::spatial_adapter::formation_geometry;

/// Given every formation the game offers
/// When the five-zone engine's seats are counted by group and M12's slots by the row of the pitch
///   they stand on (defence, midfield, attack: a wing back stands in the defence in a back five
///   and in the midfield in a 3-5-2, as the five-zone list names him a back or a midfielder)
/// Then the two hold the same number of goalkeepers, defenders, midfielders and forwards: the two
///      engines field the same shape. (The order of the two lists differs, and the adapter seats
///      each starter by his own slot, not by index: see `spatial_adapter`.)
#[test]
fn both_engines_field_every_formation_with_the_same_group_counts() {
    use domain::player::Position;
    for formation in FORMATIONS {
        let mut engine = [0usize; 4];
        for seat in formation_slots(formation) {
            engine[match seat.to_group_position() {
                Position::Goalkeeper => 0,
                Position::Defender => 1,
                Position::Midfielder => 2,
                _ => 3,
            }] += 1;
        }
        let mut m12 = [0usize; 4];
        for slot in formation_geometry(formation).expect("M12 lays the formation out") {
            m12[match slot.base.x {
                x if x < 15.0 => 0,
                x if x < 35.0 => 1,
                x if x < 65.0 => 2,
                _ => 3,
            }] += 1;
        }
        assert_eq!(engine, m12, "{formation}");
    }
}

/// Given a formation name the game does not offer
/// When M12 is asked to lay it out
/// Then it refuses: an unknown shape is never silently played as another one.
#[test]
fn m12_refuses_a_formation_the_game_does_not_offer() {
    assert!(formation_geometry("2-3-5").is_err());
    assert!(formation_slots("2-3-5").len() != 11 || FORMATIONS.iter().all(|f| *f != "2-3-5"));
}
