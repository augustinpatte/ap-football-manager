//! Tauri commands for managing a real M12 match: start, advance, order, read, pause, hand the
//! match to the assistant, and finish. Each one is an adapter over `application::interactive_match`.

use std::sync::Arc;

use ofm_core::interactive_match::{AdvanceRequest, UserCommand};
use ofm_core::state::StateManager;
use tauri::State;

use crate::application::interactive_match::{
    advance_interactive_match_internal, finish_interactive_match_internal,
    get_interactive_match_state_internal, set_interactive_assistant_internal,
    set_interactive_pause_internal, start_interactive_match_internal,
    submit_match_command_internal, AdvanceSnapshot, InteractiveSnapshot, OrderSnapshot,
};
use crate::application::live_match::FinishLiveMatchResponse;

/// Starts the interactive match of the fixture the live session selected.
#[tauri::command]
pub fn start_interactive_match(
    state: State<'_, Arc<StateManager>>,
) -> Result<InteractiveSnapshot, String> {
    start_interactive_match_internal(&state)
}

/// The match as it stands, and the key events from `event_cursor` on.
#[tauri::command]
pub fn get_interactive_match_state(
    state: State<'_, Arc<StateManager>>,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    get_interactive_match_state_internal(&state, event_cursor)
}

/// Plays on until the target, a stop (break, goal, dismissal, injury) or full time.
#[tauri::command]
pub fn advance_interactive_match(
    state: State<'_, Arc<StateManager>>,
    request: AdvanceRequest,
    event_cursor: usize,
) -> Result<AdvanceSnapshot, String> {
    advance_interactive_match_internal(&state, request, event_cursor)
}

/// An order from the manager: tactics, a formation, a substitution or its cancellation.
#[tauri::command]
pub fn submit_match_command(
    state: State<'_, Arc<StateManager>>,
    command: UserCommand,
    event_cursor: usize,
) -> Result<OrderSnapshot, String> {
    submit_match_command_internal(&state, command, event_cursor)
}

#[tauri::command]
pub fn set_interactive_pause(
    state: State<'_, Arc<StateManager>>,
    paused: bool,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    set_interactive_pause_internal(&state, paused, event_cursor)
}

#[tauri::command]
pub fn set_interactive_assistant(
    state: State<'_, Arc<StateManager>>,
    active: bool,
    event_cursor: usize,
) -> Result<InteractiveSnapshot, String> {
    set_interactive_assistant_internal(&state, active, event_cursor)
}

/// Applies the finished match to the career, through the one finalisation every match uses.
#[tauri::command]
pub fn finish_interactive_match(
    state: State<'_, Arc<StateManager>>,
) -> Result<FinishLiveMatchResponse, String> {
    finish_interactive_match_internal(&state)
}
