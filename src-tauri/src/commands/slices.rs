use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;

use domain::message::InboxMessage;
use ofm_core::player_knowledge::{players_knowledge, PlayerKnowledge};
use ofm_core::rating_view::{ratings_view, youth_prospect_readings, ProspectReading, RatingView};
use ofm_core::slices::competitions::{query_competitions, CompetitionsQuery, CompetitionsView};
use ofm_core::slices::inbox::{query_messages, MessagesQuery};
use ofm_core::slices::news::{query_news_feed, NewsFeed, NewsFeedQuery};
use ofm_core::slices::player_history::{query_player_history, PlayerHistory};
use ofm_core::slices::players::{query_page, PlayersPage, PlayersPageQuery};
use ofm_core::slices::schedule::{
    query_schedule, query_user_schedule, ScheduleQuery, ScheduleSlice, UserSchedule,
};
use ofm_core::slices::scouted_report::{query_scouted_report, ScoutedReportView};
use ofm_core::slices::session::{project_session, SessionState, SessionStateQuery};
use ofm_core::slices::squad::{query_squad, SquadPlayer};
use ofm_core::slices::staff::{query_staff, StaffSlice};
use ofm_core::slices::teams::{
    query_club_league_standing, query_directory, ClubLeagueStanding, TeamsDirectory,
    TeamsDirectoryQuery,
};
use ofm_core::state::StateManager;

const NO_ACTIVE_GAME: &str = "be.error.noActiveGameSession";

#[tauri::command]
pub async fn get_players_page(
    state: State<'_, Arc<StateManager>>,
    query: PlayersPageQuery,
) -> Result<PlayersPage, String> {
    state
        .get_game(|game| query_page(game, &query))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_teams_directory(
    state: State<'_, Arc<StateManager>>,
    query: TeamsDirectoryQuery,
) -> Result<TeamsDirectory, String> {
    state
        .get_game(|game| query_directory(game, &query))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_schedule(
    state: State<'_, Arc<StateManager>>,
    query: ScheduleQuery,
) -> Result<ScheduleSlice, String> {
    state
        .get_game(|game| query_schedule(game, &query))
        .flatten()
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

/// The manager's matches from every competition his club plays in, merged by date.
#[tauri::command]
pub async fn get_user_schedule(
    state: State<'_, Arc<StateManager>>,
) -> Result<UserSchedule, String> {
    state
        .get_game(query_user_schedule)
        .flatten()
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_news_feed(
    state: State<'_, Arc<StateManager>>,
    query: NewsFeedQuery,
) -> Result<NewsFeed, String> {
    state
        .get_game(|game| query_news_feed(game, &query))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_messages_page(
    state: State<'_, Arc<StateManager>>,
    query: MessagesQuery,
) -> Result<Vec<InboxMessage>, String> {
    state
        .get_game(|game| query_messages(game, &query))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_competitions_view(
    state: State<'_, Arc<StateManager>>,
    query: CompetitionsQuery,
) -> Result<CompetitionsView, String> {
    state
        .get_game(|game| query_competitions(game, &query))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_session_state(
    state: State<'_, Arc<StateManager>>,
    _query: SessionStateQuery,
) -> Result<SessionState, String> {
    state
        .get_game(project_session)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_club_league_standing(
    state: State<'_, Arc<StateManager>>,
    team_id: String,
) -> Result<Option<ClubLeagueStanding>, String> {
    state
        .get_game(|game| query_club_league_standing(game, &team_id))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_scouted_report(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Option<ScoutedReportView>, String> {
    state
        .get_game(|game| query_scouted_report(game, &player_id))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

/// The career and moves the game state leaves off every player; one profile loads its own.
#[tauri::command]
pub async fn get_player_history(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Option<PlayerHistory>, String> {
    state
        .get_game(|game| query_player_history(game, &player_id))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_player_knowledge(
    state: State<'_, Arc<StateManager>>,
    player_ids: Vec<String>,
) -> Result<HashMap<String, PlayerKnowledge>, String> {
    get_player_knowledge_internal(&state, &player_ids)
}

fn get_player_knowledge_internal(
    state: &StateManager,
    player_ids: &[String],
) -> Result<HashMap<String, PlayerKnowledge>, String> {
    state
        .get_game(|game| players_knowledge(game, player_ids))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

/// How the manager may read each player's overall rating: exact, a scout's range, or a public
/// standing. Screens that hold a player from the game read his rating here, never from `ovr`.
#[tauri::command]
pub async fn get_player_ratings(
    state: State<'_, Arc<StateManager>>,
    player_ids: Vec<String>,
) -> Result<HashMap<String, RatingView>, String> {
    get_player_ratings_internal(&state, &player_ids)
}

fn get_player_ratings_internal(
    state: &StateManager,
    player_ids: &[String],
) -> Result<HashMap<String, RatingView>, String> {
    state
        .get_game(|game| ratings_view(game, player_ids))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

/// How the manager reads each prospect a scout proposed in a youth recruitment message: broad
/// standings until the club signs him. An unknown message answers an empty list.
#[tauri::command]
pub async fn get_youth_prospect_readings(
    state: State<'_, Arc<StateManager>>,
    message_id: String,
) -> Result<Vec<ProspectReading>, String> {
    get_youth_prospect_readings_internal(&state, &message_id)
}

fn get_youth_prospect_readings_internal(
    state: &StateManager,
    message_id: &str,
) -> Result<Vec<ProspectReading>, String> {
    state
        .get_game(|game| youth_prospect_readings(game, message_id).unwrap_or_default())
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_squad(
    state: State<'_, Arc<StateManager>>,
    team_id: String,
) -> Result<Vec<SquadPlayer>, String> {
    get_squad_internal(&state, &team_id)
}

fn get_squad_internal(state: &StateManager, team_id: &str) -> Result<Vec<SquadPlayer>, String> {
    state
        .get_game(|game| query_squad(game, team_id))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn get_staff(
    state: State<'_, Arc<StateManager>>,
    team_id: String,
) -> Result<StaffSlice, String> {
    state
        .get_game(|game| query_staff(game, &team_id))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given no active game, when the roster adapter reads it,
    /// then the existing translated session error is returned.
    #[test]
    fn the_squad_adapter_reports_a_missing_session() {
        assert_eq!(
            get_squad_internal(&StateManager::new(), "club").unwrap_err(),
            NO_ACTIVE_GAME
        );
    }
    /// Given a thin club roster, when the IPC adapter reads the active session,
    /// then its flattened player data carries the backend eligibility verdict.
    #[test]
    fn the_squad_adapter_projects_call_ups_from_the_live_session() {
        use chrono::TimeZone;
        use domain::manager::Manager;
        use domain::player::{Player, PlayerAttributes, Position, SquadRole};
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 60, "stamina": 60, "strength": 60, "agility": 60, "passing": 60,
            "shooting": 60, "tackling": 60, "dribbling": 60, "defending": 60,
            "positioning": 60, "vision": 60, "decisions": 60, "composure": 60,
            "aggression": 60, "teamwork": 60, "leadership": 60,
            "handling": 60, "reflexes": 60, "aerial": 60
        }))
        .unwrap();
        let mut keeper = Player::new(
            "keeper".into(),
            "Keeper".into(),
            "Senior Keeper".into(),
            "2000-01-01".into(),
            "GB".into(),
            Position::Goalkeeper,
            attrs.clone(),
        );
        keeper.team_id = Some("club".into());
        let mut youth = Player::new(
            "youth".into(),
            "Youth".into(),
            "Called Youth".into(),
            "2008-01-01".into(),
            "GB".into(),
            Position::Forward,
            attrs,
        );
        youth.team_id = Some("club".into());
        youth.squad_role = SquadRole::Youth;
        let state = StateManager::new();
        state.set_game(ofm_core::game::Game::new(
            ofm_core::clock::GameClock::new(
                chrono::Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
            ),
            Manager::new(
                "m".into(),
                "M".into(),
                "Gr".into(),
                "1980-01-01".into(),
                "GB".into(),
            ),
            vec![],
            vec![keeper, youth],
            vec![],
            vec![],
        ));
        let value = serde_json::to_value(get_squad_internal(&state, "club").unwrap()).unwrap();
        let rows = value.as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let youth = rows.iter().find(|p| p["id"] == "youth").unwrap();
        assert_eq!(youth["match_day_eligible"], true);
        assert_eq!(youth["squad_role"], "Youth");
        assert!(get_squad_internal(&state, "other-club").unwrap().is_empty());
    }

    /// Given no active game, when the prospect adapter is asked, then it reports the missing
    /// session, and with a game but no such message it answers an empty list.
    #[test]
    fn the_prospect_adapter_reports_a_missing_session_and_an_unknown_message() {
        assert_eq!(
            get_youth_prospect_readings_internal(&StateManager::new(), "m").unwrap_err(),
            NO_ACTIVE_GAME
        );
        let state = StateManager::new();
        state.set_game(ofm_core::game::Game::new(
            ofm_core::clock::GameClock::new(chrono::Utc::now()),
            domain::manager::Manager::new(
                "m".into(),
                "M".into(),
                "G".into(),
                "1980-01-01".into(),
                "GB".into(),
            ),
            vec![],
            vec![],
            vec![],
            vec![],
        ));
        assert!(get_youth_prospect_readings_internal(&state, "nope")
            .unwrap()
            .is_empty());
    }

    /// Given a stranger a scout reported on (an old report, a wide range) and a youth message that
    ///       proposes one prospect
    /// When the IPC adapters answer
    /// Then the wire carries the shapes the screens read: `estimate` with `low`, `high`,
    ///      `confidence`, `current` and `scouted_on`, and a prospect with `player_id`, `current`
    ///      and `ceiling` as standings, with neither true number anywhere
    #[test]
    fn the_adapters_pin_the_estimate_and_prospect_wire_shapes() {
        use chrono::TimeZone;
        use domain::manager::Manager;
        use domain::player::{Player, PlayerAttributes, Position};
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 60, "stamina": 60, "strength": 60, "agility": 60, "passing": 60,
            "shooting": 60, "tackling": 60, "dribbling": 60, "defending": 60,
            "positioning": 60, "vision": 60, "decisions": 60, "composure": 60,
            "aggression": 60, "teamwork": 60, "leadership": 60,
            "handling": 60, "reflexes": 60, "aerial": 60
        }))
        .unwrap();
        let make = |id: &str, ovr: u8, potential: u8| {
            let mut player = Player::new(
                id.into(),
                id.into(),
                id.into(),
                "2000-01-01".into(),
                "GB".into(),
                Position::Forward,
                attrs.clone(),
            );
            player.team_id = Some("elsewhere".into());
            player.ovr = ovr;
            player.potential = potential;
            player
        };
        let mut manager = Manager::new(
            "m".into(),
            "M".into(),
            "Gr".into(),
            "1980-01-01".into(),
            "GB".into(),
        );
        manager.team_id = Some("club".into());
        let mut game = ofm_core::game::Game::new(
            ofm_core::clock::GameClock::new(
                chrono::Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
            ),
            manager,
            vec![],
            vec![make("seen", 74, 80)],
            vec![],
            vec![],
        );
        game.season_context.season_start = Some("2026-07-01".into());
        game.scouting_dossier
            .reports
            .push(ofm_core::scouting_dossier::ScoutReport {
                player_id: "seen".into(),
                scout_id: "s".into(),
                scout_name: "S".into(),
                scouted_on: "2026-03-10".into(),
                judging_ability: 40,
                judging_potential: 40,
                reports: 1,
                ability: ofm_core::scouting_dossier::Range { low: 64, high: 82 },
                potential: None,
                age: 26,
                strengths: vec![],
                weaknesses: vec![],
            });
        let mut message = domain::message::InboxMessage::new(
            "youth-1".into(),
            String::new(),
            String::new(),
            "Scout".into(),
            "2026-08-01".into(),
        );
        let mut prospect = make("prospect", 52, 78);
        prospect.team_id = None;
        message.context.youth_prospects = Some(vec![prospect]);
        game.messages.push(message);
        let state = StateManager::new();
        state.set_game(game);

        let ratings =
            serde_json::to_value(get_player_ratings_internal(&state, &["seen".into()]).unwrap())
                .unwrap();
        let prospects =
            serde_json::to_value(get_youth_prospect_readings_internal(&state, "youth-1").unwrap())
                .unwrap();

        assert_eq!(
            ratings,
            serde_json::json!({ "seen": {
                "kind": "estimate", "low": 64, "high": 82, "confidence": "low",
                "current": false, "scouted_on": "2026-03-10"
            }})
        );
        assert_eq!(
            prospects,
            serde_json::json!([{
                "player_id": "prospect",
                "current": { "kind": "standing", "standing": "limited" },
                "ceiling": { "kind": "standing", "standing": "strong" }
            }])
        );
    }

    /// Given no active game, when the ratings adapter is asked, then it reports the missing session.
    #[test]
    fn the_ratings_adapter_reports_a_missing_session() {
        assert_eq!(
            get_player_ratings_internal(&StateManager::new(), &["p".to_string()]).unwrap_err(),
            NO_ACTIVE_GAME
        );
    }

    /// Given the manager's own player and a stranger at a club outside his division,
    /// when the IPC adapter is asked about both and an id no player has,
    /// then the screen receives the wire shape it reads: `exact` with the number for his own
    /// player, `standing` with a label and no number for the stranger, nothing for the stray id.
    #[test]
    fn the_ratings_adapter_answers_exact_for_his_own_and_a_standing_for_a_stranger() {
        use chrono::TimeZone;
        use domain::manager::Manager;
        use domain::player::{Player, PlayerAttributes, Position};
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 60, "stamina": 60, "strength": 60, "agility": 60, "passing": 60,
            "shooting": 60, "tackling": 60, "dribbling": 60, "defending": 60,
            "positioning": 60, "vision": 60, "decisions": 60, "composure": 60,
            "aggression": 60, "teamwork": 60, "leadership": 60,
            "handling": 60, "reflexes": 60, "aerial": 60
        }))
        .unwrap();
        let make = |id: &str, club: &str, ovr: u8| {
            let mut player = Player::new(
                id.into(),
                id.into(),
                id.into(),
                "2000-01-01".into(),
                "GB".into(),
                Position::Forward,
                attrs.clone(),
            );
            player.team_id = Some(club.into());
            player.ovr = ovr;
            player
        };
        let mut manager = Manager::new(
            "m".into(),
            "M".into(),
            "Gr".into(),
            "1980-01-01".into(),
            "GB".into(),
        );
        manager.team_id = Some("club".into());
        let game = ofm_core::game::Game::new(
            ofm_core::clock::GameClock::new(
                chrono::Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
            ),
            manager,
            vec![],
            vec![make("own", "club", 71), make("stranger", "elsewhere", 74)],
            vec![],
            vec![],
        );
        let state = StateManager::new();
        state.set_game(game);

        let answer = get_player_ratings_internal(
            &state,
            &["own".into(), "stranger".into(), "nobody".into()],
        );
        let value = serde_json::to_value(answer.unwrap()).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "own": { "kind": "exact", "ovr": 71 },
                "stranger": { "kind": "standing", "standing": "good" },
            })
        );
    }

    /// Given no active game, when the knowledge adapter reads it,
    /// then the translated session error is returned.
    #[test]
    fn the_knowledge_adapter_reports_a_missing_session() {
        assert_eq!(
            get_player_knowledge_internal(&StateManager::new(), &["p".to_string()]).unwrap_err(),
            NO_ACTIVE_GAME
        );
    }

    /// Given the manager's own player, a free agent a scout reported on, and an id no player has,
    /// when the IPC adapter is asked about all three,
    /// then the screen receives the exact wire shape it reads: the own player tagged `own`, the
    /// scouted one with `scouted_on` and `current`, and nothing for the stray id.
    #[test]
    fn the_knowledge_adapter_answers_from_the_live_session() {
        use chrono::TimeZone;
        use domain::manager::Manager;
        use domain::player::{Player, PlayerAttributes, Position};
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 60, "stamina": 60, "strength": 60, "agility": 60, "passing": 60,
            "shooting": 60, "tackling": 60, "dribbling": 60, "defending": 60,
            "positioning": 60, "vision": 60, "decisions": 60, "composure": 60,
            "aggression": 60, "teamwork": 60, "leadership": 60,
            "handling": 60, "reflexes": 60, "aerial": 60
        }))
        .unwrap();
        let mut own = Player::new(
            "own".into(),
            "Own".into(),
            "Own Player".into(),
            "2000-01-01".into(),
            "GB".into(),
            Position::Midfielder,
            attrs.clone(),
        );
        own.team_id = Some("club".into());
        let free_agent = Player::new(
            "free".into(),
            "Free".into(),
            "Free Agent".into(),
            "2000-01-01".into(),
            "GB".into(),
            Position::Forward,
            attrs.clone(),
        );
        let mut manager = Manager::new(
            "m".into(),
            "M".into(),
            "Gr".into(),
            "1980-01-01".into(),
            "GB".into(),
        );
        manager.team_id = Some("club".into());
        let mut game = ofm_core::game::Game::new(
            ofm_core::clock::GameClock::new(
                chrono::Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
            ),
            manager,
            vec![],
            vec![own, free_agent],
            vec![],
            vec![],
        );
        game.scouted_players.push(ofm_core::game::ScoutedPlayer {
            player_id: "free".into(),
            scouted_on: "2026-07-20".into(),
            attributes: attrs,
        });
        let state = StateManager::new();
        state.set_game(game);

        let answer =
            get_player_knowledge_internal(&state, &["own".into(), "free".into(), "nobody".into()]);
        let value = serde_json::to_value(answer.unwrap()).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "own": { "kind": "own" },
                "free": { "kind": "scouted", "scouted_on": "2026-07-20", "current": true },
            })
        );
    }
}
