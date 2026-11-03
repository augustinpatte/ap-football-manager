//! Shared post-match physical wear applied to individual players.
//!
//! Used by both club matches and national-team friendlies so that a call-up
//! carries fatigue, fitness sharpening, and injury risk back to the player's
//! club (the same [`Player`] record is mutated in either context).

use domain::player::{Injury, Player, Position};
use rand::{Rng, RngExt};

/// One severity band of match injuries: how often it occurs, how long it keeps a player out, and
/// the (translated) injuries that fall in it.
struct SeverityBand {
    weight: u32,
    days_out: std::ops::RangeInclusive<u32>,
    injuries: &'static [&'static str],
}

/// The severity mix of time-loss injuries in professional football.
///
/// The weights are the UEFA Elite Club Injury Study's injuries per team per season by days of
/// absence: 19 slight (under 3 days), 15 minor (3-7), 14 moderate (8-28) and 7 major (over 28) out
/// of 55 (UEFA's Elite Club Injury Study newsletter, 2000s seasons; the same study reports match
/// injury risk of about 29 per 1000 hours). Days are uniform within a band, the major band running
/// to 90 days to stand for the long tail.
const SEVERITY_BANDS: [SeverityBand; 4] = [
    SeverityBand {
        weight: 19,
        days_out: 1..=2,
        injuries: &[
            "common.injuries.kneeBruise",
            "common.injuries.minorMuscleStrain",
        ],
    },
    SeverityBand {
        weight: 15,
        days_out: 3..=7,
        injuries: &[
            "common.injuries.twistedAnkle",
            "common.injuries.hamstringTightness",
            "common.injuries.calfStrain",
        ],
    },
    SeverityBand {
        weight: 14,
        days_out: 8..=28,
        injuries: &[
            "common.injuries.ankleLigamentSprain",
            "common.injuries.hamstringTear",
        ],
    },
    SeverityBand {
        weight: 7,
        days_out: 29..=90,
        injuries: &[
            "common.injuries.kneeLigamentTear",
            "common.injuries.metatarsalFracture",
        ],
    },
];

/// Rolls one injury: a severity band by its weight, then a name and a duration within it.
///
/// Shared by club matches and national-team friendlies so an injury means the same thing
/// wherever it was picked up.
pub fn roll_injury(rng: &mut impl Rng) -> Injury {
    let total_weight: u32 = SEVERITY_BANDS.iter().map(|band| band.weight).sum();
    let mut pick = rng.random_range(0..total_weight);
    let band = SEVERITY_BANDS
        .iter()
        .find(|band| {
            if pick < band.weight {
                true
            } else {
                pick -= band.weight;
                false
            }
        })
        .unwrap_or(&SEVERITY_BANDS[0]);

    Injury {
        name: band.injuries[rng.random_range(0..band.injuries.len())].to_string(),
        days_remaining: rng.random_range(band.days_out.clone()),
    }
}

/// Deplete a player's short-term condition based on minutes played and stamina,
/// and gradually sharpen match fitness for players with significant minutes.
///
/// A no-op for players who did not feature (`minutes == 0`). This is the minutes-and-stamina model on
/// its own, used where no match engine measured the physical load (friendlies, the legacy engine).
pub fn apply_match_wear(player: &mut Player, minutes: u8, rng: &mut impl Rng) {
    apply_match_wear_with_load(player, minutes, None, rng);
}

/// Whether the engine's load says anything about a position: a goalkeeper has no reference.
fn is_outfield(position: &Position) -> bool {
    !matches!(position, Position::Goalkeeper)
}

/// Share of the aerobic reserve the match engine spends per minute on an outfield player of
/// reference stamina in a match played with balanced tactics. Measured on 24 full matches of
/// players adapted from a career (Run F4.1: 0.00244 / 0.00255 / 0.00233 for defence / midfield /
/// attack, 180 starters each), which is why one rate serves every line: with career players the
/// engine's differences between positions come from their attributes (stamina, work rate), not from
/// where they stand, and the clamp keeps that bounded. It is the *reference* a player's match is
/// compared with, never the amount charged. `neutral_matches_cost_the_reference_load` fails when the
/// engine drifts from it. (The engine's own fixture players spread 3x between defence and attack,
/// which is a property of how that fixture is generated; do not calibrate on them.)
const REFERENCE_LOAD_PER_MINUTE: f64 = 0.0025;
/// Reference stamina and the relative change of the engine's expenditure per point above it.
const REFERENCE_STAMINA: f64 = 64.0;
const LOAD_PER_STAMINA_POINT: f64 = 0.0125;
/// A match can cost a player this much less or more than the minutes-and-stamina model charges.
const LOAD_RATIO_BOUNDS: (f64, f64) = (0.6, 1.5);

/// How hard this match was for a player compared with a neutral one at his position, minutes and
/// stamina: `1.0` is the neutral workload, a high press or a hectic game is above it, a low block
/// below. Bounded; a short appearance counts for less (a quarter of an hour says little about load).
/// `delta_l` is the share of the aerobic reserve the engine spent on him in the match (0..1).
pub fn load_ratio(position: &Position, stamina: u8, minutes: u8, delta_l: f32) -> f64 {
    if !is_outfield(position) || minutes == 0 {
        return 1.0;
    }
    let stamina_effect = 1.0 - LOAD_PER_STAMINA_POINT * (f64::from(stamina) - REFERENCE_STAMINA);
    let expected = REFERENCE_LOAD_PER_MINUTE * f64::from(minutes) * stamina_effect.max(0.5);
    let raw =
        (f64::from(delta_l.max(0.0)) / expected).clamp(LOAD_RATIO_BOUNDS.0, LOAD_RATIO_BOUNDS.1);
    let weight = (f64::from(minutes) / 45.0).min(1.0);
    1.0 + (raw - 1.0) * weight
}

/// [`apply_match_wear`] with the physical load the match engine measured (`delta_l`, see
/// [`load_ratio`]). The minutes-and-stamina depletion stays the baseline, so the recovery, rotation and
/// training models calibrated around it still hold; the engine decides only whether this match was
/// harder or easier than a neutral one. It is the one place a match charges condition: the engine's
/// own end-of-match fatigue is never written to the player as well.
pub fn apply_match_wear_with_load(
    player: &mut Player,
    minutes: u8,
    delta_l: Option<f32>,
    rng: &mut impl Rng,
) {
    if minutes == 0 {
        return; // Did not play, no wear.
    }

    let minutes_factor = minutes as f64 / 90.0;
    let stamina_factor = player.attributes.stamina as f64 / 100.0;
    let base_depletion = 40.0 * (1.0 - stamina_factor * 0.4);
    let ratio = delta_l.map_or(1.0, |load| {
        load_ratio(&player.position, player.attributes.stamina, minutes, load)
    });
    let depletion = (base_depletion * minutes_factor * ratio) as u8;
    player.condition = player.condition.saturating_sub(depletion);

    // 60+ minutes builds sharpness; probabilistic to keep changes gradual.
    if minutes >= 60 && player.fitness < 100 && rng.random_bool(0.3) {
        player.fitness = player.fitness.saturating_add(1);
    }
}

/// Injury-risk multiplier scaled by match fitness: less fit players are more
/// prone to picking up a knock.
fn injury_multiplier_from_fitness(fitness: u8) -> f64 {
    if fitness < 30 {
        3.0
    } else if fitness < 50 {
        2.0
    } else if fitness < 70 {
        1.5
    } else if fitness >= 90 {
        0.7
    } else {
        1.0
    }
}

/// Roll for a match-day injury. On a hit, sets the player's injury and returns
/// `true`. Already-injured players are skipped (returns `false`).
pub fn roll_match_injury(player: &mut Player, rng: &mut impl Rng) -> bool {
    if player.injury.is_some() {
        return false;
    }

    // Base match-day risk, slightly higher than the training-ground rate to
    // reflect competitive intensity.
    let base_prob = 1.0_f64 / 40.0;
    let prob = (base_prob * injury_multiplier_from_fitness(player.fitness)).min(1.0);
    if !rng.random_bool(prob) {
        return false;
    }

    player.injury = Some(roll_injury(rng));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{Player, PlayerAttributes, Position};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn attrs(stamina: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: 70,
            stamina,
            strength: 70,
            agility: 70,
            passing: 70,
            shooting: 70,
            tackling: 70,
            dribbling: 70,
            defending: 70,
            positioning: 70,
            vision: 70,
            decisions: 70,
            composure: 70,
            aggression: 70,
            teamwork: 70,
            leadership: 70,
            handling: 50,
            reflexes: 50,
            aerial: 60,
        }
    }

    fn make_player(stamina: u8) -> Player {
        Player::new(
            "p1".to_string(),
            "J. Doe".to_string(),
            "John Doe".to_string(),
            "2000-01-01".to_string(),
            "ENG".to_string(),
            Position::Midfielder,
            attrs(stamina),
        )
    }

    #[test]
    fn apply_match_wear_depletes_condition_by_minutes_and_stamina() {
        let mut player = make_player(100); // stamina 100 -> base depletion 24 over 90'
        player.condition = 100;
        let mut rng = StdRng::seed_from_u64(1);

        apply_match_wear(&mut player, 90, &mut rng);

        assert_eq!(player.condition, 76);
    }

    #[test]
    fn apply_match_wear_is_noop_for_unused_player() {
        let mut player = make_player(70);
        player.condition = 88;
        player.fitness = 90;
        let mut rng = StdRng::seed_from_u64(2);

        apply_match_wear(&mut player, 0, &mut rng);

        assert_eq!(player.condition, 88);
        assert_eq!(player.fitness, 90);
    }

    #[test]
    fn apply_match_wear_never_lowers_fitness() {
        let mut player = make_player(70);
        player.fitness = 80;
        let mut rng = StdRng::seed_from_u64(3);

        apply_match_wear(&mut player, 90, &mut rng);

        assert!(player.fitness >= 80);
    }

    fn depletion_with(position: Position, minutes: u8, load: Option<f32>) -> u8 {
        let mut player = make_player(64);
        player.position = position;
        player.condition = 100;
        let mut rng = StdRng::seed_from_u64(5);
        apply_match_wear_with_load(&mut player, minutes, load, &mut rng);
        100 - player.condition
    }

    /// Given no measured load (friendlies, the legacy engine),
    /// Then the wear is exactly the minutes-and-stamina model it always was.
    #[test]
    fn without_a_measured_load_the_wear_is_unchanged() {
        assert_eq!(depletion_with(Position::Midfielder, 90, None), 29);
    }

    /// Given a midfielder whose match cost what a neutral one does,
    /// Then he is charged the baseline.
    #[test]
    fn a_neutral_load_is_charged_the_baseline() {
        let neutral = (REFERENCE_LOAD_PER_MINUTE * 90.0) as f32;
        assert_eq!(
            depletion_with(Position::Midfielder, 90, Some(neutral)),
            depletion_with(Position::Midfielder, 90, None)
        );
    }

    /// Given the same player in a match that wore him out more, and one that wore him out less,
    /// Then the harder match costs more condition and the easier one less (pressing is not free).
    #[test]
    fn a_harder_match_costs_more_and_an_easier_one_less() {
        let neutral = (REFERENCE_LOAD_PER_MINUTE * 90.0) as f32;
        let base = depletion_with(Position::Midfielder, 90, Some(neutral));
        assert!(depletion_with(Position::Midfielder, 90, Some(neutral * 1.17)) > base);
        assert!(depletion_with(Position::Midfielder, 90, Some(neutral * 0.7)) < base);
    }

    /// Given an absurd load,
    /// Then the charge is bounded: no single match can spiral a player (at most 1.5x the baseline).
    #[test]
    fn the_charge_is_bounded() {
        let base = depletion_with(Position::Midfielder, 90, None);
        let extreme = depletion_with(Position::Midfielder, 90, Some(5.0));
        assert!(
            f64::from(extreme) <= f64::from(base) * 1.5 + 1.0,
            "{extreme} vs {base}"
        );
        assert!(
            depletion_with(Position::Midfielder, 90, Some(0.0)) as f64
                >= f64::from(base) * 0.6 - 1.0
        );
    }

    /// Given a goalkeeper,
    /// Then the engine's load does not move his charge (no reference exists for a keeper).
    #[test]
    fn a_goalkeepers_charge_ignores_the_load() {
        assert_eq!(
            depletion_with(Position::Goalkeeper, 90, Some(0.9)),
            depletion_with(Position::Goalkeeper, 90, None)
        );
    }

    /// Given a ten-minute cameo with an extreme load,
    /// Then it moves the charge far less than the same load over a full match.
    #[test]
    fn a_short_appearance_counts_for_less() {
        let cameo = load_ratio(&Position::Midfielder, 64, 10, 0.5);
        let full = load_ratio(&Position::Midfielder, 64, 90, 0.5);
        assert!((cameo - 1.0).abs() < (full - 1.0).abs());
    }

    /// Given two outfielders of different positions with the same share of the reserve spent,
    /// Then they had the same kind of match: position alone does not change the comparison.
    #[test]
    fn position_alone_does_not_change_the_comparison() {
        assert_eq!(
            load_ratio(&Position::CenterBack, 64, 90, 0.3),
            load_ratio(&Position::Striker, 64, 90, 0.3)
        );
    }

    /// Given many rolled injuries
    /// When they are grouped by severity band (days out)
    /// Then the shares follow the UEFA Elite Club Injury Study's 19/15/14/7 per 55 injuries:
    ///      slight (under 3 days) 34.5%, minor (3-7) 27.3%, moderate (8-28) 25.5%, major (over 28) 12.7%.
    #[test]
    fn rolled_injuries_follow_the_uefa_severity_bands() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut counts = [0u32; 4];
        let rolls = 20_000;
        for _ in 0..rolls {
            let days = roll_injury(&mut rng).days_remaining;
            let band = match days {
                0..=2 => 0,
                3..=7 => 1,
                8..=28 => 2,
                _ => 3,
            };
            counts[band] += 1;
        }
        for (count, expected) in counts.iter().zip([34.5, 27.3, 25.5, 12.7]) {
            let share = f64::from(*count) / f64::from(rolls) * 100.0;
            assert!(
                (share - expected).abs() < 1.5,
                "band share {share:.1}% should be near {expected}%"
            );
        }
    }

    /// Given any rolled injury
    /// When its name and duration are read
    /// Then the name is a translated injury key and the duration is at least a day.
    #[test]
    fn a_rolled_injury_has_a_translated_name_and_at_least_a_day() {
        let mut rng = StdRng::seed_from_u64(12);
        for _ in 0..500 {
            let injury = roll_injury(&mut rng);
            assert!(injury.name.starts_with("common.injuries."));
            assert!(injury.days_remaining >= 1);
        }
    }

    #[test]
    fn roll_match_injury_skips_already_injured_player() {
        let mut player = make_player(40);
        player.injury = Some(Injury {
            name: "existing".to_string(),
            days_remaining: 10,
        });
        let mut rng = StdRng::seed_from_u64(4);

        assert!(!roll_match_injury(&mut player, &mut rng));
        assert_eq!(player.injury.as_ref().unwrap().name, "existing");
    }

    #[test]
    fn roll_match_injury_eventually_injures_a_low_fitness_player() {
        let mut rng = StdRng::seed_from_u64(5);
        let mut injured = false;
        for _ in 0..500 {
            let mut player = make_player(70);
            player.fitness = 20; // 3x multiplier
            if roll_match_injury(&mut player, &mut rng) {
                assert!(player.injury.is_some());
                let injury = player.injury.unwrap();
                assert!(injury.name.starts_with("common.injuries."));
                assert!(injury.days_remaining >= 1);
                injured = true;
                break;
            }
        }
        assert!(
            injured,
            "a low-fitness player should be injured within 500 rolls"
        );
    }
}
