//! The lightweight tier's match report: the people in a scoreline-only fixture.
//!
//! A dormant fixture is settled by a score drawn from the two clubs' strength (see
//! [`crate::catchup`]). That score is the only thing the lightweight model decides about the
//! *match*; this module decides what the score meant for the *players*, as the same kind of
//! report the match engine produces, so the one consequence stage every finalised match ends in
//! ([`super::post_match::apply_player_consequences`]) gives a dormant club's players statistics,
//! fatigue, injuries and bans exactly as it does an active club's.
//!
//! The model is result-first, and says so: the goals are the score's goals, and each is given to a
//! scorer (and usually an assister) drawn from the eleven by position and attribute. Nothing is
//! invented to make the score fit; the score is the input. Cards and injuries are drawn per player,
//! independently of the score, at rates measured on the match engine's own matches
//! (`docs/APFM_RUN_F5_0_UNIVERSAL_SIMULATION.md`, "Lightweight tier"). What the lightweight tier
//! does not model: substitutions (the eleven play the match, less any dismissal or injury), own
//! goals, penalties, possession, and shot or pass counts. Every one of those is absent from the
//! report, not guessed at.

use domain::player::{Player, Position};
use engine::{
    EventType, GoalDetail, GoalSource, MatchEvent, MatchReport, PlayerMatchStats, Side, TeamStats,
    Zone,
};
use rand::RngExt;
use std::collections::HashMap;

/// Chance a player of the eleven is booked in a match, before his aggression.
const YELLOW_CARD_RATE: f64 = 0.10;
/// Of those booked, the share booked a second time and sent off.
const SECOND_YELLOW_SHARE: f64 = 0.03;
/// Chance of a straight red card, per player per match.
const STRAIGHT_RED_RATE: f64 = 0.0035;
/// A share of a match's goals come from nobody's assist.
const UNASSISTED_SHARE: f64 = 0.3;
/// The eleven play ninety minutes.
const FULL_MATCH_MINUTES: u8 = 90;
/// A player at or above this condition is rested for no one; below it he is ranked down...
const RESTED_CONDITION: u8 = 70;
/// ...by this much rating for each point of condition he is short of it, so a first choice at 40
/// (15 points down) loses to a reserve at most 15 rating points worse than him.
const RANK_PENALTY_PER_TIRED_POINT: f64 = 0.5;
/// A side's eleven.
const MATCH_XI: usize = 11;

/// How likely a player is to score, relative to another, by where he plays.
fn scoring_weight(position: &Position) -> f64 {
    match position {
        Position::Forward | Position::Striker => 6.0,
        Position::AttackingMidfielder | Position::RightWinger | Position::LeftWinger => 4.0,
        Position::Midfielder
        | Position::CentralMidfielder
        | Position::RightMidfielder
        | Position::LeftMidfielder => 2.0,
        Position::DefensiveMidfielder | Position::RightWingBack | Position::LeftWingBack => 0.8,
        Position::Defender | Position::CenterBack | Position::RightBack | Position::LeftBack => 0.5,
        Position::Goalkeeper => 0.01,
    }
}

/// How likely a player is to make a goal, relative to another, by where he plays.
fn assist_weight(position: &Position) -> f64 {
    match position {
        Position::AttackingMidfielder | Position::RightWinger | Position::LeftWinger => 5.0,
        Position::Midfielder
        | Position::CentralMidfielder
        | Position::RightMidfielder
        | Position::LeftMidfielder => 3.5,
        Position::Forward | Position::Striker => 3.0,
        Position::RightWingBack | Position::LeftWingBack | Position::DefensiveMidfielder => 2.0,
        Position::RightBack | Position::LeftBack => 1.2,
        Position::Defender | Position::CenterBack => 0.5,
        Position::Goalkeeper => 0.02,
    }
}

/// The index of the player drawn by `weight`, none of `excluded`.
fn draw(weights: &[f64], excluded: Option<usize>, rng: &mut impl RngExt) -> usize {
    let total: f64 = weights
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != excluded)
        .map(|(_, w)| w)
        .sum();
    let mut pick = rng.random_range(0.0..total.max(f64::MIN_POSITIVE));
    for (index, weight) in weights.iter().enumerate() {
        if Some(index) == excluded {
            continue;
        }
        if pick < *weight {
            return index;
        }
        pick -= weight;
    }
    weights
        .iter()
        .enumerate()
        .rfind(|(i, _)| Some(*i) != excluded)
        .map_or(0, |(i, _)| i)
}

struct SideLedger {
    stats: Vec<PlayerMatchStats>,
}

fn side_events(
    side: Side,
    eleven: &[&Player],
    goals: u8,
    ledger: &mut SideLedger,
    report: &mut MatchReport,
    rng: &mut impl RngExt,
) {
    // A club nobody could field has no one to give the score to.
    if eleven.is_empty() {
        return;
    }
    let scoring: Vec<f64> = eleven
        .iter()
        .map(|p| scoring_weight(&p.position) * (f64::from(p.attributes.shooting) / 50.0).powi(2))
        .collect();
    let assisting: Vec<f64> = eleven
        .iter()
        .map(|p| assist_weight(&p.position) * (f64::from(p.attributes.passing) / 50.0).powi(2))
        .collect();
    let zone = Zone::attacking_box(side);
    for _ in 0..goals {
        let minute = rng.random_range(1..=FULL_MATCH_MINUTES);
        let scorer = draw(&scoring, None, rng);
        let assist = (eleven.len() > 1 && !rng.random_bool(UNASSISTED_SHARE))
            .then(|| draw(&assisting, Some(scorer), rng));
        ledger.stats[scorer].goals += 1;
        let mut event =
            MatchEvent::new(minute, EventType::Goal, side, zone).with_player(&eleven[scorer].id);
        if let Some(assist) = assist {
            ledger.stats[assist].assists += 1;
            event = event.with_secondary(&eleven[assist].id);
        }
        report.events.push(event);
        report.goals.push(GoalDetail {
            minute,
            scorer_id: eleven[scorer].id.clone(),
            assist_id: assist.map(|a| eleven[a].id.clone()),
            goal_source: GoalSource::OpenPlay,
            side,
        });
    }
    let own_half = match side {
        Side::Home => Zone::HomeDefense,
        Side::Away => Zone::AwayDefense,
    };
    for (index, player) in eleven.iter().enumerate() {
        let aggression = f64::from(player.attributes.aggression) / 100.0;
        let mut left_at = FULL_MATCH_MINUTES;
        if rng.random_bool((YELLOW_CARD_RATE * (0.5 + aggression)).min(1.0)) {
            let first = rng.random_range(5..=FULL_MATCH_MINUTES);
            ledger.stats[index].yellow_cards = 1;
            report.events.push(
                MatchEvent::new(first, EventType::YellowCard, side, own_half)
                    .with_player(&player.id),
            );
            if rng.random_bool(SECOND_YELLOW_SHARE) && first < FULL_MATCH_MINUTES {
                let second = rng.random_range(first + 1..=FULL_MATCH_MINUTES);
                ledger.stats[index].yellow_cards = 2;
                left_at = second;
                report.events.push(
                    MatchEvent::new(second, EventType::SecondYellow, side, own_half)
                        .with_player(&player.id),
                );
            }
        } else if rng.random_bool(STRAIGHT_RED_RATE) {
            let minute = rng.random_range(5..=FULL_MATCH_MINUTES);
            ledger.stats[index].red_cards = 1;
            left_at = minute;
            report.events.push(
                MatchEvent::new(minute, EventType::RedCard, side, own_half).with_player(&player.id),
            );
        }
        if rng.random_bool(crate::player_wear::match_injury_probability(player)) {
            let minute = rng.random_range(1..=left_at);
            left_at = left_at.min(minute);
            report.events.push(
                MatchEvent::new(minute, EventType::Injury, side, own_half).with_player(&player.id),
            );
        }
        ledger.stats[index].minutes_played = left_at;
    }
}

/// The report of a lightweight fixture that ended `home_goals`-`away_goals`, played by two elevens.
pub(super) fn lightweight_report(
    (home_goals, away_goals): (u8, u8),
    home: &[&Player],
    away: &[&Player],
    rng: &mut impl RngExt,
) -> MatchReport {
    let mut report = MatchReport {
        home_goals,
        away_goals,
        home_stats: TeamStats::default(),
        away_stats: TeamStats::default(),
        events: Vec::new(),
        goals: Vec::new(),
        player_stats: HashMap::new(),
        home_possession: 50.0,
        total_minutes: FULL_MATCH_MINUTES,
        home_penalties: None,
        away_penalties: None,
    };
    for (side, eleven, goals) in [
        (Side::Home, home, home_goals),
        (Side::Away, away, away_goals),
    ] {
        let mut ledger = SideLedger {
            stats: vec![PlayerMatchStats::default(); eleven.len()],
        };
        side_events(side, eleven, goals, &mut ledger, &mut report, rng);
        let (yellows, reds) = ledger.stats.iter().fold((0u8, 0u8), |(y, r), s| {
            (y + s.yellow_cards, r + s.red_cards)
        });
        let team = match side {
            Side::Home => &mut report.home_stats,
            Side::Away => &mut report.away_stats,
        };
        team.goals = goals;
        team.yellow_cards = yellows;
        team.red_cards = reds;
        for (player, stats) in eleven.iter().zip(ledger.stats) {
            report.player_stats.insert(player.id.clone(), stats);
        }
    }
    report.events.sort_by_key(|event| event.minute);
    report
}

/// The eleven a club puts out when nothing but its own squad is known: the best eligible players,
/// with a tired player ranked down by how far his condition is below what a match asks, so a
/// worn-out first choice is rested when a fresher player is nearly as good. Ties on id, so the
/// choice never depends on storage order.
pub(super) fn rested_eleven<'a>(club: &[&'a Player]) -> Vec<&'a Player> {
    let mut eligible = crate::match_day_eligibility::eligible_among(club);
    let rank = |p: &Player| {
        f64::from(p.ovr)
            - f64::from(RESTED_CONDITION.saturating_sub(p.condition)) * RANK_PENALTY_PER_TIRED_POINT
    };
    eligible.sort_by(|a, b| rank(b).total_cmp(&rank(a)).then_with(|| a.id.cmp(&b.id)));
    eligible.truncate(MATCH_XI);
    eligible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;
    use domain::player::{Player, Position};

    /// Eleven players of one club: a goalkeeper, four defenders, four midfielders, two strikers.
    fn eleven(club: &str) -> Vec<Player> {
        (0..11)
            .map(|n| {
                let position = match n {
                    0 => Position::Goalkeeper,
                    1..=4 => Position::Defender,
                    5..=8 => Position::Midfielder,
                    _ => Position::Striker,
                };
                let mut player = Player::new(
                    format!("{club}-{n}"),
                    "P".into(),
                    format!("{club} {n}"),
                    "1998-01-01".into(),
                    "England".into(),
                    position.clone(),
                    uniform_attributes(65),
                );
                player.position = position;
                player.team_id = Some(club.into());
                player
            })
            .collect()
    }

    fn report_of_seed(seed: u64, score: (u8, u8), home: &[Player], away: &[Player]) -> MatchReport {
        let mut rng = crate::seed::rng_for_seed(seed, "dormant-report-test", "2030-08-10");
        let (h, a): (Vec<&Player>, Vec<&Player>) = (home.iter().collect(), away.iter().collect());
        lightweight_report(score, &h, &a, &mut rng)
    }

    /// Given four hundred lightweight matches of two elevens,
    /// When their reports are read,
    /// Then goals, cards, dismissals and injuries all occur, each event belongs to a player of the
    /// side it names, the player's own line agrees with it (a second yellow is two yellows, a
    /// dismissal ends the player's match at that minute, an injury ends it no later), nobody
    /// assists his own goal, and the scorers' goals are the score.
    #[test]
    fn a_lightweight_report_is_consistent_and_draws_cards_and_injuries() {
        let (home, away) = (eleven("home"), eleven("away"));
        let ids =
            |players: &[Player]| -> Vec<String> { players.iter().map(|p| p.id.clone()).collect() };
        let (home_ids, away_ids) = (ids(&home), ids(&away));
        let (mut yellows, mut seconds, mut reds, mut injuries, mut goals) = (0, 0, 0, 0, 0);
        for seed in 0..400 {
            let score = ((seed % 4) as u8, (seed % 3) as u8);
            let report = report_of_seed(seed, score, &home, &away);
            assert_eq!(report.home_goals, score.0);
            assert_eq!(report.goals.len(), usize::from(score.0 + score.1));
            let scored: u32 = report
                .player_stats
                .values()
                .map(|s| u32::from(s.goals))
                .sum();
            assert_eq!(scored, u32::from(score.0 + score.1));
            for goal in &report.goals {
                let own = if goal.side == Side::Home {
                    &home_ids
                } else {
                    &away_ids
                };
                assert!(own.contains(&goal.scorer_id), "scorer from the wrong club");
                assert_ne!(
                    goal.assist_id.as_ref(),
                    Some(&goal.scorer_id),
                    "assisted himself"
                );
                if let Some(assist) = &goal.assist_id {
                    assert!(own.contains(assist), "assist from the wrong club");
                }
                goals += 1;
            }
            for event in &report.events {
                let id = event.player_id.as_deref().unwrap_or_default();
                if matches!(
                    event.event_type,
                    EventType::YellowCard
                        | EventType::SecondYellow
                        | EventType::RedCard
                        | EventType::Injury
                ) {
                    let own = if event.side == Side::Home {
                        &home_ids
                    } else {
                        &away_ids
                    };
                    assert!(
                        own.iter().any(|p| p == id),
                        "{:?} by {id} for the wrong side",
                        event.event_type
                    );
                }
                let line = report.player_stats.get(id);
                match event.event_type {
                    EventType::YellowCard => yellows += 1,
                    EventType::SecondYellow => {
                        seconds += 1;
                        let line = line.expect("a line");
                        assert_eq!((line.yellow_cards, line.red_cards), (2, 0));
                        assert_eq!(line.minutes_played, event.minute);
                    }
                    EventType::RedCard => {
                        reds += 1;
                        let line = line.expect("a line");
                        assert_eq!((line.yellow_cards, line.red_cards), (0, 1));
                        assert_eq!(line.minutes_played, event.minute);
                    }
                    EventType::Injury => {
                        injuries += 1;
                        assert!(line.expect("a line").minutes_played <= event.minute);
                    }
                    _ => {}
                }
            }
            assert!(report.events.windows(2).all(|w| w[0].minute <= w[1].minute));
        }
        assert!(goals > 400, "goals were drawn ({goals})");
        assert!(yellows > 400, "yellow cards were drawn ({yellows})");
        assert!(
            seconds > 0 && reds > 0,
            "dismissals were drawn ({seconds} second yellows, {reds} reds)"
        );
        assert!(injuries > 100, "injuries were drawn ({injuries})");
    }

    /// Given many goals drawn for a side of one goalkeeper, four defenders, four midfielders and two
    /// strikers of equal attributes,
    /// When the scorers are counted,
    /// Then a striker scores far more than a defender, who scores more than the goalkeeper.
    #[test]
    fn goals_go_to_the_players_whose_position_scores_them() {
        let (home, away) = (eleven("home"), eleven("away"));
        let mut by_position: std::collections::HashMap<&str, u32> =
            std::collections::HashMap::new();
        for seed in 0..600 {
            for goal in &report_of_seed(seed, (3, 0), &home, &away).goals {
                let player = home
                    .iter()
                    .find(|p| p.id == goal.scorer_id)
                    .expect("a home scorer");
                let key = match player.position {
                    Position::Striker => "striker",
                    Position::Midfielder => "midfielder",
                    Position::Defender => "defender",
                    _ => "goalkeeper",
                };
                *by_position.entry(key).or_default() += 1;
            }
        }
        let per_player = |key: &str, n: u32| {
            f64::from(by_position.get(key).copied().unwrap_or(0)) / f64::from(n)
        };
        assert!(
            per_player("striker", 2) > 2.0 * per_player("defender", 4),
            "{by_position:?}"
        );
        assert!(
            per_player("defender", 4) > per_player("goalkeeper", 1),
            "{by_position:?}"
        );
    }
}
