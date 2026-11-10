//! Advance the career to a chosen date. The work is `application::fast_forward`; this is the
//! Tauri surface: a command that runs it off the webview thread and reports progress as events.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::application::fast_forward::{self, FastForwardResponse};
use ofm_core::fast_forward::FastForwardRequest;
use ofm_core::state::StateManager;

/// Event carrying an `ofm_core::fast_forward::FastForwardProgress` while a run is in progress.
pub const FAST_FORWARD_PROGRESS_EVENT: &str = "fast-forward-progress";

pub fn fast_forward_to_date_internal(
    state: &StateManager,
    request: &FastForwardRequest,
    emit_progress: &mut dyn FnMut(&ofm_core::fast_forward::FastForwardProgress),
) -> Result<FastForwardResponse, String> {
    fast_forward::fast_forward(state, request, emit_progress)
}

/// Play every day up to `request.target_date`, as far as the policy allows. Runs on a blocking
/// worker so the days never freeze the webview, and tells it where the run stands through
/// [`FAST_FORWARD_PROGRESS_EVENT`].
#[tauri::command]
pub async fn fast_forward_to_date(
    app: AppHandle,
    state: State<'_, Arc<StateManager>>,
    request: FastForwardRequest,
) -> Result<FastForwardResponse, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        fast_forward_to_date_internal(&state, &request, &mut |progress| {
            let _ = app.emit(FAST_FORWARD_PROGRESS_EVENT, progress);
        })
    })
    .await
    .map_err(|err| format!("be.error.taskJoinFailed: {err}"))?
}

/// Ask the running fast-forward to stop at the next day boundary. `false` when none is running.
#[tauri::command]
pub fn cancel_fast_forward() -> bool {
    fast_forward::request_cancel()
}
