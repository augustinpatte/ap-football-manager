//! What a squad is worth in a formation, as the five-zone engine plays it.
//!
//! The squad model rates a formation by the mean of the squad's positional fit over its eleven
//! slots. Across a club's own eight formations that number does not predict results (P6.1: 192
//! clubs in 12 worlds, leave-one-world-out, correlation +0.14 with a *negative* fitted slope):
//! almost all of its variation is a level per shape (a back line of three and three forwards rate
//! about a point higher than a 4-4-2 and earn less), and once that is removed what is left of a
//! club's squad has no relationship to its results (-0.02 +/- 0.03). The engine does not read slot
//! ratings. It reads, per group of the eleven, the means of a handful of attributes.
//!
//! This is the engine's own reading, with only the two group means that carry the formation-to-
//! formation variation of a real squad: the back line's hold (defending, tackling, positioning,
//! aerial: the defender in the attacking-third contest) and the forwards' finishing (shooting,
//! composure, decisions: the shot), plus what the engine charges the shape itself. On the same
//! held-out worlds it ranks a club's formations at +0.53 against +0.14 and earns +0.13 points a
//! match over a random shape against +0.05 (`season-harness/tests/p61_formation_selection.rs`).
//!
//! The weights are goals of goal difference a match per attribute point, fitted within club across
//! its eight formations on the study's 192 clubs; the number returned is those goals expressed in
//! the squad model's own unit, a mean rating point of the eleven, at what a rating point is worth
//! in the engine (`p61_points_per_rating_point`: 0.136 goals a match). A formation the game does
//! not offer is priced as a 4-4-2.

use domain::player::{Player, Position};

use crate::player_rating::formation_slots;
use crate::turn::squad::first_choice_eleven;

/// Goals of goal difference a match one mean rating point of a whole side is worth in the engine.
const GOALS_PER_RATING_POINT: f64 = 0.136;

/// Goals a match per point of the back line's mean hold.
const BACK_LINE_GOALS: f64 = 0.025;

/// Goals a match per point of the forwards' mean finishing.
const FINISHING_GOALS: f64 = 0.035;

/// What the engine charges a shape, in goals a match, against a 4-4-2 with the same back line and
/// forwards. Fitted with the two weights above.
const SHAPE_GOALS: [(&str, f64); 8] = [
    ("4-4-2", 0.0),
    ("4-3-3", -0.069),
    ("3-5-2", 0.072),
    ("4-5-1", -0.037),
    ("4-2-3-1", -0.022),
    ("3-4-3", 0.067),
    ("5-3-2", -0.129),
    ("4-1-4-1", -0.059),
];

/// The engine reads a group nobody stands in as a flat 50.
const EMPTY_GROUP_MEAN: f64 = 50.0;

/// What the eleven a club would field in `formation` is worth there, in mean rating points of the
/// side. Only differences between formations of one squad mean anything: the number has no zero.
///
/// The eleven is the one the club's selection names with nobody tired (`first_choice_eleven`), and a
/// player counts in the group of the slot he stands in, with his own attributes: the engine has no
/// notion of a player out of position, only of whose attributes sit in which group.
///
/// The squad is taken at full strength: a player who is injured or suspended today still counts. A
/// shape is a decision about the structure of a squad, and scored on who can play this week it
/// followed the treatment room, which put every club through about 1.4 formation changes a season
/// (the margin a change must clear is far smaller than an injury to the first-choice forward moves
/// a shape's score).
pub(crate) fn engine_fit(squad: &[&Player], formation: &str) -> f64 {
    let eleven = first_choice_eleven(squad, formation);
    let slots = formation_slots(formation);
    let group_mean = |group: Position, read: fn(&Player) -> f64| -> f64 {
        let members: Vec<f64> = slots
            .iter()
            .zip(&eleven)
            .filter(|(slot, _)| slot.to_group_position() == group)
            .map(|(_, player)| read(player))
            .collect();
        if members.is_empty() {
            EMPTY_GROUP_MEAN
        } else {
            members.iter().sum::<f64>() / members.len() as f64
        }
    };
    let back_line = group_mean(Position::Defender, |p| {
        let a = &p.attributes;
        (f64::from(a.defending)
            + f64::from(a.tackling)
            + f64::from(a.positioning)
            + f64::from(a.aerial))
            / 4.0
    });
    let finishing = group_mean(Position::Forward, |p| {
        let a = &p.attributes;
        (f64::from(a.shooting) + f64::from(a.composure) + f64::from(a.decisions)) / 3.0
    });
    let shape = SHAPE_GOALS
        .iter()
        .find(|(name, _)| *name == formation)
        .map_or(0.0, |(_, goals)| *goals);
    (shape + BACK_LINE_GOALS * back_line + FINISHING_GOALS * finishing) / GOALS_PER_RATING_POINT
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

    /// Given the same squad in a 3-5-2 and in a 5-3-2
    /// When the engine's fit is read for each
    /// Then the shape the engine rewards is worth more, by exactly what it charges the shapes
    ///      (in rating points), whoever is fielded in them: 3-5-2 stands one point and a half
    ///      above 5-3-2 on shape alone.
    #[test]
    fn the_engine_prices_a_shape_in_rating_points() {
        let squad = squad();
        let players = refs(&squad);
        let price = |formation| {
            let shape = SHAPE_GOALS.iter().find(|(f, _)| *f == formation).unwrap().1;
            shape / GOALS_PER_RATING_POINT
        };
        assert!(price("3-5-2") - price("5-3-2") > 1.4);
        // Same eleven in both would make the difference exactly the price; with the squad's own
        // eleven the difference is the price plus what the lineups differ by.
        assert!(engine_fit(&players, "3-5-2") > engine_fit(&players, "5-3-2"));
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
    ///      structure of its squad, not to this week's treatment room. (Scored on who can play today,
    ///      a club changed formation 110 times a season across 80 clubs, against 23 before.)
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

    /// Given a formation the game does not offer
    /// When it is read
    /// Then it is priced as a 4-4-2 and nothing panics.
    #[test]
    fn a_formation_the_game_does_not_offer_is_priced_as_a_four_four_two() {
        let squad = squad();
        let players = refs(&squad);
        let odd = engine_fit(&players, "4-4-2");
        assert!(odd.is_finite());
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
