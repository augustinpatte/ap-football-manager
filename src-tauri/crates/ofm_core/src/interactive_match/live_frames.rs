//! What a screen draws of a match that is still being played.
//!
//! The M12 session records frames only for a whole-match run, and a managed match has to stay
//! open to orders, so the controller reads the running simulation instead: at the engine's own
//! frame interval it copies the positions, the ball, the possession and the score out of the
//! engine's world. Nothing is predicted, interpolated or invented on this side; a frame is the
//! world as it was at `t_ms`, and the sampling is read-only (the world digest and the result of
//! the match are the same with or without it, which `tests::frames` pins).
//!
//! Frames carry players by *roster index* (the position in [`LiveRoster`]) so the roster, which
//! never changes during a match, is sent once and a frame stays small.

use std::collections::HashMap;

use apfm_match_lab::MatchInput;
use apfm_match_lab::ids::PlayerId;
use apfm_match_lab::output::{EventKind, MatchEvent, Period};
use apfm_match_lab::pitch::Side;
use apfm_match_lab::sim::runner::Simulation;
use apfm_match_lab::sim::state::{GameState, StoppageReason};
use serde::Serialize;

/// `[roster_index, x, y, heading]`: one player on the pitch. Metres, origin bottom-left of the
/// 105 x 68 pitch, heading in radians with 0 along +x.
pub type PlayerSample = (u16, f32, f32, f32);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveFrame {
    pub t_ms: u32,
    pub period: Period,
    pub state: GameState,
    /// Only while `state` is a stoppage.
    pub stoppage: Option<StoppageReason>,
    /// `[x, y, height]` in metres.
    pub ball: [f32; 3],
    /// Roster index of the player on the ball, when somebody has it.
    pub holder: Option<u16>,
    /// The team of the last player who controlled the ball.
    pub possession_side: Option<Side>,
    pub score: [u8; 2],
    /// Players on the pitch only: substitutes and dismissed players are absent.
    pub players: Vec<PlayerSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RosterEntry {
    pub id: String,
    pub side: Side,
}

/// Every player of both squads, in the order frames index them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveRoster(pub Vec<RosterEntry>);

impl LiveRoster {
    pub fn of(input: &MatchInput, engine_roster: &[PlayerId]) -> Self {
        let side_of = |id: &PlayerId| {
            if input.home.players.iter().any(|p| &p.id == id) {
                Side::Home
            } else {
                Side::Away
            }
        };
        Self(
            engine_roster
                .iter()
                .map(|id| RosterEntry {
                    id: id.0.clone(),
                    side: side_of(id),
                })
                .collect(),
        )
    }
}

/// The match clock as a person reads it: where the period started, where its nominal time ends,
/// and the added time once the referee has announced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockView {
    pub period_start_ms: u32,
    /// Nominal minutes played before this period began (0, 45, 90, 105 for a 45-minute half),
    /// which is what a stopwatch shows at the period's first instant.
    pub period_base_ms: u32,
    pub period_nominal_ms: u32,
    pub nominal_end_ms: u32,
    /// `None` until nominal time is reached; `Some(0)` is "no added time".
    pub added_time_ms: Option<u32>,
}

/// Reads the clock from the canonical event stream (`period_start`, `added_time`), which is
/// the engine's own record of both.
pub fn clock_view(events: &[MatchEvent], period: Period, half_ms: u32) -> ClockView {
    let period_start_ms = events
        .iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::PeriodStart) && e.period == period)
        .map_or(0, |e| e.t_ms);
    let announced = events
        .iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::AddedTime { .. }) && e.period == period);
    let extra_ms = half_ms / 3;
    let (period_base_ms, nominal_ms) = match period {
        Period::FirstHalf => (0, half_ms),
        Period::SecondHalf => (half_ms, half_ms),
        Period::ExtraTimeFirst => (2 * half_ms, extra_ms),
        Period::ExtraTimeSecond => (2 * half_ms + extra_ms, extra_ms),
    };
    let nominal_end_ms = match announced {
        Some(e) => e.t_ms,
        None => period_start_ms + nominal_ms,
    };
    ClockView {
        period_start_ms,
        period_base_ms,
        period_nominal_ms: nominal_ms,
        nominal_end_ms,
        added_time_ms: match announced.map(|e| &e.kind) {
            Some(EventKind::AddedTime { added_ms }) => Some(*added_ms),
            _ => None,
        },
    }
}

/// Copies frames out of a running simulation on the engine's frame grid.
#[derive(Debug, Clone)]
pub struct FrameSampler {
    index_of: HashMap<PlayerId, u16>,
    interval_ms: u32,
    last_t_ms: Option<u32>,
}

impl FrameSampler {
    pub fn new(roster: &[PlayerId], interval_ms: u32) -> Self {
        Self {
            index_of: roster
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i as u16))
                .collect(),
            interval_ms: interval_ms.max(1),
            last_t_ms: None,
        }
    }

    /// Forgets which instant was last handed out, so the next sample is the current one again.
    pub fn forget(&mut self) {
        self.last_t_ms = None;
    }

    /// The first grid time strictly after the last frame taken.
    pub fn next_grid_ms(&self, now_ms: u32) -> u32 {
        let from = self.last_t_ms.map_or(now_ms, |t| t.max(now_ms));
        (from / self.interval_ms + 1) * self.interval_ms
    }

    /// The frame at the simulation's current time, or `None` if one was already taken for it.
    pub fn sample(&mut self, sim: &Simulation) -> Option<LiveFrame> {
        let world = sim.world();
        let t_ms = sim.clock().now_ms();
        if self.last_t_ms == Some(t_ms) {
            return None;
        }
        self.last_t_ms = Some(t_ms);
        let players = world
            .agents
            .iter()
            .enumerate()
            .filter(|(i, _)| !world.is_dismissed(*i))
            .filter_map(|(_, agent)| {
                let index = *self.index_of.get(&agent.id)?;
                Some((
                    index,
                    round2(agent.pos.x),
                    round2(agent.pos.y),
                    round2(agent.heading),
                ))
            })
            .collect();
        let holder = world
            .football
            .and_then(|f| f.owner)
            .and_then(|o| world.agents.get(usize::from(o)))
            .and_then(|agent| self.index_of.get(&agent.id).copied());
        Some(LiveFrame {
            t_ms,
            period: world.period,
            state: world.state,
            stoppage: world.stoppage,
            ball: [
                round2(world.ball.x),
                round2(world.ball.y),
                round2(world.ball.z),
            ],
            holder,
            possession_side: world.football.and_then(|f| f.possession_side),
            score: [world.score.home, world.score.away],
            players,
        })
    }
}

/// Centimetres: the engine's own recording resolution.
fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}
