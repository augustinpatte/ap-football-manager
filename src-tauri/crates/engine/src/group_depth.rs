//! How the engine reads a group of the eleven that is short of a full group.
//!
//! The resolvers read a group by the mean of the players standing in it: a random forward shoots,
//! a random defender faces him, and the men behind a shot are the mean of the defenders. That mean
//! is taken over the *deployed* group, so a squad's best players concentrated into a thin group are
//! worth the best of them: one 90-rated finisher alone up front shoots like a 90-rated finisher
//! every time, while the same man in a front three shares the shots with two 60s. The same is true
//! of three international centre backs behind a back three.
//!
//! A group is read here as if the slots it leaves empty were filled by an average outfield player of
//! the same side, up to a full group: four defenders, four midfielders, two forwards (the shape the
//! engine's numbers terms are written against). A full group is unaffected. The rule is level-
//! relative — the missing slots are filled at the side's own standard, not at a fixed 50 — so a
//! world whose players are all better or worse moves every group alike.
//!
//! **One definition, two readers.** The live Z4/5 engine reads it through
//! [`crate::shared::defensive_quality`] and the contest factors in `live_match::helpers`;
//! `ofm_core::formation_fit` reads it through [`depth_filled_mean`] so the formation the AI ranks
//! best is the formation the engine plays best. Anything that scores a formation must not use a
//! plain mean: that is the reading that sent P6.1's clubs into thin groups and lifted the league's
//! goals a match by 15 to 20 %.
//!
//! At [`MatchConfig::group_depth_weight`](crate::types::MatchConfig::group_depth_weight) 0 the
//! functions here are the identity: a mean is a plain mean and a factor is exactly 1.0, so a
//! configuration written before this existed plays the football it was calibrated on, bit for bit.

use crate::types::Position;

/// The number of players a full group of the eleven has: a back four, a midfield four, a front two.
pub const FULL_GROUP_DEFENDERS: usize = 4;
pub const FULL_GROUP_MIDFIELDERS: usize = 4;
pub const FULL_GROUP_FORWARDS: usize = 2;

/// The weight the shipped game reads groups at: the depth-aware reading, at full strength.
pub const SHIPPED_GROUP_DEPTH_WEIGHT: f64 = 1.0;

/// The number of players a full group of the eleven has, by group.
pub fn full_group(group: Position) -> usize {
    match group {
        Position::Defender => FULL_GROUP_DEFENDERS,
        Position::Midfielder => FULL_GROUP_MIDFIELDERS,
        Position::Forward => FULL_GROUP_FORWARDS,
        // A goalkeeper is a group of one; a group the engine does not draw from is read as it was.
        Position::Goalkeeper => 1,
    }
}

/// The mean of a group, with the slots it leaves empty of a full group filled at `baseline`.
///
/// `weight` 0 is a plain mean (so an older configuration is unchanged); `weight` 1 is the
/// depth-aware reading; between the two it is a blend. An empty group is `baseline` — which is what
/// the engine already fell back to when a side had nobody in a group.
pub fn depth_filled_mean(values: &[f64], reference: usize, baseline: f64, weight: f64) -> f64 {
    if values.is_empty() {
        return baseline;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let weight = weight.clamp(0.0, 1.0);
    if weight == 0.0 || reference == 0 || values.len() >= reference {
        return mean;
    }
    let shortfall = (reference - values.len()) as f64;
    let filled = (values.iter().sum::<f64>() + shortfall * baseline) / reference as f64;
    mean + weight * (filled - mean)
}

/// What a group short of a full one is worth **as a multiplier on one of its players**, relative to
/// the plain mean the resolvers would otherwise read: 1.0 for a full group, below 1.0 for a thin
/// group of better-than-average players, above 1.0 for a thin group of worse ones.
///
/// This is the form the resolvers can use: they draw one player and rate *him*, so the group's
/// depth has to reach the contest as a factor on his rating. `outfield` is the side's whole outfield
/// on the pitch, the standard the missing slots are filled at. Exactly 1.0 when the weight is 0, so
/// a configuration written before this existed plays the same football.
pub fn depth_factor(group: &[f64], outfield: &[f64], reference: usize, weight: f64) -> f64 {
    if weight <= 0.0 || group.is_empty() || group.len() >= reference {
        return 1.0;
    }
    let mean = group.iter().sum::<f64>() / group.len() as f64;
    // A group rated at zero has no ratio to return; an attribute mean is never negative.
    if !mean.is_finite() || mean <= 0.0 {
        return 1.0;
    }
    let baseline = if outfield.is_empty() {
        mean
    } else {
        outfield.iter().sum::<f64>() / outfield.len() as f64
    };
    depth_filled_mean(group, reference, baseline, weight) / mean
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a group with the players a full group has
    /// When it is read at any weight
    /// Then nothing changes: the reading only speaks about the slots a side leaves empty.
    #[test]
    fn a_full_group_is_its_plain_mean_at_every_weight() {
        let values = [70.0, 60.0, 80.0, 50.0];
        for weight in [0.0, 0.5, 1.0] {
            assert_eq!(
                depth_filled_mean(&values, FULL_GROUP_DEFENDERS, 65.0, weight),
                65.0
            );
            assert_eq!(
                depth_factor(&values, &values, FULL_GROUP_DEFENDERS, weight),
                1.0
            );
        }
    }

    /// Given a thin group of players better than the side's average, and the same group full
    /// When each is read
    /// Then the thin group is worth less than its own plain mean: a squad's best players do not
    ///      keep the best of themselves when they are concentrated into a group nobody else fills.
    #[test]
    fn a_thin_group_of_the_best_players_is_worth_less_than_its_mean() {
        let thin = [90.0];
        let full = [90.0, 60.0];
        let plain_thin = 90.0;
        let read_thin = depth_filled_mean(&thin, FULL_GROUP_FORWARDS, 60.0, 1.0);
        let read_full = depth_filled_mean(&full, FULL_GROUP_FORWARDS, 60.0, 1.0);
        assert!((read_thin - 75.0).abs() < 1e-12);
        assert!((read_full - 75.0).abs() < 1e-12);
        assert!(
            read_thin < plain_thin,
            "the lone 90 finisher is still read as a 90"
        );
        assert!(
            (read_thin - read_full).abs() < 1e-12,
            "one 90 and a 60 with one slot empty is the same group as the two of them"
        );
    }

    /// Given the same thin and full groups of better-than-average players
    /// When each is read as the factor the resolvers apply to a drawn player
    /// Then the thin one's drawn player is worth less, and the full one's is worth what he is.
    #[test]
    fn the_drawn_player_of_a_thin_group_is_worth_less() {
        let outfield = [70.0, 70.0, 70.0, 60.0, 60.0, 60.0, 60.0, 60.0, 60.0, 60.0];
        let thin = depth_factor(&[90.0], &outfield, FULL_GROUP_FORWARDS, 1.0);
        let full = depth_factor(&[90.0, 60.0], &outfield, FULL_GROUP_FORWARDS, 1.0);
        assert!(thin < 1.0, "a lone star is free: {thin}");
        assert_eq!(full, 1.0);
    }

    /// Given a thin group of players worse than the side's average
    /// When it is read
    /// Then it is worth more than its plain mean: the mean over-rewards a bad thin group too, and
    ///      the reading is about the group's depth, not about punishing good players.
    #[test]
    fn a_thin_group_of_weaker_players_is_worth_more_than_its_mean() {
        let read = depth_filled_mean(&[40.0], FULL_GROUP_DEFENDERS, 70.0, 1.0);
        assert!(read > 40.0);
        let factor = depth_factor(
            &[40.0],
            &[70.0, 70.0, 70.0, 70.0, 70.0, 70.0, 70.0, 70.0, 70.0, 70.0],
            FULL_GROUP_DEFENDERS,
            1.0,
        );
        assert!(factor > 1.0, "{factor}");
    }

    /// Given a configuration written before the reading existed (weight 0)
    /// When a thin group is read
    /// Then every function is the identity: the mean is the plain mean and the factor is exactly
    ///      1.0, so the football it was calibrated on is unchanged bit for bit.
    #[test]
    fn a_weight_of_zero_is_bit_for_bit_the_plain_mean() {
        let thin = [90.0, 55.0];
        assert_eq!(
            depth_filled_mean(&thin, FULL_GROUP_DEFENDERS, 70.0, 0.0),
            72.5
        );
        assert_eq!(
            depth_factor(&thin, &[70.0; 10], FULL_GROUP_DEFENDERS, 0.0),
            1.0
        );
    }

    /// Given a group with nobody in it
    /// When it is read
    /// Then it is the side's own standard, which is the fallback the engine already used.
    #[test]
    fn an_empty_group_is_the_sides_own_standard() {
        assert_eq!(depth_filled_mean(&[], FULL_GROUP_FORWARDS, 64.0, 1.0), 64.0);
        assert_eq!(
            depth_factor(&[], &[64.0; 10], FULL_GROUP_FORWARDS, 1.0),
            1.0
        );
    }

    /// Given the groups a 4-4-2 eleven fills
    /// When a full group is asked for
    /// Then it is four defenders, four midfielders, two forwards, and a keeper is one.
    #[test]
    fn a_full_group_is_the_shape_the_numbers_terms_are_written_against() {
        assert_eq!(full_group(Position::Defender), 4);
        assert_eq!(full_group(Position::Midfielder), 4);
        assert_eq!(full_group(Position::Forward), 2);
        assert_eq!(full_group(Position::Goalkeeper), 1);
    }
}
