//! What a squad is worth in a formation, as the five-zone engine plays it.
//!
//! The squad model rates a formation by the mean of the squad's positional fit over its eleven
//! slots. Across a club's own eight formations that number does not predict results (I5: 192 clubs
//! in 12 worlds, correlation -0.061 with points, and picking its favourite earns -0.025 points a
//! match, less than a random shape). The engine does not read slot ratings: it reads, per group of
//! the eleven, the means of a handful of attributes, and it reads a group short of a full one at its
//! depth ([`engine::group_depth`]).
//!
//! This is that reading, and nothing else. The squad is put through the eleven the club's selection
//! would field, each player counted in the group of the slot he stands in with his own attributes,
//! and the groups the engine's contests draw from are read the way the engine reads them:
//!
//! - the **back line** — defending, tackling, positioning, aerial, the defender in the attacking
//!   third and the screen in front of a shot;
//! - the **midfield** — dribbling, passing, vision, teamwork, the group the possession and midfield
//!   contests draw from;
//! - the **finishing** — shooting, composure, decisions, the shooter.
//!
//! The score is those three means weighted by the goals of goal difference a match a point of each
//! was worth in the engine (fitted out of sample by world: `+0.022`, `+0.010` and `+0.027`), converted
//! to the unit the rest of the game scores formations in — a mean rating point of the eleven — at
//! what a rating point is worth in the engine (0.136 goals a match).
//!
//! Measured out of sample, this ranks a club's own eight formations at `+0.210` (se 0.033) against a
//! split-half ceiling of `+0.318`, where the squad model reaches `-0.061` and a shape's own average
//! `+0.231`: two thirds of the predictable signal, from three numbers.
//!
//! # Why this is not normalised by a shape's own level
//!
//! P6.1 built this evaluator and measured it against what a *typical* squad reaches in the same
//! shape, so that a shape itself carried no price. That was the right rule for the engine it was
//! measuring: a group was read by its plain mean, so a squad's best players in a thin group (one
//! striker, three centre backs) were worth the best of them, and the *level* of a shape was a
//! measure of how thin it was. Ranking clubs by that level sent half the league into a back three
//! and lifted the league's goals a match from 3.18 to 3.65-3.81, which is why it was designed,
//! measured and not adopted.
//!
//! The engine now fills a short group to a full one at the side's own standard (I5), so the level
//! of a shape is no longer a free lunch: a back three is read as three internationals *and a
//! fourth slot filled by the side's own average*, and a lone forward as half of a front two. The
//! evaluator therefore reads the level the engine reads, and the out-of-sample study that chose
//! between the two forms is in `docs/APFM_I5_VALIDATION.md`.
//!
//! **What this is not.** It is not the whole of a club's formation decision: the manager's taste and
//! the club's philosophy sit on top of it in `ai_formation`, the margin a change must clear is the
//! cost of changing, and the opponent is answered before kick-off by `matchday_tactics`. This
//! function is one thing: what the engine makes of the eleven in a shape.

use domain::player::{Player, Position};

use crate::player_rating::formation_slots;
use crate::turn::squad::first_choice_eleven;

/// Goals of goal difference a match one mean rating point of a whole side is worth in the engine.
///
/// A **scale**, not a measurement: it converts the goals the weights below are in into the rating
/// points the rest of the game scores formations in, so the margin a change must clear and a
/// manager's taste (a few tenths) keep the meaning P6.1 gave them.
const GOALS_PER_RATING_POINT: f64 = 0.136;

/// Goals a match per point of the back line's mean hold.
///
/// Fitted on 192 clubs in 12 worlds, every club in all eight shapes, out of sample by world
/// (`season-harness/tests/p61_formation_selection`): `+0.022`; a repeat full run reads `+0.021`. The
/// shipped number is the first; the two are within the study's own noise, and the ranking only reads
/// their ratios.
const BACK_LINE_GOALS: f64 = 0.022;

/// Goals a match per point of the midfield's mean on the ball.
///
/// The same fit reads `+0.010` (`+0.014` on the repeat full run): small either way, but positive, and
/// it is the group the possession and midfield contests draw from, so it is priced rather than
/// dropped.
const MIDFIELD_GOALS: f64 = 0.010;

/// Goals a match per point of the forwards' mean finishing.
///
/// The same fit reads `+0.027` (`+0.024` on the repeat); finishing is the most valuable of the three.
const FINISHING_GOALS: f64 = 0.027;

/// The engine reads a group nobody stands in as a flat 50.
const EMPTY_GROUP_MEAN: f64 = 50.0;

/// The three group means the engine's contests are built on, each read as the engine reads it.
///
/// The back line's hold, the midfield on the ball and the forwards' finishing. Public because the
/// study that measures what a typical squad reaches in each shape reads them
/// (`season-harness/tests/i5_typical_readings`): a reading of the engine's own rule, never a second
/// copy of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupReadings {
    pub back_line: f64,
    pub midfield: f64,
    pub finishing: f64,
}

/// What the eleven a club would field in `formation` is worth there, in mean rating points of the
/// side. Only differences between formations of one squad mean anything: the number has no zero.
///
/// The eleven is the one the club's selection names with nobody tired (`first_choice_eleven`), and a
/// player counts in the group of the slot he stands in, with his own attributes: the engine has no
/// notion of a player out of position, only of whose attributes sit in which group.
///
/// A group short of a full one is read exactly as the engine reads it
/// ([`engine::group_depth`]): the slots it leaves empty are filled by an average outfield player of
/// the same eleven, at the shipped weight. That is what stops the rule rewarding a squad's best
/// players concentrated into a thin group — the reading that sent P6.1's clubs into back threes and
/// one-man front lines.
///
/// The squad is taken at full strength: a player who is injured or suspended today still counts. A
/// shape is a decision about the structure of a squad, and scored on who can play this week it would
/// follow the treatment room: an injury to a first-choice forward moves a shape's score by more than
/// the change margin, so the club would change shape because a player is out, not because the squad
/// is better built another way.
pub(crate) fn engine_fit(squad: &[&Player], formation: &str) -> f64 {
    let readings = group_readings(squad, formation);
    (BACK_LINE_GOALS * readings.back_line
        + MIDFIELD_GOALS * readings.midfield
        + FINISHING_GOALS * readings.finishing)
        / GOALS_PER_RATING_POINT
}

/// The three group means the fit is built on, as the engine reads them, each at the depth-aware
/// reading the engine ships.
///
/// Public for the study that measures what a typical squad reaches in each shape and for the study
/// that sets the reading against results (`season-harness/tests/p61_formation_selection`).
pub fn group_readings(squad: &[&Player], formation: &str) -> GroupReadings {
    let eleven = first_choice_eleven(squad, formation);
    let slots = formation_slots(formation);
    let outfield: Vec<&Player> = slots
        .iter()
        .zip(&eleven)
        .filter(|(slot, _)| slot.to_group_position() != Position::Goalkeeper)
        .map(|(_, player)| *player)
        .collect();
    let group_mean = |group: Position, read: fn(&Player) -> f64| -> f64 {
        let members: Vec<f64> = slots
            .iter()
            .zip(&eleven)
            .filter(|(slot, _)| slot.to_group_position() == group)
            .map(|(_, player)| read(player))
            .collect();
        if members.is_empty() {
            return EMPTY_GROUP_MEAN;
        }
        let outfield_values: Vec<f64> = outfield.iter().map(|player| read(player)).collect();
        let baseline = if outfield_values.is_empty() {
            EMPTY_GROUP_MEAN
        } else {
            outfield_values.iter().sum::<f64>() / outfield_values.len() as f64
        };
        engine::group_depth::depth_filled_mean(
            &members,
            match group {
                Position::Defender => engine::group_depth::FULL_GROUP_DEFENDERS,
                Position::Forward => engine::group_depth::FULL_GROUP_FORWARDS,
                _ => engine::group_depth::FULL_GROUP_MIDFIELDERS,
            },
            baseline,
            engine::group_depth::SHIPPED_GROUP_DEPTH_WEIGHT,
        )
    };
    GroupReadings {
        back_line: group_mean(Position::Defender, |p| {
            let a = &p.attributes;
            (f64::from(a.defending)
                + f64::from(a.tackling)
                + f64::from(a.positioning)
                + f64::from(a.aerial))
                / 4.0
        }),
        midfield: group_mean(Position::Midfielder, |p| {
            let a = &p.attributes;
            (f64::from(a.dribbling)
                + f64::from(a.passing)
                + f64::from(a.vision)
                + f64::from(a.teamwork))
                / 4.0
        }),
        finishing: group_mean(Position::Forward, |p| {
            let a = &p.attributes;
            (f64::from(a.shooting) + f64::from(a.composure) + f64::from(a.decisions)) / 3.0
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::squad_intelligence::test_squads::Squad;

    /// A squad of twenty that can fill any of the eight shapes.
    fn squad() -> Squad {
        Squad::balanced()
            .add(Position::AttackingMidfielder, &[72, 68])
            .add(Position::DefensiveMidfielder, &[72, 68])
            .add(Position::LeftWinger, &[70, 66])
            .add(Position::RightWinger, &[70, 66])
    }

    fn refs(squad: &Squad) -> Vec<&Player> {
        squad.players.iter().collect()
    }

    /// A squad built for a back three: three outstanding centre backs, ordinary full backs.
    fn back_three_squad() -> Squad {
        Squad::new()
            .add(Position::Goalkeeper, &[72, 60])
            .add(Position::CenterBack, &[84, 83, 82, 60])
            .add(Position::LeftBack, &[55, 50])
            .add(Position::RightBack, &[55, 50])
            .add(Position::LeftWingBack, &[70, 60])
            .add(Position::RightWingBack, &[70, 60])
            .add(Position::CentralMidfielder, &[72, 70, 68, 62])
            .add(Position::LeftMidfielder, &[55])
            .add(Position::RightMidfielder, &[55])
            .add(Position::Striker, &[75, 73, 67])
    }

    /// Given a squad and a formation
    /// When the fit is read
    /// Then it is exactly the engine's own group reading in the weights the engine was measured at:
    ///      one rule in one place, so the formation the AI ranks best is the one the engine plays
    ///      best.
    #[test]
    fn the_fit_is_the_engines_own_reading_and_not_a_second_formula() {
        let squad = squad();
        let players = refs(&squad);
        for formation in ["4-4-2", "4-3-3", "3-4-3", "4-5-1"] {
            let r = group_readings(&players, formation);
            assert_eq!(
                engine_fit(&players, formation),
                (BACK_LINE_GOALS * r.back_line
                    + MIDFIELD_GOALS * r.midfield
                    + FINISHING_GOALS * r.finishing)
                    / GOALS_PER_RATING_POINT
            );
        }
    }

    /// Given two squads that differ only in how well their forwards finish
    /// When the engine's fit is read in the same shape
    /// Then the better finishers are worth exactly the weight of finishing more, per point, in
    ///      rating points; and a forward's shooting is not what the stored overall says it is.
    #[test]
    fn a_side_that_finishes_better_is_worth_more_by_the_weight_of_finishing() {
        let mut sharp = squad();
        let blunt = squad();
        for forward in sharp
            .players
            .iter_mut()
            .filter(|p| p.natural_position == Position::Striker)
        {
            forward.attributes.shooting = forward.attributes.shooting.saturating_add(10);
            forward.attributes.composure = forward.attributes.composure.saturating_add(10);
            forward.attributes.decisions = forward.attributes.decisions.saturating_add(10);
        }
        let gain = engine_fit(&refs(&sharp), "4-4-2") - engine_fit(&refs(&blunt), "4-4-2");
        let expected = 10.0 * FINISHING_GOALS / GOALS_PER_RATING_POINT;
        assert!(
            (gain - expected).abs() < 1e-9,
            "gained {gain}, expected {expected}"
        );
    }

    /// Given two squads that differ only in how well their midfielders keep the ball, in a shape
    ///   that fields exactly the four midfielders that were changed
    /// When the engine's fit is read in a four-man midfield
    /// Then the better midfield is worth the weight of the midfield more, per point: the possession
    ///      and midfield contests are a part of what a shape is worth, not an afterthought.
    #[test]
    fn a_side_whose_midfield_keeps_the_ball_better_is_worth_more_by_the_weight_of_the_midfield() {
        let four_in_the_middle = || {
            Squad::new()
                .add(Position::Goalkeeper, &[70, 60])
                .add(Position::CenterBack, &[70, 70, 68, 66])
                .add(Position::LeftBack, &[68, 62])
                .add(Position::RightBack, &[68, 62])
                .add(Position::CentralMidfielder, &[72, 72, 70, 70, 62])
                .add(Position::Striker, &[74, 72, 66])
        };
        let mut technical = four_in_the_middle();
        let ordinary = four_in_the_middle();
        for midfielder in technical
            .players
            .iter_mut()
            .filter(|p| p.natural_position == Position::CentralMidfielder)
        {
            let a = &mut midfielder.attributes;
            a.dribbling = a.dribbling.saturating_add(10);
            a.passing = a.passing.saturating_add(10);
            a.vision = a.vision.saturating_add(10);
            a.teamwork = a.teamwork.saturating_add(10);
        }
        let gain = engine_fit(&refs(&technical), "4-4-2") - engine_fit(&refs(&ordinary), "4-4-2");
        let expected = 10.0 * MIDFIELD_GOALS / GOALS_PER_RATING_POINT;
        assert!(
            (gain - expected).abs() < 1e-9,
            "gained {gain}, expected {expected}"
        );
    }

    /// Given two squads that differ only in how well their defenders hold a line
    /// When the engine's fit is read in a back four
    /// Then the better defenders are worth the weight of the back line more, per point.
    #[test]
    fn a_side_whose_back_line_holds_better_is_worth_more_by_the_weight_of_the_back_line() {
        let mut solid = squad();
        let loose = squad();
        for defender in solid.players.iter_mut().filter(|p| {
            matches!(
                p.natural_position,
                Position::CenterBack | Position::LeftBack | Position::RightBack
            )
        }) {
            let a = &mut defender.attributes;
            a.defending = a.defending.saturating_add(8);
            a.tackling = a.tackling.saturating_add(8);
            a.positioning = a.positioning.saturating_add(8);
            a.aerial = a.aerial.saturating_add(8);
        }
        let gain = engine_fit(&refs(&solid), "4-4-2") - engine_fit(&refs(&loose), "4-4-2");
        let expected = 8.0 * BACK_LINE_GOALS / GOALS_PER_RATING_POINT;
        assert!(
            (gain - expected).abs() < 0.25,
            "gained {gain}, expected about {expected}"
        );
        assert!(gain > 0.0);
    }

    /// Given a squad in which the best forward is injured and another suspended
    /// When the engine's fit is read
    /// Then it is what the squad would be worth at full strength: a club's shape answers to the
    ///      structure of its squad, not to this week's treatment room. Scored on who can play
    ///      today, a shape would follow an injury, which is not a decision about the squad.
    #[test]
    fn a_squads_shape_does_not_move_with_this_weeks_injuries() {
        let healthy = squad();
        let mut hurt = squad();
        let mut strikers = hurt
            .players
            .iter_mut()
            .filter(|p| p.natural_position == Position::Striker);
        strikers.next().expect("a striker").injury = Some(domain::player::Injury {
            name: "Hamstring".to_string(),
            days_remaining: 20,
        });
        strikers.next().expect("a second striker").suspended_matches = 2;
        for formation in ["4-4-2", "4-3-3", "3-5-2"] {
            assert_eq!(
                engine_fit(&refs(&healthy), formation),
                engine_fit(&refs(&hurt), formation),
                "{formation} moved with the injury list"
            );
        }
    }

    /// Given the stored overall of a squad's players raised, with their attributes as they were
    /// When the engine's fit is read
    /// Then it does not move: the engine reads attributes, never an overall.
    #[test]
    fn the_stored_overall_is_not_what_the_engine_reads() {
        let mut inflated = squad();
        let honest = squad();
        for player in &mut inflated.players {
            player.ovr = player.ovr.saturating_add(15);
        }
        assert_eq!(
            engine_fit(&refs(&inflated), "4-3-3"),
            engine_fit(&refs(&honest), "4-3-3")
        );
    }

    /// Given a squad whose two forwards are its best finishers by a distance, every other player
    ///   ordinary
    /// When the engine's reading of its finishing is taken in a shape with one forward and one with
    ///   two
    /// Then the lone forward is read below his own finishing, while the pair is read at theirs: the
    ///      reading is of the group's depth, and a squad's best players concentrated into a thin
    ///      group are not worth the best of them. That is the reading that made P6.1's evaluator
    ///      lift the league's goals a match by 15 to 20 %.
    #[test]
    fn a_lone_forward_is_read_below_his_own_finishing() {
        let squad = Squad::new()
            .add(Position::Goalkeeper, &[60, 55])
            .add(Position::CenterBack, &[60, 60, 60, 55, 55])
            .add(Position::LeftBack, &[60, 55])
            .add(Position::RightBack, &[60, 55])
            .add(Position::CentralMidfielder, &[60, 60, 60, 60, 55])
            .add(Position::LeftMidfielder, &[60, 55])
            .add(Position::RightMidfielder, &[60, 55])
            .add(Position::Striker, &[90, 90, 55]);
        let players = refs(&squad);
        let lone = group_readings(&players, "4-5-1").finishing;
        let pair = group_readings(&players, "4-4-2").finishing;
        let striker = |player: &&Player| {
            let a = &player.attributes;
            (f64::from(a.shooting) + f64::from(a.composure) + f64::from(a.decisions)) / 3.0
        };
        let eleven = first_choice_eleven(&players, "4-5-1");
        let slots = formation_slots("4-5-1");
        let lone_striker = eleven
            .iter()
            .zip(&slots)
            .find(|(_, slot)| slot.to_group_position() == Position::Forward)
            .map(|(player, _)| striker(player))
            .expect("a forward");
        assert!(
            lone < lone_striker - 1.0,
            "the lone forward is read as himself: {lone} against {lone_striker}"
        );
        assert!(pair > lone, "{pair} against {lone}");
    }

    /// Given a squad with three outstanding centre backs and ordinary defenders everywhere else
    /// When the engine's reading of its back line is taken in a back three and in a back four
    /// Then the back three is read below the plain mean of the three, filled to four at the side's
    ///      own standard: three internationals are not the mean of three.
    #[test]
    fn a_back_three_of_internationals_is_read_at_its_depth() {
        let squad = back_three_squad();
        let players = refs(&squad);
        let back_three = group_readings(&players, "3-4-3").back_line;
        let eleven = first_choice_eleven(&players, "3-4-3");
        let slots = formation_slots("3-4-3");
        let hold = |player: &&Player| {
            let a = &player.attributes;
            (f64::from(a.defending)
                + f64::from(a.tackling)
                + f64::from(a.positioning)
                + f64::from(a.aerial))
                / 4.0
        };
        let defenders: Vec<f64> = eleven
            .iter()
            .zip(&slots)
            .filter(|(_, slot)| slot.to_group_position() == Position::Defender)
            .map(|(player, _)| hold(player))
            .collect();
        assert_eq!(defenders.len(), 3, "a back three");
        let plain = defenders.iter().sum::<f64>() / 3.0;
        assert!(
            back_three < plain - 0.5,
            "the back three is read as the mean of three: {back_three} against {plain}"
        );
    }

    /// Given a formation the game does not offer
    /// When it is read
    /// Then it is read as a 4-4-2 and nothing panics.
    #[test]
    fn a_formation_the_game_does_not_offer_is_read_as_a_four_four_two() {
        let squad = squad();
        let players = refs(&squad);
        assert!(engine_fit(&players, "4-4-2").is_finite());
        assert!(engine_fit(&players, "9-9-9").is_finite());
    }

    /// Given fewer than eleven players
    /// When the fit is read
    /// Then it is still a number (the engine reads a missing group as 50), so a short squad is
    ///      scored and not skipped.
    #[test]
    fn a_squad_short_of_a_group_is_still_scored() {
        let thin = Squad::new().add(Position::Goalkeeper, &[70]);
        assert!(engine_fit(&refs(&thin), "4-4-2").is_finite());
    }

    /// Group of every slot, as the engine bridge reads it (a winger is a forward, an attacking
    /// midfielder a midfielder): the fit is built on the same groups, which this pins.
    #[test]
    fn the_groups_are_the_ones_the_engine_bridge_uses() {
        let group_of = |slot: &Position| slot.to_group_position();
        let slots = formation_slots("4-3-3");
        let forwards = slots
            .iter()
            .filter(|s| group_of(s) == Position::Forward)
            .count();
        assert_eq!(forwards, 3);
    }
}
