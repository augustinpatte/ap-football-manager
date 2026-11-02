//! Advancing the career to a date: the application's side of `ofm_core::fast_forward`.
//!
//! The loop and its rules are in `ofm_core::fast_forward`; this module holds what only the
//! application owns — the game lock, the one-operation gate shared with saving and live matches,
//! the stats store, the blockers, a cancel flag and the progress a screen can show.
//!
//! * **One day per lock.** Each step is a whole day inside one `update_game`, so the lock is free
//!   between days: an MCP agent or a command that edits tactics is applied between two days and
//!   never torn through one, and a cancel lands at the next day boundary.
//! * **One operation.** The run holds [`super::live_session::idle_operation`] for its whole
//!   length. A save or a live match requested meanwhile waits for the run to end instead of seeing
//!   the world part-way; a second fast-forward is refused outright ([`ACTIVE`]).
//! * **One copy of the state.** The screen gets the world once, at the end. A day-by-day loop on
//!   the screen pays that copy, and the IPC serialization of it, on every day.
//!
//! ## Integration with the assistant coach (delegation)
//!
//! The assistant works inside the day: `turn::process_day` runs `staff_system::delegation::
//! run_training` and `run_after_day`, so a delegated duty is performed on every day of a
//! fast-forward exactly as on a single day. What a fast-forward adds is the *approval* contract:
//! the proposals the assistant leaves in `game.delegation.proposals` are decisions
//! ([`ofm_core::fast_forward::DecisionKind::DelegationProposal`]). A stopping policy halts on a
//! proposal that appears during the run; the target-date policy lists it in the report. Nothing in
//! this module knows what a duty is, so a change to the assistant needs no change here as long as
//! it keeps proposals in `game.delegation.proposals` and acts inside `process_day`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use log::info;
use serde::{Deserialize, Serialize};

use ofm_core::advance_results::{collect_advance_results, AdvanceMatchResult};
use ofm_core::fast_forward::{
    DecisionKind, FastForward, FastForwardProgress, FastForwardRequest, FastForwardSummary,
    PendingDecision, StopReason,
};
use ofm_core::game::Game;
use ofm_core::state::StateManager;

use super::time_blockers::compute_blocking_actions;

/// Whether a fast-forward is running. A second request while one runs is refused: both would
/// queue behind the same operation lock and then play on from wherever the first left the clock.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set by [`request_cancel`], read between days.
static CANCEL: AtomicBool = AtomicBool::new(false);

/// How often a screen is told where the run stands. A day can take a few milliseconds in a small
/// world; a render per day would cost more than the day.
#[cfg(not(test))]
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);
/// Every day, so a test can act at an exact day boundary.
#[cfg(test)]
const PROGRESS_INTERVAL: Duration = Duration::ZERO;

/// What the screen receives when a run ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastForwardResponse {
    #[serde(flatten)]
    pub summary: FastForwardSummary,
    /// The world as it stands between two days. One copy, for the screen.
    pub game: Game,
    /// Every match finished during the run in the user's competitions and the national teams, for
    /// the same recap a single day produces.
    pub results: Vec<AdvanceMatchResult>,
}

struct ActiveRun;

impl ActiveRun {
    fn claim() -> Result<Self, String> {
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "be.error.fastForward.alreadyRunning".to_string())?;
        CANCEL.store(false, Ordering::Release);
        Ok(Self)
    }
}

impl Drop for ActiveRun {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}

/// Asks the running fast-forward to stop at the next day boundary. `false` when none is running.
pub fn request_cancel() -> bool {
    if ACTIVE.load(Ordering::Acquire) {
        CANCEL.store(true, Ordering::Release);
        true
    } else {
        false
    }
}

/// The blockers as decisions. Their identity is the blocker's id and what it names, so the same
/// squad problem about different players is a new decision.
fn blocker_decisions(game: &Game) -> Vec<PendingDecision> {
    compute_blocking_actions(game)
        .into_iter()
        .filter_map(|blocker| {
            let id = blocker.get("id")?.as_str()?.to_string();
            let params: BTreeMap<String, String> = blocker
                .get("text_params")
                .and_then(|params| params.as_object())
                .map(|params| {
                    params
                        .iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|text| (key.clone(), text.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(PendingDecision {
                kind: DecisionKind::Blocker,
                id,
                params,
            })
        })
        .collect()
}

/// Plays the request, telling `on_progress` where it stands (at most every
/// [`PROGRESS_INTERVAL`], and at the start and the end).
pub fn fast_forward(
    state: &StateManager,
    request: &FastForwardRequest,
    on_progress: &mut dyn FnMut(&FastForwardProgress),
) -> Result<FastForwardResponse, String> {
    let _operation = super::live_session::idle_operation(state)?;
    let _active = ActiveRun::claim()?;
    crate::platform::keep_simulation_awake();
    info!(
        "[cmd] fast_forward: target={}, policy={:?}",
        request.target_date, request.policy
    );

    let mut run = state
        .update_game(|game| FastForward::begin(game, request, &mut blocker_decisions))
        .ok_or("be.error.noActiveGameSession")??;
    let career_token = state
        .get_game(|game| game.career_token)
        .ok_or("be.error.noActiveGameSession")?;

    let mut told = Instant::now();
    let mut announce = |run: &FastForward, game: &Game, force: bool| {
        if force || told.elapsed() >= PROGRESS_INTERVAL {
            told = Instant::now();
            on_progress(&run.progress(game));
        }
    };
    state.get_game(|game| announce(&run, game, true));

    let stop = loop {
        if CANCEL.load(Ordering::Acquire) {
            break StopReason::Cancelled;
        }
        let mut captures = Vec::new();
        let step = state
            .update_game(|game| {
                if game.career_token != career_token {
                    return Some(StopReason::Error {
                        error: "be.error.fastForward.careerChanged".to_string(),
                    });
                }
                let stop = run.step(game, &mut blocker_decisions, &mut |capture| {
                    captures.push(capture);
                });
                announce(&run, game, false);
                stop
            })
            .ok_or("be.error.noActiveGameSession")?;
        for capture in captures {
            state.append_stats_state(capture);
        }
        if let Some(stop) = step {
            break stop;
        }
    };

    let since = run.start_date().to_string();
    let (summary, game, results) = state
        .get_game(|game| {
            let summary = run.finish(game, stop);
            (
                summary,
                game.ui_clone(),
                collect_advance_results(game, &since),
            )
        })
        .ok_or("be.error.noActiveGameSession")?;
    on_progress(&summary.progress);
    info!(
        "[cmd] fast_forward: ended {:?} after {} days at {}",
        summary.stop, summary.progress.days_done, summary.progress.current_date
    );
    Ok(FastForwardResponse {
        summary,
        game,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::Team;
    use ofm_core::clock::GameClock;
    use ofm_core::fast_forward::InterruptPolicy;

    use crate::application::time_advancement::advance_time_with_mode;

    /// Serializes the tests that claim the one fast-forward slot.
    static SLOT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn quiet_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.hire("team1".into());
        let team = Team::new(
            "team1".into(),
            "Test FC".into(),
            "TST".into(),
            "England".into(),
            "Town".into(),
            "Ground".into(),
            10_000,
        );
        Game::new(clock, manager, vec![team], vec![], vec![], vec![])
    }

    fn request(target: &str, policy: InterruptPolicy) -> FastForwardRequest {
        FastForwardRequest {
            target_date: target.to_string(),
            policy,
        }
    }

    /// Given a quiet game, when a fast-forward runs a fortnight and the same days are played one at
    /// a time through the single-day command, then both end on the same date with the same money.
    #[test]
    fn a_fast_forward_ends_where_the_same_days_played_singly_end() {
        let _slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let by_days = StateManager::new();
        by_days.set_game(quiet_game());
        for _ in 0..14 {
            advance_time_with_mode(&by_days, "delegate").expect("a day");
        }
        let at_once = StateManager::new();
        at_once.set_game(quiet_game());
        let response = fast_forward(
            &at_once,
            &request("2025-06-29", InterruptPolicy::ToTargetDate),
            &mut |_| {},
        )
        .expect("a fast-forward");

        assert_eq!(response.summary.stop, StopReason::TargetReached);
        assert_eq!(response.summary.progress.days_done, 14);
        let singly = by_days
            .get_game(|game| {
                (
                    game.clock.current_date,
                    game.teams[0].finance,
                    game.messages.len(),
                    game.news.len(),
                )
            })
            .unwrap();
        let together = at_once
            .get_game(|game| {
                (
                    game.clock.current_date,
                    game.teams[0].finance,
                    game.messages.len(),
                    game.news.len(),
                )
            })
            .unwrap();
        assert_eq!(singly, together);
    }

    /// Given a run in progress, when a second is requested, then it is refused and the first is
    /// unaffected; and when the first ends, a new one can start.
    #[test]
    fn a_second_fast_forward_is_refused_while_one_runs() {
        let _slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let held = ActiveRun::claim().expect("the slot is free");
        let state = StateManager::new();
        state.set_game(quiet_game());
        let refused = fast_forward(
            &state,
            &request("2025-06-20", InterruptPolicy::ToTargetDate),
            &mut |_| {},
        );
        assert_eq!(refused.unwrap_err(), "be.error.fastForward.alreadyRunning");
        assert!(request_cancel(), "a running fast-forward can be cancelled");
        drop(held);
        assert!(
            !request_cancel(),
            "nothing runs, so there is nothing to cancel"
        );
        fast_forward(
            &state,
            &request("2025-06-20", InterruptPolicy::ToTargetDate),
            &mut |_| {},
        )
        .expect("the slot is free again");
    }

    /// Given a cancel requested while the run is on its third day, when the next boundary comes,
    /// then the run ends `Cancelled` with exactly three whole days played, the world is the one
    /// three single days make, and the game carries on from there.
    #[test]
    fn a_cancel_stops_the_run_between_days_and_the_game_carries_on() {
        let _slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = StateManager::new();
        state.set_game(quiet_game());
        let response = fast_forward(
            &state,
            &request("2025-08-01", InterruptPolicy::ToTargetDate),
            &mut |progress| {
                if progress.days_done == 3 {
                    request_cancel();
                }
            },
        )
        .expect("a cancelled run is still a result");

        assert_eq!(response.summary.stop, StopReason::Cancelled);
        assert_eq!(response.summary.progress.days_done, 3);
        assert_eq!(response.summary.progress.current_date, "2025-06-18");

        let reference = StateManager::new();
        reference.set_game(quiet_game());
        for _ in 0..3 {
            advance_time_with_mode(&reference, "delegate").expect("a day");
        }
        let finance_of = |state: &StateManager| state.get_game(|g| g.teams[0].finance).unwrap();
        assert_eq!(finance_of(&state), finance_of(&reference));

        let before = state.get_game(|game| game.clock.current_date).unwrap();
        advance_time_with_mode(&state, "delegate").expect("the game carries on after a cancel");
        let after = state.get_game(|game| game.clock.current_date).unwrap();
        assert_eq!(after - before, chrono::Duration::days(1));
    }
}
