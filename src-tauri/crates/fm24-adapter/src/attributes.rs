//! 52 FM attributes (1-100) -> the 19 OFM attributes (1-99), and the OFM potential.
//!
//! A deterministic, position-aware reduction -- never one average of everything. Every OFM
//! attribute is a weighted blend of the FM attributes that mean the same thing on the pitch;
//! three of them (`positioning`, `reflexes`, `aerial`) change recipe with the player's role,
//! because FM spreads "where to stand" over *Positioning* (defenders, keepers) and *Off the Ball*
//! (attackers). The FM originals are never modified: they stay in world.db and in the extension
//! file next to the world.
//!
//! Weights sum to 1 in every recipe (tested), so a player whose relevant FM attributes are all `x`
//! gets `x` back: the reduction changes the shape, not the level. Level calibration against OFM's
//! own tiers is a separate, documented step (see docs/FM24_IMPORT_VALIDATION.md).

use crate::source::FmAttrs;
use domain::player::{PlayerAttributes, Position};

/// The role family that decides the recipes of the role-dependent attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Goalkeeper,
    /// Centre-backs, full-backs, wing-backs, defensive midfielders: "stand in the right place".
    Defensive,
    /// Central midfielders.
    Midfield,
    /// Attacking midfielders, wide midfielders, wingers: "find the space".
    Creative,
    Striker,
}

fn role_of(position: &Position) -> Role {
    match position {
        Position::Goalkeeper => Role::Goalkeeper,
        Position::CenterBack
        | Position::LeftBack
        | Position::RightBack
        | Position::LeftWingBack
        | Position::RightWingBack
        | Position::DefensiveMidfielder
        | Position::Defender => Role::Defensive,
        Position::CentralMidfielder | Position::Midfielder => Role::Midfield,
        Position::AttackingMidfielder
        | Position::LeftMidfielder
        | Position::RightMidfielder
        | Position::LeftWinger
        | Position::RightWinger => Role::Creative,
        Position::Striker | Position::Forward => Role::Striker,
    }
}

/// Weighted blend of FM attributes, rounded and bounded to OFM's 1-99.
fn blend(fm: &FmAttrs, recipe: &[(&str, f64)]) -> u8 {
    let total: f64 = recipe.iter().map(|(_, weight)| weight).sum();
    let value: f64 = recipe
        .iter()
        .map(|(name, weight)| fm.get(name) * weight)
        .sum::<f64>()
        / total;
    value.round().clamp(1.0, 99.0) as u8
}

/// The recipes, as data, so the documentation and the tests read the same table as the code.
pub fn recipe(name: &str, position: &Position) -> Vec<(&'static str, f64)> {
    let role = role_of(position);
    match name {
        "pace" => vec![("pace", 0.6), ("acceleration", 0.4)],
        "stamina" => vec![("stamina", 0.75), ("natural_fitness", 0.25)],
        "strength" => vec![("strength", 1.0)],
        "agility" => vec![("agility", 0.5), ("balance", 0.5)],
        "passing" => vec![("passing", 0.7), ("technique", 0.3)],
        "shooting" => vec![("finishing", 0.7), ("long_shots", 0.3)],
        "tackling" => vec![("tackling", 1.0)],
        "dribbling" => vec![("dribbling", 0.6), ("technique", 0.2), ("first_touch", 0.2)],
        "defending" => vec![
            ("marking", 0.5),
            ("anticipation", 0.25),
            ("concentration", 0.25),
        ],
        "positioning" => match role {
            Role::Goalkeeper => vec![("positioning", 0.7), ("command_of_area", 0.3)],
            Role::Defensive => vec![("positioning", 0.8), ("anticipation", 0.2)],
            Role::Midfield => vec![("positioning", 0.5), ("off_the_ball", 0.5)],
            Role::Creative => vec![("positioning", 0.25), ("off_the_ball", 0.75)],
            Role::Striker => vec![("positioning", 0.2), ("off_the_ball", 0.8)],
        },
        "vision" => vec![("vision", 0.85), ("flair", 0.15)],
        "decisions" => vec![("decisions", 0.8), ("anticipation", 0.2)],
        "composure" => vec![("composure", 0.8), ("important_matches", 0.2)],
        "aggression" => vec![("aggression", 0.7), ("bravery", 0.3)],
        "teamwork" => vec![("teamwork", 0.7), ("work_rate", 0.3)],
        "leadership" => vec![("leadership", 0.8), ("determination", 0.2)],
        "handling" => vec![("handling", 1.0)],
        "reflexes" => match role {
            Role::Goalkeeper => vec![("reflexes", 0.65), ("one_on_ones", 0.2), ("agility", 0.15)],
            _ => vec![("reflexes", 1.0)],
        },
        "aerial" => match role {
            Role::Goalkeeper => vec![
                ("aerial_reach", 0.5),
                ("command_of_area", 0.3),
                ("jumping_reach", 0.2),
            ],
            _ => vec![("heading", 0.55), ("jumping_reach", 0.45)],
        },
        other => panic!("unknown OFM attribute {other}"),
    }
}

/// The 19 OFM attributes, in the engine's own order.
pub const OFM_ATTRIBUTES: [&str; 19] = [
    "pace",
    "stamina",
    "strength",
    "agility",
    "passing",
    "shooting",
    "tackling",
    "dribbling",
    "defending",
    "positioning",
    "vision",
    "decisions",
    "composure",
    "aggression",
    "teamwork",
    "leadership",
    "handling",
    "reflexes",
    "aerial",
];

pub fn convert(fm: &FmAttrs, position: &Position) -> PlayerAttributes {
    let get = |name: &str| blend(fm, &recipe(name, position));
    PlayerAttributes {
        pace: get("pace"),
        stamina: get("stamina"),
        strength: get("strength"),
        agility: get("agility"),
        passing: get("passing"),
        shooting: get("shooting"),
        tackling: get("tackling"),
        dribbling: get("dribbling"),
        defending: get("defending"),
        positioning: get("positioning"),
        vision: get("vision"),
        decisions: get("decisions"),
        composure: get("composure"),
        aggression: get("aggression"),
        teamwork: get("teamwork"),
        leadership: get("leadership"),
        handling: get("handling"),
        reflexes: get("reflexes"),
        aerial: get("aerial"),
    }
}

/// OFM's own position-weighted rating of a converted block.
pub fn ovr(attributes: &PlayerAttributes, position: &Position) -> u8 {
    ofm_core::player_rating::ovr_from_attributes(attributes, position).round() as u8
}

/// Potential on OFM's scale: the OFM rating plus FM's remaining headroom (PA - CA, FM 1-200)
/// converted at `points_per_ca` OFM points per FM CA point. Never below the rating, never above 99.
/// An additive headroom (not a ratio) keeps a low-CA prospect from being inflated by a small CA.
pub fn potential(ovr: u8, ca: i64, pa: i64, points_per_ca: f64) -> u8 {
    let headroom = ((pa - ca).max(0) as f64 * points_per_ca).round() as i64;
    (i64::from(ovr) + headroom).clamp(i64::from(ovr).max(1), 99) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_POSITION: [Position; 14] = [
        Position::Goalkeeper,
        Position::CenterBack,
        Position::LeftBack,
        Position::RightBack,
        Position::LeftWingBack,
        Position::RightWingBack,
        Position::DefensiveMidfielder,
        Position::CentralMidfielder,
        Position::LeftMidfielder,
        Position::RightMidfielder,
        Position::AttackingMidfielder,
        Position::LeftWinger,
        Position::RightWinger,
        Position::Striker,
    ];

    #[test]
    fn every_recipe_sums_to_one_and_names_real_fm_attributes() {
        for position in &EVERY_POSITION {
            for name in OFM_ATTRIBUTES {
                let recipe = recipe(name, position);
                let total: f64 = recipe.iter().map(|(_, w)| w).sum();
                assert!(
                    (total - 1.0).abs() < 1e-9,
                    "{name} for {position:?} sums to {total}"
                );
                for (fm_name, _) in recipe {
                    assert!(
                        crate::source::ATTRIBUTE_COLUMNS.contains(&fm_name),
                        "{fm_name} is not an FM attribute"
                    );
                }
            }
        }
    }

    #[test]
    fn a_flat_player_stays_flat() {
        let flat = FmAttrs::from_pairs(&[]);
        let converted = convert(&flat, &Position::Striker);
        assert_eq!(converted.pace, 50);
        assert_eq!(converted.aerial, 50);
        assert_eq!(converted.shooting, 50);
    }

    #[test]
    fn values_are_bounded_to_1_99() {
        let top = FmAttrs(
            crate::source::ATTRIBUTE_COLUMNS
                .iter()
                .map(|n| (*n, 100))
                .collect(),
        );
        let bottom = FmAttrs(
            crate::source::ATTRIBUTE_COLUMNS
                .iter()
                .map(|n| (*n, 0))
                .collect(),
        );
        assert_eq!(convert(&top, &Position::Striker).pace, 99);
        assert_eq!(convert(&bottom, &Position::Striker).pace, 1);
    }

    #[test]
    fn the_same_fm_player_gets_a_different_positioning_by_role() {
        // Positioning high, off the ball low: a defender stands in the right place, a striker does not.
        let fm = FmAttrs::from_pairs(&[
            ("positioning", 90),
            ("off_the_ball", 30),
            ("anticipation", 90),
        ]);
        let as_defender = convert(&fm, &Position::CenterBack).positioning;
        let as_striker = convert(&fm, &Position::Striker).positioning;
        assert!(
            as_defender > as_striker + 30,
            "{as_defender} vs {as_striker}"
        );
    }

    #[test]
    fn goalkeeper_attributes_use_the_goalkeeping_recipes() {
        let fm = FmAttrs::from_pairs(&[
            ("handling", 90),
            ("reflexes", 92),
            ("one_on_ones", 88),
            ("aerial_reach", 85),
            ("command_of_area", 85),
            ("jumping_reach", 80),
            ("heading", 20),
            ("agility", 80),
        ]);
        let gk = convert(&fm, &Position::Goalkeeper);
        let outfield = convert(&fm, &Position::CenterBack);
        assert!(gk.aerial >= 80, "keeper aerial {}", gk.aerial);
        assert!(
            outfield.aerial < gk.aerial,
            "heading 20 drags an outfielder's aerial down"
        );
        // A keeper's reflexes also reflect one-on-ones and agility; an outfielder's are just FM reflexes.
        assert_eq!(outfield.reflexes, 92);
        assert_eq!(gk.reflexes, 89);
        let worse_one_on_ones =
            FmAttrs::from_pairs(&[("reflexes", 92), ("one_on_ones", 40), ("agility", 80)]);
        assert!(convert(&worse_one_on_ones, &Position::Goalkeeper).reflexes < gk.reflexes);
    }

    fn profile(pairs: &[(&'static str, i64)]) -> FmAttrs {
        FmAttrs::from_pairs(pairs)
    }

    /// Hierarchy: within a role, a clearly better FM profile must be rated clearly higher by OFM's own
    /// position-weighted rating, whatever the conversion does to the shape.
    #[test]
    fn a_better_fm_profile_is_a_better_ofm_player_in_every_role() {
        type Profile<'a> = &'a [(&'a str, i64)];
        let roles: [(Position, Profile, Profile); 5] = [
            // (position, elite, lower-division)
            (
                Position::Goalkeeper,
                &[
                    ("handling", 92),
                    ("reflexes", 94),
                    ("one_on_ones", 90),
                    ("aerial_reach", 88),
                    ("command_of_area", 88),
                    ("positioning", 92),
                    ("decisions", 88),
                    ("composure", 90),
                ],
                &[
                    ("handling", 55),
                    ("reflexes", 55),
                    ("one_on_ones", 50),
                    ("aerial_reach", 55),
                    ("command_of_area", 50),
                    ("positioning", 55),
                    ("decisions", 50),
                    ("composure", 50),
                ],
            ),
            (
                Position::CenterBack,
                &[
                    ("marking", 90),
                    ("tackling", 90),
                    ("positioning", 92),
                    ("strength", 90),
                    ("heading", 88),
                    ("jumping_reach", 85),
                    ("anticipation", 90),
                    ("concentration", 88),
                    ("decisions", 88),
                    ("composure", 90),
                ],
                &[
                    ("marking", 55),
                    ("tackling", 55),
                    ("positioning", 55),
                    ("strength", 60),
                    ("heading", 55),
                    ("jumping_reach", 55),
                    ("anticipation", 50),
                    ("concentration", 50),
                    ("decisions", 50),
                    ("composure", 50),
                ],
            ),
            (
                Position::CentralMidfielder,
                &[
                    ("passing", 92),
                    ("vision", 92),
                    ("decisions", 90),
                    ("stamina", 88),
                    ("dribbling", 85),
                    ("positioning", 85),
                    ("off_the_ball", 85),
                    ("teamwork", 88),
                    ("tackling", 80),
                    ("technique", 90),
                ],
                &[
                    ("passing", 55),
                    ("vision", 55),
                    ("decisions", 55),
                    ("stamina", 60),
                    ("dribbling", 50),
                    ("positioning", 50),
                    ("off_the_ball", 50),
                    ("teamwork", 55),
                    ("tackling", 50),
                    ("technique", 50),
                ],
            ),
            (
                Position::RightWinger,
                &[
                    ("pace", 92),
                    ("acceleration", 92),
                    ("dribbling", 92),
                    ("passing", 80),
                    ("crossing", 85),
                    ("vision", 82),
                    ("off_the_ball", 85),
                    ("technique", 90),
                    ("stamina", 85),
                ],
                &[
                    ("pace", 60),
                    ("acceleration", 60),
                    ("dribbling", 55),
                    ("passing", 50),
                    ("crossing", 50),
                    ("vision", 50),
                    ("off_the_ball", 50),
                    ("technique", 50),
                    ("stamina", 55),
                ],
            ),
            (
                Position::Striker,
                &[
                    ("finishing", 94),
                    ("off_the_ball", 92),
                    ("composure", 90),
                    ("decisions", 85),
                    ("long_shots", 80),
                    ("pace", 85),
                    ("acceleration", 85),
                    ("dribbling", 80),
                    ("technique", 85),
                ],
                &[
                    ("finishing", 55),
                    ("off_the_ball", 55),
                    ("composure", 50),
                    ("decisions", 50),
                    ("long_shots", 45),
                    ("pace", 55),
                    ("acceleration", 55),
                    ("dribbling", 50),
                    ("technique", 50),
                ],
            ),
        ];
        for (position, elite, lower) in roles {
            let elite_ovr = ovr(&convert(&profile(elite), &position), &position);
            let lower_ovr = ovr(&convert(&profile(lower), &position), &position);
            assert!(
                elite_ovr >= lower_ovr + 15,
                "{position:?}: elite {elite_ovr} vs lower division {lower_ovr}"
            );
        }
    }

    #[test]
    fn potential_adds_the_converted_headroom_and_is_bounded() {
        assert_eq!(
            potential(70, 150, 150, 0.45),
            70,
            "no headroom -> at the ceiling"
        );
        assert_eq!(potential(55, 100, 140, 0.45), 73);
        assert_eq!(potential(90, 150, 200, 0.45), 99, "capped at 99");
        assert_eq!(
            potential(70, 160, 140, 0.45),
            70,
            "PA below CA never lowers the rating"
        );
        assert!(potential(1, 1, 1, 0.45) >= 1);
    }
}
