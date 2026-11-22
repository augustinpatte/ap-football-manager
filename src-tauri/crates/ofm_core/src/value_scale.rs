//! The scale a world's market values sit on, and the one rule that carries a value through time.
//!
//! A world's values have a scale of their own. The shipped generator's players are worth a few
//! million apiece; an imported FM world's best players are worth over a hundred million and its
//! ordinary ones a few hundred thousand. A projection that re-priced a player from a fixed formula
//! every season would put each of them on the formula's scale the first time it ran: the stars of an
//! imported world lose four fifths of their value and the ordinary players gain a hundredfold.
//!
//! So a value is *carried*, not recomputed: it keeps the position it had on its world's scale and
//! moves with what the player becomes. [`ValueScale::carry`] multiplies a player's value by the
//! scale's own ratio between the state he is in and the one he was in. The scale is the world's
//! (`anchors` and `age_factors`, fitted from its own players when the world is built); a world
//! without one uses the native formula, which is what projection has always used for it.
//!
//! A player who has no value yet (a newgen) takes [`ValueScale::value`], so he enters the market at
//! the level of players like him, not at a fixed price.

use serde::{Deserialize, Serialize};

/// Bumped when the meaning of a scale's fields changes, so a stored scale of another version is
/// ignored rather than misread.
pub const VALUE_SCALE_VERSION: u32 = 1;

/// The first age with an entry in [`ValueScale::age_factors`]; older and younger ages use the last
/// and first entry.
pub const FIRST_AGE: i32 = 16;

/// What a player is, as far as his value is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub ovr: u8,
    pub potential: u8,
    pub age: i32,
}

/// The share of the distance to his ceiling that is bought along with a player's rating: a teenager
/// is bought for what he will be, a twenty-six-year-old for what he is.
pub(crate) fn ceiling_share(age: i32) -> f64 {
    match age {
        ..=19 => 0.6,
        20..=21 => 0.45,
        22..=23 => 0.3,
        24..=25 => 0.15,
        _ => 0.0,
    }
}

/// A player's rating with the credited part of his ceiling added: what the market prices him on.
pub fn quality(ovr: u8, potential: u8, age: i32) -> f64 {
    let gap = (i32::from(potential) - i32::from(ovr)).max(0) as f64;
    f64::from(ovr) + gap * ceiling_share(age)
}

/// The value the native formula gives a player: a cubic in rating with a youth premium and a
/// credit for the distance to his ceiling. The formula every projection used before worlds carried a
/// scale, and still the one for a world that has none.
pub fn native_value(ovr: u8, potential: u8, age: i32) -> u64 {
    if ovr == 0 {
        return 0;
    }
    let ability = f64::from(ovr);
    let ceiling = f64::from(potential.max(ovr));
    let youth = if age <= 21 {
        1.6
    } else if age <= 25 {
        1.3
    } else if age <= 29 {
        1.0
    } else if age <= 32 {
        0.6
    } else {
        0.25
    };
    let growth = 1.0 + (ceiling - ability).max(0.0) / 100.0;
    ((ability / 50.0).powi(3) * 2_000_000.0 * youth * growth).max(0.0) as u64
}

/// A world's value scale. Empty (`Default`) means "the native formula".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueScale {
    pub version: u32,
    /// The value, in euros, of a player of prime age at each of these ratings, ascending by rating.
    /// Between two anchors the value is interpolated on a logarithmic scale.
    pub anchors: Vec<(u8, u64)>,
    /// How much of the prime-age value a player of each age is worth, in thousandths, from
    /// [`FIRST_AGE`] up.
    pub age_factors: Vec<u32>,
}

impl ValueScale {
    /// The scale of a world that carries none.
    pub fn native() -> Self {
        Self::default()
    }

    /// Whether this scale was fitted from a world's players (rather than the native formula).
    pub fn is_calibrated(&self) -> bool {
        self.version == VALUE_SCALE_VERSION
            && self.anchors.len() >= 2
            && !self.age_factors.is_empty()
    }

    fn curve(&self, rating: f64) -> f64 {
        let ln = |value: u64| (value.max(1) as f64).ln();
        let rating = rating.clamp(0.0, 120.0);
        // The segment that holds `rating`, or the end one, extended, beyond the anchors.
        let index = self
            .anchors
            .partition_point(|(anchor, _)| f64::from(*anchor) <= rating)
            .clamp(1, self.anchors.len() - 1);
        let (low, high) = (self.anchors[index - 1], self.anchors[index]);
        let span = f64::from(high.0) - f64::from(low.0);
        let slope = if span > 0.0 {
            (ln(high.1) - ln(low.1)) / span
        } else {
            0.0
        };
        (ln(low.1) + (rating - f64::from(low.0)) * slope).exp()
    }

    /// The value of a prime-age player of this quality (a rating, with the credited part of his
    /// ceiling added) on a calibrated scale; what the age factors are measured against.
    pub fn prime_value(&self, quality: f64) -> f64 {
        if self.anchors.len() < 2 {
            return 0.0;
        }
        self.curve(quality)
    }

    fn age_factor(&self, age: i32) -> f64 {
        let index = (age - FIRST_AGE).clamp(0, self.age_factors.len() as i32 - 1) as usize;
        f64::from(self.age_factors[index]) / 1000.0
    }

    /// What a player is worth on this world's scale, from nothing but what he is.
    pub fn value(&self, standing: Standing) -> u64 {
        if !self.is_calibrated() {
            return native_value(standing.ovr, standing.potential, standing.age);
        }
        let priced = self.curve(quality(standing.ovr, standing.potential, standing.age));
        round_significant(priced * self.age_factor(standing.age)) as u64
    }

    /// The value of a player who was worth `anchor` when he was in `from`, now that he is in `to`:
    /// his own value, moved by the scale's ratio between the two. A player whose state has not
    /// changed keeps his value exactly; one whose state is not on the scale (a zero) is left as he is.
    pub fn carry(&self, anchor: u64, from: Standing, to: Standing) -> u64 {
        if from == to || anchor == 0 {
            return anchor;
        }
        let (then, now) = (self.value(from), self.value(to));
        if then == 0 {
            return self.value(to);
        }
        round_significant(anchor as f64 * now as f64 / then as f64) as u64
    }
}

/// Three significant figures: a value does not claim a precision the model does not have, and a
/// last-bit difference of a platform's `ln`/`exp` cannot reach a saved world.
pub(crate) fn round_significant(value: f64) -> f64 {
    if value < 1.0 {
        return 0.0;
    }
    let magnitude = 10f64.powi(value.log10().floor() as i32 - 2);
    (value / magnitude).round() * magnitude
}

/// A scale shaped like an imported world's: roughly a tenfold rise in value for every ten rating
/// points at the top, a few hundred thousand euros for an ordinary player. For tests that need a world
/// whose scale is not the native one.
#[cfg(test)]
pub(crate) fn sample_imported_scale() -> ValueScale {
    ValueScale {
        version: VALUE_SCALE_VERSION,
        anchors: vec![
            (30, 12_000),
            (50, 60_000),
            (60, 190_000),
            (70, 1_200_000),
            (80, 18_000_000),
            (90, 120_000_000),
            (95, 160_000_000),
        ],
        // Ages 16..=40: young discount, a plateau 26-29, then a decline.
        age_factors: vec![
            400, 450, 520, 600, 700, 800, 900, 950, 980, 1000, 1000, 1000, 1000, 960, 900, 830,
            740, 640, 540, 440, 350, 280, 220, 170, 130,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing(ovr: u8, potential: u8, age: i32) -> Standing {
        Standing {
            ovr,
            potential,
            age,
        }
    }

    fn scale() -> ValueScale {
        sample_imported_scale()
    }

    /// Given a world that carries no scale,
    /// When a player is valued,
    /// Then he gets the native formula's value: a world without a scale behaves as it always did.
    #[test]
    fn a_world_without_a_scale_uses_the_native_formula() {
        let native = ValueScale::native();

        assert!(!native.is_calibrated());
        assert_eq!(native.value(standing(95, 95, 27)), native_value(95, 95, 27));
        assert!(native.carry(50_000_000, standing(80, 80, 25), standing(82, 82, 26)) > 0);
    }

    /// Given a state that has not changed,
    /// When a value is carried,
    /// Then it is exactly the value he had.
    #[test]
    fn a_player_who_has_not_changed_keeps_his_value_exactly() {
        let s = scale();

        assert_eq!(
            s.carry(121_000_000, standing(87, 89, 28), standing(87, 89, 28)),
            121_000_000
        );
    }

    /// Given a star worth 120 M and 14 M on the native scale for an elite 27-year-old,
    /// When he is carried a season on, ageing a year with his rating unchanged,
    /// Then he keeps almost all of his value: projection to another season is not a revaluation.
    #[test]
    fn a_star_carried_a_season_keeps_his_position_on_the_scale() {
        let s = scale();

        let next = s.carry(120_000_000, standing(90, 92, 27), standing(90, 92, 28));

        assert!(next > 100_000_000 && next <= 120_000_000, "{next}");
        // The native formula would have given him a different order of magnitude altogether.
        assert!(native_value(90, 92, 28) < 20_000_000);
    }

    /// Given an ordinary player worth 300 k on the scale,
    /// When he is carried a season on,
    /// Then he does not gain a hundredfold: the native formula's value for him is in millions.
    #[test]
    fn an_ordinary_player_is_not_inflated_by_the_season() {
        let s = scale();

        let next = s.carry(300_000, standing(62, 66, 26), standing(62, 66, 27));

        assert!(next <= 330_000, "{next}");
        assert!(native_value(62, 66, 27) > 2_000_000);
    }

    /// Given a young player who improves by four rating points in a season,
    /// When his value is carried,
    /// Then it rises, and by the scale's own ratio rather than a fixed factor.
    #[test]
    fn a_player_who_improves_is_worth_more_by_the_scales_own_ratio() {
        let s = scale();
        let (from, to) = (standing(70, 80, 21), standing(74, 80, 22));

        let next = s.carry(2_000_000, from, to);
        let ratio = s.value(to) as f64 / s.value(from) as f64;

        assert!(next > 2_000_000);
        assert!(
            (next as f64 / 2_000_000.0 - ratio).abs() < 0.02,
            "{next} {ratio}"
        );
    }

    /// Given the scale,
    /// When a rising rating is valued at a fixed age,
    /// Then the value never falls: the scale has no inversions.
    #[test]
    fn value_never_falls_as_rating_rises() {
        let s = scale();

        let values: Vec<u64> = (30..=99).map(|o| s.value(standing(o, o, 27))).collect();

        assert!(values.windows(2).all(|w| w[0] <= w[1]), "{values:?}");
    }

    /// Given the same player at 27 and at 36,
    /// When valued,
    /// Then the older is worth less, and a teenager with a ceiling is worth more than a veteran of
    /// the same rating.
    #[test]
    fn age_and_ceiling_move_the_value_the_way_a_market_does() {
        let s = scale();

        assert!(s.value(standing(80, 80, 36)) < s.value(standing(80, 80, 27)));
        assert!(s.value(standing(70, 90, 19)) > s.value(standing(70, 90, 30)));
    }

    /// Given a stored scale of another version or with no anchors,
    /// When it is used,
    /// Then it is ignored and the native formula applies, rather than a misread curve.
    #[test]
    fn a_scale_of_another_version_is_not_trusted() {
        let mut other = scale();
        other.version = VALUE_SCALE_VERSION + 1;

        assert!(!other.is_calibrated());
        assert_eq!(other.value(standing(80, 80, 27)), native_value(80, 80, 27));
    }

    /// Given an older save or world file with no scale,
    /// When it is read,
    /// Then it defaults to the native scale.
    #[test]
    fn a_missing_scale_reads_as_native() {
        let read: ValueScale = serde_json::from_str("{}").expect("an empty object reads");

        assert!(!read.is_calibrated());
    }
}
