//! What a club is willing to pay for strength, and how far ahead it looks.
//!
//! A contender buys to win now and will pay for it; a small club that develops its own looks
//! further ahead and counts every pound. These are *tendencies* read from the club's existing
//! strategy profile (`transfers::strategy`), not scripts: they tilt two numbers — how much each
//! horizon year counts, and how many points of strength a club asks for its whole transfer budget
//! — and the decision is still made by what a player adds to the side.

use super::horizon::HORIZON;
use crate::transfers::{Ambition, ClubProfile, Means};

/// How a club weighs strength against money and time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appetite {
    /// How much this season, next season and the one after count; sums to one.
    pub weights: [f32; HORIZON],
    /// Points of expected team strength (mean rating of the eleven slots) the club asks for
    /// spending its whole transfer budget, or a whole week's wage budget on one contract. Lower is
    /// more eager to spend.
    pub price: f32,
}

/// Weight on the present for each ambition; the rest is split between the two years after.
const HORIZON_WEIGHTS: [(Ambition, [f32; HORIZON]); 3] = [
    (Ambition::Contender, [0.70, 0.22, 0.08]),
    (Ambition::Established, [0.55, 0.30, 0.15]),
    (Ambition::Fighter, [0.45, 0.33, 0.22]),
];

/// How much of the present's weight a fully youth-minded club moves to the second year ahead.
const YOUTH_SHIFT: f32 = 0.15;

impl Appetite {
    /// The appetite of a club nothing is known about.
    pub(crate) const NEUTRAL: Self = Self {
        weights: [0.55, 0.30, 0.15],
        price: 1.0,
    };

    /// The appetite a club's strategy profile implies.
    pub(crate) fn of(profile: &ClubProfile) -> Self {
        let mut weights = HORIZON_WEIGHTS
            .iter()
            .find(|(ambition, _)| *ambition == profile.ambition)
            .map_or(Self::NEUTRAL.weights, |(_, weights)| *weights);
        let shift = YOUTH_SHIFT * profile.youth_focus.clamp(0.0, 1.0) as f32;
        weights[0] -= shift;
        weights[2] += shift;
        let means = match profile.means {
            Means::Wealthy => 0.6,
            Means::Comfortable => 1.0,
            Means::Constrained => 1.8,
        };
        let ambition = match profile.ambition {
            Ambition::Contender => 0.8,
            Ambition::Established => 1.0,
            Ambition::Fighter => 1.1,
        };
        Self {
            weights,
            price: means * ambition,
        }
    }

    /// The same appetite, seen by an assistant of `rating` (0 to 100). A better assistant plans
    /// further ahead: the weight he gives the years to come grows smoothly from 40 % of the club's
    /// own (a poor eye for the future) to all of it, and what he takes from them is given to the
    /// present. There is no rating at which anything switches on.
    pub(crate) fn seen_by_assistant(self, rating: f64) -> Self {
        let foresight = (0.4 + 0.6 * (rating / 100.0).clamp(0.0, 1.0)) as f32;
        let mut weights = self.weights;
        let future = (weights[1] + weights[2]) * foresight;
        let share = weights[1] / (weights[1] + weights[2]);
        weights[1] = future * share;
        weights[2] = future * (1.0 - share);
        weights[0] = 1.0 - weights[1] - weights[2];
        Self { weights, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(ambition: Ambition, means: Means, youth_focus: f64) -> ClubProfile {
        ClubProfile {
            ambition,
            means,
            youth_focus,
        }
    }

    /// Given every profile the market can produce
    /// When the appetite is read
    /// Then the horizon weights sum to one and no weight is negative.
    #[test]
    fn the_horizon_weights_are_always_a_distribution() {
        for ambition in [
            Ambition::Contender,
            Ambition::Established,
            Ambition::Fighter,
        ] {
            for means in [Means::Wealthy, Means::Comfortable, Means::Constrained] {
                for youth in [0.0, 0.5, 1.0] {
                    let a = Appetite::of(&profile(ambition, means, youth));
                    assert!((a.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                    assert!(a.weights.iter().all(|w| *w > 0.0));
                }
            }
        }
    }

    /// Given a contender and a small club that develops its own
    /// When their appetites are compared
    /// Then the contender counts the present more and pays more readily.
    #[test]
    fn a_contender_values_now_and_pays_more_readily_than_a_developing_club() {
        let contender = Appetite::of(&profile(Ambition::Contender, Means::Wealthy, 0.0));
        let fighter = Appetite::of(&profile(Ambition::Fighter, Means::Constrained, 1.0));
        assert!(contender.weights[0] > fighter.weights[0]);
        assert!(contender.price < fighter.price);
    }

    /// Given assistants of rating 0, 50 and 100
    /// When they read the same club's appetite
    /// Then the weight on the future rises smoothly with the rating and never jumps.
    #[test]
    fn an_assistants_foresight_grows_smoothly_with_his_rating() {
        let base = Appetite::NEUTRAL;
        let mut previous = base.seen_by_assistant(0.0).weights[1];
        assert!(previous < base.weights[1]);
        for rating in 1..=100 {
            let seen = base.seen_by_assistant(f64::from(rating));
            assert!((seen.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
            let step = seen.weights[1] - previous;
            assert!((0.0..0.005).contains(&step), "rating {rating}: step {step}");
            previous = seen.weights[1];
        }
        assert!((base.seen_by_assistant(100.0).weights[1] - base.weights[1]).abs() < 1e-5);
    }
}
