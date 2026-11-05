use db::save_manager::SaveManager;
use ofm_core::state::StateManager;
use std::collections::HashSet;
use std::sync::MutexGuard;

/// Keep exit atomic with start/finish and career replacement, including unsaved careers.
pub(crate) fn exit_to_menu(
    state: &StateManager,
    save_manager: &mut SaveManager,
) -> Result<bool, String> {
    let operation = super::live_session::savable_operation(state)?;
    let saved = state.get_save_id().is_some_and(|id| !id.is_empty());
    if saved {
        persist_game_under_operation(state, save_manager, &operation)?;
    }
    state.clear_game();
    state.clear_save_id();
    Ok(saved)
}

/// Persist the active career under the live-session idle gate.
pub fn persist_active_game(
    state: &StateManager,
    save_manager: &mut SaveManager,
) -> Result<(), String> {
    let operation = super::live_session::savable_operation(state)?;
    persist_game_under_operation(state, save_manager, &operation)
}

// Ordinary saves and save-on-exit snapshot dirty journal ids before I/O, then
// remove only the flushed ids so concurrent new posts remain dirty. The guard
// token keeps this shared flush protocol inside its caller's idle gate.
fn persist_game_under_operation(
    state: &StateManager,
    save_manager: &mut SaveManager,
    _operation: &MutexGuard<'_, ()>,
) -> Result<(), String> {
    let save_id = state
        .get_save_id()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "be.error.noActiveSaveSession".to_string())?;
    let (game, flushed, folded) = state
        .get_game(|game| {
            (
                game.clone(),
                game.cash_journal_dirty_ids.clone(),
                game.cash_journal_fold_pending.clone(),
            )
        })
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())?;
    let stats = state
        .get_stats_state(|stats| stats.clone())
        .unwrap_or_default();
    let in_progress = match_in_progress(state)?;
    save_manager.save_game_with_stats_and_match(&game, &stats, &save_id, in_progress.as_ref())?;
    let flushed: HashSet<String> = flushed.into_iter().collect();
    state.update_game(|live| {
        live.cash_journal_dirty_ids
            .retain(|id| !flushed.contains(id));
        // Only the fold this save carried: a later rollover's is still owed to the file.
        if live.cash_journal_fold_pending == folded {
            live.cash_journal_fold_pending = None;
        }
    });
    Ok(())
}

/// The interactive match to store with the career, or `None` when no match is being played (which
/// clears any checkpoint an earlier save left, so a finished match is never resumable).
fn match_in_progress(
    state: &StateManager,
) -> Result<Option<db::interactive_match_repo::StoredMatch>, String> {
    use ofm_core::interactive_match::saved::{self, RoundBaselines};
    let Some(baselines) = state.with_live_match(|session| RoundBaselines::of(session)) else {
        return Ok(None);
    };
    let stored = state.with_interactive_match(|m| {
        saved::encode(m, baselines).map(|payload| db::interactive_match_repo::StoredMatch {
            competition_id: m.identity().competition_id.clone(),
            fixture_id: m.identity().fixture_id.clone(),
            payload,
        })
    });
    match stored {
        None => Ok(None),
        Some(result) => result.map(Some).map_err(|e| e.translation_key()),
    }
}
