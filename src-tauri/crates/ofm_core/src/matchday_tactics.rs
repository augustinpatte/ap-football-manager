//! What an AI manager does about the side in front of him, before kick-off.
//!
//! A club's plan — the nine dials an [`crate::ai_tactics`] blueprint sets — is built from the club's
//! own squad and its manager's identity, and it is persisted: it is what the club believes about
//! football, not what it believes about Saturday's opponents. Neither of those is a match plan. A
//! side that plays a very low line every week is asking to be squeezed, and a side that presses
//! aggressively every week is asking for the ball to be played over its head, and until I5 the AI
//! never noticed: both touchlines played the same nine dials whoever was in front of them.
//!
//! This is that plan's **match-day** adjustment and nothing else. It is a pure function of the two
//! plans and the opponent's style, it runs once, before kick-off, on the home and away sides of a
//! live match, and by construction it cannot make anything of it self-sustaining or oscillating:
//!
//! - It never reads the score, the clock or a random number, so the same two plans always produce
//!   the same adjustment and two identical runs agree to every digit.
//! - It **never writes back**. It turns dials on the live match's copy of the side, so the club's
//!   saved plan is the same after the match as before it. What a club believes does not change
//!   because of one opponent.
//! - It makes at most [`MAX_MATCHDAY_MOVES`] changes, and it will not take a side past the
//!   engine's ration of under-priced dials ([`engine::ai::MAX_UNDER_PRICED_DIALS`]): a match-day
//!   adjustment is a nudge towards the opponent, not a second tactical identity.
//! - It only ever names a change that is a real answer to something the opponent is doing, and
//!   every answer has a cost the engine charges: pushing the line up to squeeze a deep block
//!   leaves space behind it, going long against a press gives the ball away more often, compacting
//!   against a wide side concedes the flanks.
//!
//! Nothing here is an advantage: the same rule runs for both touchlines, so a manager who gains
//! something from reading his opponent has given his opponent the same read.

use engine::types::{
    BreakSpeed, DefensiveLine, DefensiveShape, PlayStyle, PressingIntensity, TacticalDial,
    TacticsBuildUpStyle, TacticsConfig, TacticsPitchWidth,
};

/// The most dials one side may turn for one opponent. Two is a match plan; three is a different
/// club.
pub(crate) const MAX_MATCHDAY_MOVES: usize = 2;

/// The opponent-specific adjustment to `own`'s plan for a side playing `opponent`'s way.
///
/// The moves are in the order a manager would reach for them, and only the first
/// [`MAX_MATCHDAY_MOVES`] that change something are returned. A side whose own plan already holds
/// the answer gets nothing named: a dial set where it was going to be set is not an instruction.
pub(crate) fn opponent_response(
    own: &TacticsConfig,
    opponent_style: PlayStyle,
    opponent: &TacticsConfig,
) -> Vec<TacticalDial> {
    let mut moves = Vec::new();
    let mut planned = own.clone();
    let mut reach_for = |dial: TacticalDial| {
        if moves.len() >= MAX_MATCHDAY_MOVES {
            return;
        }
        let mut trial = planned.clone();
        dial.set_on(&mut trial);
        if trial != planned
            && engine::ai::under_priced_dials(&trial) <= engine::ai::MAX_UNDER_PRICED_DIALS
        {
            planned = trial;
            moves.push(dial);
        }
    };

    // A deep block asks to be squeezed and stretched: hold a high line so their clearances are
    // contested in their half, and pull the block wide so the space between its lines opens. There
    // is no quicker tempo to name: the engine has two, and `Direct` is both the faster and the
    // default, so a match-day plan can only ask a side to be patient
    // (`engine::types::Tempo::Patient`), which is the opposite of squeezing a block.
    if matches!(
        opponent.defensive_line,
        DefensiveLine::VeryLow | DefensiveLine::Low
    ) {
        reach_for(TacticalDial::DefensiveLine(DefensiveLine::High));
        reach_for(TacticalDial::Width(TacticsPitchWidth::Wide));
    }

    // A high line asks to be played over: break fast into the space behind it, and go long rather
    // than trying to pass through a side that is standing on the halfway line.
    if opponent.defensive_line == DefensiveLine::High {
        reach_for(TacticalDial::BreakSpeed(BreakSpeed::Fast));
        reach_for(TacticalDial::BuildUpStyle(TacticsBuildUpStyle::Long));
    }

    // An aggressive press asks to be bypassed: playing out from the back against it is how sides
    // lose the ball on the edge of their own box.
    if opponent.pressing_intensity == PressingIntensity::Aggressive {
        reach_for(TacticalDial::BuildUpStyle(TacticsBuildUpStyle::Long));
    }

    // A side that spreads out leaves the middle open; a side that crowds the middle leaves the
    // flanks open. Answer with the other one.
    match opponent.width {
        TacticsPitchWidth::Wide => reach_for(TacticalDial::DefensiveShape(DefensiveShape::Compact)),
        TacticsPitchWidth::Narrow => reach_for(TacticalDial::Width(TacticsPitchWidth::Wide)),
        TacticsPitchWidth::Normal => {}
    }

    // An attacking side wants a shoot-out. Do not give it one: drop the press and make it pass
    // through a shape instead of running at a back line that is already pushed up.
    if opponent_style == PlayStyle::Attacking {
        reach_for(TacticalDial::PressingIntensity(PressingIntensity::Passive));
    }

    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> TacticsConfig {
        TacticsConfig::default()
    }

    /// Given an opponent playing a deep block
    /// When the match-day adjustment is read
    /// Then the side holds a higher line and stretches the block wide, and nothing else moves:
    ///      the answer to a block is to squeeze it and pull it apart.
    #[test]
    fn a_deep_block_is_answered_with_a_higher_line_and_a_wide_shape() {
        let opponent = TacticsConfig {
            defensive_line: DefensiveLine::VeryLow,
            ..neutral()
        };
        let moves = opponent_response(&neutral(), PlayStyle::Defensive, &opponent);
        assert_eq!(
            moves,
            vec![
                TacticalDial::DefensiveLine(DefensiveLine::High),
                TacticalDial::Width(TacticsPitchWidth::Wide),
            ]
        );
    }

    /// Given an opponent holding a high line
    /// When the match-day adjustment is read
    /// Then the side looks to break fast and to play over the line rather than through it.
    #[test]
    fn a_high_line_is_answered_by_breaking_fast_and_going_long() {
        let opponent = TacticsConfig {
            defensive_line: DefensiveLine::High,
            ..neutral()
        };
        let moves = opponent_response(&neutral(), PlayStyle::Balanced, &opponent);
        assert_eq!(
            moves,
            vec![
                TacticalDial::BreakSpeed(BreakSpeed::Fast),
                TacticalDial::BuildUpStyle(TacticsBuildUpStyle::Long),
            ]
        );
    }

    /// Given an opponent pressing aggressively
    /// When the match-day adjustment is read
    /// Then the side goes long rather than playing out from the back into the press.
    #[test]
    fn an_aggressive_press_is_answered_by_going_long() {
        let opponent = TacticsConfig {
            pressing_intensity: PressingIntensity::Aggressive,
            ..neutral()
        };
        let moves = opponent_response(&neutral(), PlayStyle::HighPress, &opponent);
        assert_eq!(
            moves,
            vec![TacticalDial::BuildUpStyle(TacticsBuildUpStyle::Long)]
        );
    }

    /// Given a wide opponent and a narrow one
    /// When each is read
    /// Then the answer is the other width: compact against the wide side, wide against the narrow.
    #[test]
    fn width_is_answered_with_the_other_width() {
        let wide = TacticsConfig {
            width: TacticsPitchWidth::Wide,
            ..neutral()
        };
        let narrow = TacticsConfig {
            width: TacticsPitchWidth::Narrow,
            ..neutral()
        };
        assert_eq!(
            opponent_response(&neutral(), PlayStyle::Balanced, &wide),
            vec![TacticalDial::DefensiveShape(DefensiveShape::Compact)]
        );
        assert_eq!(
            opponent_response(&neutral(), PlayStyle::Balanced, &narrow),
            vec![TacticalDial::Width(TacticsPitchWidth::Wide)]
        );
    }

    /// Given an opponent whose plan asks nothing of anybody
    /// When the match-day adjustment is read
    /// Then nothing is named: an adjustment is an answer, not a habit.
    #[test]
    fn a_neutral_opponent_asks_nothing() {
        assert!(opponent_response(&neutral(), PlayStyle::Balanced, &neutral()).is_empty());
    }

    /// Given an opponent that plays attacking football
    /// When the match-day adjustment is read
    /// Then the side drops its press and asks it to pass through a shape, rather than trading
    ///      chances with a side built to attack.
    #[test]
    fn an_attacking_side_is_answered_by_dropping_the_press() {
        assert_eq!(
            opponent_response(&neutral(), PlayStyle::Attacking, &neutral()),
            vec![TacticalDial::PressingIntensity(PressingIntensity::Passive)]
        );
    }

    /// Given an opponent that asks several things at once (a very low line, aggressive press and a
    ///   wide shape)
    /// When the match-day adjustment is read
    /// Then it makes at most two moves: a match plan, not a second identity.
    #[test]
    fn no_opponent_takes_the_plan_past_two_moves() {
        let opponent = TacticsConfig {
            defensive_line: DefensiveLine::VeryLow,
            pressing_intensity: PressingIntensity::Aggressive,
            width: TacticsPitchWidth::Wide,
            ..neutral()
        };
        let moves = opponent_response(&neutral(), PlayStyle::Attacking, &opponent);
        assert_eq!(
            moves,
            vec![
                TacticalDial::DefensiveLine(DefensiveLine::High),
                TacticalDial::Width(TacticsPitchWidth::Wide),
            ],
            "the deep block is answered first and the cap stops the rest"
        );
        assert_eq!(moves.len(), MAX_MATCHDAY_MOVES);
    }

    /// Given a side already holding two of the engine's under-priced dials (a low line and a
    ///   narrow shape) and an opponent that spreads wide, whose answer would be a compact block,
    ///   a third rationed dial
    /// When the match-day adjustment is read
    /// Then the compact block is refused and nothing is named, while the same opponent does draw
    ///      it from a side that holds none: the ration, not the opponent, is what stopped it.
    #[test]
    fn the_match_day_plan_never_breaks_the_under_priced_ration() {
        let opponent = TacticsConfig {
            width: TacticsPitchWidth::Wide,
            ..neutral()
        };
        assert_eq!(
            opponent_response(&neutral(), PlayStyle::Defensive, &opponent),
            vec![TacticalDial::DefensiveShape(DefensiveShape::Compact)],
            "control: a side holding no rationed dial answers a wide opponent with a compact block"
        );

        let own = TacticsConfig {
            defensive_line: DefensiveLine::Low,
            width: TacticsPitchWidth::Narrow,
            ..neutral()
        };
        assert_eq!(
            engine::ai::under_priced_dials(&own),
            engine::ai::MAX_UNDER_PRICED_DIALS,
            "the fixture holds the whole ration"
        );
        assert_eq!(
            opponent_response(&own, PlayStyle::Defensive, &opponent),
            Vec::<TacticalDial>::new()
        );
    }

    /// Given a side whose own plan already holds the answer
    /// When the match-day adjustment is read
    /// Then the answer is not named twice: a dial left where it was is not an instruction.
    #[test]
    fn an_answer_the_plan_already_holds_is_not_named() {
        let own = TacticsConfig {
            defensive_line: DefensiveLine::High,
            width: TacticsPitchWidth::Wide,
            ..neutral()
        };
        let opponent = TacticsConfig {
            defensive_line: DefensiveLine::Low,
            ..neutral()
        };
        assert!(opponent_response(&own, PlayStyle::Defensive, &opponent).is_empty());
    }
}
