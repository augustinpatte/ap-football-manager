//! Layer B of the interactive match: the session controller.
//!
//! One [`InteractiveMatch`] drives one M12 [`MatchSession`] for one fixture. It owns what the
//! engine must not: who may command which side (the manager, the assistant, the opponent's
//! AI), the OFM tactical plans, delegation, the order ledger, and the hand-over to the
//! career's own finalisation. It decides nothing about football. A goal, a card or a result
//! only ever comes out of the engine; this module forwards orders to it as commands and reads
//! its state back.
//!
//! # Authority
//! Three actors are registered with the session, each for one side only:
//! * `manager`: the player, for the player's side, always;
//! * `assistant`: for the player's side, only while the match is handed over
//!   (`Duty::Tactics` delegated) and switched on. The assistant stands back from whatever the
//!   manager ordered in the last [`ASSISTANT_STAND_BACK_MS`], so a human decision is never
//!   overwritten;
//! * `opponent-ai`: for the other side (or both, when nobody manages).
//!
//! The engine refuses a command whose actor is not registered for the side, so these
//! boundaries are enforced where the football is, not only here.
//!
//! # Finalisation
//! [`InteractiveMatch::complete`] turns a session that reached full time into a `MatchReport`
//! through the same validation and projection as a whole-match run. Applying it to the career
//! is the application layer's job and goes through the existing finalisation; a match that has
//! not reached full time is never completed.

pub mod ai;
pub mod plan;
pub mod saved;

use std::collections::BTreeMap;

use apfm_match_lab::MatchInput;
use apfm_match_lab::ids::PlayerId;
use apfm_match_lab::output::{EventKind, MatchEvent};
use apfm_match_lab::pitch::Side;
use apfm_match_lab::sim::SimulationConfig;
use apfm_match_lab::sim::motion::MotionConfig;
use apfm_match_lab::sim::session::{
    Actor, Advance, AdvanceOptions, CommandEnvelope, MatchCommand, MatchSession, Outcome,
    SessionCheckpoint, SessionView,
};
use domain::delegation::Duty;
use domain::team::{PressingIntensity, Tempo};
use engine::MatchReport;
use serde::{Deserialize, Serialize};

use self::ai::{AiContext, AiOrder, Checkpoint, Group, REVIEW_EVERY_MS, Squad};
use self::plan::{PlanError, TacticalChange, TacticalPlan, commands_between, supported_formations};
use crate::game::Game;
use crate::spatial_runner::{self, SpatialRunError};

pub const MANAGER_ACTOR: &str = "manager";
pub const ASSISTANT_ACTOR: &str = "assistant";
pub const OPPONENT_ACTOR: &str = "opponent-ai";

/// How long the assistant leaves alone a kind of order the manager has just given
/// (simulated time). It is the live-match counterpart of the F3 rule that a delegated duty
/// "never overwrites a manager's own decision". Remove it only with that rule.
pub const ASSISTANT_STAND_BACK_MS: u32 = 15 * 60_000;

// ------------------------------------------------------------------------ errors

#[derive(Debug)]
pub enum InteractiveError {
    /// The fixture could not be adapted or the engine could not run.
    Run(SpatialRunError),
    /// The player's club is not in this fixture, so there is nobody to take orders from.
    NotYourMatch,
    AssistantNotDelegated,
    /// The match has not reached full time.
    NotFinished,
    AlreadyCompleted,
    /// The order contains a formation the engine cannot build.
    UnsupportedFormation,
    /// The order changes nothing.
    NothingToChange,
    Engine(String),
}

impl InteractiveError {
    /// The error as the player is told about it: always a translation key.
    pub fn translation_key(&self) -> String {
        match self {
            InteractiveError::Run(e) => e.translation_key(),
            InteractiveError::NotYourMatch => "be.error.interactive.notYourMatch".into(),
            InteractiveError::AssistantNotDelegated => {
                "be.error.interactive.assistantNotDelegated".into()
            }
            InteractiveError::NotFinished => "be.error.interactive.notFinished".into(),
            InteractiveError::AlreadyCompleted => "be.error.interactive.alreadyCompleted".into(),
            InteractiveError::UnsupportedFormation => {
                "be.error.interactive.unsupportedFormation".into()
            }
            InteractiveError::NothingToChange => "be.error.interactive.nothingToChange".into(),
            InteractiveError::Engine(_) => "be.error.interactive.engineFailed".into(),
        }
    }
}

impl From<PlanError> for InteractiveError {
    fn from(e: PlanError) -> Self {
        match e {
            PlanError::UnsupportedFormation(_) => InteractiveError::UnsupportedFormation,
            PlanError::NothingToChange => InteractiveError::NothingToChange,
        }
    }
}

impl std::fmt::Display for InteractiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.translation_key())
    }
}

// ------------------------------------------------------------------------ requests and views

/// Which fixture an interactive match plays: the identity the application's live session holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureIdentity {
    pub fixture_index: usize,
    pub competition_id: String,
    pub fixture_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserOrder {
    Tactics(TacticalChange),
    Substitute {
        player_off_id: String,
        player_on_id: String,
    },
    CancelSubstitution {
        player_off_id: String,
    },
}

/// An order from the manager. `command_id` makes a retry harmless; `expected_revision` is the
/// revision of the view the manager acted on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserCommand {
    pub command_id: String,
    pub expected_revision: u64,
    pub order: UserOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderReceipt {
    pub command_id: String,
    pub accepted: bool,
    /// Accepted but not yet in force: a substitution waits for the next stoppage.
    pub queued: bool,
    /// The engine's rejection code (`stale_revision`, `no_substitutions_left`, ...).
    pub rejection: Option<String>,
    pub revision: u64,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AdvanceTarget {
    /// This many more simulated minutes.
    ForMinutes(u32),
    ToMinute(u32),
    ToEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceRequest {
    pub target: AdvanceTarget,
    /// Stop after a goal, a dismissal or an injury (the view then shows it).
    pub stop_at_key_events: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Reached,
    PeriodBreak,
    KeyEvent,
    Paused,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanView {
    pub style: domain::team::PlayStyle,
    pub pressing: PressingIntensity,
    pub tempo: Tempo,
    pub formation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InteractiveView {
    pub session: SessionView,
    pub user_side: Option<Side>,
    pub assistant_available: bool,
    pub assistant_active: bool,
    pub supported_formations: Vec<String>,
    pub home_plan: PlanView,
    pub away_plan: PlanView,
    pub regulation_minutes: u32,
}

/// An event of the canonical stream, with its id (its index in the stream).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyEvent {
    pub id: usize,
    #[serde(flatten)]
    pub event: MatchEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdvanceOutcome {
    pub stop: StopReason,
    pub view: InteractiveView,
}

// ------------------------------------------------------------------------ the controller

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SideAi {
    last_tactical_change_ms: Option<u32>,
    /// Until when (simulated ms) the assistant leaves each group of orders alone.
    stand_back_until: [Option<u32>; Group::COUNT],
}

enum Stage {
    Playing(Box<MatchSession>),
    Completed(Box<MatchReport>),
}

pub struct InteractiveMatch {
    identity: FixtureIdentity,
    input: MatchInput,
    stage: Stage,
    user_side: Option<Side>,
    assistant_available: bool,
    assistant_active: bool,
    /// Current plans and the plans at kickoff, indexed by [`side_index`].
    plans: [TacticalPlan; 2],
    identities: [TacticalPlan; 2],
    squad: Squad,
    ai: [SideAi; 2],
    /// What the manager has been answered, by order id, so a retry returns the same answer.
    orders: BTreeMap<String, OrderReceipt>,
    next_command: u64,
    /// Events before this index have been looked at for checkpoints.
    ai_cursor: usize,
    /// Next simulated time at which a review is due.
    next_review_ms: u32,
    regulation_ms: u32,
}

/// A fresh identifier for a session: commands addressed to any other session are refused.
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn side_index(side: Side) -> usize {
    usize::from(side == Side::Away)
}

impl InteractiveMatch {
    /// Starts the match of the fixture a live session identifies, from the career as it is.
    pub fn start(
        game: &Game,
        identity: FixtureIdentity,
        session_id: &str,
    ) -> Result<Self, InteractiveError> {
        let adapted = spatial_runner::adapt_selected_fixture(
            game,
            identity.fixture_index,
            &identity.competition_id,
            &identity.fixture_id,
        )
        .map_err(InteractiveError::Run)?;
        let json = serde_json::to_string(&adapted.input)
            .map_err(|e| InteractiveError::Run(SpatialRunError::InputContract(e.to_string())))?;
        let input = MatchInput::from_json(&json)
            .map_err(|e| InteractiveError::Run(SpatialRunError::InputContract(e.to_string())))?;

        let plan_of = |team_id: &str| {
            game.teams
                .iter()
                .find(|t| t.id == team_id)
                .map(TacticalPlan::of_team)
                .ok_or_else(|| {
                    InteractiveError::Run(SpatialRunError::InputContract(
                        "SelectedTeamNotFound".into(),
                    ))
                })
        };
        let home_plan = plan_of(&input.home.id.0)?;
        let away_plan = plan_of(&input.away.id.0)?;
        let manager_team = game.manager.team_id.as_deref();
        let user_side = match manager_team {
            Some(id) if id == input.home.id.0 => Some(Side::Home),
            Some(id) if id == input.away.id.0 => Some(Side::Away),
            _ => None,
        };

        let mut cfg = SimulationConfig::from_input(&input);
        cfg.record_frames = false;
        cfg.record_events = true;
        let mut session = MatchSession::new(
            session_id,
            &input,
            cfg,
            MotionConfig::default(),
            spatial_runner::match_football_config(),
        )
        .map_err(|e| InteractiveError::Engine(e.to_string()))?;
        let regulation_ms = u32::from(input.config.half_length_minutes) * 2 * 60_000;
        grant_actors(&mut session, user_side, false);
        Ok(Self {
            identity,
            squad: Squad::of(&input),
            input,
            stage: Stage::Playing(Box::new(session)),
            user_side,
            assistant_available: user_side.is_some() && game.delegation.is_delegated(Duty::Tactics),
            assistant_active: false,
            identities: [home_plan.clone(), away_plan.clone()],
            plans: [home_plan, away_plan],
            ai: [SideAi::default(), SideAi::default()],
            orders: BTreeMap::new(),
            next_command: 0,
            ai_cursor: 0,
            next_review_ms: REVIEW_EVERY_MS,
            regulation_ms,
        })
    }

    pub fn identity(&self) -> &FixtureIdentity {
        &self.identity
    }

    pub fn user_side(&self) -> Option<Side> {
        self.user_side
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.stage, Stage::Completed(_))
    }

    fn session(&self) -> Option<&MatchSession> {
        match &self.stage {
            Stage::Playing(s) => Some(s),
            _ => None,
        }
    }

    fn playing(&mut self) -> Result<&mut MatchSession, InteractiveError> {
        match &mut self.stage {
            Stage::Playing(s) => Ok(s),
            _ => Err(InteractiveError::AlreadyCompleted),
        }
    }

    // ---- reading

    pub fn view(&self) -> Option<InteractiveView> {
        let session = self.session()?.view();
        let plan_view = |p: &TacticalPlan| PlanView {
            style: p.style.clone(),
            pressing: p.phase.pressing_intensity.clone(),
            tempo: p.phase.tempo.clone(),
            formation: p.formation.clone(),
        };
        Some(InteractiveView {
            session,
            user_side: self.user_side,
            assistant_available: self.assistant_available,
            assistant_active: self.assistant_active,
            supported_formations: supported_formations(),
            home_plan: plan_view(&self.plans[0]),
            away_plan: plan_view(&self.plans[1]),
            regulation_minutes: self.regulation_ms / 60_000,
        })
    }

    /// The key events from `cursor` on, and the cursor to ask from next time.
    pub fn events_since(&self, cursor: usize) -> (Vec<KeyEvent>, usize) {
        let Some(session) = self.session() else {
            return (Vec::new(), cursor);
        };
        let start = cursor.min(session.events().len());
        let events = session.events()[start..]
            .iter()
            .enumerate()
            .filter(|(_, e)| is_key_event(&e.kind))
            .map(|(i, e)| KeyEvent {
                id: start + i,
                event: e.clone(),
            })
            .collect();
        (events, session.events().len())
    }

    /// Every command the engine has answered, in order: the manager's, the assistant's and the
    /// opponent's, each under its own actor.
    pub fn command_log(&self) -> &[apfm_match_lab::sim::session::LedgerEntry] {
        self.session().map_or(&[], |s| s.ledger())
    }

    /// Until when the assistant leaves `group` alone on the player's side (simulated ms).
    pub fn assistant_stand_back_until(&self, group: Group) -> Option<u32> {
        let side = self.user_side?;
        self.ai[side_index(side)].stand_back_until[group.index()]
    }

    // ---- the manager's orders

    /// Hands the player's side to the assistant, or takes it back. The assistant needs the
    /// match-day tactics duty to be delegated; taking back always works.
    pub fn set_assistant(&mut self, active: bool) -> Result<(), InteractiveError> {
        let side = self.user_side.ok_or(InteractiveError::NotYourMatch)?;
        if active && !self.assistant_available {
            return Err(InteractiveError::AssistantNotDelegated);
        }
        self.assistant_active = active;
        let session = self.playing()?;
        if active {
            session.grant(ASSISTANT_ACTOR, side);
        } else {
            session.revoke(ASSISTANT_ACTOR);
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<bool, InteractiveError> {
        Ok(self.playing()?.pause())
    }

    pub fn resume(&mut self) -> Result<bool, InteractiveError> {
        Ok(self.playing()?.resume())
    }

    /// Carries out an order of the manager for the player's side.
    pub fn submit(&mut self, command: UserCommand) -> Result<OrderReceipt, InteractiveError> {
        let side = self.user_side.ok_or(InteractiveError::NotYourMatch)?;
        if let Some(previous) = self.orders.get(&command.command_id) {
            return Ok(OrderReceipt {
                duplicate: true,
                ..previous.clone()
            });
        }
        let revision = self.playing()?.revision();
        let receipt = if command.expected_revision != revision {
            OrderReceipt {
                command_id: command.command_id.clone(),
                accepted: false,
                queued: false,
                rejection: Some("stale_revision".into()),
                revision,
                duplicate: false,
            }
        } else {
            let order = match command.order {
                UserOrder::Tactics(change) => Order::Tactics(change),
                UserOrder::Substitute {
                    player_off_id,
                    player_on_id,
                } => Order::Substitute {
                    off: PlayerId::new(player_off_id),
                    on: PlayerId::new(player_on_id),
                },
                UserOrder::CancelSubstitution { player_off_id } => Order::Cancel {
                    off: PlayerId::new(player_off_id),
                },
            };
            let groups = order.groups();
            let receipt = self.dispatch(MANAGER_ACTOR, side, &command.command_id, &order)?;
            if receipt.accepted {
                let until = self.playing()?.now_ms() + ASSISTANT_STAND_BACK_MS;
                for group in groups {
                    self.ai[side_index(side)].stand_back_until[group.index()] = Some(until);
                }
            }
            receipt
        };
        self.orders.insert(command.command_id, receipt.clone());
        Ok(receipt)
    }

    /// Sends one order to the engine as its M12 commands, all or nothing.
    fn dispatch(
        &mut self,
        actor: &str,
        side: Side,
        base_id: &str,
        order: &Order,
    ) -> Result<OrderReceipt, InteractiveError> {
        let (commands, next_plan) = match order {
            Order::Tactics(change) => {
                let next = self.plans[side_index(side)].with(change);
                (
                    commands_between(&self.plans[side_index(side)], &next)?,
                    Some(next),
                )
            }
            Order::Substitute { off, on } => (
                vec![MatchCommand::RequestSubstitution {
                    off: off.clone(),
                    on: on.clone(),
                }],
                None,
            ),
            Order::Cancel { off } => (
                vec![MatchCommand::CancelPendingSubstitution { off: off.clone() }],
                None,
            ),
        };
        let session = self.playing()?;
        let mut receipt = OrderReceipt {
            command_id: base_id.to_string(),
            accepted: true,
            queued: false,
            rejection: None,
            revision: session.revision(),
            duplicate: false,
        };
        for (n, command) in commands.into_iter().enumerate() {
            let answer = session.submit(CommandEnvelope {
                session_id: session.id().to_string(),
                command_id: format!("{base_id}#{n}"),
                actor: Actor {
                    id: actor.to_string(),
                    side,
                },
                expected_revision: session.revision(),
                command,
            });
            receipt.revision = answer.revision;
            match answer.outcome {
                Outcome::Applied => {}
                Outcome::Queued { .. } => receipt.queued = true,
                Outcome::Rejected(why) => {
                    receipt.accepted = false;
                    receipt.rejection = Some(why.code().to_string());
                    break;
                }
            }
        }
        if let (true, Some(next)) = (receipt.accepted, next_plan) {
            self.plans[side_index(side)] = next;
        }
        Ok(receipt)
    }

    // ---- advancing

    /// Plays on. Between stops the computer-managed sides are consulted at their checkpoints;
    /// the result says why play stopped.
    pub fn advance(&mut self, request: AdvanceRequest) -> Result<AdvanceOutcome, InteractiveError> {
        let now = self.playing()?.now_ms();
        let target = match request.target {
            AdvanceTarget::ForMinutes(m) => now.saturating_add(m.saturating_mul(60_000)),
            AdvanceTarget::ToMinute(m) => m.saturating_mul(60_000),
            AdvanceTarget::ToEnd => u32::MAX,
        };
        let stop = loop {
            let step_to = target.min(self.next_review_ms.max(now));
            let options = AdvanceOptions {
                stop_at_key_events: true,
                stop_at_period_breaks: true,
            };
            let reason = self
                .playing()?
                .advance_to(step_to, options)
                .map_err(|e| InteractiveError::Engine(e.to_string()))?;
            let at = self.playing()?.now_ms();
            let review_due = at >= self.next_review_ms;
            if review_due {
                self.next_review_ms = (at / REVIEW_EVERY_MS + 1) * REVIEW_EVERY_MS;
            }
            self.consult_computer_managers(reason == Advance::PeriodBreak, review_due)?;
            match reason {
                Advance::Finished => break StopReason::Finished,
                Advance::Paused => break StopReason::Paused,
                Advance::PeriodBreak => break StopReason::PeriodBreak,
                Advance::KeyEvent if request.stop_at_key_events => break StopReason::KeyEvent,
                Advance::KeyEvent => {}
                Advance::Reached => {
                    if at >= target {
                        break StopReason::Reached;
                    }
                }
            }
        };
        let view = self.view().ok_or(InteractiveError::AlreadyCompleted)?;
        Ok(AdvanceOutcome { stop, view })
    }

    /// Asks each computer-managed side what it wants to do, at the checkpoint the last stretch
    /// of play produced, and sends it through the command pipeline.
    fn consult_computer_managers(
        &mut self,
        at_break: bool,
        review_due: bool,
    ) -> Result<(), InteractiveError> {
        let cursor = self.ai_cursor;
        let (new_events, finished) = {
            let session = self.playing()?;
            (session.events_since(cursor).to_vec(), session.is_finished())
        };
        self.ai_cursor += new_events.len();
        if finished {
            return Ok(());
        }
        let dismissal = new_events.iter().any(|e| {
            matches!(
                &e.kind,
                EventKind::Card { colour, .. }
                    if *colour != apfm_match_lab::output::CardColour::Yellow
            )
        });
        for side in [Side::Home, Side::Away] {
            let actor = match (self.user_side, self.assistant_active) {
                (Some(user), true) if user == side => ASSISTANT_ACTOR.to_string(),
                (Some(user), _) if user == side => continue,
                _ => opponent_actor(side),
            };
            let injured = new_events
                .iter()
                .any(|e| e.side == Some(side) && matches!(e.kind, EventKind::Injury { .. }));
            let checkpoint = if dismissal {
                Checkpoint::Dismissal
            } else if injured {
                Checkpoint::Injury
            } else if at_break {
                Checkpoint::Halftime
            } else if review_due {
                Checkpoint::Review
            } else {
                continue;
            };
            self.manage(&actor, side, checkpoint)?;
        }
        Ok(())
    }

    fn manage(
        &mut self,
        actor: &str,
        side: Side,
        checkpoint: Checkpoint,
    ) -> Result<(), InteractiveError> {
        let view = self.playing()?.view();
        let now = view.clock_ms;
        let state = &self.ai[side_index(side)];
        let mut blocked = [false; Group::COUNT];
        for (b, until) in blocked.iter_mut().zip(state.stand_back_until) {
            *b = until.is_some_and(|t| now < t);
        }
        let orders = ai::decide(&AiContext {
            side,
            view: &view,
            plan: &self.plans[side_index(side)],
            identity: &self.identities[side_index(side)],
            squad: &self.squad,
            checkpoint,
            regulation_ms: self.regulation_ms,
            blocked,
            last_tactical_change_ms: state.last_tactical_change_ms,
        });
        for order in orders {
            self.next_command += 1;
            let id = format!("{actor}-{}-{}", side_index(side), self.next_command);
            let order = match order {
                AiOrder::Tactics(change) => Order::Tactics(change),
                AiOrder::Substitute { off, on } => Order::Substitute { off, on },
            };
            let tactical = matches!(order, Order::Tactics(_));
            let receipt = self.dispatch(actor, side, &id, &order);
            // A computer manager's refusal is information, not an error: the engine said no.
            if receipt.is_ok_and(|r| r.accepted) && tactical {
                self.ai[side_index(side)].last_tactical_change_ms = Some(now);
            }
        }
        Ok(())
    }

    // ---- finishing

    /// Turns a match that reached full time into the career report. Never for a partial match.
    /// The session is kept until the report exists, so a failure leaves the match intact and
    /// the call can simply be made again.
    pub fn complete(&mut self) -> Result<&MatchReport, InteractiveError> {
        if let Stage::Playing(session) = &self.stage {
            if !session.is_finished() {
                return Err(InteractiveError::NotFinished);
            }
            let run = session
                .finished_run()
                .map_err(|e| InteractiveError::Engine(e.to_string()))?;
            let context = spatial_runner::session_projection_context(&self.input);
            let mut report = spatial_runner::report_from_session_output(&context, &run.output)
                .map_err(InteractiveError::Run)?;
            spatial_runner::attach_physical_load(&mut report, &self.input, &run.output);
            self.stage = Stage::Completed(Box::new(report));
        }
        match &self.stage {
            Stage::Completed(report) => Ok(report),
            Stage::Playing(_) => Err(InteractiveError::NotFinished),
        }
    }

    // ---- save and reload

    /// The compact authoritative state: the engine's checkpoint (input, configuration and the
    /// journal of accepted commands) and the controller's own memory. No frames, no per-tick
    /// log.
    pub fn checkpoint(&self) -> Result<InteractiveCheckpoint, InteractiveError> {
        let session = self.session().ok_or(InteractiveError::AlreadyCompleted)?;
        Ok(InteractiveCheckpoint {
            identity: self.identity.clone(),
            user_side: self.user_side,
            assistant_available: self.assistant_available,
            assistant_active: self.assistant_active,
            plans: self.plans.clone(),
            identities: self.identities.clone(),
            ai: self.ai.clone(),
            orders: self.orders.clone(),
            next_command: self.next_command,
            ai_cursor: self.ai_cursor,
            next_review_ms: self.next_review_ms,
            session: session
                .checkpoint()
                .map_err(|e| InteractiveError::Engine(e.to_string()))?,
        })
    }

    pub fn restore(cp: InteractiveCheckpoint) -> Result<Self, InteractiveError> {
        let input = cp.session.input.clone();
        let session = MatchSession::restore(cp.session)
            .map_err(|e| InteractiveError::Engine(e.to_string()))?;
        let regulation_ms = u32::from(input.config.half_length_minutes) * 2 * 60_000;
        Ok(Self {
            identity: cp.identity,
            squad: Squad::of(&input),
            input,
            stage: Stage::Playing(Box::new(session)),
            user_side: cp.user_side,
            assistant_available: cp.assistant_available,
            assistant_active: cp.assistant_active,
            plans: cp.plans,
            identities: cp.identities,
            ai: cp.ai,
            orders: cp.orders,
            next_command: cp.next_command,
            ai_cursor: cp.ai_cursor,
            next_review_ms: cp.next_review_ms,
            regulation_ms,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveCheckpoint {
    identity: FixtureIdentity,
    user_side: Option<Side>,
    assistant_available: bool,
    assistant_active: bool,
    plans: [TacticalPlan; 2],
    identities: [TacticalPlan; 2],
    ai: [SideAi; 2],
    orders: BTreeMap<String, OrderReceipt>,
    next_command: u64,
    ai_cursor: usize,
    next_review_ms: u32,
    session: SessionCheckpoint,
}

enum Order {
    Tactics(TacticalChange),
    Substitute { off: PlayerId, on: PlayerId },
    Cancel { off: PlayerId },
}

impl Order {
    fn groups(&self) -> Vec<Group> {
        match self {
            Order::Tactics(c) => {
                let mut groups = Vec::new();
                if c.style.is_some() || c.pressing.is_some() || c.tempo.is_some() {
                    groups.push(Group::Tactics);
                }
                if c.formation.is_some() {
                    groups.push(Group::Formation);
                }
                groups
            }
            Order::Substitute { .. } | Order::Cancel { .. } => vec![Group::Substitution],
        }
    }
}

/// Registers who may command which side: the manager (and the assistant, when on) for the
/// player's side, the computer's own manager for every other.
fn grant_actors(session: &mut MatchSession, user_side: Option<Side>, assistant_active: bool) {
    for side in [Side::Home, Side::Away] {
        if Some(side) == user_side {
            session.grant(MANAGER_ACTOR, side);
            if assistant_active {
                session.grant(ASSISTANT_ACTOR, side);
            }
        } else {
            session.grant(&opponent_actor(side), side);
        }
    }
}

fn opponent_actor(side: Side) -> String {
    format!("{OPPONENT_ACTOR}-{}", side_index(side))
}

/// An event worth showing to a person (the rest of the stream is football detail).
fn is_key_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Goal { .. }
            | EventKind::Card { .. }
            | EventKind::Substitution { .. }
            | EventKind::Injury { .. }
            | EventKind::GoalkeeperChange { .. }
            | EventKind::TacticalChange { .. }
            | EventKind::Penalty { .. }
            | EventKind::PeriodStart
            | EventKind::PeriodEnd
            | EventKind::MatchEnd
    )
}

#[cfg(test)]
mod tests;
