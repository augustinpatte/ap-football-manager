//! Pure M12 v1 output-contract projection into OFM's existing `MatchReport`.
//!
//! These types mirror only the published wire contract needed for projection;
//! this crate deliberately does not depend on the unlicensed M12 repository.
//! The projection validates input and builds a report, but never applies it to a game.

use engine::{
    EventType, GoalDetail, GoalSource, MatchEvent, MatchReport, PlayerMatchStats, Side, TeamStats,
    Zone,
};
use std::collections::{HashMap, HashSet};

pub const OUTPUT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionContext {
    pub match_id: String,
    pub home_team_id: String,
    pub away_team_id: String,
    pub home_player_ids: Vec<String>,
    pub away_player_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialMatchResult {
    pub schema_version: u32,
    pub match_id: String,
    pub home_team_id: String,
    pub away_team_id: String,
    pub score: SpatialScore,
    pub end_status: SpatialEndStatus,
    pub events: Vec<SpatialEvent>,
    pub home_stats: SpatialTeamStats,
    pub away_stats: SpatialTeamStats,
    pub player_stats: Vec<SpatialPlayerStats>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpatialScore {
    pub home: u8,
    pub away: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialEndStatus {
    RegulationComplete,
    ExtraTimeComplete,
    ShootoutComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialSide {
    Home,
    Away,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialCard {
    Yellow,
    SecondYellow,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialShotOutcome {
    Saved,
    Blocked,
    OffTarget,
    Goal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpatialEventKind {
    Goal {
        scorer_id: String,
        assist_id: Option<String>,
        own_goal: bool,
    },
    Penalty {
        taker_id: String,
    },
    Shot {
        player_id: String,
        outcome: SpatialShotOutcome,
    },
    Card {
        player_id: String,
        card: SpatialCard,
    },
    Substitution {
        player_off_id: String,
        player_on_id: String,
    },
    Injury {
        player_id: String,
    },
    ShootoutKick {
        taker_id: String,
        scored: bool,
    },
    Corner,
    Foul {
        player_id: String,
    },
    PassCompleted {
        player_id: String,
    },
    PassFailed {
        player_id: String,
    },
    Tackle {
        player_id: String,
    },
    Interception {
        player_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialEvent {
    pub t_ms: u32,
    pub side: SpatialSide,
    pub kind: SpatialEventKind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpatialTeamStats {
    pub possession_pct: f32,
    pub shots: u16,
    pub shots_on_target: u16,
    pub passes_attempted: u16,
    pub passes_completed: u16,
    pub tackles: u16,
    pub interceptions: u16,
    pub fouls: u16,
    pub corners: u16,
    pub yellow_cards: u8,
    pub red_cards: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialPlayerStats {
    pub player_id: String,
    pub minutes_played: u8,
    pub goals: u8,
    pub assists: u8,
    pub shots: u8,
    pub passes_attempted: u16,
    pub passes_completed: u16,
    pub tackles_won: u8,
    pub interceptions: u8,
    pub rating: f32,
}

#[derive(Debug, Clone)]
pub struct ProjectedMatch {
    pub report: MatchReport,
    pub limitations: Vec<ProjectionLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionLimitation {
    InjuryDurationIsNotRepresented { player_id: String },
    SpatialStatisticIsNotRepresented { name: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    UnsupportedSchema(u32),
    MatchIdentityMismatch,
    TeamIdentityMismatch,
    DuplicatePlayerId(String),
    UnknownPlayer(String),
    PlayerOnWrongTeam(String),
    DuplicateEvent,
    NonChronologicalEvents,
    GoalScoreMismatch {
        expected_home: u8,
        expected_away: u8,
        found_home: u8,
        found_away: u8,
    },
    ShootoutStatusMismatch,
    InvalidPossession,
    PlayerStatOverflow(String),
    InvalidRating(String),
    InvalidSubstitution(String),
    OwnGoalCannotBeProjected(String),
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProjectionError {}

/// Converts an M12 v1 result into an OFM report without changing any career state.
pub fn project(
    context: &ProjectionContext,
    input: &SpatialMatchResult,
) -> Result<ProjectedMatch, ProjectionError> {
    validate_context(context, input)?;
    validate_events(context, input)?;
    validate_player_stats(context, input)?;
    validate_stats(input)?;
    let projected_events = project_events(input)?;
    let (home_goals, away_goals) = score_events(&projected_events.events);
    if (home_goals, away_goals) != (input.score.home, input.score.away) {
        return Err(ProjectionError::GoalScoreMismatch {
            expected_home: input.score.home,
            expected_away: input.score.away,
            found_home: home_goals,
            found_away: away_goals,
        });
    }
    let (home_penalties, away_penalties) = shootout_score(&projected_events.events);
    if matches!(input.end_status, SpatialEndStatus::ShootoutComplete) != home_penalties.is_some() {
        return Err(ProjectionError::ShootoutStatusMismatch);
    }
    Ok(ProjectedMatch {
        report: MatchReport {
            home_goals,
            away_goals,
            home_stats: project_team_stats(&input.home_stats, home_goals),
            away_stats: project_team_stats(&input.away_stats, away_goals),
            events: projected_events.events,
            goals: projected_events.goals,
            player_stats: project_player_stats(&input.player_stats)?,
            home_possession: f64::from(input.home_stats.possession_pct),
            total_minutes: total_minutes(input),
            home_penalties,
            away_penalties,
        },
        limitations: projected_events.limitations,
    })
}

fn validate_context(
    context: &ProjectionContext,
    input: &SpatialMatchResult,
) -> Result<(), ProjectionError> {
    if input.schema_version != OUTPUT_SCHEMA_VERSION {
        return Err(ProjectionError::UnsupportedSchema(input.schema_version));
    }
    if input.match_id != context.match_id {
        return Err(ProjectionError::MatchIdentityMismatch);
    }
    if input.home_team_id != context.home_team_id
        || input.away_team_id != context.away_team_id
        || input.home_team_id == input.away_team_id
    {
        return Err(ProjectionError::TeamIdentityMismatch);
    }
    let mut players = HashSet::new();
    for id in context
        .home_player_ids
        .iter()
        .chain(&context.away_player_ids)
    {
        if !players.insert(id) {
            return Err(ProjectionError::DuplicatePlayerId(id.clone()));
        }
    }
    Ok(())
}

fn validate_events(
    context: &ProjectionContext,
    input: &SpatialMatchResult,
) -> Result<(), ProjectionError> {
    let home: HashSet<&str> = context.home_player_ids.iter().map(String::as_str).collect();
    let away: HashSet<&str> = context.away_player_ids.iter().map(String::as_str).collect();
    let mut prior = None;
    let mut seen = HashSet::new();
    for event in &input.events {
        if prior.is_some_and(|time| event.t_ms < time) {
            return Err(ProjectionError::NonChronologicalEvents);
        }
        prior = Some(event.t_ms);
        if !seen.insert(format!("{:?}", event)) {
            return Err(ProjectionError::DuplicateEvent);
        }
        for player_id in event_players(&event.kind) {
            validate_player(player_id, event.side, &home, &away)?;
        }
        if let SpatialEventKind::Substitution {
            player_off_id,
            player_on_id,
        } = &event.kind
            && player_off_id == player_on_id
        {
            return Err(ProjectionError::InvalidSubstitution(player_off_id.clone()));
        }
    }
    Ok(())
}

fn validate_player(
    id: &str,
    side: SpatialSide,
    home: &HashSet<&str>,
    away: &HashSet<&str>,
) -> Result<(), ProjectionError> {
    let known = home.contains(id) || away.contains(id);
    if !known {
        return Err(ProjectionError::UnknownPlayer(id.to_string()));
    }
    let right_side = matches!(
        (side, home.contains(id)),
        (SpatialSide::Home, true) | (SpatialSide::Away, false)
    );
    if !right_side {
        return Err(ProjectionError::PlayerOnWrongTeam(id.to_string()));
    }
    Ok(())
}

fn event_players(kind: &SpatialEventKind) -> Vec<&str> {
    match kind {
        SpatialEventKind::Goal {
            scorer_id,
            assist_id,
            ..
        } => std::iter::once(scorer_id.as_str())
            .chain(assist_id.iter().map(String::as_str))
            .collect(),
        SpatialEventKind::Penalty {
            taker_id: scorer_id,
        }
        | SpatialEventKind::Shot {
            player_id: scorer_id,
            ..
        }
        | SpatialEventKind::Card {
            player_id: scorer_id,
            ..
        }
        | SpatialEventKind::Injury {
            player_id: scorer_id,
        }
        | SpatialEventKind::ShootoutKick {
            taker_id: scorer_id,
            ..
        }
        | SpatialEventKind::Foul {
            player_id: scorer_id,
        }
        | SpatialEventKind::PassCompleted {
            player_id: scorer_id,
        }
        | SpatialEventKind::PassFailed {
            player_id: scorer_id,
        }
        | SpatialEventKind::Tackle {
            player_id: scorer_id,
        }
        | SpatialEventKind::Interception {
            player_id: scorer_id,
        } => vec![scorer_id],
        SpatialEventKind::Substitution {
            player_off_id,
            player_on_id,
        } => vec![player_off_id, player_on_id],
        SpatialEventKind::Corner => Vec::new(),
    }
}

fn validate_stats(input: &SpatialMatchResult) -> Result<(), ProjectionError> {
    if !(0.0..=100.0).contains(&input.home_stats.possession_pct)
        || !(0.0..=100.0).contains(&input.away_stats.possession_pct)
        || (input.home_stats.possession_pct + input.away_stats.possession_pct - 100.0).abs() > 0.01
    {
        return Err(ProjectionError::InvalidPossession);
    }
    Ok(())
}

fn validate_player_stats(
    context: &ProjectionContext,
    input: &SpatialMatchResult,
) -> Result<(), ProjectionError> {
    let known: HashSet<&str> = context
        .home_player_ids
        .iter()
        .chain(&context.away_player_ids)
        .map(String::as_str)
        .collect();
    let mut seen = HashSet::new();
    for stat in &input.player_stats {
        if !known.contains(stat.player_id.as_str()) {
            return Err(ProjectionError::UnknownPlayer(stat.player_id.clone()));
        }
        if !seen.insert(stat.player_id.as_str()) {
            return Err(ProjectionError::DuplicatePlayerId(stat.player_id.clone()));
        }
    }
    Ok(())
}

struct ProjectedEvents {
    events: Vec<MatchEvent>,
    goals: Vec<GoalDetail>,
    limitations: Vec<ProjectionLimitation>,
}

fn project_events(input: &SpatialMatchResult) -> Result<ProjectedEvents, ProjectionError> {
    let mut events = Vec::new();
    let mut goals = Vec::new();
    let mut limitations = vec![
        ProjectionLimitation::SpatialStatisticIsNotRepresented { name: "xg" },
        ProjectionLimitation::SpatialStatisticIsNotRepresented { name: "distance_m" },
        ProjectionLimitation::SpatialStatisticIsNotRepresented { name: "sprints" },
    ];
    let mut previous_was_penalty = false;
    for event in &input.events {
        let side = side(event.side);
        let minute = minute(event.t_ms);
        let (event_type, player_id, secondary_player_id) = match &event.kind {
            SpatialEventKind::Goal {
                scorer_id,
                assist_id,
                own_goal,
            } => {
                if *own_goal {
                    return Err(ProjectionError::OwnGoalCannotBeProjected(scorer_id.clone()));
                }
                goals.push(GoalDetail {
                    minute,
                    scorer_id: scorer_id.clone(),
                    assist_id: assist_id.clone(),
                    goal_source: if previous_was_penalty {
                        GoalSource::Penalty
                    } else {
                        GoalSource::OpenPlay
                    },
                    side,
                });
                (
                    if previous_was_penalty {
                        EventType::PenaltyGoal
                    } else {
                        EventType::Goal
                    },
                    Some(scorer_id.clone()),
                    assist_id.clone(),
                )
            }
            SpatialEventKind::Penalty { taker_id } => {
                (EventType::PenaltyAwarded, Some(taker_id.clone()), None)
            }
            SpatialEventKind::Shot { player_id, outcome } => (
                match outcome {
                    SpatialShotOutcome::Saved => EventType::ShotSaved,
                    SpatialShotOutcome::Blocked => EventType::ShotBlocked,
                    SpatialShotOutcome::OffTarget => EventType::ShotOffTarget,
                    SpatialShotOutcome::Goal => EventType::ShotOnTarget,
                },
                Some(player_id.clone()),
                None,
            ),
            SpatialEventKind::Card { player_id, card } => (
                match card {
                    SpatialCard::Yellow => EventType::YellowCard,
                    SpatialCard::SecondYellow => EventType::SecondYellow,
                    SpatialCard::Red => EventType::RedCard,
                },
                Some(player_id.clone()),
                None,
            ),
            SpatialEventKind::Substitution {
                player_off_id,
                player_on_id,
            } => (
                EventType::Substitution,
                Some(player_on_id.clone()),
                Some(player_off_id.clone()),
            ),
            SpatialEventKind::Injury { player_id } => {
                limitations.push(ProjectionLimitation::InjuryDurationIsNotRepresented {
                    player_id: player_id.clone(),
                });
                (EventType::Injury, Some(player_id.clone()), None)
            }
            SpatialEventKind::ShootoutKick { taker_id, scored } => (
                if *scored {
                    EventType::ShootoutGoal
                } else {
                    EventType::ShootoutMiss
                },
                Some(taker_id.clone()),
                None,
            ),
            SpatialEventKind::Corner => (EventType::Corner, None, None),
            SpatialEventKind::Foul { player_id } => {
                (EventType::Foul, Some(player_id.clone()), None)
            }
            SpatialEventKind::PassCompleted { player_id } => {
                (EventType::PassCompleted, Some(player_id.clone()), None)
            }
            SpatialEventKind::PassFailed { player_id } => {
                (EventType::PassIntercepted, Some(player_id.clone()), None)
            }
            SpatialEventKind::Tackle { player_id } => {
                (EventType::Tackle, Some(player_id.clone()), None)
            }
            SpatialEventKind::Interception { player_id } => {
                (EventType::Interception, Some(player_id.clone()), None)
            }
        };
        events.push(MatchEvent {
            minute,
            event_type,
            side,
            zone: Zone::Midfield,
            player_id,
            secondary_player_id,
            detail: None,
        });
        previous_was_penalty = matches!(event.kind, SpatialEventKind::Penalty { .. });
    }
    Ok(ProjectedEvents {
        events,
        goals,
        limitations,
    })
}

fn project_team_stats(stats: &SpatialTeamStats, goals: u8) -> TeamStats {
    TeamStats {
        goals,
        shots: stats.shots,
        shots_on_target: stats.shots_on_target,
        shots_off_target: stats.shots.saturating_sub(stats.shots_on_target),
        shots_blocked: 0,
        passes_completed: stats.passes_completed,
        passes_intercepted: stats
            .passes_attempted
            .saturating_sub(stats.passes_completed),
        tackles: stats.tackles,
        interceptions: stats.interceptions,
        fouls: stats.fouls,
        corners: stats.corners,
        free_kicks: 0,
        penalties: 0,
        yellow_cards: stats.yellow_cards,
        red_cards: stats.red_cards,
        possession_ticks: 0,
    }
}

fn project_player_stats(
    stats: &[SpatialPlayerStats],
) -> Result<HashMap<String, PlayerMatchStats>, ProjectionError> {
    let mut result = HashMap::new();
    for stat in stats {
        if stat.passes_attempted > u16::from(u8::MAX) || stat.passes_completed > u16::from(u8::MAX)
        {
            return Err(ProjectionError::PlayerStatOverflow(stat.player_id.clone()));
        }
        if !(0.0..=10.0).contains(&stat.rating) {
            return Err(ProjectionError::InvalidRating(stat.player_id.clone()));
        }
        if result
            .insert(
                stat.player_id.clone(),
                PlayerMatchStats {
                    minutes_played: stat.minutes_played,
                    goals: stat.goals,
                    assists: stat.assists,
                    shots: stat.shots,
                    shots_on_target: 0,
                    passes_completed: stat.passes_completed as u8,
                    passes_attempted: stat.passes_attempted as u8,
                    tackles_won: stat.tackles_won,
                    interceptions: stat.interceptions,
                    fouls_committed: 0,
                    yellow_cards: 0,
                    red_cards: 0,
                    rating: stat.rating,
                },
            )
            .is_some()
        {
            return Err(ProjectionError::DuplicatePlayerId(stat.player_id.clone()));
        }
    }
    Ok(result)
}

fn score_events(events: &[MatchEvent]) -> (u8, u8) {
    events
        .iter()
        .filter(|event| event.is_goal())
        .fold((0, 0), |(home, away), event| match event.side {
            Side::Home => (home + 1, away),
            Side::Away => (home, away + 1),
        })
}
fn shootout_score(events: &[MatchEvent]) -> (Option<u8>, Option<u8>) {
    let count = |side| {
        events
            .iter()
            .filter(|event| event.side == side && event.event_type == EventType::ShootoutGoal)
            .count() as u8
    };
    let any = events.iter().any(|event| {
        matches!(
            event.event_type,
            EventType::ShootoutGoal | EventType::ShootoutMiss
        )
    });
    any.then(|| (count(Side::Home), count(Side::Away)))
        .map_or((None, None), |(home, away)| (Some(home), Some(away)))
}
fn side(side: SpatialSide) -> Side {
    match side {
        SpatialSide::Home => Side::Home,
        SpatialSide::Away => Side::Away,
    }
}
fn minute(t_ms: u32) -> u8 {
    (t_ms / 60_000).min(u32::from(u8::MAX)) as u8
}
fn total_minutes(input: &SpatialMatchResult) -> u8 {
    if matches!(input.end_status, SpatialEndStatus::RegulationComplete) {
        90
    } else {
        120
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> ProjectionContext {
        ProjectionContext {
            match_id: "m1".into(),
            home_team_id: "h".into(),
            away_team_id: "a".into(),
            home_player_ids: vec!["h1".into(), "h2".into(), "h3".into()],
            away_player_ids: vec!["a1".into(), "a2".into(), "a3".into()],
        }
    }
    fn result(score: SpatialScore, events: Vec<SpatialEvent>) -> SpatialMatchResult {
        SpatialMatchResult {
            schema_version: 1,
            match_id: "m1".into(),
            home_team_id: "h".into(),
            away_team_id: "a".into(),
            score,
            end_status: SpatialEndStatus::RegulationComplete,
            events,
            home_stats: SpatialTeamStats {
                possession_pct: 50.0,
                ..Default::default()
            },
            away_stats: SpatialTeamStats {
                possession_pct: 50.0,
                ..Default::default()
            },
            player_stats: vec![],
        }
    }
    fn event(t_ms: u32, side: SpatialSide, kind: SpatialEventKind) -> SpatialEvent {
        SpatialEvent { t_ms, side, kind }
    }
    /// Given a scoreless contractual result, when projected, then OFM keeps the 0-0 score.
    #[test]
    fn scoreless_result_projects() {
        let projected = project(&context(), &result(SpatialScore::default(), vec![])).unwrap();
        assert_eq!(
            (projected.report.home_goals, projected.report.away_goals),
            (0, 0)
        );
    }
    /// Given one ordinary goal, when projected, then the scorer and score are retained.
    #[test]
    fn one_nil_projects_scorer() {
        let output = result(
            SpatialScore { home: 1, away: 0 },
            vec![event(
                60_000,
                SpatialSide::Home,
                SpatialEventKind::Goal {
                    scorer_id: "h1".into(),
                    assist_id: Some("h2".into()),
                    own_goal: false,
                },
            )],
        );
        let projected = project(&context(), &output).unwrap();
        assert_eq!(projected.report.goals[0].assist_id.as_deref(), Some("h2"));
    }
    /// Given a drawn result, when projected, then each side's goals are counted independently.
    #[test]
    fn two_two_projects() {
        let output = result(
            SpatialScore { home: 2, away: 2 },
            vec![
                event(
                    1,
                    SpatialSide::Home,
                    SpatialEventKind::Goal {
                        scorer_id: "h1".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
                event(
                    2,
                    SpatialSide::Away,
                    SpatialEventKind::Goal {
                        scorer_id: "a1".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
                event(
                    3,
                    SpatialSide::Home,
                    SpatialEventKind::Goal {
                        scorer_id: "h2".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
                event(
                    4,
                    SpatialSide::Away,
                    SpatialEventKind::Goal {
                        scorer_id: "a2".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
            ],
        );
        assert_eq!(
            (
                project(&context(), &output).unwrap().report.home_goals,
                project(&context(), &output).unwrap().report.away_goals
            ),
            (2, 2)
        );
    }
    /// Given extra time and a shootout, when projected, then shootout goals do not inflate match goals.
    #[test]
    fn extra_time_and_shootout_stay_separate() {
        let mut output = result(
            SpatialScore { home: 1, away: 1 },
            vec![
                event(
                    100,
                    SpatialSide::Home,
                    SpatialEventKind::Goal {
                        scorer_id: "h1".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
                event(
                    200,
                    SpatialSide::Away,
                    SpatialEventKind::Penalty {
                        taker_id: "a1".into(),
                    },
                ),
                event(
                    250,
                    SpatialSide::Away,
                    SpatialEventKind::Goal {
                        scorer_id: "a1".into(),
                        assist_id: None,
                        own_goal: false,
                    },
                ),
                event(
                    300,
                    SpatialSide::Home,
                    SpatialEventKind::ShootoutKick {
                        taker_id: "h2".into(),
                        scored: true,
                    },
                ),
                event(
                    400,
                    SpatialSide::Away,
                    SpatialEventKind::ShootoutKick {
                        taker_id: "a2".into(),
                        scored: false,
                    },
                ),
            ],
        );
        output.end_status = SpatialEndStatus::ShootoutComplete;
        let report = project(&context(), &output).unwrap().report;
        assert_eq!(
            (
                report.home_goals,
                report.away_goals,
                report.home_penalties,
                report.away_penalties
            ),
            (1, 1, Some(1), Some(0))
        );
    }
    /// Given a non-goal event, when projected, then it does not alter the contractual score.
    #[test]
    fn non_goal_events_do_not_count() {
        let output = result(
            SpatialScore::default(),
            vec![event(
                1,
                SpatialSide::Home,
                SpatialEventKind::Shot {
                    player_id: "h1".into(),
                    outcome: SpatialShotOutcome::Goal,
                },
            )],
        );
        assert_eq!(project(&context(), &output).unwrap().report.home_goals, 0);
    }
    /// Given cards and an injury, when projected, then native event types and the medical limitation are explicit.
    #[test]
    fn discipline_and_injury_project() {
        let output = result(
            SpatialScore::default(),
            vec![
                event(
                    1,
                    SpatialSide::Home,
                    SpatialEventKind::Card {
                        player_id: "h1".into(),
                        card: SpatialCard::Yellow,
                    },
                ),
                event(
                    2,
                    SpatialSide::Away,
                    SpatialEventKind::Card {
                        player_id: "a1".into(),
                        card: SpatialCard::Red,
                    },
                ),
                event(
                    3,
                    SpatialSide::Home,
                    SpatialEventKind::Injury {
                        player_id: "h2".into(),
                    },
                ),
            ],
        );
        let projected = project(&context(), &output).unwrap();
        assert!(
            projected
                .report
                .events
                .iter()
                .any(|e| e.event_type == EventType::RedCard)
        );
        assert!(projected.limitations.iter().any(|l| matches!(
            l,
            ProjectionLimitation::InjuryDurationIsNotRepresented { .. }
        )));
    }
    /// Given a valid substitution, when projected, then OFM's on/off convention is retained.
    #[test]
    fn substitutions_project() {
        let output = result(
            SpatialScore::default(),
            vec![event(
                3_600_000,
                SpatialSide::Home,
                SpatialEventKind::Substitution {
                    player_off_id: "h1".into(),
                    player_on_id: "h2".into(),
                },
            )],
        );
        let report = project(&context(), &output).unwrap().report;
        assert_eq!(report.events[0].secondary_player_id.as_deref(), Some("h1"));
    }
    /// Given invalid identities, duplicated events, bad temporal order or a mismatched score, when projected, then it is refused.
    #[test]
    fn inconsistent_results_are_refused() {
        let mut output = result(
            SpatialScore::default(),
            vec![
                event(
                    2,
                    SpatialSide::Home,
                    SpatialEventKind::Foul {
                        player_id: "h1".into(),
                    },
                ),
                event(
                    1,
                    SpatialSide::Home,
                    SpatialEventKind::Foul {
                        player_id: "h1".into(),
                    },
                ),
            ],
        );
        assert!(matches!(
            project(&context(), &output),
            Err(ProjectionError::NonChronologicalEvents)
        ));
        output.events = vec![event(
            1,
            SpatialSide::Home,
            SpatialEventKind::Foul {
                player_id: "bad".into(),
            },
        )];
        assert!(matches!(
            project(&context(), &output),
            Err(ProjectionError::UnknownPlayer(player_id)) if player_id == "bad"
        ));
        output.events = vec![
            event(
                1,
                SpatialSide::Home,
                SpatialEventKind::Foul {
                    player_id: "h1".into(),
                },
            ),
            event(
                1,
                SpatialSide::Home,
                SpatialEventKind::Foul {
                    player_id: "h1".into(),
                },
            ),
        ];
        assert!(matches!(
            project(&context(), &output),
            Err(ProjectionError::DuplicateEvent)
        ));
        output.events = vec![];
        output.score.home = 1;
        assert!(matches!(
            project(&context(), &output),
            Err(ProjectionError::GoalScoreMismatch { .. })
        ));
        output.score.home = 0;
        output.player_stats = vec![SpatialPlayerStats {
            player_id: "unknown".into(),
            minutes_played: 1,
            goals: 0,
            assists: 0,
            shots: 0,
            passes_attempted: 0,
            passes_completed: 0,
            tackles_won: 0,
            interceptions: 0,
            rating: 6.0,
        }];
        assert!(matches!(
            project(&context(), &output),
            Err(ProjectionError::UnknownPlayer(player_id)) if player_id == "unknown"
        ));
    }
    /// Given identical input, when projected twice, then the report remains deterministic.
    #[test]
    fn projection_is_deterministic() {
        let output = result(
            SpatialScore::default(),
            vec![event(
                1,
                SpatialSide::Home,
                SpatialEventKind::Foul {
                    player_id: "h1".into(),
                },
            )],
        );
        assert_eq!(
            format!("{:?}", project(&context(), &output).unwrap()),
            format!("{:?}", project(&context(), &output).unwrap())
        );
    }
}
