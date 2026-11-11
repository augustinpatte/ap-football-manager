//! What an assistant can tell about a player he has not signed.
//!
//! An AI club reads the world as it is. The manager's assistant does not: he sees the players of
//! his own division at a distance, and the others only through a scout's report. His reading of a
//! player's rating is blurred by a bounded, deterministic error that shrinks smoothly as his own
//! rating rises — there is no rating at which he suddenly sees clearly — and shrinks again for a
//! player a scout has just reported on. The same assistant reads the same player the same way every
//! day, so a decision never flickers.

use crate::player_knowledge::PlayerKnowledge;
use crate::stable_hash::stable_hash;

/// Largest error, in rating points, of an assistant rated 0 reading a player he only sees at a
/// distance. An assistant of 100 reads exactly.
const MAX_BLUR_POINTS: f64 = 6.0;

/// A scout's current report halves the error: someone went and looked.
const SCOUTED_FACTOR: f64 = 0.5;

/// A report that no longer describes the player is no better than a glance.
const STALE_FACTOR: f64 = 1.0;

/// Seed of the error, so it is unrelated to any other hash in the game.
const BLUR_SEED: u64 = 0x7065_7263_6569_7665; // "perceive"

/// The error, in rating points, with which `assistant_id` (rated `rating`) reads `player_id` given
/// what the club knows of him. Zero for the club's own players.
pub(crate) fn blur_points(
    rating: f64,
    assistant_id: &str,
    player_id: &str,
    knowledge: &PlayerKnowledge,
) -> f32 {
    let factor = match knowledge {
        PlayerKnowledge::Own => return 0.0,
        PlayerKnowledge::Known | PlayerKnowledge::Unknown => 1.0,
        PlayerKnowledge::Scouted { current: true, .. } => SCOUTED_FACTOR,
        PlayerKnowledge::Scouted { current: false, .. } => STALE_FACTOR,
    };
    let reach = (100.0 - rating).clamp(0.0, 100.0) / 100.0 * MAX_BLUR_POINTS * factor;
    let hash = stable_hash(format!("{assistant_id}/{player_id}").as_bytes(), BLUR_SEED);
    let unit = (hash % 2001) as f64 / 1000.0 - 1.0;
    (unit * reach) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> PlayerKnowledge {
        PlayerKnowledge::Known
    }

    /// Given a perfect assistant and a poor one
    /// When both read a player they only see at a distance
    /// Then the perfect one is exact and the poor one is off by a bounded amount, the same way
    /// every time.
    #[test]
    fn the_error_is_bounded_exact_for_a_perfect_assistant_and_repeatable() {
        assert_eq!(blur_points(100.0, "a", "p", &known()), 0.0);
        let first = blur_points(0.0, "a", "p", &known());
        assert_eq!(first, blur_points(0.0, "a", "p", &known()));
        for id in 0..200 {
            let blur = blur_points(0.0, "a", &format!("p{id}"), &known());
            assert!(blur.abs() <= MAX_BLUR_POINTS as f32, "{blur}");
        }
    }

    /// Given the same pair at two neighbouring ratings
    /// When the error is read
    /// Then it moves by a fraction of a point: competence matters gradually.
    #[test]
    fn one_rating_point_changes_the_error_by_a_fraction_of_a_point() {
        for id in 0..50 {
            let player = format!("p{id}");
            for rating in 20..80 {
                let a = blur_points(f64::from(rating), "a", &player, &known());
                let b = blur_points(f64::from(rating + 1), "a", &player, &known());
                assert!((a - b).abs() <= 0.07, "{player} {rating}: {a} vs {b}");
            }
        }
    }

    /// Given the club's own player and a player a scout has reported on
    /// When a poor assistant reads them
    /// Then the own player is exact and the scouted one is read with half the reach.
    #[test]
    fn own_players_are_exact_and_a_scout_report_halves_the_error() {
        assert_eq!(blur_points(0.0, "a", "p", &PlayerKnowledge::Own), 0.0);
        let scouted = PlayerKnowledge::Scouted {
            scouted_on: "2026-08-01".into(),
            current: true,
        };
        let stale = PlayerKnowledge::Scouted {
            scouted_on: "2026-08-01".into(),
            current: false,
        };
        let full = blur_points(0.0, "a", "p", &known());
        assert!((blur_points(0.0, "a", "p", &scouted) - full * 0.5).abs() < 1e-4);
        assert!((blur_points(0.0, "a", "p", &stale) - full).abs() < 1e-4);
    }
}
