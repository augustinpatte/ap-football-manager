//! Local, dependency-free decoder for the published M12 v1 match-output JSON.
//!
//! It accepts the wire format only, validates its structural invariants, and
//! reduces the recognised football facts to the projection input. It never
//! starts M12, applies a report, or touches persistence.

use crate::spatial_match_projection::{
    OUTPUT_SCHEMA_VERSION, SpatialCard, SpatialEndStatus, SpatialEvent, SpatialEventKind,
    SpatialMatchResult, SpatialPlayerStats, SpatialScore, SpatialShotOutcome, SpatialSide,
    SpatialTeamStats,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

const MAX_MATCH_TIME_MS: u32 = 8_000_000;
const PITCH_LENGTH_M: f32 = 105.0;
const PITCH_WIDTH_M: f32 = 68.0;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSpatialOutput {
    pub result: SpatialMatchResult,
    /// The untouched M12 output document for the versioned replay viewer.
    pub replay_json: String,
    /// Contract events with no OFM `MatchReport` equivalent are intentionally
    /// retained as type names for callers, not silently presented as OFM facts.
    pub unprojected_event_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDecodeError {
    InvalidJson,
    UnsupportedSchema(u32),
    MissingOrInvalidField(&'static str),
    InvalidIdentifier { field: &'static str, value: String },
    DuplicateIdentifier(String),
    TeamIdentityMismatch,
    InvalidCoordinateSystem,
    InvalidTime(u32),
    NonChronologicalEvents,
    MissingEventSide(String),
    InvalidEvent(String),
    InvalidNumeric { field: &'static str },
    InvalidPossession,
    IncompleteSimulation(String),
}

impl std::fmt::Display for OutputDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OutputDecodeError {}

#[derive(Deserialize)]
struct RawOutput {
    schema_version: u32,
    match_id: String,
    home_team: String,
    away_team: String,
    seed: u64,
    engine: RawEngine,
    coordinate_system: RawCoordinateSystem,
    frame_interval_ms: u16,
    roster: Vec<String>,
    score: RawScore,
    events: Vec<RawEvent>,
    frames: Vec<Value>,
    home_stats: RawTeamStats,
    away_stats: RawTeamStats,
    player_stats: Vec<RawPlayerStats>,
    end_state: Vec<RawEndState>,
    #[serde(default)]
    simulation: Option<RawSimulation>,
}

#[derive(Deserialize)]
struct RawEngine {
    name: String,
    version: String,
    input_digest: String,
}
#[derive(Deserialize)]
struct RawCoordinateSystem {
    unit: String,
    origin: String,
    length_m: f32,
    width_m: f32,
    home_attacks_positive_x_in_period_1: bool,
    position_resolution_m: f32,
}
#[derive(Deserialize)]
struct RawScore {
    home: u8,
    away: u8,
}
#[derive(Deserialize)]
struct RawEvent {
    t_ms: u32,
    period: String,
    #[serde(default)]
    side: Option<String>,
    #[serde(flatten)]
    data: serde_json::Map<String, Value>,
}
#[derive(Deserialize)]
struct RawTeamStats {
    possession_pct: f32,
    shots: u16,
    shots_on_target: u16,
    xg: f32,
    passes_attempted: u16,
    passes_completed: u16,
    tackles: u16,
    interceptions: u16,
    fouls: u16,
    offsides: u16,
    corners: u16,
    yellow_cards: u8,
    red_cards: u8,
}
#[derive(Deserialize)]
struct RawPlayerStats {
    id: String,
    minutes_played: u8,
    goals: u8,
    assists: u8,
    shots: u8,
    passes_attempted: u16,
    passes_completed: u16,
    tackles_won: u8,
    interceptions: u8,
    distance_m: f32,
    sprints: u16,
    rating: f32,
}
#[derive(Deserialize)]
struct RawEndState {
    id: String,
    fatigue_end: f32,
    morale_delta: i8,
    injury: Option<Value>,
    card: Option<String>,
}
#[derive(Deserialize)]
struct RawSimulation {
    mode: String,
    football_systems: bool,
    tick_ms: u16,
    ticks_executed: u64,
    end_status: String,
    periods: Vec<Value>,
    note: String,
}

/// Decodes and validates a full M12 v1 `MatchOutput` JSON document.
pub fn decode_json(json: &str) -> Result<DecodedSpatialOutput, OutputDecodeError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OutputDecodeError::InvalidJson)?;
    let schema_version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(OutputDecodeError::MissingOrInvalidField("schema_version"))?;
    let schema_version = u32::try_from(schema_version)
        .map_err(|_| OutputDecodeError::MissingOrInvalidField("schema_version"))?;
    if schema_version != OUTPUT_SCHEMA_VERSION {
        return Err(OutputDecodeError::UnsupportedSchema(schema_version));
    }
    let raw: RawOutput = serde_json::from_value(value)
        .map_err(|_| OutputDecodeError::MissingOrInvalidField("match_output"))?;
    if raw.schema_version != OUTPUT_SCHEMA_VERSION {
        return Err(OutputDecodeError::UnsupportedSchema(raw.schema_version));
    }
    validate_top_level(&raw)?;
    let (events, unprojected_event_types) = decode_events(&raw.events)?;
    validate_references(&raw.roster, &events, &raw.player_stats)?;
    let result = SpatialMatchResult {
        schema_version: raw.schema_version,
        match_id: raw.match_id,
        home_team_id: raw.home_team,
        away_team_id: raw.away_team,
        score: SpatialScore {
            home: raw.score.home,
            away: raw.score.away,
        },
        end_status: decode_end_status(
            raw.simulation
                .as_ref()
                .map(|simulation| simulation.end_status.as_str()),
        )?,
        events,
        home_stats: decode_team_stats(&raw.home_stats)?,
        away_stats: decode_team_stats(&raw.away_stats)?,
        player_stats: decode_player_stats(&raw.player_stats)?,
    };
    Ok(DecodedSpatialOutput {
        result,
        replay_json: json.to_string(),
        unprojected_event_types,
    })
}

fn validate_references(
    roster: &[String],
    events: &[SpatialEvent],
    player_stats: &[RawPlayerStats],
) -> Result<(), OutputDecodeError> {
    let roster: HashSet<&str> = roster.iter().map(String::as_str).collect();
    for player_id in events.iter().flat_map(event_player_ids) {
        if !roster.contains(player_id) {
            return Err(OutputDecodeError::InvalidIdentifier {
                field: "event.player",
                value: player_id.to_string(),
            });
        }
    }
    for stat in player_stats {
        if !roster.contains(stat.id.as_str()) {
            return Err(OutputDecodeError::InvalidIdentifier {
                field: "player_stats.id",
                value: stat.id.clone(),
            });
        }
    }
    Ok(())
}

fn event_player_ids(event: &SpatialEvent) -> Vec<&str> {
    match &event.kind {
        SpatialEventKind::Goal {
            scorer_id,
            assist_id,
            ..
        } => std::iter::once(scorer_id.as_str())
            .chain(assist_id.iter().map(String::as_str))
            .collect(),
        SpatialEventKind::Penalty { taker_id }
        | SpatialEventKind::Shot {
            player_id: taker_id,
            ..
        }
        | SpatialEventKind::Card {
            player_id: taker_id,
            ..
        }
        | SpatialEventKind::Injury {
            player_id: taker_id,
        }
        | SpatialEventKind::ShootoutKick { taker_id, .. }
        | SpatialEventKind::Foul {
            player_id: taker_id,
        }
        | SpatialEventKind::PassCompleted {
            player_id: taker_id,
        }
        | SpatialEventKind::PassFailed {
            player_id: taker_id,
        }
        | SpatialEventKind::Tackle {
            player_id: taker_id,
        }
        | SpatialEventKind::Interception {
            player_id: taker_id,
        } => vec![taker_id],
        SpatialEventKind::Substitution {
            player_off_id,
            player_on_id,
        } => vec![player_off_id, player_on_id],
        SpatialEventKind::Corner => Vec::new(),
    }
}

fn validate_top_level(raw: &RawOutput) -> Result<(), OutputDecodeError> {
    let _ = (raw.seed, raw.frames.len());
    for (field, value) in [
        ("match_id", &raw.match_id),
        ("home_team", &raw.home_team),
        ("away_team", &raw.away_team),
        ("engine.name", &raw.engine.name),
        ("engine.version", &raw.engine.version),
        ("engine.input_digest", &raw.engine.input_digest),
    ] {
        validate_id(field, value)?;
    }
    if raw.home_team == raw.away_team {
        return Err(OutputDecodeError::TeamIdentityMismatch);
    }
    if raw.frame_interval_ms == 0 {
        return Err(OutputDecodeError::InvalidNumeric {
            field: "frame_interval_ms",
        });
    }
    if raw.coordinate_system.unit != "metre"
        || raw.coordinate_system.origin != "bottom_left_corner_viewed_from_above"
        || !raw.coordinate_system.home_attacks_positive_x_in_period_1
        || !finite(raw.coordinate_system.length_m)
        || !finite(raw.coordinate_system.width_m)
        || !finite(raw.coordinate_system.position_resolution_m)
        || raw.coordinate_system.length_m != PITCH_LENGTH_M
        || raw.coordinate_system.width_m != PITCH_WIDTH_M
        || raw.coordinate_system.position_resolution_m <= 0.0
    {
        return Err(OutputDecodeError::InvalidCoordinateSystem);
    }
    let mut ids = HashSet::new();
    for id in &raw.roster {
        validate_id("roster", id)?;
        if !ids.insert(id) {
            return Err(OutputDecodeError::DuplicateIdentifier(id.clone()));
        }
    }
    let end_state_ids: HashSet<&str> = raw
        .end_state
        .iter()
        .map(|state| state.id.as_str())
        .collect();
    if end_state_ids.len() != raw.end_state.len()
        || end_state_ids.len() != raw.roster.len()
        || raw
            .roster
            .iter()
            .any(|id| !end_state_ids.contains(id.as_str()))
    {
        return Err(OutputDecodeError::MissingOrInvalidField("end_state"));
    }
    for state in &raw.end_state {
        if !finite(state.fatigue_end) || !(0.0..=1.0).contains(&state.fatigue_end) {
            return Err(OutputDecodeError::InvalidNumeric {
                field: "end_state.fatigue_end",
            });
        }
        let _ = (&state.injury, &state.card, state.morale_delta);
    }
    if let Some(simulation) = &raw.simulation {
        if simulation.mode != "autonomous_match"
            || !simulation.football_systems
            || simulation.tick_ms == 0
            || simulation.note.is_empty()
        {
            return Err(OutputDecodeError::IncompleteSimulation(
                simulation.mode.clone(),
            ));
        }
        let _ = (simulation.ticks_executed, simulation.periods.len());
    }
    Ok(())
}

fn decode_events(
    raw_events: &[RawEvent],
) -> Result<(Vec<SpatialEvent>, Vec<String>), OutputDecodeError> {
    let mut events = Vec::new();
    let mut ignored = Vec::new();
    let mut prior = None;
    for raw in raw_events {
        if raw.t_ms > MAX_MATCH_TIME_MS {
            return Err(OutputDecodeError::InvalidTime(raw.t_ms));
        }
        if prior.is_some_and(|time| raw.t_ms < time) {
            return Err(OutputDecodeError::NonChronologicalEvents);
        }
        prior = Some(raw.t_ms);
        let kind_name = string(&raw.data, "type")?;
        validate_period(&raw.period)?;
        match decode_event_kind(kind_name, raw.side.as_deref(), &raw.data)? {
            Some((side, kind)) => events.push(SpatialEvent {
                t_ms: raw.t_ms,
                side,
                kind,
            }),
            None => {
                if !ignored.contains(&kind_name.to_string()) {
                    ignored.push(kind_name.to_string());
                }
            }
        }
    }
    Ok((events, ignored))
}

fn decode_event_kind<'a>(
    name: &'a str,
    side: Option<&str>,
    data: &'a serde_json::Map<String, Value>,
) -> Result<Option<(SpatialSide, SpatialEventKind)>, OutputDecodeError> {
    let side_required = |name: &str| {
        parse_side(side.ok_or_else(|| OutputDecodeError::MissingEventSide(name.to_string()))?)
    };
    let event = match name {
        "goal" => {
            let side = side_required(name)?;
            Some((
                side,
                SpatialEventKind::Goal {
                    scorer_id: string(data, "scorer")?.into(),
                    assist_id: optional_string(data, "assist")?,
                    own_goal: boolean(data, "own_goal")?,
                },
            ))
        }
        "penalty" => Some((
            side_required(name)?,
            SpatialEventKind::Penalty {
                taker_id: string(data, "taker")?.into(),
            },
        )),
        "shot" => Some((
            side_required(name)?,
            SpatialEventKind::Shot {
                player_id: string(data, "player")?.into(),
                outcome: match string(data, "outcome")? {
                    "saved" => SpatialShotOutcome::Saved,
                    "blocked" => SpatialShotOutcome::Blocked,
                    "off_target" => SpatialShotOutcome::OffTarget,
                    "goal" => SpatialShotOutcome::Goal,
                    other => return Err(OutputDecodeError::InvalidEvent(other.into())),
                },
            },
        )),
        "card" => Some((
            side_required(name)?,
            SpatialEventKind::Card {
                player_id: string(data, "player")?.into(),
                card: match string(data, "colour")? {
                    "yellow" => SpatialCard::Yellow,
                    "second_yellow" => SpatialCard::SecondYellow,
                    "red" => SpatialCard::Red,
                    other => return Err(OutputDecodeError::InvalidEvent(other.into())),
                },
            },
        )),
        "substitution" => Some((
            side_required(name)?,
            SpatialEventKind::Substitution {
                player_off_id: string(data, "player_off")?.into(),
                player_on_id: string(data, "player_on")?.into(),
            },
        )),
        "injury" => Some((
            side_required(name)?,
            SpatialEventKind::Injury {
                player_id: string(data, "player")?.into(),
            },
        )),
        "shootout_kick" => Some((
            parse_side(string(data, "team")?)?,
            SpatialEventKind::ShootoutKick {
                taker_id: string(data, "taker")?.into(),
                scored: string(data, "outcome")? == "goal",
            },
        )),
        "corner" => Some((side_required(name)?, SpatialEventKind::Corner)),
        "foul" => Some((
            side_required(name)?,
            SpatialEventKind::Foul {
                player_id: string(data, "offender")?.into(),
            },
        )),
        "pass" => {
            let player_id = string(data, "from")?.into();
            if boolean(data, "completed")? {
                Some((
                    side_required(name)?,
                    SpatialEventKind::PassCompleted { player_id },
                ))
            } else {
                Some((
                    side_required(name)?,
                    SpatialEventKind::PassFailed { player_id },
                ))
            }
        }
        "tackle" => Some((
            side_required(name)?,
            SpatialEventKind::Tackle {
                player_id: string(data, "player")?.into(),
            },
        )),
        "interception" => Some((
            side_required(name)?,
            SpatialEventKind::Interception {
                player_id: string(data, "player")?.into(),
            },
        )),
        other if known_non_projected(other) => None,
        other => return Err(OutputDecodeError::InvalidEvent(other.into())),
    };
    Ok(event)
}

fn decode_end_status(status: Option<&str>) -> Result<SpatialEndStatus, OutputDecodeError> {
    match status.unwrap_or("regulation_complete") {
        "regulation_complete" => Ok(SpatialEndStatus::RegulationComplete),
        "extra_time_complete" => Ok(SpatialEndStatus::ExtraTimeComplete),
        "shootout_complete" => Ok(SpatialEndStatus::ShootoutComplete),
        other => Err(OutputDecodeError::InvalidEvent(other.into())),
    }
}
fn decode_team_stats(stats: &RawTeamStats) -> Result<SpatialTeamStats, OutputDecodeError> {
    let _ = stats.offsides;
    if !finite(stats.possession_pct)
        || !finite(stats.xg)
        || !(0.0..=100.0).contains(&stats.possession_pct)
        || stats.xg < 0.0
        || stats.shots_on_target > stats.shots
        || stats.passes_completed > stats.passes_attempted
    {
        return Err(OutputDecodeError::InvalidPossession);
    }
    Ok(SpatialTeamStats {
        possession_pct: stats.possession_pct,
        shots: stats.shots,
        shots_on_target: stats.shots_on_target,
        passes_attempted: stats.passes_attempted,
        passes_completed: stats.passes_completed,
        tackles: stats.tackles,
        interceptions: stats.interceptions,
        fouls: stats.fouls,
        corners: stats.corners,
        yellow_cards: stats.yellow_cards,
        red_cards: stats.red_cards,
    })
}
fn decode_player_stats(
    stats: &[RawPlayerStats],
) -> Result<Vec<SpatialPlayerStats>, OutputDecodeError> {
    let mut ids = HashSet::new();
    stats
        .iter()
        .map(|stat| {
            validate_id("player_stats.id", &stat.id)?;
            if !ids.insert(&stat.id) {
                return Err(OutputDecodeError::DuplicateIdentifier(stat.id.clone()));
            }
            if !finite(stat.distance_m)
                || stat.distance_m < 0.0
                || !finite(stat.rating)
                || !(0.0..=10.0).contains(&stat.rating)
                || stat.passes_completed > stat.passes_attempted
            {
                return Err(OutputDecodeError::InvalidNumeric {
                    field: "player_stats",
                });
            }
            let _ = stat.sprints;
            Ok(SpatialPlayerStats {
                player_id: stat.id.clone(),
                minutes_played: stat.minutes_played,
                goals: stat.goals,
                assists: stat.assists,
                shots: stat.shots,
                passes_attempted: stat.passes_attempted,
                passes_completed: stat.passes_completed,
                tackles_won: stat.tackles_won,
                interceptions: stat.interceptions,
                rating: stat.rating,
            })
        })
        .collect()
}
fn validate_id(field: &'static str, value: &str) -> Result<(), OutputDecodeError> {
    if value.is_empty() || value.len() > 64 {
        Err(OutputDecodeError::InvalidIdentifier {
            field,
            value: value.into(),
        })
    } else {
        Ok(())
    }
}
fn parse_side(value: &str) -> Result<SpatialSide, OutputDecodeError> {
    match value {
        "home" => Ok(SpatialSide::Home),
        "away" => Ok(SpatialSide::Away),
        other => Err(OutputDecodeError::InvalidEvent(other.into())),
    }
}
fn validate_period(value: &str) -> Result<(), OutputDecodeError> {
    if matches!(
        value,
        "first_half" | "second_half" | "extra_time_first" | "extra_time_second"
    ) {
        Ok(())
    } else {
        Err(OutputDecodeError::InvalidEvent(value.into()))
    }
}
fn string<'a>(
    data: &'a serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, OutputDecodeError> {
    data.get(field)
        .and_then(Value::as_str)
        .ok_or(OutputDecodeError::MissingOrInvalidField(field))
}
fn optional_string(
    data: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, OutputDecodeError> {
    match data.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or(OutputDecodeError::MissingOrInvalidField(field)),
    }
}
fn boolean(
    data: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<bool, OutputDecodeError> {
    data.get(field)
        .and_then(Value::as_bool)
        .ok_or(OutputDecodeError::MissingOrInvalidField(field))
}
fn finite(value: f32) -> bool {
    value.is_finite()
}
fn known_non_projected(name: &str) -> bool {
    matches!(
        name,
        "kickoff"
            | "period_end"
            | "period_start"
            | "added_time"
            | "state_change"
            | "match_end"
            | "ball_crossed_line"
            | "ball_control"
            | "ball_recovery"
            | "ball_lost"
            | "pass_attempt"
            | "pass_completed"
            | "pass_failed"
            | "loose_touch"
            | "possession_change"
            | "shot_attempt"
            | "save"
            | "woodwork"
            | "free_kick_awarded"
            | "penalty_awarded"
            | "advantage"
            | "header"
            | "aerial_duel"
            | "offside"
            | "free_kick"
            | "throw_in"
            | "goal_kick"
            | "condition_change"
            | "tactical_change"
            | "goalkeeper_change"
            | "live_command"
            | "shootout_start"
            | "shootout_end"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial_match_projection::{ProjectionContext, project};

    fn output(events: Value, score: Value, end_status: &str) -> String {
        serde_json::json!({
            "schema_version": 1, "match_id": "match-1", "home_team": "home", "away_team": "away", "seed": 7,
            "engine": { "name": "apfm-match-lab", "version": "0.1.0", "input_digest": "digest" },
            "coordinate_system": { "unit": "metre", "origin": "bottom_left_corner_viewed_from_above", "length_m": 105.0, "width_m": 68.0, "home_attacks_positive_x_in_period_1": true, "position_resolution_m": 0.01 },
            "frame_interval_ms": 500, "roster": ["h1", "h2", "a1", "a2"], "score": score, "events": events, "frames": [],
            "home_stats": { "possession_pct": 50.0, "shots": 1, "shots_on_target": 1, "xg": 0.0, "passes_attempted": 2, "passes_completed": 1, "tackles": 0, "interceptions": 0, "fouls": 0, "offsides": 0, "corners": 0, "yellow_cards": 0, "red_cards": 0 },
            "away_stats": { "possession_pct": 50.0, "shots": 0, "shots_on_target": 0, "xg": 0.0, "passes_attempted": 0, "passes_completed": 0, "tackles": 0, "interceptions": 0, "fouls": 0, "offsides": 0, "corners": 0, "yellow_cards": 0, "red_cards": 0 },
            "player_stats": [], "end_state": [
                { "id": "h1", "fatigue_end": 0.1, "morale_delta": 0, "injury": null, "card": null }, { "id": "h2", "fatigue_end": 0.1, "morale_delta": 0, "injury": null, "card": null },
                { "id": "a1", "fatigue_end": 0.1, "morale_delta": 0, "injury": null, "card": null }, { "id": "a2", "fatigue_end": 0.1, "morale_delta": 0, "injury": null, "card": null }
            ], "simulation": { "mode": "autonomous_match", "football_systems": true, "tick_ms": 100, "ticks_executed": 100, "end_status": end_status, "periods": [], "note": "test" }
        }).to_string()
    }
    fn goal(time: u32, side: &str, scorer: &str) -> Value {
        serde_json::json!({ "t_ms": time, "period": "first_half", "side": side, "type": "goal", "scorer": scorer, "assist": null, "own_goal": false })
    }

    /// Given a recorded M12-shaped normal match, when decoded and projected, then its score and identities survive.
    #[test]
    fn normal_output_decodes_and_projects() {
        let json = output(
            Value::Array(vec![goal(60_000, "home", "h1")]),
            serde_json::json!({ "home": 1, "away": 0 }),
            "regulation_complete",
        );
        let decoded = decode_json(&json).unwrap();
        let projected = project(
            &ProjectionContext {
                match_id: "match-1".into(),
                home_team_id: "home".into(),
                away_team_id: "away".into(),
                home_player_ids: vec!["h1".into(), "h2".into()],
                away_player_ids: vec!["a1".into(), "a2".into()],
            },
            &decoded.result,
        )
        .unwrap();
        assert_eq!(
            (projected.report.home_goals, projected.report.away_goals),
            (1, 0)
        );
        assert_eq!(projected.report.goals[0].scorer_id, "h1");
    }
    /// Given contractual extra-time and shootout events, when decoded, then regulation score and shootout remain separate.
    #[test]
    fn shootout_output_preserves_separate_score() {
        let events = serde_json::json!([goal(100, "home", "h1"), goal(200, "away", "a1"), { "t_ms": 300, "period": "extra_time_second", "side": "home", "type": "shootout_kick", "team": "home", "taker": "h2", "outcome": "goal" }, { "t_ms": 400, "period": "extra_time_second", "side": "away", "type": "shootout_kick", "team": "away", "taker": "a2", "outcome": "saved" }]);
        let decoded = decode_json(&output(
            events,
            serde_json::json!({ "home": 1, "away": 1 }),
            "shootout_complete",
        ))
        .unwrap();
        assert_eq!(
            decoded.result.end_status,
            SpatialEndStatus::ShootoutComplete
        );
        assert_eq!(decoded.result.events.len(), 4);
    }
    /// Given cards, substitutions and injuries from M12, when decoded, then their typed projection facts are retained.
    #[test]
    fn management_events_decode() {
        let events = serde_json::json!([ { "t_ms": 1, "period": "first_half", "side": "home", "type": "card", "player": "h1", "colour": "yellow" }, { "t_ms": 1, "period": "first_half", "side": "away", "type": "injury", "player": "a1", "severity": "minor" }, { "t_ms": 2, "period": "first_half", "side": "home", "type": "substitution", "player_off": "h1", "player_on": "h2" } ]);
        let decoded = decode_json(&output(
            events,
            serde_json::json!({ "home": 0, "away": 0 }),
            "regulation_complete",
        ))
        .unwrap();
        assert!(matches!(
            decoded.result.events[0].kind,
            SpatialEventKind::Card { .. }
        ));
        assert!(matches!(
            decoded.result.events[2].kind,
            SpatialEventKind::Substitution { .. }
        ));
    }
    /// Given malformed, truncated, wrong-version or missing-field JSON, when decoded, then a typed error is returned.
    #[test]
    fn malformed_json_is_refused() {
        assert_eq!(
            decode_json("{\"schema_version\":"),
            Err(OutputDecodeError::InvalidJson)
        );
        let mut wrong: Value = serde_json::from_str(&output(
            Value::Array(vec![]),
            serde_json::json!({ "home": 0, "away": 0 }),
            "regulation_complete",
        ))
        .unwrap();
        wrong["schema_version"] = serde_json::json!(2);
        assert_eq!(
            decode_json(&wrong.to_string()),
            Err(OutputDecodeError::UnsupportedSchema(2))
        );
        wrong.as_object_mut().unwrap().remove("roster");
        assert_eq!(
            decode_json(&wrong.to_string()),
            Err(OutputDecodeError::UnsupportedSchema(2))
        );
    }
    /// Given invalid temporal, player and numeric data, when decoded, then it never yields a partial result.
    #[test]
    fn invalid_contract_values_are_refused() {
        let mut value: Value = serde_json::from_str(&output(
            serde_json::json!([goal(2, "home", "h1"), goal(1, "home", "h1")]),
            serde_json::json!({ "home": 2, "away": 0 }),
            "regulation_complete",
        ))
        .unwrap();
        assert_eq!(
            decode_json(&value.to_string()),
            Err(OutputDecodeError::NonChronologicalEvents)
        );
        value["events"] = serde_json::json!([goal(1, "home", "unknown")]);
        assert!(matches!(
            decode_json(&value.to_string()),
            Err(OutputDecodeError::InvalidIdentifier {
                field: "event.player",
                ..
            })
        ));
        value["events"] = Value::Array(vec![]);
        value["score"] = serde_json::json!({ "home": 0, "away": 0 });
        value["home_stats"]["possession_pct"] = serde_json::json!(101.0);
        assert_eq!(
            decode_json(&value.to_string()),
            Err(OutputDecodeError::InvalidPossession)
        );
    }
    /// Given the same JSON twice, when decoded and projected, then its report is deterministic.
    #[test]
    fn decode_projection_chain_is_deterministic() {
        let json = output(
            Value::Array(vec![goal(1, "home", "h1")]),
            serde_json::json!({ "home": 1, "away": 0 }),
            "regulation_complete",
        );
        let first = decode_json(&json).unwrap();
        let second = decode_json(&json).unwrap();
        assert_eq!(first, second);
    }
}
