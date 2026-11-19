//! Tauri commands for the scouting dossier, the shortlist and the academy review. Each is a thin
//! adapter over an `_internal` function so the MCP tools can share it.

use log::info;
use std::sync::Arc;
use tauri::State;

use ofm_core::academy_review::{academy_review, AcademyReview};
use ofm_core::game::Game;
use ofm_core::scouting_view::{dossier_view, targets_view, DossierView, ScoutingTargetView};
use ofm_core::state::StateManager;

use crate::commands::util::mutate_active_game;

const NO_ACTIVE_GAME: &str = "be.error.noActiveGameSession";

pub fn get_scouting_dossier_internal(state: &StateManager) -> Result<DossierView, String> {
    state
        .get_game(dossier_view)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

pub fn get_scouting_targets_internal(
    state: &StateManager,
    player_ids: &[String],
) -> Result<Vec<ScoutingTargetView>, String> {
    state
        .get_game(|game| targets_view(game, player_ids))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

pub fn get_academy_review_internal(state: &StateManager) -> Result<Option<AcademyReview>, String> {
    state
        .get_game(academy_review)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

pub fn shortlist_player_internal(state: &StateManager, player_id: &str) -> Result<Game, String> {
    mutate_active_game(state, |game| {
        ofm_core::scouting_dossier::shortlist_add(game, player_id)
    })
}

pub fn unshortlist_player_internal(state: &StateManager, player_id: &str) -> Result<Game, String> {
    mutate_active_game(state, |game| {
        ofm_core::scouting_dossier::shortlist_remove(game, player_id)
    })
}

pub fn cancel_scouting_internal(state: &StateManager, assignment_id: &str) -> Result<Game, String> {
    mutate_active_game(state, |game| {
        ofm_core::scouting::cancel_scouting(game, assignment_id)
    })
}

pub fn reassign_scouting_internal(
    state: &StateManager,
    assignment_id: &str,
    scout_id: &str,
) -> Result<Game, String> {
    mutate_active_game(state, |game| {
        ofm_core::scouting::reassign_scouting(game, assignment_id, scout_id)
    })
}

#[tauri::command]
pub async fn get_scouting_dossier(
    state: State<'_, Arc<StateManager>>,
) -> Result<DossierView, String> {
    get_scouting_dossier_internal(&state)
}

#[tauri::command]
pub async fn get_scouting_targets(
    state: State<'_, Arc<StateManager>>,
    player_ids: Vec<String>,
) -> Result<Vec<ScoutingTargetView>, String> {
    get_scouting_targets_internal(&state, &player_ids)
}

#[tauri::command]
pub async fn get_academy_review(
    state: State<'_, Arc<StateManager>>,
) -> Result<Option<AcademyReview>, String> {
    get_academy_review_internal(&state)
}

#[tauri::command]
pub fn shortlist_player(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Game, String> {
    info!("[cmd] shortlist_player: player_id={player_id}");
    shortlist_player_internal(&state, &player_id)
}

#[tauri::command]
pub fn unshortlist_player(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Game, String> {
    info!("[cmd] unshortlist_player: player_id={player_id}");
    unshortlist_player_internal(&state, &player_id)
}

#[tauri::command]
pub fn cancel_scouting(
    state: State<'_, Arc<StateManager>>,
    assignment_id: String,
) -> Result<Game, String> {
    info!("[cmd] cancel_scouting: assignment_id={assignment_id}");
    cancel_scouting_internal(&state, &assignment_id)
}

#[tauri::command]
pub fn reassign_scouting(
    state: State<'_, Arc<StateManager>>,
    assignment_id: String,
    scout_id: String,
) -> Result<Game, String> {
    info!("[cmd] reassign_scouting: assignment_id={assignment_id}, scout_id={scout_id}");
    reassign_scouting_internal(&state, &assignment_id, &scout_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use domain::manager::Manager;
    use domain::player::{Player, PlayerAttributes, Position};

    fn state_with_a_target() -> StateManager {
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 60, "stamina": 60, "strength": 60, "agility": 60, "passing": 60,
            "shooting": 60, "tackling": 60, "dribbling": 60, "defending": 60,
            "positioning": 60, "vision": 60, "decisions": 60, "composure": 60,
            "aggression": 60, "teamwork": 60, "leadership": 60,
            "handling": 60, "reflexes": 60, "aerial": 60
        }))
        .unwrap();
        let mut target = Player::new(
            "target".into(),
            "T".into(),
            "Target".into(),
            "2003-01-01".into(),
            "GB".into(),
            Position::Forward,
            attrs,
        );
        target.team_id = Some("other".into());
        let mut manager = Manager::new("m".into(), "M".into(), "Gr".into(), "1980-01-01".into(), "GB".into());
        manager.team_id = Some("club".into());
        let game = Game::new(
            ofm_core::clock::GameClock::new(chrono::Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()),
            manager,
            vec![],
            vec![target],
            vec![],
            vec![],
        );
        let state = StateManager::new();
        state.set_game(game);
        state
    }

    /// Given no active game
    /// When any scouting command is asked
    /// Then it answers with the session key, not a panic or an empty page.
    #[test]
    fn every_command_reports_a_missing_session() {
        let state = StateManager::new();
        assert_eq!(get_scouting_dossier_internal(&state).unwrap_err(), NO_ACTIVE_GAME);
        assert_eq!(get_academy_review_internal(&state).unwrap_err(), NO_ACTIVE_GAME);
        assert_eq!(shortlist_player_internal(&state, "x").unwrap_err(), NO_ACTIVE_GAME);
    }

    /// Given a live game with one target
    /// When he is shortlisted through the command adapter and then removed
    /// Then the returned game and the dossier read the same, and a second removal is refused with
    ///      the translation key the screen shows.
    #[test]
    fn the_shortlist_round_trips_through_the_adapter() {
        let state = state_with_a_target();
        let game = shortlist_player_internal(&state, "target").unwrap();
        assert_eq!(game.scouting_dossier.shortlist.len(), 1);
        let view = get_scouting_dossier_internal(&state).unwrap();
        assert_eq!(view.shortlist.len(), 1);
        assert!(view.shortlist[0].shortlisted);
        unshortlist_player_internal(&state, "target").unwrap();
        assert_eq!(
            unshortlist_player_internal(&state, "target").unwrap_err(),
            "be.error.scouting.shortlistNotListed"
        );
    }
}
