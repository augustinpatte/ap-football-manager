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
