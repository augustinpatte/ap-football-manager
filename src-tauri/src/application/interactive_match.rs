//! Application layer of the interactive match: start, advance, command, read and finish.
//!
//! Every function takes only the `StateManager`, so the Tauri commands stay thin adapters over
//! them. The football lives in `ofm_core::interactive_match` (the M12 session and its
//! controller); the career is changed only by `finish_interactive_match_internal`, which hands
//! the report to the one finalisation every match uses (`finish_spatial_match`).
//!
//! Errors are translation keys: the player sees them.

use log::info;
use ofm_core::interactive_match::{
    self, AdvanceOutcome, AdvanceRequest, FixtureIdentity, InteractiveMatch, InteractiveView,
    KeyEvent, OrderReceipt, StopReason, UserCommand,
};
use ofm_core::state::StateManager;
use serde::Serialize;

use super::live_match::{finish_spatial_match, FinishLiveMatchResponse};

const NO_ACTIVE_MATCH: &str = "be.error.interactive.noActiveMatch";

/// What the screen needs after any call: the state, and the key events it has not shown yet.
#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSnapshot {
    pub view: InteractiveView,
    pub events: Vec<KeyEvent>,
    /// Ask for events from here next time.
    pub next_event_cursor: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdvanceSnapshot {
    pub stop: StopReason,
    pub snapshot: InteractiveSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderSnapshot {
    pub receipt: OrderReceipt,
    pub snapshot: InteractiveSnapshot,
}

fn snapshot_of(m: &InteractiveMatch, cursor: usize) -> Result<InteractiveSnapshot, String> {
    let view = m.view().ok_or("be.error.interactive.alreadyCompleted")?;
    let (events, next_event_cursor) = m.events_since(cursor);
    Ok(InteractiveSnapshot {
        view,
        events,
        next_event_cursor,
    })
}

fn with_match<R>(
    state: &StateManager,
    f: impl FnOnce(&mut InteractiveMatch) -> Result<R, String>,
) -> Result<R, String> {
    state
        .with_interactive_match(f)
        .ok_or_else(|| NO_ACTIVE_MATCH.to_string())?
}

/// The fixture the live session has selected: the one identity every match of the day is
/// finalised against.
fn live_identity(state: &StateManager) -> Option<FixtureIdentity> {
    state.with_live_match(|session| FixtureIdentity {
        fixture_index: session.fixture_index,
        competition_id: session.competition_id.clone(),
        fixture_id: session.fixture_id.clone(),
    })
}

/// Starts the interactive match of the fixture the live session has selected. Starting twice
/// returns the match already under way (a double click starts nothing new), but only if it is
/// the match of the fixture selected now: one left over from another fixture is discarded.
pub fn start_interactive_match_internal(
    state: &StateManager,
) -> Result<InteractiveSnapshot, String> {
    let _operation = super::live_session::operation();
    info!("[cmd] start_interactive_match");
    let identity = live_identity(state).ok_or("be.error.noActiveLiveMatch")?;
    if let Some(existing) =
        state.with_interactive_match(|m| (m.identity() == &identity).then(|| snapshot_of(m, 0)))
    {
        if let Some(snapshot) = existing {
            return snapshot;
        }
        drop(state.take_interactive_match());
    }
    let started = state
        .get_game(|game| {
            InteractiveMatch::start(game, identity, &interactive_match::new_session_id())
        })
        .ok_or("be.error.noActiveGameSession")?
        .map_err(|e| e.translation_key())?;
    let snapshot = snapshot_of(&started, 0)?;
    state.set_interactive_match(started);
    Ok(snapshot)
}

pub fn get_interactive_match_state_internal(
    state: &StateManager,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    with_match(state, |m| snapshot_of(m, event_cursor))
}

/// Plays on. The computer-managed sides are consulted inside; the manager's own side is only
/// ever changed by his orders (or the assistant's, if he has handed the match over).
pub fn advance_interactive_match_internal(
    state: &StateManager,
    request: AdvanceRequest,
    event_cursor: usize,
) -> Result<AdvanceSnapshot, String> {
    with_match(state, |m| {
        let AdvanceOutcome { stop, .. } = m.advance(request).map_err(|e| e.translation_key())?;
        Ok(AdvanceSnapshot {
            stop,
            snapshot: snapshot_of(m, event_cursor)?,
        })
    })
}

pub fn submit_match_command_internal(
    state: &StateManager,
    command: UserCommand,
    event_cursor: usize,
) -> Result<OrderSnapshot, String> {
    with_match(state, |m| {
        let receipt = m.submit(command).map_err(|e| e.translation_key())?;
        Ok(OrderSnapshot {
            receipt,
            snapshot: snapshot_of(m, event_cursor)?,
        })
    })
}

/// Pauses or resumes the clock. A paused match still takes orders.
pub fn set_interactive_pause_internal(
    state: &StateManager,
    paused: bool,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    with_match(state, |m| {
        if paused { m.pause() } else { m.resume() }.map_err(|e| e.translation_key())?;
        snapshot_of(m, event_cursor)
    })
}

/// Hands the player's side to the assistant, or takes it back.
pub fn set_interactive_assistant_internal(
    state: &StateManager,
    active: bool,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    with_match(state, |m| {
        m.set_assistant(active).map_err(|e| e.translation_key())?;
        snapshot_of(m, event_cursor)
    })
}

/// Applies the finished match to the career through the one finalisation every match uses.
///
/// A match that has not reached full time is refused, never finalised as if it had. If the
/// report cannot be applied (a stale fixture), the finished match is kept so the player can
/// try again, and the career is unchanged.
pub fn finish_interactive_match_internal(
    state: &StateManager,
) -> Result<FinishLiveMatchResponse, String> {
    let _operation = super::live_session::operation();
    info!("[cmd] finish_interactive_match");
    let mut interactive = state
        .take_interactive_match()
        .ok_or_else(|| NO_ACTIVE_MATCH.to_string())?;
    // A match that belongs to another fixture than the one now selected must never be applied
    // to it: its teams and score would land on the wrong fixture.
    if live_identity(state).as_ref() != Some(interactive.identity()) {
        return Err(NO_ACTIVE_MATCH.to_string());
    }
    let report = match interactive.complete() {
        Ok(report) => report.clone(),
        Err(error) => {
            state.set_interactive_match(interactive);
            return Err(error.translation_key());
        }
    };
    match finish_spatial_match(state, report) {
        Ok(response) => Ok(response),
        Err(error) => {
            state.set_interactive_match(interactive);
            Err(error)
        }
    }
}

/// After a career is loaded: resumes the match it was saved in the middle of, if any.
///
/// Nothing here can fail the load. A stored match that is stale (its fixture has been played), or
/// that cannot be trusted (another version, altered, or not reproducible by this engine), is
/// discarded; for the untrustworthy kinds the player is told once, and the fixture is still
/// scheduled, so the match can be played from kick-off.
pub fn restore_saved_match(
    state: &StateManager,
    saves: &mut db::save_manager::SaveManager,
    save_id: &str,
) {
    use ofm_core::interactive_match::saved;
    state.set_restore_notice(None);
    let stored = match saves.load_interactive_match(save_id) {
        Ok(Some(stored)) => stored,
        Ok(None) => return,
        Err(error) => {
            log::warn!("[restore] could not read the stored match: {error}");
            return;
        }
    };
    let discarded = || {
        state.set_restore_notice(Some(
            saved::RestoreProblem::Unusable.translation_key().into(),
        ))
    };
    let Ok(stored) = stored else {
        discarded();
        return;
    };
    let Ok((baselines, interactive)) = saved::decode(&stored.payload) else {
        discarded();
        return;
    };
    let identity = interactive.identity().clone();
    if identity.competition_id != stored.competition_id || identity.fixture_id != stored.fixture_id
    {
        discarded();
        return;
    }
    // Rebuilding the live session is idempotent for the day's other fixtures (already played). A
    // fixture that is no longer scheduled means the match was finished: nothing to resume.
    if super::live_match::start_live_match_with_identity(
        state,
        identity.fixture_index,
        "live",
        false,
        Some(&identity.competition_id),
        Some(&identity.fixture_id),
    )
    .is_err()
    {
        return;
    }
    state.with_live_match(|session| baselines.apply(session));
    state.set_interactive_match(interactive);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::live_match::start_live_match_with_identity;
    use crate::commands::spatial_match::tests::game_with_spatial_fixture;
    use db::save_manager::SaveManager;
    use domain::league::FixtureStatus;
    use domain::team::PlayStyle;
    use ofm_core::game::Game;
    use ofm_core::interactive_match::{plan::TacticalChange, AdvanceTarget, UserOrder};
    use tempfile::TempDir;

    const COMPETITION: &str = "competition-m12-1";
    const FIXTURE: &str = "fixture-m12-1";

    fn begin(state: &StateManager) -> InteractiveSnapshot {
        start_live_match_with_identity(state, 0, "live", false, Some(COMPETITION), Some(FIXTURE))
            .expect("the live session that carries the fixture identity");
        start_interactive_match_internal(state).expect("interactive start")
    }

    fn advance(state: &StateManager, target: AdvanceTarget) -> AdvanceSnapshot {
        advance_interactive_match_internal(
            state,
            AdvanceRequest {
                target,
                stop_at_key_events: false,
            },
            0,
        )
        .expect("advance")
    }

    fn to_halftime(state: &StateManager) {
        loop {
            let out = advance(state, AdvanceTarget::ToEnd);
            match out.stop {
                StopReason::PeriodBreak => return,
                StopReason::Finished => panic!("the match ended before the break"),
                _ => {}
            }
        }
    }

    fn to_full_time(state: &StateManager) {
        loop {
            if advance(state, AdvanceTarget::ToEnd).stop == StopReason::Finished {
                return;
            }
        }
    }

    fn snapshot(state: &StateManager) -> InteractiveSnapshot {
        get_interactive_match_state_internal(state, 0).expect("state")
    }

    fn order(state: &StateManager, id: &str, order: UserOrder) -> OrderSnapshot {
        let revision = snapshot(state).view.session.revision;
        submit_match_command_internal(
            state,
            UserCommand {
                command_id: id.into(),
                expected_revision: revision,
                order,
            },
            0,
        )
        .expect("order")
    }

    fn standings_played(game: &Game) -> u32 {
        game.competitions[0]
            .standings
            .iter()
            .map(|s| s.played)
            .sum()
    }

    /// A starter and a bench player of the user's side, both outfield.
    fn legal_swap(view: &InteractiveView) -> (String, String) {
        // the engine's position type is not this crate's to name: read it by its Debug name
        let off = view
            .session
            .home
            .on_pitch
            .iter()
            .find(|p| !format!("{:?}", p.role).contains("Gk"))
            .expect("an outfield starter");
        let on = view
            .session
            .home
            .bench
            .iter()
            .find(|p| !format!("{:?}", p.natural_position).contains("Gk"))
            .expect("an outfield substitute");
        (off.id.0.clone(), on.id.0.clone())
    }

    /// Given a real saved career and its fixture,
    /// When the user plays it interactively (halftime, a mentality change, a legal substitution,
    /// full time) and it is finalised through the canonical path,
    /// Then the fixture, table, appearances and report are written exactly once, nothing was
    /// written before finalisation, the substitution is in the record, and a fresh SQLite reload
    /// reads the same career. A second finish changes nothing.
    #[test]
    fn a_managed_match_is_finalised_once_and_survives_a_reload() {
        let directory = TempDir::new().expect("saves directory");
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let mut saves = SaveManager::init(directory.path()).expect("save manager");
        let save_id = saves.create_save(&game, "interactive").expect("save");
        state.set_save_id(save_id.clone());
        state.set_game(game);

        let start = begin(&state);
        assert_eq!(start.view.session.clock_ms, 0);

        to_halftime(&state);
        let at_break = snapshot(&state);
        assert!(at_break.view.session.clock_ms > 0);
        let before = state
            .get_game(|g| {
                (
                    standings_played(g),
                    g.competitions[0].fixtures[0].status.clone(),
                )
            })
            .unwrap();
        assert_eq!(
            before,
            (0, FixtureStatus::Scheduled),
            "nothing is written before finalisation"
        );

        let tactics = order(
            &state,
            "mentality",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                ..Default::default()
            }),
        );
        assert!(tactics.receipt.accepted, "{:?}", tactics.receipt);
        assert_eq!(tactics.snapshot.view.home_plan.style, PlayStyle::Attacking);

        let (off, on) = legal_swap(&tactics.snapshot.view);
        let swap = order(
            &state,
            "swap",
            UserOrder::Substitute {
                player_off_id: off.clone(),
                player_on_id: on.clone(),
            },
        );
        assert!(
            swap.receipt.accepted && swap.receipt.queued,
            "{:?}",
            swap.receipt
        );

        to_full_time(&state);
        let end = snapshot(&state);
        assert!(end.view.session.finished);
        assert!(end.view.session.home.on_pitch.iter().any(|p| p.id.0 == on));
        assert!(end.view.session.home.used.iter().any(|p| p.id.0 == off));
        let score = (end.view.session.score_home, end.view.session.score_away);
        let home_swaps = end
            .events
            .iter()
            .filter_map(|e| serde_json::to_value(&e.event).ok())
            .filter(|v| v["type"] == "substitution" && v["side"] == "home")
            .count();
        assert_eq!(
            home_swaps, 1,
            "the manager's substitution is one canonical event"
        );

        // still nothing in the career: the standings move only at finalisation
        assert_eq!(state.get_game(standings_played).unwrap(), 0);

        let finalised = finish_interactive_match_internal(&state).expect("canonical finalisation");
        let game = &finalised.game;
        let fixture = &game.competitions[0].fixtures[0];
        assert_eq!(fixture.status, FixtureStatus::Completed);
        let result = fixture.result.as_ref().expect("a stored result");
        assert_eq!((result.home_goals, result.away_goals), (score.0, score.1));
        assert_eq!(
            standings_played(game),
            2,
            "both clubs have one league match"
        );
        let appearances = |id: &str| {
            game.players
                .iter()
                .find(|p| p.id == id)
                .map_or(0, |p| p.stats.appearances)
        };
        assert!(appearances(&off) >= 1, "the player who went off played");
        assert!(appearances(&on) >= 1, "the substitute played");
        let report = result.report.as_ref().expect("a compact report");
        let swaps: Vec<_> = report
            .events
            .iter()
            .filter(|e| e.event_type == "Substitution" && e.side == "Home")
            .collect();
        assert_eq!(
            swaps.len(),
            1,
            "the manager's substitution is in the stored record exactly once"
        );
        let ids = [
            swaps[0].player_id.as_deref(),
            swaps[0].secondary_player_id.as_deref(),
        ];
        assert!(ids.contains(&Some(off.as_str())) && ids.contains(&Some(on.as_str())));

        // fatigue persists, and substitutions change who carries it: the player who played
        // the whole match is more worn than the substitute, who is more worn than a man who
        // never left the bench
        let condition = |id: &str| {
            game.players
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.condition)
        };
        let full_match = end
            .view
            .session
            .home
            .on_pitch
            .iter()
            .filter(|p| p.id.0 != on)
            .max_by(|a, b| a.minutes_played.total_cmp(&b.minutes_played))
            .map(|p| p.id.0.clone())
            .expect("a player who played throughout");
        let unused = end
            .view
            .session
            .home
            .bench
            .first()
            .map(|p| p.id.0.clone())
            .expect("a substitute who never came on");
        let (worn, sub, rested) = (
            condition(&full_match).expect("full-match player"),
            condition(&on).expect("substitute"),
            condition(&unused).expect("unused substitute"),
        );
        assert!(
            worn < sub,
            "90 minutes ({worn}) wear a player more than a half ({sub})"
        );
        assert!(
            sub <= rested,
            "a half ({sub}) costs more than none ({rested})"
        );

        // goals credited to players add up to the score, except the own goals, which count for the other
        // side and are credited to nobody (the stored record names the player who scored them)
        let credited: u32 = game.players.iter().map(|p| p.stats.goals).sum();
        let own_goals = report
            .events
            .iter()
            .filter(|e| e.event_type == "Goal" && e.own_goal_by.is_some())
            .count() as u32;
        assert_eq!(
            credited + own_goals,
            u32::from(result.home_goals) + u32::from(result.away_goals)
        );

        // the interactive state is gone; finishing again must not process the fixture twice
        assert!(state.with_interactive_match(|_| ()).is_none());
        assert_eq!(
            finish_interactive_match_internal(&state).unwrap_err(),
            "be.error.interactive.noActiveMatch"
        );
        assert_eq!(state.get_game(standings_played).unwrap(), 2);

        saves
            .save_game(&finalised.game, &save_id)
            .expect("save the career");
        drop(saves);
        let mut reader = SaveManager::init(directory.path()).expect("fresh reader");
        let reloaded = reader.load_game(&save_id).expect("reload");
        let fixture = &reloaded.competitions[0].fixtures[0];
        assert_eq!(fixture.status, FixtureStatus::Completed);
        let stored = fixture.result.as_ref().expect("persisted result");
        assert_eq!(
            (stored.home_goals, stored.away_goals),
            (result.home_goals, result.away_goals)
        );
        assert_eq!(standings_played(&reloaded), 2);
        assert!(reloaded
            .players
            .iter()
            .any(|p| p.id == on && p.stats.appearances >= 1));
    }

    /// Given a match that has not reached full time,
    /// When it is finished,
    /// Then it is refused with a key, the career is untouched, and the match can still be played.
    #[test]
    fn a_partial_match_is_never_finalised() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        advance(&state, AdvanceTarget::ForMinutes(20));

        assert_eq!(
            finish_interactive_match_internal(&state).unwrap_err(),
            "be.error.interactive.notFinished"
        );
        let (played, status) = state
            .get_game(|g| {
                (
                    standings_played(g),
                    g.competitions[0].fixtures[0].status.clone(),
                )
            })
            .unwrap();
        assert_eq!((played, status), (0, FixtureStatus::Scheduled));
        assert!(
            state.with_live_match(|_| ()).is_some(),
            "the live session is kept"
        );
        advance(&state, AdvanceTarget::ForMinutes(5));
        to_full_time(&state);
        assert!(finish_interactive_match_internal(&state).is_ok());
    }

    /// Given a finished match whose fixture was completed elsewhere meanwhile,
    /// When it is finished,
    /// Then finalisation is refused with the usual key, the career gets no second result, and
    /// the finished match is kept: put the fixture right and the same result can be applied.
    #[test]
    fn a_failed_finalisation_keeps_the_finished_match_for_a_retry() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        to_full_time(&state);
        state
            .update_game(|g| g.competitions[0].fixtures[0].status = FixtureStatus::Completed)
            .unwrap();

        let error = finish_interactive_match_internal(&state).unwrap_err();
        assert_eq!(error, "be.error.liveMatch.fixtureNotScheduled");
        assert_eq!(state.get_game(standings_played).unwrap(), 0);
        assert!(state.with_interactive_match(|_| ()).is_some());
        assert!(state.with_live_match(|_| ()).is_some());

        state
            .update_game(|g| g.competitions[0].fixtures[0].status = FixtureStatus::Scheduled)
            .unwrap();
        assert!(finish_interactive_match_internal(&state).is_ok());
        assert_eq!(state.get_game(standings_played).unwrap(), 2);
    }

    /// Given a finished match,
    /// When two requests finish it at the same moment,
    /// Then exactly one applies it and the table moves once.
    #[test]
    fn two_simultaneous_finishes_apply_the_match_once() {
        let state = std::sync::Arc::new(StateManager::new());
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        to_full_time(&state);
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let state = std::sync::Arc::clone(&state);
                std::thread::spawn(move || finish_interactive_match_internal(&state))
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(state.get_game(standings_played).unwrap(), 2);
    }

    /// Given a match under way, a retry of the same start,
    /// Then it returns the same match instead of starting another, and a command addressed to
    /// nothing is refused with a key.
    #[test]
    fn starting_twice_returns_the_match_in_progress() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        let first = begin(&state);
        advance(&state, AdvanceTarget::ForMinutes(3));
        let again = start_interactive_match_internal(&state).unwrap();
        assert_eq!(again.view.session.session_id, first.view.session.session_id);
        assert!(
            again.view.session.clock_ms > 0,
            "the same match, not a new one"
        );

        let none = StateManager::new();
        assert_eq!(
            get_interactive_match_state_internal(&none, 0).unwrap_err(),
            "be.error.interactive.noActiveMatch"
        );
    }

    /// Given a career where the match cannot be built,
    /// When an interactive start is requested,
    /// Then it fails with a translation key and leaves the career and the live session as they were.
    #[test]
    fn a_match_that_cannot_start_leaves_the_pre_match_state() {
        let state = StateManager::new();
        let mut game = game_with_spatial_fixture();
        game.players
            .retain(|p| p.team_id.as_deref() != Some("away"));
        state.set_game(game);
        let live = start_live_match_with_identity(
            &state,
            0,
            "live",
            false,
            Some(COMPETITION),
            Some(FIXTURE),
        );
        if let Err(error) = live {
            // the classic session refuses an empty squad before M12 is asked: still a key
            assert!(error.starts_with("be.error."), "{error}");
            assert!(state.with_interactive_match(|_| ()).is_none());
            assert_eq!(state.get_game(standings_played).unwrap(), 0);
            return;
        }
        let error = start_interactive_match_internal(&state).unwrap_err();
        assert!(error.starts_with("be.error."), "{error}");
        assert!(state.with_interactive_match(|_| ()).is_none());
        assert!(state.with_live_match(|_| ()).is_some());
        assert_eq!(state.get_game(standings_played).unwrap(), 0);
    }

    /// Given an interactive match for one fixture and a live session that now identifies
    /// another,
    /// When the match is resumed, played out and finished,
    /// Then it is never resumed or applied to the other fixture: start discards it, finish
    /// refuses it, and the career is untouched.
    #[test]
    fn a_match_of_another_fixture_is_never_resumed_or_applied() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        to_full_time(&state);
        state
            .with_live_match(|session| session.fixture_id = "another-fixture".to_string())
            .expect("a live session");

        // finishing: refused, nothing applied
        assert_eq!(
            finish_interactive_match_internal(&state).unwrap_err(),
            "be.error.interactive.noActiveMatch"
        );
        assert_eq!(
            state.get_game(standings_played).unwrap(),
            0,
            "nothing was applied"
        );
        assert!(
            state.with_interactive_match(|_| ()).is_none(),
            "the stale match is gone"
        );

        // starting: a stale match still in the slot is discarded, not handed back
        state.set_game(game_with_spatial_fixture());
        let original = {
            state.with_live_match(|session| session.fixture_id = FIXTURE.to_string());
            begin_without_live(&state)
        };
        state.with_live_match(|session| session.fixture_id = "yet-another".to_string());
        let error = start_interactive_match_internal(&state).unwrap_err();
        assert!(error.starts_with("be.error."), "{error}");
        assert!(
            state.with_interactive_match(|_| ()).is_none(),
            "the match of {} was not resumed for another fixture",
            original.view.session.session_id
        );
    }

    /// Starts the interactive match for a live session that already exists.
    fn begin_without_live(state: &StateManager) -> InteractiveSnapshot {
        start_interactive_match_internal(state).expect("interactive start")
    }

    /// Given an interactive match under way,
    /// When the day's match is finished another way (the classic finish),
    /// Then the interactive match is discarded with it: it cannot be applied a second time.
    #[test]
    fn finishing_the_match_another_way_discards_the_interactive_one() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        advance(&state, AdvanceTarget::ForMinutes(10));
        crate::application::live_match::finish_live_match(&state).expect("classic finish");
        assert!(state.with_interactive_match(|_| ()).is_none());
        assert_eq!(
            finish_interactive_match_internal(&state).unwrap_err(),
            "be.error.interactive.noActiveMatch"
        );
        assert_eq!(
            state.get_game(standings_played).unwrap(),
            2,
            "applied once, by the classic path"
        );
    }

    /// Given a match under way,
    /// When it is paused, advanced and resumed,
    /// Then a paused match does not move (and says so), it still takes orders, and resuming
    /// lets the clock run again.
    #[test]
    fn a_paused_match_stands_still_but_takes_orders() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        advance(&state, AdvanceTarget::ForMinutes(5));
        let paused = set_interactive_pause_internal(&state, true, 0).unwrap();
        assert!(paused.view.session.paused);
        let clock = paused.view.session.clock_ms;

        let still = advance(&state, AdvanceTarget::ForMinutes(10));
        assert_eq!(still.stop, StopReason::Paused);
        assert_eq!(
            still.snapshot.view.session.clock_ms, clock,
            "the clock did not move"
        );

        let answer = order(
            &state,
            "while-paused",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Defensive),
                ..Default::default()
            }),
        );
        assert!(answer.receipt.accepted, "{:?}", answer.receipt);

        set_interactive_pause_internal(&state, false, 0).unwrap();
        let moving = advance(&state, AdvanceTarget::ForMinutes(2));
        assert!(moving.snapshot.view.session.clock_ms > clock);
    }

    /// Given a club that has not delegated match-day tactics,
    /// When the assistant is switched on,
    /// Then it is refused with its key and the match is unaffected.
    #[test]
    fn the_assistant_is_refused_without_delegation() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        assert_eq!(
            set_interactive_assistant_internal(&state, true, 0).unwrap_err(),
            "be.error.interactive.assistantNotDelegated"
        );
        assert!(!snapshot(&state).view.assistant_active);
    }

    /// Given an illegal substitution (a goalkeeper coming on for an outfield player),
    /// When it is ordered,
    /// Then the engine's reason comes back, nothing is queued, and the order is not repeated
    /// when retried with the same id.
    #[test]
    fn an_illegal_substitution_is_refused_with_its_reason() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        begin(&state);
        // the engine knows who its goalkeeper is once play has started
        advance(&state, AdvanceTarget::ForMinutes(1));
        let view = snapshot(&state).view;
        let off = view
            .session
            .home
            .on_pitch
            .iter()
            .find(|p| !format!("{:?}", p.role).contains("Gk"))
            .expect("an outfield starter")
            .id
            .0
            .clone();
        let keeper = view
            .session
            .home
            .bench
            .iter()
            .find(|p| format!("{:?}", p.natural_position).contains("Gk"))
            .expect("a goalkeeper on the bench")
            .id
            .0
            .clone();
        let swap = || UserOrder::Substitute {
            player_off_id: off.clone(),
            player_on_id: keeper.clone(),
        };
        let first = order(&state, "illegal", swap());
        assert!(!first.receipt.accepted);
        assert_eq!(
            first.receipt.rejection.as_deref(),
            Some("goalkeeper_mismatch")
        );
        assert!(first
            .snapshot
            .view
            .session
            .home
            .pending_substitutions
            .is_empty());

        let retry = order(&state, "illegal", swap());
        assert!(retry.receipt.duplicate);
        assert_eq!(
            retry.receipt.rejection.as_deref(),
            Some("goalkeeper_mismatch")
        );
    }

    // ------------------------------------------------------------- saved mid-match

    /// A career with a saved id on disk and a match played to 20 minutes.
    fn career_mid_match(directory: &TempDir) -> (StateManager, SaveManager, String) {
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let mut saves = SaveManager::init(directory.path()).expect("save manager");
        let save_id = saves.create_save(&game, "mid-match").expect("save");
        state.set_save_id(save_id.clone());
        state.set_game(game);
        begin(&state);
        advance(&state, AdvanceTarget::ForMinutes(20));
        (state, saves, save_id)
    }

    /// Reloads the career the way `load_game` does, into a fresh application state.
    fn reload(saves: &mut SaveManager, save_id: &str) -> StateManager {
        let state = StateManager::new();
        let game = saves.load_game(save_id).expect("load");
        let stats = saves.load_stats_state(save_id).expect("stats");
        crate::application::career::install_career(&state, game, stats, Some(save_id.to_string()));
        restore_saved_match(&state, saves, save_id);
        state
    }

    /// Given a match 20 minutes in with an accepted order,
    /// When the career is saved and reloaded into a fresh state,
    /// Then the same match is there (clock, score, tactics), it plays on to a result, the fixture is
    /// finalised exactly once, and a later save holds no checkpoint.
    #[test]
    fn a_match_saved_in_progress_resumes_and_finishes_once() {
        let directory = TempDir::new().expect("dir");
        let (state, mut saves, save_id) = career_mid_match(&directory);
        let tactics = order(
            &state,
            "mentality",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                ..Default::default()
            }),
        );
        assert!(tactics.receipt.accepted);
        let before = snapshot(&state).view;
        crate::application::saving::persist_active_game(&state, &mut saves)
            .expect("a match in progress can be saved");

        let resumed = reload(&mut saves, &save_id);
        assert!(resumed.take_restore_notice().is_none());
        let after = snapshot(&resumed).view;
        assert_eq!(after.session.clock_ms, before.session.clock_ms);
        assert_eq!(after.session.score_home, before.session.score_home);
        assert_eq!(after.home_plan.style, PlayStyle::Attacking);

        to_full_time(&resumed);
        let finished = finish_interactive_match_internal(&resumed).expect("finishes");
        let _ = finished;
        assert_eq!(
            resumed
                .get_game(|g| g.competitions[0].fixtures[0].status.clone())
                .unwrap(),
            FixtureStatus::Completed
        );
        assert!(
            finish_interactive_match_internal(&resumed).is_err(),
            "no second finalisation"
        );

        crate::application::saving::persist_active_game(&resumed, &mut saves).expect("save");
        assert!(saves.load_interactive_match(&save_id).unwrap().is_none());
        // nothing large is kept: an interactive match records no frames and stores no replay
        assert!(
            saves
                .load_match_replay(&save_id, COMPETITION, FIXTURE)
                .map(|replay| replay.is_none())
                .unwrap_or(true),
            "no positional replay is stored for an interactive match"
        );
    }

    /// Given a match 20 minutes in with a substitution still waiting for a stoppage and a tactical
    /// order already in force,
    /// When the career is saved, reloaded into a fresh state and played to full time,
    /// Then the waiting substitution is still waiting, comes into force exactly once, the tactical
    /// order is still in force, and no event is recorded twice (the event stream before the save is a
    /// prefix of the one after it, and finalisation, which refuses a duplicated event, succeeds).
    #[test]
    fn a_pending_substitution_and_a_tactical_order_survive_a_reload_without_duplicates() {
        let directory = TempDir::new().expect("dir");
        let (state, mut saves, save_id) = career_mid_match(&directory);
        let tactics = order(
            &state,
            "press",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                ..Default::default()
            }),
        );
        assert!(tactics.receipt.accepted);
        let (off, on) = legal_swap(&snapshot(&state).view);
        let swap = order(
            &state,
            "swap",
            UserOrder::Substitute {
                player_off_id: off.clone(),
                player_on_id: on.clone(),
            },
        );
        assert!(swap.receipt.accepted, "{:?}", swap.receipt.rejection);
        let before = snapshot(&state);
        let waiting = before.view.session.home.pending_substitutions.clone();
        assert!(
            !waiting.is_empty(),
            "the scenario needs a substitution that is still waiting"
        );
        let events_before = before.next_event_cursor;
        crate::application::saving::persist_active_game(&state, &mut saves)
            .expect("a match with a waiting substitution can be saved");

        let resumed = reload(&mut saves, &save_id);
        assert!(resumed.take_restore_notice().is_none());
        let after = snapshot(&resumed);
        assert_eq!(
            after.view.session.home.pending_substitutions, waiting,
            "the substitution is still waiting, neither lost nor applied early"
        );
        assert_eq!(after.view.home_plan.style, PlayStyle::Attacking);
        assert_eq!(
            after.next_event_cursor, events_before,
            "restoring replays the match without emitting its events a second time"
        );

        to_full_time(&resumed);
        let report_events = snapshot(&resumed);
        let swaps = resumed
            .with_interactive_match(|m| {
                m.view()
                    .map(|v| v.session.home.substitutions_used)
                    .unwrap_or(0)
            })
            .expect("the match");
        assert_eq!(
            swaps, 1,
            "the waiting substitution came into force exactly once"
        );
        assert!(report_events
            .view
            .session
            .home
            .pending_substitutions
            .is_empty());
        finish_interactive_match_internal(&resumed)
            .expect("finalisation refuses a duplicated event, so this proves there is none");
    }

    /// Given interactive matches in which players are injured and sent off by the engine's referee,
    /// When they are finalised through the canonical path,
    /// Then injuries reach the career as injuries on the players hurt (and only on them), every
    /// dismissal is a red card in his season record, and every player sent off is suspended: three
    /// matches for a straight red, one for a second yellow, and nobody else.
    ///
    /// The seeds are scanned until the engine has produced both an injury and a dismissal on its
    /// own (the referee is calibrated to about one dismissal a six or seven matches), so nothing is
    /// forced and the test cannot pass by finding neither.
    #[test]
    fn injuries_and_dismissals_of_an_interactive_match_reach_the_career() {
        let (mut injuries_seen, mut injuries_in_career) = (0, 0);
        let (mut dismissals_seen, mut straight_reds, mut second_yellows) = (0, 0, 0);
        for seed in 1..=150u64 {
            let state = StateManager::new();
            let mut game = game_with_spatial_fixture();
            game.seed = seed;
            let reds_before: u32 = game.players.iter().map(|p| p.stats.red_cards).sum();
            state.set_game(game);
            begin(&state);
            to_full_time(&state);
            let events = snapshot(&state).events;
            let kinds: Vec<serde_json::Value> = events
                .iter()
                .map(|e| serde_json::to_value(&e.event.kind).expect("kind"))
                .collect();
            let injured_in_match: std::collections::BTreeSet<String> = kinds
                .iter()
                .filter(|k| k["type"] == "injury")
                .filter_map(|k| k["player"].as_str().map(str::to_string))
                .collect();
            // (player, colour) of every dismissal; a straight red is `red`, a second caution
            // `second_yellow`
            let dismissals: Vec<(String, String)> = kinds
                .iter()
                .filter(|k| k["type"] == "card" && k["colour"] != "yellow")
                .filter_map(|k| {
                    Some((
                        k["player"].as_str()?.to_string(),
                        k["colour"].as_str()?.to_string(),
                    ))
                })
                .collect();
            finish_interactive_match_internal(&state).expect("finishes");
            let (injured_after, reds_after, banned) = state
                .get_game(|g| {
                    (
                        g.players
                            .iter()
                            .filter(|p| p.injury.is_some())
                            .map(|p| p.id.clone())
                            .collect::<std::collections::BTreeSet<_>>(),
                        g.players.iter().map(|p| p.stats.red_cards).sum::<u32>(),
                        g.players
                            .iter()
                            .filter(|p| p.suspended_matches > 0)
                            .map(|p| (p.id.clone(), p.suspended_matches))
                            .collect::<std::collections::BTreeMap<_, _>>(),
                    )
                })
                .expect("game");
            // The club's medical staff catch some knocks before they become injuries (the same roll
            // for every match, `match_injuries`), so the career can hold fewer injuries than the
            // match recorded, never others.
            assert!(
                injured_after.is_subset(&injured_in_match),
                "seed {seed}: only players hurt in the match are injured in the career"
            );
            injuries_in_career += injured_after.len();
            injuries_seen += injured_in_match.len();
            let straight = dismissals.iter().filter(|(_, c)| c == "red").count() as u32;
            assert_eq!(
                reds_after - reds_before,
                dismissals.len() as u32,
                "seed {seed}: every dismissal is a red card on a record"
            );
            let expected: std::collections::BTreeMap<String, u8> = dismissals
                .iter()
                .map(|(id, colour)| {
                    (
                        id.clone(),
                        if colour == "red" {
                            ofm_core::suspension::DIRECT_RED_BAN
                        } else {
                            ofm_core::suspension::SECOND_YELLOW_BAN
                        },
                    )
                })
                .collect();
            assert_eq!(
                banned, expected,
                "seed {seed}: the players sent off, and only they, are suspended"
            );
            dismissals_seen += dismissals.len();
            straight_reds += straight as usize;
            second_yellows += dismissals.len() - straight as usize;
            if injuries_in_career > 0 && dismissals_seen > 0 && seed >= 40 {
                break;
            }
        }
        assert!(
            injuries_seen > 0,
            "the seeds must include injuries to prove anything"
        );
        assert!(
            injuries_in_career > 0,
            "injuries of an interactive match must reach the career, not all be caught"
        );
        assert!(
            dismissals_seen > 0,
            "the referee must have sent somebody off in these seeds, or this proves nothing about cards"
        );
        println!(
            "{injuries_seen} injuries in matches, {injuries_in_career} in the career; {dismissals_seen} dismissals ({straight_reds} straight, {second_yellows} second yellows)"
        );
    }

    /// Given an interactive match in which a defender puts the ball into his own net, produced by the
    /// engine on its own (the seeds are scanned until one does; nothing is forced),
    /// When it is finalised through the canonical path,
    /// Then the goal counts for the other side in the score; the player who scored it belongs to the
    /// side that conceded; nobody is credited with a goal, an assist or a scorer entry for it; the
    /// match history (the compact record kept on the fixture) names him; and the totals agree: goals
    /// credited to players plus own goals equal the goals in the score.
    #[test]
    fn a_natural_own_goal_is_credited_to_the_other_side_and_names_its_player() {
        for seed in 1..=400u64 {
            let state = StateManager::new();
            let mut game = game_with_spatial_fixture();
            game.seed = seed;
            state.set_game(game);
            begin(&state);
            to_full_time(&state);
            let own: Vec<(String, String)> = snapshot(&state)
                .events
                .iter()
                .filter_map(|e| {
                    let k = serde_json::to_value(&e.event.kind).ok()?;
                    (k["type"] == "goal" && k["own_goal"] == true).then(|| {
                        (
                            k["scorer"].as_str().unwrap_or_default().to_string(),
                            serde_json::to_value(e.event.side)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_string))
                                .unwrap_or_default(),
                        )
                    })
                })
                .collect();
            if own.is_empty() {
                continue;
            }
            finish_interactive_match_internal(&state).expect("finishes");
            let game = state.get_game(|g| g.clone()).expect("game");
            let fixture = &game.competitions[0].fixtures[0];
            let result = fixture.result.as_ref().expect("a result");
            let report = result.report.as_ref().expect("a stored report");
            let credited: u32 = game.players.iter().map(|p| p.stats.goals).sum();
            let scored = u32::from(result.home_goals) + u32::from(result.away_goals);
            assert_eq!(
                credited + own.len() as u32,
                scored,
                "seed {seed}: goals credited to players plus own goals are the goals in the score"
            );
            assert_eq!(
                result.home_scorers.len() + result.away_scorers.len(),
                scored as usize - own.len(),
                "seed {seed}: nobody is a scorer of an own goal"
            );
            for (player, benefiting_side) in &own {
                let club = game
                    .players
                    .iter()
                    .find(|p| &p.id == player)
                    .and_then(|p| p.team_id.clone())
                    .expect("the player belongs to a club");
                let conceding_club = if benefiting_side == "home" {
                    &fixture.away_team_id
                } else {
                    &fixture.home_team_id
                };
                assert_eq!(
                    &club, conceding_club,
                    "seed {seed}: he plays for the side that conceded"
                );
                assert!(
                    report
                        .events
                        .iter()
                        .any(|e| e.own_goal_by.as_deref() == Some(player.as_str())
                            && e.player_id.is_none()
                            && e.side.to_lowercase() == *benefiting_side),
                    "seed {seed}: the match history names {player} for the {benefiting_side} side"
                );
            }
            println!("natural own goal at seed {seed}: {own:?}");
            return;
        }
        panic!("no own goal in 400 seeds: the engine should produce some on its own");
    }

    /// Given a match saved 20 minutes in,
    /// Then the stored checkpoint is compact (inputs and accepted commands, not frames).
    #[test]
    fn a_stored_checkpoint_is_compact() {
        let directory = TempDir::new().expect("dir");
        let (state, mut saves, save_id) = career_mid_match(&directory);
        crate::application::saving::persist_active_game(&state, &mut saves).expect("save");
        let stored = saves
            .load_interactive_match(&save_id)
            .unwrap()
            .expect("a match is stored")
            .expect("and it is readable");
        assert!(
            stored.payload.len() < 400_000,
            "{} bytes at minute 20",
            stored.payload.len()
        );
    }

    /// Given a stored checkpoint that was altered on disk,
    /// When the career is loaded,
    /// Then it loads, no match is resumed, the player is told once, and the fixture is still
    /// scheduled so it can be played from kick-off.
    #[test]
    fn a_damaged_saved_match_is_discarded_with_one_notice() {
        let directory = TempDir::new().expect("dir");
        let (state, mut saves, save_id) = career_mid_match(&directory);
        crate::application::saving::persist_active_game(&state, &mut saves).expect("save");
        let db_file = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "db"))
            .expect("the save file");
        let database = db::game_database::GameDatabase::open_save(&db_file).unwrap();
        database
            .conn()
            .execute(
                "UPDATE interactive_match_checkpoint SET payload = payload || ' '",
                [],
            )
            .unwrap();
        drop(database);

        let resumed = reload(&mut saves, &save_id);
        assert!(resumed.with_interactive_match(|_| ()).is_none());
        assert_eq!(
            resumed.take_restore_notice().as_deref(),
            Some("be.error.interactive.savedMatchDiscarded")
        );
        assert!(resumed.take_restore_notice().is_none(), "told once");
        assert_eq!(
            resumed
                .get_game(|g| g.competitions[0].fixtures[0].status.clone())
                .unwrap(),
            FixtureStatus::Scheduled
        );
    }

    /// Given a career saved with no match in progress (every old save is like this),
    /// When it is loaded,
    /// Then nothing is restored and nothing is announced.
    #[test]
    fn a_career_without_a_saved_match_loads_as_before() {
        let directory = TempDir::new().expect("dir");
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let mut saves = SaveManager::init(directory.path()).expect("save manager");
        let save_id = saves.create_save(&game, "plain").expect("save");
        state.set_save_id(save_id.clone());
        state.set_game(game);
        crate::application::saving::persist_active_game(&state, &mut saves).expect("save");
        let loaded = reload(&mut saves, &save_id);
        assert!(loaded.with_interactive_match(|_| ()).is_none());
        assert!(loaded.with_live_match(|_| ()).is_none());
        assert!(loaded.take_restore_notice().is_none());
    }

    /// Given a live match that is not an interactive one,
    /// When a save is requested,
    /// Then it is still refused, as before.
    #[test]
    fn a_non_interactive_live_match_still_blocks_saving() {
        let directory = TempDir::new().expect("dir");
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let mut saves = SaveManager::init(directory.path()).expect("save manager");
        let save_id = saves.create_save(&game, "live").expect("save");
        state.set_save_id(save_id);
        state.set_game(game);
        start_live_match_with_identity(&state, 0, "live", false, Some(COMPETITION), Some(FIXTURE))
            .expect("live");
        assert_eq!(
            crate::application::saving::persist_active_game(&state, &mut saves).unwrap_err(),
            "be.error.liveMatch.inProgress"
        );
    }
}
