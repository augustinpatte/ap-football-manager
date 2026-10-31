//! Pure OFM → spatial-M12 input contract adapter.
//!
//! This module intentionally mirrors the published JSON v1 contract instead of
//! importing `apfm-match-lab`: that repository remains `UNLICENSED`.  It has no
//! state, persistence, Tauri command, or match-engine side effect.
//!
//! The mapping is the provisional I1 matrix.  Direct OFM attributes are copied;
//! documented proxies are calculated in memory only. Phase settings without a
//! one-to-one M12 v1 dial make bounded changes to the closest existing input.

use domain::{
    league::Fixture,
    player::{Footedness, Player, Position as OfmPosition},
    team::{
        BreakSpeed, BuildUpStyle, CounterPressDuration, DefensiveLine, DefensiveShape,
        MarkingStyle, PitchWidth, PlayStyle, PlayerRole, PressingIntensity, TacticsPhaseSettings,
        Tempo,
    },
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::{stable_hash::stable_hash, turn::squad::set_piece_takers_among};

pub const INPUT_SCHEMA_VERSION: u32 = 1;
pub const PITCH_LENGTH_M: f32 = 105.0;
pub const PITCH_WIDTH_M: f32 = 68.0;
pub const MAX_ID_LEN: usize = 64;
pub const MAX_SEED: u64 = (1 << 53) - 1;
const ADAPTATION_VERSION: &str = "ofm-m12-contract-v1";

/// The final OFM selection for one side.  Selection remains owned by OFM's
/// `turn::squad`; callers pass the selected XI and bench into this pure adapter.
pub struct TeamSelection<'a> {
    pub team: &'a domain::team::Team,
    pub starters: Vec<&'a Player>,
    pub bench: Vec<&'a Player>,
}

/// Everything needed to make one deterministic spatial input.
pub struct MatchSelection<'a> {
    pub fixture: &'a Fixture,
    pub game_seed: u64,
    pub home: TeamSelection<'a>,
    pub away: TeamSelection<'a>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdaptedMatch {
    pub input: MatchInput,
    pub metadata: AdaptationMetadata,
    pub input_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdaptationMetadata {
    pub version: String,
    /// These settings remain visible to callers but have no M12 v1 field.
    pub unrepresented_tactics: Vec<UnrepresentedTactic>,
    /// Values supplied because OFM has no equivalent persisted field.
    pub defaults: Vec<AppliedDefault>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnrepresentedTactic {
    pub setting: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppliedDefault {
    pub field: &'static str,
    /// The input entity receiving the imputed value; `None` is match-level.
    pub subject_id: Option<String>,
    /// Why OFM cannot provide a source value.  This is provenance, never a claim
    /// that the generated value is real player data.
    pub provenance: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchInput {
    pub schema_version: u32,
    pub match_id: String,
    pub config: MatchConfig,
    pub home: SpatialTeam,
    pub away: SpatialTeam,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchConfig {
    pub seed: u64,
    pub pitch_length_m: f32,
    pub pitch_width_m: f32,
    pub half_length_minutes: u8,
    pub tick_ms: u16,
    pub frame_interval_ms: u16,
    pub max_substitutions: u8,
    pub allow_extra_time: bool,
    pub home_advantage: f32,
    pub record_frames: bool,
    pub record_events: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpatialTeam {
    pub id: String,
    pub name: String,
    pub formation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_possession_formation: Option<String>,
    pub tactics: Tactics,
    pub lineup: Vec<LineupSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_possession_lineup: Option<Vec<LineupSlot>>,
    pub players: Vec<SpatialPlayer>,
    pub bench: Vec<String>,
    pub captain: String,
    pub penalty_taker: String,
    pub free_kick_taker: String,
    pub substitution_plan: Vec<()>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineupSlot {
    pub player: String,
    pub role: SpatialPosition,
    pub base: Point,
    pub instructions: PlayerInstructions,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpatialPosition {
    Gk,
    Rb,
    Cb,
    Lb,
    Rwb,
    Lwb,
    Dm,
    Cm,
    Am,
    Rm,
    Lm,
    Rw,
    Lw,
    Ss,
    St,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Foot {
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpatialPlayer {
    pub id: String,
    pub name: String,
    pub shirt_number: u8,
    pub natural_position: SpatialPosition,
    pub preferred_foot: Foot,
    pub height_cm: u16,
    pub attributes: Attributes,
    pub condition: Condition,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attributes {
    pub pace: u8,
    pub acceleration: u8,
    pub stamina: u8,
    pub strength: u8,
    pub agility: u8,
    pub jumping: u8,
    pub passing: u8,
    pub first_touch: u8,
    pub dribbling: u8,
    pub finishing: u8,
    pub long_shots: u8,
    pub crossing: u8,
    pub heading: u8,
    pub tackling: u8,
    pub marking: u8,
    pub vision: u8,
    pub positioning: u8,
    pub decisions: u8,
    pub composure: u8,
    pub aggression: u8,
    pub work_rate: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goalkeeping: Option<GoalkeeperAttributes>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalkeeperAttributes {
    pub reflexes: u8,
    pub handling: u8,
    pub aerial_reach: u8,
    pub one_on_one: u8,
    pub kicking: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Condition {
    pub fatigue: f32,
    pub morale: u8,
    pub sharpness: u8,
    pub niggle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlayerInstructions {
    pub positional_freedom: u8,
    pub forward_runs: u8,
    pub risk_taking: u8,
    pub pressing: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tactics {
    pub mentality: u8,
    pub pressing: u8,
    pub line_height: u8,
    pub width: u8,
    pub tempo: u8,
    pub directness: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdaptationError {
    EmptyId { kind: &'static str },
    IdTooLong { kind: &'static str, id: String },
    SameTeams,
    UnsupportedFormation(String),
    WrongStarterCount { team_id: String, found: usize },
    DuplicatePlayerId(String),
    PlayerOnBothTeams(String),
    TeamDoesNotMatchFixture { expected: String, found: String },
    PlayerMissingFromTeam { player_id: String, team_id: String },
    DuplicateShirtNumber { team_id: String, number: u8 },
    InvalidCaptainOrTaker { team_id: String, player_id: String },
    NoGoalkeeper { team_id: String },
    RatingOutOfBounds { player_id: String },
}

impl std::fmt::Display for AdaptationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AdaptationError {}

/// Converts one final OFM fixture selection to the unlicensed-engine-neutral
/// JSON contract.  It never mutates the game, a player, or a fixture.
pub fn adapt(selection: MatchSelection<'_>) -> Result<AdaptedMatch, AdaptationError> {
    validate_id("fixture", &selection.fixture.id)?;
    validate_id("team", &selection.fixture.home_team_id)?;
    validate_id("team", &selection.fixture.away_team_id)?;
    if selection.fixture.home_team_id == selection.fixture.away_team_id {
        return Err(AdaptationError::SameTeams);
    }
    if selection.home.team.id != selection.fixture.home_team_id {
        return Err(AdaptationError::TeamDoesNotMatchFixture {
            expected: selection.fixture.home_team_id.clone(),
            found: selection.home.team.id.clone(),
        });
    }
    if selection.away.team.id != selection.fixture.away_team_id {
        return Err(AdaptationError::TeamDoesNotMatchFixture {
            expected: selection.fixture.away_team_id.clone(),
            found: selection.away.team.id.clone(),
        });
    }
    let unrepresented_tactics = unrepresented_tactics(&selection);
    let mut defaults = Vec::new();
    let home = adapt_team(selection.home, &mut defaults)?;
    let away = adapt_team(selection.away, &mut defaults)?;
    let mut all_players = HashSet::new();
    for id in home.players.iter().chain(&away.players).map(|p| &p.id) {
        if !all_players.insert(id) {
            return Err(AdaptationError::PlayerOnBothTeams(id.clone()));
        }
    }
    let input = MatchInput {
        schema_version: INPUT_SCHEMA_VERSION,
        match_id: selection.fixture.id.clone(),
        config: MatchConfig {
            seed: spatial_seed(selection.game_seed, &selection.fixture.id),
            pitch_length_m: PITCH_LENGTH_M,
            pitch_width_m: PITCH_WIDTH_M,
            half_length_minutes: 45,
            tick_ms: 100,
            frame_interval_ms: 500,
            max_substitutions: 5,
            allow_extra_time: false,
            home_advantage: 0.5,
            record_frames: true,
            record_events: true,
        },
        home,
        away,
    };
    let metadata = AdaptationMetadata {
        version: ADAPTATION_VERSION.to_string(),
        unrepresented_tactics,
        defaults,
    };
    let bytes = serde_json::to_vec(&input).map_err(|_| AdaptationError::EmptyId {
        kind: "serialization",
    })?;
    let input_digest = hex::encode(Sha256::digest(bytes));
    Ok(AdaptedMatch {
        input,
        metadata,
        input_digest,
    })
}

fn adapt_team(
    selection: TeamSelection<'_>,
    defaults: &mut Vec<AppliedDefault>,
) -> Result<SpatialTeam, AdaptationError> {
    let team = selection.team;
    validate_id("team", &team.id)?;
    let layout = formation_geometry(&team.formation)?;
    let defensive_formation = (!team.out_of_possession_formation.trim().is_empty())
        .then(|| team.out_of_possession_formation.trim().to_string());
    let defensive_layout = defensive_formation
        .as_deref()
        .and_then(|formation| formation_geometry(formation).ok());
    if selection.starters.len() != 11 {
        return Err(AdaptationError::WrongStarterCount {
            team_id: team.id.clone(),
            found: selection.starters.len(),
        });
    }
    let mut ids = HashSet::new();
    for player in selection.starters.iter().chain(&selection.bench) {
        validate_id("player", &player.id)?;
        if player.team_id.as_deref() != Some(team.id.as_str()) {
            return Err(AdaptationError::PlayerMissingFromTeam {
                player_id: player.id.clone(),
                team_id: team.id.clone(),
            });
        }
        if !ids.insert(&player.id) {
            return Err(AdaptationError::DuplicatePlayerId(player.id.clone()));
        }
    }
    let shirts = shirt_numbers(&selection.starters, &selection.bench, &team.id, defaults)?;
    let mut lineup = Vec::with_capacity(11);
    for ((player, slot), shirt) in selection.starters.iter().zip(layout).zip(shirts.iter()) {
        let role = map_position(&slot.position)?;
        lineup.push(LineupSlot {
            player: player.id.clone(),
            role,
            base: slot.base,
            instructions: role_instructions(
                team.player_roles
                    .get(&player.id)
                    .unwrap_or(&PlayerRole::Standard),
            ),
        });
        let _ = shirt;
    }
    let out_of_possession_lineup = match defensive_layout {
        Some(defensive_layout) => {
            let mut defensive_lineup = Vec::with_capacity(defensive_layout.len());
            for (player, slot) in selection.starters.iter().zip(defensive_layout) {
                defensive_lineup.push(LineupSlot {
                    player: player.id.clone(),
                    role: map_position(&slot.position)?,
                    base: slot.base,
                    instructions: role_instructions(
                        team.player_roles
                            .get(&player.id)
                            .unwrap_or(&PlayerRole::Standard),
                    ),
                });
            }
            Some(defensive_lineup)
        }
        None => None,
    };
    if lineup
        .iter()
        .filter(|slot| slot.role == SpatialPosition::Gk)
        .count()
        != 1
    {
        return Err(AdaptationError::NoGoalkeeper {
            team_id: team.id.clone(),
        });
    }
    let mut players = Vec::with_capacity(ids.len());
    for (index, player) in selection
        .starters
        .iter()
        .chain(&selection.bench)
        .enumerate()
    {
        players.push(adapt_player(
            player,
            shirts[index],
            lineup
                .iter()
                .find(|slot| slot.player == player.id)
                .is_some_and(|slot| slot.role == SpatialPosition::Gk),
            defaults,
        )?);
    }
    let starter_ids: Vec<String> = selection.starters.iter().map(|p| p.id.clone()).collect();
    let (auto_captain, auto_penalty, auto_free_kick, _) =
        set_piece_takers_among(&selection.starters);
    let choose =
        |saved: &Option<String>, fallback: Option<String>| -> Result<String, AdaptationError> {
            let id = saved
                .as_ref()
                .filter(|id| starter_ids.contains(id))
                .cloned()
                .or(fallback)
                .ok_or_else(|| AdaptationError::InvalidCaptainOrTaker {
                    team_id: team.id.clone(),
                    player_id: String::new(),
                })?;
            Ok(id)
        };
    Ok(SpatialTeam {
        id: team.id.clone(),
        name: team.name.clone(),
        formation: team.formation.clone(),
        out_of_possession_formation: defensive_formation,
        tactics: tactics(&team.play_style, &team.tactics_phase),
        lineup,
        out_of_possession_lineup,
        players,
        bench: selection.bench.iter().map(|p| p.id.clone()).collect(),
        captain: choose(&team.match_roles.captain, auto_captain)?,
        penalty_taker: choose(&team.match_roles.penalty_taker, auto_penalty)?,
        free_kick_taker: choose(&team.match_roles.free_kick_taker, auto_free_kick)?,
        substitution_plan: Vec::new(),
    })
}

fn adapt_player(
    player: &Player,
    shirt_number: u8,
    is_goalkeeper: bool,
    defaults: &mut Vec<AppliedDefault>,
) -> Result<SpatialPlayer, AdaptationError> {
    if player.condition > 100
        || player.morale > 100
        || player.fitness > 100
        || attribute_values(&player.attributes)
            .iter()
            .any(|value| *value > 100)
    {
        return Err(AdaptationError::RatingOutOfBounds {
            player_id: player.id.clone(),
        });
    }
    let natural_position = map_position(&player.natural_position)?;
    let a = &player.attributes;
    let goalkeeping = if is_goalkeeper {
        defaults.push(AppliedDefault {
            field: "goalkeeping.one_on_one",
            subject_id: Some(player.id.clone()),
            provenance: "OFM has no source",
            value: "50 (OFM has no source)".into(),
        });
        defaults.push(AppliedDefault {
            field: "goalkeeping.kicking",
            subject_id: Some(player.id.clone()),
            provenance: "OFM has no source",
            value: "50 (OFM has no source)".into(),
        });
        Some(GoalkeeperAttributes {
            reflexes: a.reflexes,
            handling: a.handling,
            aerial_reach: a.aerial,
            one_on_one: 50,
            kicking: 50,
        })
    } else {
        None
    };
    defaults.push(AppliedDefault {
        field: "height_cm",
        subject_id: Some(player.id.clone()),
        provenance: "OFM has no source",
        value: "180".into(),
    });
    defaults.push(AppliedDefault {
        field: "attributes.balance",
        subject_id: Some(player.id.clone()),
        provenance: "M12 applies its neutral default because the field is omitted",
        value: "absent (M12 neutral default 50)".into(),
    });
    Ok(SpatialPlayer {
        id: player.id.clone(),
        name: player.match_name.clone(),
        shirt_number,
        natural_position,
        preferred_foot: match player.footedness {
            Footedness::Left => Foot::Left,
            Footedness::Right => Foot::Right,
            Footedness::Both => Foot::Both,
        },
        height_cm: 180,
        attributes: Attributes {
            pace: a.pace,
            acceleration: a.pace,
            stamina: a.stamina,
            strength: a.strength,
            agility: a.agility,
            jumping: a.aerial,
            passing: a.passing,
            first_touch: weighted(a.dribbling, 65, a.composure, 35),
            dribbling: a.dribbling,
            finishing: a.shooting,
            long_shots: a.shooting,
            crossing: a.passing,
            heading: a.aerial,
            tackling: a.tackling,
            marking: weighted(a.defending, 65, a.positioning, 35),
            vision: a.vision,
            positioning: a.positioning,
            decisions: a.decisions,
            composure: a.composure,
            aggression: a.aggression,
            work_rate: weighted(a.stamina, 70, a.teamwork, 30),
            balance: None,
            goalkeeping,
        },
        condition: Condition {
            fatigue: f32::from(100_u8.saturating_sub(player.condition)) / 100.0,
            morale: player.morale,
            sharpness: player.fitness,
            niggle: 0.0,
        },
    })
}

fn weighted(left: u8, left_weight: u16, right: u8, right_weight: u16) -> u8 {
    ((u16::from(left) * left_weight + u16::from(right) * right_weight + 50) / 100) as u8
}

fn attribute_values(attributes: &domain::player::PlayerAttributes) -> [u8; 19] {
    [
        attributes.pace,
        attributes.stamina,
        attributes.strength,
        attributes.agility,
        attributes.passing,
        attributes.shooting,
        attributes.tackling,
        attributes.dribbling,
        attributes.defending,
        attributes.positioning,
        attributes.vision,
        attributes.decisions,
        attributes.composure,
        attributes.aggression,
        attributes.teamwork,
        attributes.leadership,
        attributes.handling,
        attributes.reflexes,
        attributes.aerial,
    ]
}

fn validate_id(kind: &'static str, id: &str) -> Result<(), AdaptationError> {
    if id.is_empty() {
        Err(AdaptationError::EmptyId { kind })
    } else if id.len() > MAX_ID_LEN {
        Err(AdaptationError::IdTooLong {
            kind,
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}
fn spatial_seed(game_seed: u64, fixture_id: &str) -> u64 {
    stable_hash(fixture_id.as_bytes(), game_seed) % (MAX_SEED + 1)
}

fn shirt_numbers(
    starters: &[&Player],
    bench: &[&Player],
    team_id: &str,
    defaults: &mut Vec<AppliedDefault>,
) -> Result<Vec<u8>, AdaptationError> {
    let mut used = HashSet::new();
    let mut next = 1_u8;
    let mut result = Vec::new();
    for player in starters.iter().chain(bench) {
        let number = if let Some(number) = player.jersey_number {
            number
        } else {
            while used.contains(&next) {
                next = next.saturating_add(1);
            }
            defaults.push(AppliedDefault {
                field: "shirt_number",
                subject_id: Some(player.id.clone()),
                provenance: "OFM jersey_number missing",
                value: format!("{next} (deterministic generated number)"),
            });
            next
        };
        if !used.insert(number) {
            return Err(AdaptationError::DuplicateShirtNumber {
                team_id: team_id.to_string(),
                number,
            });
        }
        result.push(number);
    }
    Ok(result)
}

fn map_position(position: &OfmPosition) -> Result<SpatialPosition, AdaptationError> {
    Ok(match position {
        OfmPosition::Goalkeeper => SpatialPosition::Gk,
        OfmPosition::RightBack => SpatialPosition::Rb,
        OfmPosition::CenterBack | OfmPosition::Defender => SpatialPosition::Cb,
        OfmPosition::LeftBack => SpatialPosition::Lb,
        OfmPosition::RightWingBack => SpatialPosition::Rwb,
        OfmPosition::LeftWingBack => SpatialPosition::Lwb,
        OfmPosition::DefensiveMidfielder => SpatialPosition::Dm,
        OfmPosition::CentralMidfielder | OfmPosition::Midfielder => SpatialPosition::Cm,
        OfmPosition::AttackingMidfielder => SpatialPosition::Am,
        OfmPosition::RightMidfielder => SpatialPosition::Rm,
        OfmPosition::LeftMidfielder => SpatialPosition::Lm,
        OfmPosition::RightWinger => SpatialPosition::Rw,
        OfmPosition::LeftWinger => SpatialPosition::Lw,
        OfmPosition::Striker | OfmPosition::Forward => SpatialPosition::St,
    })
}

/// The I1 role-preset matrix.  M12 v1 does not natively model these roles;
/// the values are input presets, not claims of equivalent engine behaviours.
pub fn role_instructions(role: &PlayerRole) -> PlayerInstructions {
    let (positional_freedom, forward_runs, risk_taking, pressing) = match role {
        PlayerRole::Standard => (50, 50, 50, 50),
        PlayerRole::BallPlayingKeeper => (45, 0, 60, 30),
        PlayerRole::SweeperKeeper => (65, 0, 55, 35),
        PlayerRole::Stopper => (35, 10, 30, 70),
        PlayerRole::CoverCB => (35, 15, 35, 35),
        PlayerRole::BallPlayingCB => (50, 15, 65, 35),
        PlayerRole::AttackingFB => (65, 75, 55, 65),
        PlayerRole::DefensiveFB => (25, 20, 30, 55),
        PlayerRole::InvertedFB => (70, 55, 60, 55),
        PlayerRole::WingBack => (75, 85, 55, 65),
        PlayerRole::AnchorMan => (20, 10, 35, 55),
        PlayerRole::BallWinner => (55, 30, 40, 85),
        PlayerRole::DeepLyingPlaymaker => (60, 25, 75, 50),
        PlayerRole::BoxToBox => (70, 70, 60, 70),
        PlayerRole::Carrilero => (65, 55, 45, 60),
        PlayerRole::Mezzala => (75, 75, 70, 60),
        PlayerRole::AdvancedPlaymaker => (80, 50, 85, 45),
        PlayerRole::ShadowStriker => (80, 85, 75, 65),
        PlayerRole::WideForward => (70, 75, 70, 65),
        PlayerRole::InsideForward => (75, 85, 75, 65),
        PlayerRole::InvertedWinger => (80, 70, 85, 50),
        PlayerRole::Poacher => (55, 95, 60, 40),
        PlayerRole::TargetMan => (45, 60, 50, 55),
        PlayerRole::DeepLyingForward => (75, 60, 75, 55),
        PlayerRole::False9 => (90, 50, 85, 55),
        PlayerRole::PressingForward => (65, 85, 60, 90),
        PlayerRole::CompleteForward => (75, 80, 75, 75),
    };
    PlayerInstructions {
        positional_freedom,
        forward_runs,
        risk_taking,
        pressing,
    }
}

pub fn tactics(style: &PlayStyle, phase: &TacticsPhaseSettings) -> Tactics {
    let bounded = |value: i16| value.clamp(0, 100) as u8;
    let transition_pressing = match phase.counter_press_duration {
        CounterPressDuration::None => -8,
        CounterPressDuration::Short => 0,
        CounterPressDuration::Long => 8,
    };
    let marking_pressing = match phase.marking_style {
        MarkingStyle::Zonal => 0,
        MarkingStyle::Mixed => 3,
        MarkingStyle::ManToMan => 6,
    };
    let shape_width = match phase.defensive_shape {
        DefensiveShape::Stretched => 8,
        DefensiveShape::Normal => 0,
        DefensiveShape::Compact => -8,
    };
    let break_speed = match phase.break_speed {
        BreakSpeed::Slow => -10,
        BreakSpeed::Medium => 0,
        BreakSpeed::Fast => 10,
    };
    Tactics {
        mentality: match style {
            PlayStyle::Defensive => 30,
            PlayStyle::Counter => 45,
            PlayStyle::Balanced => 50,
            PlayStyle::Possession => 55,
            PlayStyle::HighPress => 60,
            PlayStyle::Attacking => 70,
        },
        pressing: bounded(
            match phase.pressing_intensity {
                PressingIntensity::Passive => 25,
                PressingIntensity::Medium => 55,
                PressingIntensity::Aggressive => 85,
            } + transition_pressing
                + marking_pressing,
        ),
        line_height: match phase.defensive_line {
            DefensiveLine::VeryLow => 10,
            DefensiveLine::Low => 35,
            DefensiveLine::Medium => 60,
            DefensiveLine::High => 85,
        },
        width: bounded(
            match phase.width {
                PitchWidth::Narrow => 25,
                PitchWidth::Normal => 50,
                PitchWidth::Wide => 75,
            } + shape_width,
        ),
        tempo: bounded(
            match phase.tempo {
                Tempo::Patient => 30,
                Tempo::Direct => 70,
            } + break_speed,
        ),
        directness: bounded(
            match phase.build_up_style {
                BuildUpStyle::Short => 25,
                BuildUpStyle::Mixed => 50,
                BuildUpStyle::Long => 75,
            } + break_speed,
        ),
    }
}

fn unrepresented_tactics(selection: &MatchSelection<'_>) -> Vec<UnrepresentedTactic> {
    let _ = selection;
    Vec::new()
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometrySlot {
    pub position: OfmPosition,
    pub base: Point,
}

/// Deterministic 105×68m team-frame layouts for every formation exposed by OFM.
pub fn formation_geometry(formation: &str) -> Result<Vec<GeometrySlot>, AdaptationError> {
    let (lines, positions): (&[usize], &[&[OfmPosition]]) = match formation {
        "4-4-2" => (
            &[4, 4, 2],
            &[
                &[
                    OfmPosition::RightBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftBack,
                ],
                &[
                    OfmPosition::RightMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::LeftMidfielder,
                ],
                &[OfmPosition::Striker, OfmPosition::Striker],
            ],
        ),
        "4-3-3" => (
            &[4, 3, 3],
            &[
                &[
                    OfmPosition::RightBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftBack,
                ],
                &[
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                ],
                &[
                    OfmPosition::RightWinger,
                    OfmPosition::Striker,
                    OfmPosition::LeftWinger,
                ],
            ],
        ),
        "3-5-2" => (
            &[3, 5, 2],
            &[
                &[
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                ],
                &[
                    OfmPosition::RightWingBack,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::LeftWingBack,
                ],
                &[OfmPosition::Striker, OfmPosition::Striker],
            ],
        ),
        "4-5-1" => (
            &[4, 5, 1],
            &[
                &[
                    OfmPosition::RightBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftBack,
                ],
                &[
                    OfmPosition::RightMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::LeftMidfielder,
                ],
                &[OfmPosition::Striker],
            ],
        ),
        "4-2-3-1" => (
            &[4, 2, 3, 1],
            &[
                &[
                    OfmPosition::RightBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftBack,
                ],
                &[
                    OfmPosition::DefensiveMidfielder,
                    OfmPosition::DefensiveMidfielder,
                ],
                &[
                    OfmPosition::RightWinger,
                    OfmPosition::AttackingMidfielder,
                    OfmPosition::LeftWinger,
                ],
                &[OfmPosition::Striker],
            ],
        ),
        "3-4-3" => (
            &[3, 4, 3],
            &[
                &[
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                ],
                &[
                    OfmPosition::RightMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::LeftMidfielder,
                ],
                &[
                    OfmPosition::RightWinger,
                    OfmPosition::Striker,
                    OfmPosition::LeftWinger,
                ],
            ],
        ),
        "5-3-2" => (
            &[5, 3, 2],
            &[
                &[
                    OfmPosition::RightWingBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftWingBack,
                ],
                &[
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                ],
                &[OfmPosition::Striker, OfmPosition::Striker],
            ],
        ),
        "4-1-4-1" => (
            &[4, 1, 4, 1],
            &[
                &[
                    OfmPosition::RightBack,
                    OfmPosition::CenterBack,
                    OfmPosition::CenterBack,
                    OfmPosition::LeftBack,
                ],
                &[OfmPosition::DefensiveMidfielder],
                &[
                    OfmPosition::RightMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::CentralMidfielder,
                    OfmPosition::LeftMidfielder,
                ],
                &[OfmPosition::Striker],
            ],
        ),
        _ => return Err(AdaptationError::UnsupportedFormation(formation.to_string())),
    };
    let mut slots = vec![GeometrySlot {
        position: OfmPosition::Goalkeeper,
        base: Point { x: 5.0, y: 34.0 },
    }];
    let x_rows: Vec<f32> = if lines.len() == 3 {
        vec![20.0, 40.0, 60.0]
    } else {
        vec![20.0, 34.0, 50.0, 64.0]
    };
    for ((count, row), x) in lines.iter().zip(positions).zip(x_rows) {
        for (index, position) in row.iter().enumerate().take(*count) {
            slots.push(GeometrySlot {
                position: position.clone(),
                base: Point {
                    x,
                    y: spread_y(*count, index),
                },
            });
        }
    }
    Ok(slots)
}
fn spread_y(count: usize, index: usize) -> f32 {
    if count == 1 {
        34.0
    } else {
        8.0 + 52.0 * index as f32 / (count - 1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;
    use domain::{
        league::{FixtureCompetition, FixtureStatus},
        player::Position,
        team::{BreakSpeed, CounterPressDuration, DefensiveShape, MarkingStyle, Team},
    };

    fn player(id: &str, position: Position, value: u8) -> Player {
        let mut player = Player::new(
            id.into(),
            id.into(),
            id.into(),
            "2000-01-01".into(),
            "GB".into(),
            position,
            uniform_attributes(value),
        );
        player.team_id = Some("home".into());
        player.jersey_number = Some(id.trim_start_matches('p').parse().unwrap_or(99));
        player
    }
    fn fixture() -> Fixture {
        Fixture {
            id: "fixture-1".into(),
            competition_id: "league".into(),
            matchday: 1,
            date: "2030-01-01".into(),
            home_team_id: "home".into(),
            away_team_id: "away".into(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            result: None,
            postponement: None,
        }
    }
    fn selection() -> (Fixture, Team, Team, Vec<Player>, Vec<Player>) {
        let fixture = fixture();
        let home = Team::new(
            "home".into(),
            "Home".into(),
            "H".into(),
            "GB".into(),
            "H".into(),
            "H".into(),
            1,
        );
        let away = Team::new(
            "away".into(),
            "Away".into(),
            "A".into(),
            "GB".into(),
            "A".into(),
            "A".into(),
            1,
        );
        let mut home_players: Vec<Player> = formation_geometry("4-4-2")
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, slot)| player(&format!("p{i}"), slot.position.clone(), 60))
            .collect();
        let mut away_players: Vec<Player> = formation_geometry("4-4-2")
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let mut p = player(&format!("a{i}"), slot.position.clone(), 60);
                p.team_id = Some("away".into());
                p.jersey_number = Some((i + 20) as u8);
                p
            })
            .collect();
        home_players.push(player("p11", Position::Forward, 61));
        away_players.push({
            let mut p = player("a11", Position::Forward, 61);
            p.team_id = Some("away".into());
            p.jersey_number = Some(31);
            p
        });
        (fixture, home, away, home_players, away_players)
    }
    fn adapted<'a>(
        fixture: &'a Fixture,
        home: &'a Team,
        away: &'a Team,
        home_players: &'a [Player],
        away_players: &'a [Player],
    ) -> Result<AdaptedMatch, AdaptationError> {
        adapt(MatchSelection {
            fixture,
            game_seed: 42,
            home: TeamSelection {
                team: home,
                starters: home_players[..11].iter().collect(),
                bench: home_players[11..].iter().collect(),
            },
            away: TeamSelection {
                team: away,
                starters: away_players[..11].iter().collect(),
                bench: away_players[11..].iter().collect(),
            },
        })
    }

    /// Given a final OFM lineup, when adapted, then every identity and composition is retained.
    #[test]
    fn the_adapter_keeps_fixture_team_player_xi_bench_and_set_piece_ids() {
        let (fixture, home, away, hp, ap) = selection();
        let result = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        assert_eq!(result.input.match_id, "fixture-1");
        assert_eq!(result.input.home.lineup.len(), 11);
        assert_eq!(result.input.home.bench, ["p11"]);
        assert_eq!(result.input.home.players[0].id, "p0");
        assert!(result.input.config.seed <= MAX_SEED);
    }

    /// Given a club with distinct possession and defensive formations
    /// When its final XI is adapted for M12
    /// Then both shapes retain the same player order and expose different anchors.
    #[test]
    fn the_adapter_sends_a_slot_aligned_defensive_shape_to_m12() {
        let (fixture, mut home, away, hp, ap) = selection();
        home.out_of_possession_formation = "5-3-2".into();

        let result = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        let defensive_lineup = result.input.home.out_of_possession_lineup.as_ref().unwrap();

        assert_eq!(
            result.input.home.out_of_possession_formation.as_deref(),
            Some("5-3-2")
        );
        assert_eq!(defensive_lineup.len(), 11);
        assert_eq!(
            defensive_lineup
                .iter()
                .map(|slot| &slot.player)
                .collect::<Vec<_>>(),
            result
                .input
                .home
                .lineup
                .iter()
                .map(|slot| &slot.player)
                .collect::<Vec<_>>()
        );
        assert_ne!(defensive_lineup[4].base, result.input.home.lineup[4].base);
    }
    /// Given I1 proxy inputs, when attributes are adapted, then the documented rounded formula is used.
    #[test]
    fn every_direct_and_proxy_attribute_is_bounded_and_uses_the_i1_formula() {
        let (fixture, home, away, mut hp, ap) = selection();
        hp[0].attributes.dribbling = 60;
        hp[0].attributes.composure = 80;
        hp[0].attributes.defending = 20;
        hp[0].attributes.positioning = 80;
        hp[0].attributes.stamina = 70;
        hp[0].attributes.teamwork = 40;
        let attributes = &adapted(&fixture, &home, &away, &hp, &ap)
            .unwrap()
            .input
            .home
            .players[0]
            .attributes;
        assert_eq!(attributes.first_touch, 67);
        assert_eq!(attributes.marking, 41);
        assert_eq!(attributes.work_rate, 61);
        assert_eq!(attributes.acceleration, hp[0].attributes.pace);
        assert!(
            attributes_values(attributes)
                .iter()
                .all(|value| *value <= 100)
        );
    }
    fn attributes_values(a: &Attributes) -> [u8; 21] {
        [
            a.pace,
            a.acceleration,
            a.stamina,
            a.strength,
            a.agility,
            a.jumping,
            a.passing,
            a.first_touch,
            a.dribbling,
            a.finishing,
            a.long_shots,
            a.crossing,
            a.heading,
            a.tackling,
            a.marking,
            a.vision,
            a.positioning,
            a.decisions,
            a.composure,
            a.aggression,
            a.work_rate,
        ]
    }
    /// Given every OFM role, when projected, then each has a bounded deterministic preset.
    #[test]
    fn all_twenty_seven_roles_have_bounded_presets() {
        let roles = [
            PlayerRole::Standard,
            PlayerRole::BallPlayingKeeper,
            PlayerRole::SweeperKeeper,
            PlayerRole::Stopper,
            PlayerRole::CoverCB,
            PlayerRole::BallPlayingCB,
            PlayerRole::AttackingFB,
            PlayerRole::DefensiveFB,
            PlayerRole::InvertedFB,
            PlayerRole::WingBack,
            PlayerRole::AnchorMan,
            PlayerRole::BallWinner,
            PlayerRole::DeepLyingPlaymaker,
            PlayerRole::BoxToBox,
            PlayerRole::Carrilero,
            PlayerRole::Mezzala,
            PlayerRole::AdvancedPlaymaker,
            PlayerRole::ShadowStriker,
            PlayerRole::WideForward,
            PlayerRole::InsideForward,
            PlayerRole::InvertedWinger,
            PlayerRole::Poacher,
            PlayerRole::TargetMan,
            PlayerRole::DeepLyingForward,
            PlayerRole::False9,
            PlayerRole::PressingForward,
            PlayerRole::CompleteForward,
        ];
        assert_eq!(roles.len(), 27);
        for role in roles {
            let i = role_instructions(&role);
            assert!(
                [
                    i.positional_freedom,
                    i.forward_runs,
                    i.risk_taking,
                    i.pressing
                ]
                .iter()
                .all(|value| *value <= 100)
            );
        }
    }
    /// Given every represented tactical option, when projected, then it is stable; absent concepts are metadata.
    #[test]
    fn all_nine_tactical_settings_are_projected_or_reported() {
        let (fixture, mut home, away, hp, ap) = selection();
        home.tactics_phase.build_up_style = BuildUpStyle::Long;
        home.tactics_phase.width = PitchWidth::Wide;
        home.tactics_phase.tempo = Tempo::Patient;
        home.tactics_phase.defensive_line = DefensiveLine::High;
        home.tactics_phase.pressing_intensity = PressingIntensity::Aggressive;
        home.tactics_phase.defensive_shape = DefensiveShape::Compact;
        home.tactics_phase.marking_style = MarkingStyle::ManToMan;
        home.tactics_phase.counter_press_duration = CounterPressDuration::Long;
        home.tactics_phase.break_speed = BreakSpeed::Fast;
        let result = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        assert_eq!(
            result.input.home.tactics,
            Tactics {
                mentality: 50,
                pressing: 99,
                line_height: 85,
                width: 67,
                tempo: 40,
                directness: 85
            }
        );
        assert!(result.metadata.unrepresented_tactics.is_empty());
    }
    /// Given every value of each tactical setting, when adapted, then every
    /// setting changes a bounded M12 team-tactics input.
    #[test]
    fn every_tactical_setting_variant_is_covered() {
        let phase = TacticsPhaseSettings::default();
        for (value, expected) in [
            (BuildUpStyle::Short, 25),
            (BuildUpStyle::Mixed, 50),
            (BuildUpStyle::Long, 75),
        ] {
            let mut phase = phase.clone();
            phase.build_up_style = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).directness, expected);
        }
        for (value, expected) in [
            (PitchWidth::Narrow, 25),
            (PitchWidth::Normal, 50),
            (PitchWidth::Wide, 75),
        ] {
            let mut phase = phase.clone();
            phase.width = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).width, expected);
        }
        for (value, expected) in [(Tempo::Patient, 30), (Tempo::Direct, 70)] {
            let mut phase = phase.clone();
            phase.tempo = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).tempo, expected);
        }
        for (value, expected) in [
            (DefensiveLine::VeryLow, 10),
            (DefensiveLine::Low, 35),
            (DefensiveLine::Medium, 60),
            (DefensiveLine::High, 85),
        ] {
            let mut phase = phase.clone();
            phase.defensive_line = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).line_height, expected);
        }
        for (value, expected) in [
            (PressingIntensity::Passive, 17),
            (PressingIntensity::Medium, 47),
            (PressingIntensity::Aggressive, 77),
        ] {
            let mut phase = phase.clone();
            phase.pressing_intensity = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).pressing, expected);
        }
        for (style, expected) in [
            (PlayStyle::Defensive, 30),
            (PlayStyle::Counter, 45),
            (PlayStyle::Balanced, 50),
            (PlayStyle::Possession, 55),
            (PlayStyle::HighPress, 60),
            (PlayStyle::Attacking, 70),
        ] {
            assert_eq!(tactics(&style, &phase).mentality, expected);
        }
        for (value, expected) in [
            (DefensiveShape::Stretched, 58),
            (DefensiveShape::Normal, 50),
            (DefensiveShape::Compact, 42),
        ] {
            let mut phase = phase.clone();
            phase.defensive_shape = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).width, expected);
        }
        for (value, expected) in [
            (MarkingStyle::Zonal, 47),
            (MarkingStyle::Mixed, 50),
            (MarkingStyle::ManToMan, 53),
        ] {
            let mut phase = phase.clone();
            phase.marking_style = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).pressing, expected);
        }
        for (value, expected) in [
            (CounterPressDuration::None, 47),
            (CounterPressDuration::Short, 55),
            (CounterPressDuration::Long, 63),
        ] {
            let mut phase = phase.clone();
            phase.counter_press_duration = value;
            assert_eq!(tactics(&PlayStyle::Balanced, &phase).pressing, expected);
        }
        for (value, expected) in [
            (BreakSpeed::Slow, (60, 40)),
            (BreakSpeed::Medium, (70, 50)),
            (BreakSpeed::Fast, (80, 60)),
        ] {
            let mut phase = phase.clone();
            phase.break_speed = value;
            let projected = tactics(&PlayStyle::Balanced, &phase);
            assert_eq!((projected.tempo, projected.directness), expected);
        }
    }

    /// Given two contrasting saved tactical blueprints, when adapted for M12,
    /// then their bounded team-tactics inputs are materially distinct.
    #[test]
    fn contrasting_preset_blueprints_produce_distinct_bounded_m12_inputs() {
        let park_the_bus = TacticsPhaseSettings {
            width: PitchWidth::Narrow,
            defensive_line: DefensiveLine::VeryLow,
            pressing_intensity: PressingIntensity::Passive,
            defensive_shape: DefensiveShape::Compact,
            ..TacticsPhaseSettings::default()
        };
        let fast_transition = TacticsPhaseSettings {
            pressing_intensity: PressingIntensity::Aggressive,
            counter_press_duration: CounterPressDuration::Short,
            break_speed: BreakSpeed::Fast,
            ..TacticsPhaseSettings::default()
        };

        let defensive = tactics(&PlayStyle::Defensive, &park_the_bus);
        let transition = tactics(&PlayStyle::Counter, &fast_transition);

        assert_ne!(defensive, transition);
        assert!(defensive.line_height < transition.line_height);
        assert!(defensive.width < transition.width);
        assert!(defensive.pressing < transition.pressing);
        assert!(defensive.tempo < transition.tempo);
        for value in [
            defensive.mentality,
            defensive.pressing,
            defensive.line_height,
            defensive.width,
            defensive.tempo,
            defensive.directness,
            transition.mentality,
            transition.pressing,
            transition.line_height,
            transition.width,
            transition.tempo,
            transition.directness,
        ] {
            assert!(value <= 100);
        }
    }
    /// Given generated adapter values, when defaults are reported, then every
    /// imputation names both its recipient and its source limitation.
    #[test]
    fn all_imputed_values_have_explicit_player_provenance() {
        let (fixture, home, away, hp, ap) = selection();
        let result = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        assert!(
            result
                .metadata
                .defaults
                .iter()
                .all(|default| { default.subject_id.is_some() && !default.provenance.is_empty() })
        );
        assert!(result.metadata.defaults.iter().any(|default| {
            default.field == "height_cm" && default.subject_id.as_deref() == Some("p0")
        }));
    }
    /// Given every formation exposed by OFM, when laid out, then it has eleven in-bounds positions and one goalkeeper.
    #[test]
    fn every_supported_formation_has_valid_deterministic_geometry() {
        for formation in [
            "4-4-2", "4-3-3", "3-5-2", "4-5-1", "4-2-3-1", "3-4-3", "5-3-2", "4-1-4-1",
        ] {
            let slots = formation_geometry(formation).unwrap();
            assert_eq!(slots.len(), 11);
            assert_eq!(
                slots
                    .iter()
                    .filter(|slot| slot.position == Position::Goalkeeper)
                    .count(),
                1
            );
            assert!(
                slots.iter().all(|slot| (0.0..=105.0).contains(&slot.base.x)
                    && (0.0..=68.0).contains(&slot.base.y))
            );
            assert_eq!(slots, formation_geometry(formation).unwrap());
        }
    }
    /// Given identical OFM inputs, when adapted twice, then JSON and digest are reproducible.
    #[test]
    fn adaptation_json_and_digest_are_reproducible() {
        let (fixture, home, away, hp, ap) = selection();
        let first = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        let second = adapted(&fixture, &home, &away, &hp, &ap).unwrap();
        assert_eq!(
            serde_json::to_string(&first.input).unwrap(),
            serde_json::to_string(&second.input).unwrap()
        );
        assert_eq!(first.input_digest, second.input_digest);
    }
    /// Given invalid final compositions, when adapted, then no partial input is produced.
    #[test]
    fn invalid_or_missing_ofm_data_is_rejected() {
        let (fixture, home, away, hp, ap) = selection();
        assert!(matches!(
            adapt(MatchSelection {
                fixture: &fixture,
                game_seed: 1,
                home: TeamSelection {
                    team: &home,
                    starters: hp[..10].iter().collect(),
                    bench: vec![]
                },
                away: TeamSelection {
                    team: &away,
                    starters: ap[..11].iter().collect(),
                    bench: vec![]
                }
            }),
            Err(AdaptationError::WrongStarterCount { .. })
        ));
        let mut bad = fixture.clone();
        bad.id.clear();
        assert!(matches!(
            adapted(&bad, &home, &away, &hp, &ap),
            Err(AdaptationError::EmptyId { .. })
        ));
        let mut invalid_players = hp.clone();
        invalid_players[0].condition = 101;
        assert!(matches!(
            adapted(&fixture, &home, &away, &invalid_players, &ap),
            Err(AdaptationError::RatingOutOfBounds { .. })
        ));
    }
}
