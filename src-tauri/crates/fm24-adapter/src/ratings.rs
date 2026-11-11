//! The level of a player on OFM's scale, anchored on FM's Current Ability.
//!
//! The 19 OFM attributes are blended from FM's 52, each on the same 1-100 scale (FM's 1-20 times
//! five). A weighted mean of attributes looks reasonable and is badly compressed: a world-class
//! full-back has 90s on pace and acceleration and ordinary values on much of the rest, so his mean
//! lands in the high 60s, hardly above a competent second-division defender's. FM's own overall
//! for the player is Current Ability (CA, 1-200), which is already position-weighted by FM, so the
//! level comes from CA through a monotone curve and the attributes are moved to that level with
//! their shape kept (a fast full-back stays fast, a keeper stays a keeper).
//!
//! The curve is data (`[ratings]` in `adapter.toml`), not code, so its calibration is visible and a
//! different world can ship another.

use crate::config::RatingsCfg;

/// OFM rating for an FM Current Ability (or Potential Ability), by linear interpolation between the
/// configured knots; below the first knot and above the last it takes their values.
pub fn ovr_from_ca(ca: i64, cfg: &RatingsCfg) -> u8 {
    let knots = &cfg.ca_to_ovr;
    let ca = ca as f64;
    let Some(first) = knots.first() else {
        return 1;
    };
    if ca <= first[0] {
        return first[1].round().clamp(1.0, 99.0) as u8;
    }
    for pair in knots.windows(2) {
        let (low, high) = (pair[0], pair[1]);
        if ca <= high[0] {
            let share = (ca - low[0]) / (high[0] - low[0]);
            return (low[1] + share * (high[1] - low[1]))
                .round()
                .clamp(1.0, 99.0) as u8;
        }
    }
    knots
        .last()
        .map_or(1, |last| last[1].round().clamp(1.0, 99.0) as u8)
}

/// Potential on the same scale as the rating, never below it and never above 99. A player whose
/// PA is unknown (0) has no headroom to claim.
pub fn potential(ovr: u8, pa: i64, cfg: &RatingsCfg) -> u8 {
    if pa <= 0 {
        return ovr;
    }
    ovr_from_ca(pa, cfg).max(ovr).min(99)
}

/// Whether the knots are usable: at least two, strictly increasing in CA, never decreasing in
/// rating. A curve that is not monotone would let a worse player outrank a better one.
pub fn validate(cfg: &RatingsCfg) -> Result<(), String> {
    let knots = &cfg.ca_to_ovr;
    if knots.len() < 2 {
        return Err("ratings.ca_to_ovr needs at least two knots".to_string());
    }
    for pair in knots.windows(2) {
        if pair[1][0] <= pair[0][0] {
            return Err(format!(
                "ratings.ca_to_ovr: CA must increase ({} then {})",
                pair[0][0], pair[1][0]
            ));
        }
        if pair[1][1] < pair[0][1] {
            return Err(format!(
                "ratings.ca_to_ovr: the rating must not fall as CA rises ({} then {})",
                pair[0][1], pair[1][1]
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg() -> RatingsCfg {
        Config::default_config().ratings
    }

    /// Given the shipped curve
    /// When it is checked
    /// Then it is a valid monotone curve.
    #[test]
    fn the_shipped_curve_is_valid() {
        validate(&cfg()).expect("valid");
    }

    /// Given any two abilities in order
    /// When they are rated
    /// Then the better is never rated lower.
    #[test]
    fn a_better_ability_is_never_rated_lower() {
        let cfg = cfg();
        let mut previous = 0;
        for ca in 0..=200 {
            let rating = ovr_from_ca(ca, &cfg);
            assert!(rating >= previous, "CA {ca}: {rating} after {previous}");
            previous = rating;
        }
    }

    /// Given abilities typical of a division floor, a strong top-flight starter and the very best
    /// When they are rated
    /// Then the hierarchy spans the scale the game uses (a second-division floor in the high 50s,
    /// a star in the high 80s or more), not the 55-70 band an attribute mean gives.
    #[test]
    fn the_curve_spreads_the_hierarchy_over_the_scale() {
        let cfg = cfg();
        assert!((55..=60).contains(&ovr_from_ca(75, &cfg)), "division floor");
        assert!(ovr_from_ca(150, &cfg) >= 82, "strong top-flight starter");
        assert!(ovr_from_ca(180, &cfg) >= 88, "world class");
        assert!(ovr_from_ca(180, &cfg) - ovr_from_ca(75, &cfg) >= 28);
    }

    /// Given a curve that falls, or whose CA does not increase
    /// When it is validated
    /// Then it is refused.
    #[test]
    fn a_curve_that_could_reverse_the_hierarchy_is_refused() {
        let falling = RatingsCfg {
            ca_to_ovr: vec![[50.0, 60.0], [100.0, 55.0]],
            ..cfg()
        };
        assert!(validate(&falling).is_err());
        let repeated = RatingsCfg {
            ca_to_ovr: vec![[50.0, 50.0], [50.0, 60.0]],
            ..cfg()
        };
        assert!(validate(&repeated).is_err());
    }

    /// Given a potential at or above the current ability, and one below it, and an unknown one
    /// When potential is rated
    /// Then it is never below the rating or above 99, and unknown means no headroom.
    #[test]
    fn potential_is_on_the_same_scale_and_bounded() {
        let cfg = cfg();
        let now = ovr_from_ca(110, &cfg);
        assert!(potential(now, 160, &cfg) > now);
        assert_eq!(potential(now, 90, &cfg), now, "never below the rating");
        assert_eq!(potential(now, 0, &cfg), now, "unknown PA claims nothing");
        assert!(potential(98, 200, &cfg) <= 99);
    }

    // --- conversion of whole players ---------------------------------------------------------

    use crate::journal::Journal;
    use crate::pipeline::convert_player;
    use crate::source::{
        AttributeScale, FmAttrs, detect_attribute_scale, normalise_attribute_scale, sample_row,
    };

    /// A world-class attacking full-back: electric pace, acceleration and stamina, ordinary
    /// everywhere that matters less for the post (FM scale, 1-100).
    const FAST_FULLBACK: [(&str, i64); 8] = [
        ("pace", 95),
        ("acceleration", 95),
        ("stamina", 85),
        ("tackling", 60),
        ("marking", 55),
        ("positioning", 60),
        ("passing", 60),
        ("crossing", 80),
    ];

    /// A competent defender of a second division: solid in the middle of every defensive trait.
    const SOLID_CENTRE_BACK: [(&str, i64); 8] = [
        ("tackling", 70),
        ("marking", 70),
        ("positioning", 70),
        ("strength", 70),
        ("heading", 70),
        ("anticipation", 65),
        ("pace", 45),
        ("acceleration", 45),
    ];

    fn converted(
        ca: i64,
        pa: i64,
        code: &str,
        attributes: &[(&'static str, i64)],
    ) -> crate::pipeline::Converted {
        let cfg = Config::default_config();
        let row = sample_row(ca, pa, code, attributes);
        convert_player(&row, &cfg, &mut Journal::default()).expect("a usable player")
    }

    /// Given an elite full-back and a second-division centre-back whose attribute means are close
    /// When both are converted
    /// Then the full-back, who has the much higher Current Ability, is rated clearly above the
    /// centre-back: the hierarchy follows ability, not the arithmetic of attributes.
    #[test]
    fn an_elite_fullback_outranks_a_second_division_defender() {
        let elite = converted(150, 155, "DR", &FAST_FULLBACK);
        let solid = converted(95, 95, "DC", &SOLID_CENTRE_BACK);
        assert!(
            elite.ovr >= solid.ovr + 12,
            "elite {} vs solid {}",
            elite.ovr,
            solid.ovr
        );
        let unanchored = {
            let mut cfg = Config::default_config();
            cfg.ratings.anchor_to_ca = false;
            let row = sample_row(150, 155, "DR", &FAST_FULLBACK);
            convert_player(&row, &cfg, &mut Journal::default()).unwrap()
        };
        assert!(
            unanchored.ovr < 75,
            "without the anchor the attribute mean is {}: the compression this fixes",
            unanchored.ovr
        );
    }

    /// Given players of the same ability at different positions (a keeper, a centre-back, a winger,
    /// a striker)
    /// When they are converted
    /// Then they are rated alike (within a point of the curve): a position does not carry a
    /// penalty of its own.
    #[test]
    fn the_same_ability_gives_the_same_rating_at_every_position() {
        let cfg = Config::default_config();
        let expected = ovr_from_ca(130, &cfg.ratings);
        let attrs_of = |pairs: &[(&'static str, i64)]| pairs.to_vec();
        for (code, attributes) in [
            (
                "GK",
                attrs_of(&[("handling", 80), ("reflexes", 85), ("one_on_ones", 75)]),
            ),
            ("DC", attrs_of(&SOLID_CENTRE_BACK)),
            (
                "AML",
                attrs_of(&[("dribbling", 85), ("pace", 90), ("acceleration", 90)]),
            ),
            (
                "STC",
                attrs_of(&[("finishing", 85), ("off_the_ball", 80), ("composure", 75)]),
            ),
        ] {
            let player = converted(130, 130, code, &attributes);
            assert!(
                (i16::from(player.ovr) - i16::from(expected)).abs() <= 1,
                "{code}: {} vs {expected}",
                player.ovr
            );
        }
    }

    /// Given a fast full-back at the anchor
    /// When his attributes are looked at
    /// Then his specialism is kept: pace stays far above passing, and it is his pace and stamina
    /// (not an irrelevant trait) that carry the rating.
    #[test]
    fn anchoring_keeps_the_shape_of_a_specialist() {
        let player = converted(150, 155, "DR", &FAST_FULLBACK);
        assert!(player.attributes.pace >= player.attributes.passing + 20);
        assert!(player.attributes.pace >= player.attributes.shooting + 20);
    }

    /// Given the same player twice
    /// When converted
    /// Then the result is identical (no randomness).
    #[test]
    fn conversion_is_deterministic() {
        let first = converted(140, 160, "MC", &[("passing", 80), ("vision", 78)]);
        let second = converted(140, 160, "MC", &[("passing", 80), ("vision", 78)]);
        assert_eq!(first.ovr, second.ovr);
        assert_eq!(first.attributes, second.attributes);
        assert_eq!(first.potential, second.potential);
    }

    /// Given a young player with a high potential and an older one at his peak
    /// When converted
    /// Then potential is never below the current rating, and the young one has headroom.
    #[test]
    fn potential_is_headroom_over_the_rating_not_a_second_rating() {
        let young = converted(80, 160, "STC", &[("finishing", 70)]);
        let peak = converted(150, 150, "STC", &[("finishing", 85)]);
        assert!(
            young.potential >= young.ovr + 20,
            "{} / {}",
            young.potential,
            young.ovr
        );
        assert_eq!(peak.potential, peak.ovr);
    }

    /// Given a player the save gives no ability for
    /// When converted
    /// Then he keeps the rating his attributes give and the journal says he was not anchored.
    #[test]
    fn a_player_without_ability_keeps_his_attribute_rating_and_is_reported() {
        let cfg = Config::default_config();
        let row = sample_row(0, 0, "MC", &[("passing", 80), ("vision", 78)]);
        let mut journal = Journal::default();
        let player = convert_player(&row, &cfg, &mut journal).unwrap();
        let by_attributes = crate::attributes::ovr(&player.attributes, &player.position);
        assert_eq!(player.ovr, by_attributes);
        assert_eq!(journal.count("player.rating_unanchored"), 1);
    }

    /// Given attribute columns that are FM's own 1-20, and others that are already 1-100
    /// When the scale is detected and normalised
    /// Then 1-20 is multiplied by five once, 1-100 is left alone, and normalising twice changes
    /// nothing more (attributes are never converted twice).
    #[test]
    fn the_attribute_scale_is_detected_and_applied_exactly_once() {
        let mut one_to_twenty = vec![sample_row(100, 100, "MC", &[])];
        one_to_twenty[0].attrs = FmAttrs(
            crate::source::ATTRIBUTE_COLUMNS
                .iter()
                .map(|n| (*n, 12))
                .collect(),
        );
        one_to_twenty[0].attrs.0.insert("pace", 19);
        assert_eq!(
            detect_attribute_scale(&one_to_twenty),
            Some(AttributeScale::OneToTwenty)
        );
        assert_eq!(
            normalise_attribute_scale(&mut one_to_twenty),
            Some(AttributeScale::OneToTwenty)
        );
        assert_eq!(one_to_twenty[0].attrs.get("pace"), 95.0);
        assert_eq!(
            normalise_attribute_scale(&mut one_to_twenty),
            Some(AttributeScale::OneToHundred)
        );
        assert_eq!(
            one_to_twenty[0].attrs.get("pace"),
            95.0,
            "the second pass changes nothing"
        );

        let mut one_to_hundred = vec![sample_row(100, 100, "MC", &[("pace", 90)])];
        assert_eq!(
            normalise_attribute_scale(&mut one_to_hundred),
            Some(AttributeScale::OneToHundred)
        );
        assert_eq!(one_to_hundred[0].attrs.get("pace"), 90.0);
        assert_eq!(detect_attribute_scale(&[]), None);
    }
}
