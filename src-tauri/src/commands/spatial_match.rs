use crate::{SaveManagerState, SAVE_MANAGER_UNAVAILABLE_ERROR};
use ofm_core::state::StateManager;
use ofm_core::{game::Game, spatial_runner};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct SpatialMatchResponse {
    pub game: Game,
    pub home_goals: u8,
    pub away_goals: u8,
    pub events: Vec<engine::MatchEvent>,
}

#[derive(Serialize)]
pub struct SpatialReplayResponse {
    pub replay_json: String,
}

fn selected_fixture(state: &StateManager) -> Result<(usize, String, String), String> {
    state
        .with_live_match(|session| {
            (
                session.fixture_index,
                session.competition_id.clone(),
                session.fixture_id.clone(),
            )
        })
        .ok_or("be.error.noActiveLiveMatch".to_string())
}

fn persist_replay(
    state: &StateManager,
    save_manager: &SaveManagerState,
    competition_id: &str,
    fixture_id: &str,
    replay_json: &str,
) {
    let Some(save_id) = state.get_save_id() else {
        return;
    };
    let Ok(mut manager) = save_manager.0.lock() else {
        log::warn!("M12 replay was not persisted: save manager unavailable");
        return;
    };
    if let Err(error) = manager.save_match_replay(&save_id, competition_id, fixture_id, replay_json)
    {
        log::warn!("M12 replay was not persisted for {competition_id}/{fixture_id}: {error}");
    }
}

/// Runs the already selected fixture with M12.
///
/// M12 and the contract projection run against a read-only game snapshot. The
/// active session is consumed only once a complete native report exists, so a
/// failed simulation leaves the player at the pre-match screen.
#[tauri::command]
pub fn simulate_spatial_match(
    state: State<'_, Arc<StateManager>>,
    save_manager: State<'_, Arc<SaveManagerState>>,
) -> Result<SpatialMatchResponse, String> {
    // Hold the application-wide gate through M12's read-only execution and
    // the final write. A second click, save, or ordinary live-match finish
    // cannot consume the same session between those two phases.
    let _operation = crate::application::live_session::operation();
    let (fixture_index, competition_id, fixture_id) = selected_fixture(&state)?;
    let prepared = state
        .get_game(|game| {
            spatial_runner::build_selected_fixture_replay(
                game,
                fixture_index,
                &competition_id,
                &fixture_id,
            )
        })
        .ok_or("be.error.noActiveGameSession")?
        .map_err(|error| error.to_string())?;
    let finalized =
        crate::application::live_match::finish_spatial_match(&state, prepared.report.clone())?;
    persist_replay(
        &state,
        &save_manager,
        &competition_id,
        &fixture_id,
        &prepared.replay_json,
    );
    Ok(SpatialMatchResponse {
        home_goals: prepared.report.home_goals,
        away_goals: prepared.report.away_goals,
        events: prepared.report.events,
        game: finalized.game,
    })
}

/// Builds one deterministic M12 replay for the active fixture without applying it.
#[tauri::command]
pub fn prepare_spatial_replay(
    state: State<'_, Arc<StateManager>>,
) -> Result<SpatialReplayResponse, String> {
    let _operation = crate::application::live_session::operation();
    let (fixture_index, competition_id, fixture_id) = selected_fixture(&state)?;
    let prepared = state
        .get_game(|game| {
            spatial_runner::build_selected_fixture_replay(
                game,
                fixture_index,
                &competition_id,
                &fixture_id,
            )
        })
        .ok_or("be.error.noActiveGameSession")?
        .map_err(|error| error.to_string())?;
    let replay_json = prepared.replay_json.clone();
    state.set_spatial_replay(prepared);
    Ok(SpatialReplayResponse { replay_json })
}

/// Applies the exact replay prepared for the 2D viewer and stores it if possible.
#[tauri::command]
pub fn finish_spatial_replay(
    state: State<'_, Arc<StateManager>>,
    save_manager: State<'_, Arc<SaveManagerState>>,
) -> Result<SpatialMatchResponse, String> {
    let _operation = crate::application::live_session::operation();
    let (_, competition_id, fixture_id) = selected_fixture(&state)?;
    let prepared = state
        .take_spatial_replay()
        .ok_or("be.error.spatialReplay.notPrepared")?;
    let finalized =
        match crate::application::live_match::finish_spatial_match(&state, prepared.report.clone())
        {
            Ok(response) => response,
            Err(error) => {
                state.set_spatial_replay(prepared);
                return Err(error);
            }
        };
    persist_replay(
        &state,
        &save_manager,
        &competition_id,
        &fixture_id,
        &prepared.replay_json,
    );
    Ok(SpatialMatchResponse {
        home_goals: prepared.report.home_goals,
        away_goals: prepared.report.away_goals,
        events: prepared.report.events,
        game: finalized.game,
    })
}

/// Retrieves a stored replay. Invalid JSON is returned for viewer validation, never re-simulated.
#[tauri::command]
pub fn load_spatial_replay(
    state: State<'_, Arc<StateManager>>,
    save_manager: State<'_, Arc<SaveManagerState>>,
    competition_id: String,
    fixture_id: String,
) -> Result<Option<SpatialReplayResponse>, String> {
    let save_id = state.get_save_id().ok_or("be.error.noActiveSaveSession")?;
    let mut manager = save_manager
        .0
        .lock()
        .map_err(|_| SAVE_MANAGER_UNAVAILABLE_ERROR.to_string())?;
    manager
        .load_match_replay(&save_id, &competition_id, &fixture_id)
        .map(|replay_json| replay_json.map(|replay_json| SpatialReplayResponse { replay_json }))
}

#[cfg(test)]
mod tests {
    use super::persist_replay;
    use crate::{
        application::live_match::{finish_spatial_match, start_live_match_with_identity},
        SaveManagerState,
    };
    use chrono::{TimeZone, Utc};
    use db::save_manager::SaveManager;
    use domain::{
        league::{Fixture, FixtureCompetition, FixtureStatus, League, StandingEntry},
        manager::Manager,
        player::{Player, PlayerAttributes, Position},
        team::Team,
    };
    use ofm_core::{
        clock::GameClock,
        game::Game,
        spatial_match_projection::{
            project, ProjectionContext, ProjectionError, SpatialEndStatus, SpatialMatchResult,
            SpatialScore, SpatialTeamStats,
        },
        spatial_runner::build_selected_fixture_replay,
        state::StateManager,
    };
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn attributes(position: Position) -> PlayerAttributes {
        let goalkeeper = matches!(position, Position::Goalkeeper);
        PlayerAttributes {
            pace: 68,
            stamina: 72,
            strength: 67,
            agility: 69,
            passing: 70,
            shooting: if goalkeeper { 25 } else { 71 },
            tackling: if goalkeeper { 25 } else { 66 },
            dribbling: if goalkeeper { 25 } else { 69 },
            defending: if goalkeeper { 25 } else { 66 },
            positioning: 68,
            vision: 69,
            decisions: 70,
            composure: 71,
            aggression: 52,
            teamwork: 73,
            leadership: 60,
            handling: if goalkeeper { 78 } else { 20 },
            reflexes: if goalkeeper { 78 } else { 20 },
            aerial: 68,
        }
    }

    fn player(id: String, team_id: &str, position: Position) -> Player {
        let mut player = Player::new(
            id.clone(),
            id.clone(),
            id,
            "1995-01-01".into(),
            "England".into(),
            position.clone(),
            attributes(position),
        );
        player.team_id = Some(team_id.into());
        player.condition = 94;
        player.morale = 76;
        player.fitness = 88;
        player
    }

    fn squad(team_id: &str) -> Vec<Player> {
        let positions = [
            Position::Goalkeeper,
            Position::Defender,
            Position::Defender,
            Position::Defender,
            Position::Defender,
            Position::Midfielder,
            Position::Midfielder,
            Position::Midfielder,
            Position::Midfielder,
            Position::Forward,
            Position::Forward,
            Position::Goalkeeper,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
        ];
        positions
            .into_iter()
            .enumerate()
            .map(|(index, position)| player(format!("{team_id}-p{index}"), team_id, position))
            .collect()
    }

    fn team(id: &str, name: &str) -> Team {
        Team::new(
            id.into(),
            name.into(),
            id[..3].into(),
            "England".into(),
            "London".into(),
            "Test Ground".into(),
            25_000,
        )
    }

    fn game_with_spatial_fixture() -> Game {
        let mut manager = Manager::new(
            "manager".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.hire("home".into());
        let home = team("home", "Home FC");
        let away = team("away", "Away FC");
        let fixture = Fixture {
            id: "fixture-m12-1".into(),
            competition_id: "competition-m12-1".into(),
            matchday: 1,
            date: "2025-06-15".into(),
            home_team_id: home.id.clone(),
            away_team_id: away.id.clone(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            result: None,
            postponement: None,
        };
        let competition = League {
            id: "competition-m12-1".into(),
            name: "M12 Test League".into(),
            season: 1,
            fixtures: vec![fixture],
            standings: vec![
                StandingEntry::new(home.id.clone()),
                StandingEntry::new(away.id.clone()),
            ],
            ..League::default()
        };
        let mut players = squad(&home.id);
        players.extend(squad(&away.id));
        let mut game = Game::new(
            GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()),
            manager,
            vec![home, away],
            players,
            vec![],
            vec![],
        );
        game.seed = 73_926;
        game.league = Some(competition.clone());
        game.competitions = vec![competition];
        game
    }

    fn prepare_session(state: &StateManager) {
        start_live_match_with_identity(
            state,
            0,
            "instant",
            false,
            Some("competition-m12-1"),
            Some("fixture-m12-1"),
        )
        .expect("production match start");
    }

    fn prepared_replay(state: &StateManager) -> ofm_core::spatial_runner::PreparedSpatialReplay {
        state
            .get_game(|game| {
                build_selected_fixture_replay(game, 0, "competition-m12-1", "fixture-m12-1")
            })
            .expect("active game")
            .expect("production M12 simulation and projection")
    }

    /// Given a complete saved career and an M12 report,
    /// when the official finalisation path writes the replay and the career is reloaded,
    /// then the fixture, score, standings, cumulative player statistics and exact replay survive.
    #[test]
    fn a_finalized_m12_match_and_replay_survive_a_fresh_sqlite_reload() {
        let directory = TempDir::new().expect("temporary saves directory");
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let mut manager = SaveManager::init(directory.path()).expect("save manager");
        let save_id = manager
            .create_save(&game, "M12 integration")
            .expect("create save");
        state.set_save_id(save_id.clone());
        state.set_game(game);
        prepare_session(&state);
        let prepared = prepared_replay(&state);
        let expected_report = prepared.report.clone();

        let finalized =
            finish_spatial_match(&state, prepared.report).expect("official finalisation");
        let save_manager = SaveManagerState(Mutex::new(manager));
        persist_replay(
            &state,
            &save_manager,
            "competition-m12-1",
            "fixture-m12-1",
            &prepared.replay_json,
        );
        save_manager
            .0
            .lock()
            .expect("save manager lock")
            .save_game(&finalized.game, &save_id)
            .expect("save finalized career");
        drop(save_manager);

        let mut reader = SaveManager::init(directory.path()).expect("fresh save manager");
        let reloaded = reader
            .load_game(&save_id)
            .expect("load through SaveManager");
        let fixture = &reloaded.competitions[0].fixtures[0];
        let result = fixture.result.as_ref().expect("persisted match result");
        assert_eq!(fixture.status, FixtureStatus::Completed);
        assert_eq!(result.home_goals, expected_report.home_goals);
        assert_eq!(result.away_goals, expected_report.away_goals);
        assert_eq!(
            reloaded.competitions[0]
                .standings
                .iter()
                .map(|standing| standing.played)
                .sum::<u32>(),
            2,
            "both teams have one cumulative league appearance"
        );
        assert!(
            reloaded
                .players
                .iter()
                .any(|player| player.stats.appearances > 0),
            "the finalized report updated cumulative player statistics"
        );
        assert_eq!(
            reader
                .load_match_replay(&save_id, "competition-m12-1", "fixture-m12-1")
                .expect("load stored replay"),
            Some(prepared.replay_json),
            "loading a replay reads the stored M12 document instead of simulating a new match"
        );
    }

    /// Given an M12 report for a selected fixture,
    /// when finalisation is requested twice, then exactly one request completes the fixture and
    /// the other is rejected without a second standings or statistics mutation.
    #[test]
    fn m12_finalisation_is_single_use_and_rejects_a_second_request() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        prepare_session(&state);
        let prepared = prepared_replay(&state);

        finish_spatial_match(&state, prepared.report.clone()).expect("first finalisation");
        assert_eq!(
            finish_spatial_match(&state, prepared.report).unwrap_err(),
            "be.error.noActiveLiveMatch"
        );
        let game = state.get_game(Clone::clone).expect("active game");
        assert_eq!(
            game.competitions[0].fixtures[0].status,
            FixtureStatus::Completed
        );
        assert_eq!(game.competitions[0].standings[0].played, 1);
        assert_eq!(game.competitions[0].standings[1].played, 1);
    }

    /// Given a real M12 report but no live-match session,
    /// when finalisation is requested before its authoritative preconditions exist, then the
    /// request is rejected and the active career remains untouched.
    #[test]
    fn m12_finalisation_before_a_live_session_is_rejected_without_mutation() {
        let state = StateManager::new();
        let game = game_with_spatial_fixture();
        let prepared =
            build_selected_fixture_replay(&game, 0, "competition-m12-1", "fixture-m12-1")
                .expect("production M12 report");
        state.set_game(game);
        let before = state.get_game(Clone::clone).expect("active game");

        assert_eq!(
            finish_spatial_match(&state, prepared.report).unwrap_err(),
            "be.error.noActiveLiveMatch"
        );
        let after = state.get_game(Clone::clone).expect("active game");
        assert_eq!(
            serde_json::to_value(&after).expect("serializable game"),
            serde_json::to_value(&before).expect("serializable game"),
            "precondition failure cannot change the career"
        );
    }

    /// Given an active fixture and an M12 result with the wrong match identity,
    /// when projection refuses that result before the official finaliser runs, then the live
    /// session and career stay available for a valid replay rather than being partially applied.
    #[test]
    fn a_projection_failure_preserves_the_unfinalized_live_session() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        prepare_session(&state);
        let before = state.get_game(Clone::clone).expect("active game");
        let invalid_result = SpatialMatchResult {
            schema_version: 1,
            match_id: "different-fixture".into(),
            home_team_id: "home".into(),
            away_team_id: "away".into(),
            score: SpatialScore::default(),
            end_status: SpatialEndStatus::RegulationComplete,
            events: vec![],
            home_stats: SpatialTeamStats {
                possession_pct: 50.0,
                ..Default::default()
            },
            away_stats: SpatialTeamStats {
                possession_pct: 50.0,
                ..Default::default()
            },
            player_stats: vec![],
        };
        let context = ProjectionContext {
            match_id: "fixture-m12-1".into(),
            home_team_id: "home".into(),
            away_team_id: "away".into(),
            home_player_ids: before
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some("home"))
                .map(|player| player.id.clone())
                .collect(),
            away_player_ids: before
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some("away"))
                .map(|player| player.id.clone())
                .collect(),
        };

        assert!(matches!(
            project(&context, &invalid_result),
            Err(ProjectionError::MatchIdentityMismatch)
        ));
        assert!(state.with_live_match(|_| ()).is_some());
        assert_eq!(
            serde_json::to_value(state.get_game(Clone::clone).expect("active game"))
                .expect("serializable game"),
            serde_json::to_value(before).expect("serializable baseline"),
            "projection is pure and cannot partially finalize a career"
        );
    }

    /// Given a prepared M12 report whose fixture was completed by another writer,
    /// when the official finalisation path runs, then it refuses the stale fixture, restores the
    /// live session and does not add standings or player-stat mutations.
    #[test]
    fn a_stale_completed_fixture_leaves_no_partial_m12_finalisation() {
        let state = StateManager::new();
        state.set_game(game_with_spatial_fixture());
        prepare_session(&state);
        let prepared = prepared_replay(&state);
        state.update_game(|game| {
            game.competitions[0].fixtures[0].status = FixtureStatus::Completed;
            game.league.as_mut().expect("legacy league").fixtures[0].status =
                FixtureStatus::Completed;
        });
        let before = state.get_game(Clone::clone).expect("active game");

        assert_eq!(
            finish_spatial_match(&state, prepared.report).unwrap_err(),
            "be.error.liveMatch.fixtureNotScheduled"
        );
        assert!(
            state.take_live_match().is_some(),
            "the pre-match session is restored"
        );
        let after = state.get_game(Clone::clone).expect("active game");
        assert_eq!(
            serde_json::to_value(&after.competitions).expect("serializable competitions"),
            serde_json::to_value(&before.competitions).expect("serializable competitions"),
            "a rejected finalisation has no partial competition mutation"
        );
        assert_eq!(
            after
                .players
                .iter()
                .map(|player| &player.stats)
                .collect::<Vec<_>>(),
            before
                .players
                .iter()
                .map(|player| &player.stats)
                .collect::<Vec<_>>(),
            "a rejected finalisation has no partial cumulative player statistics"
        );
    }

    /// Given two simultaneous M12 finalisation requests for one live session,
    /// when both reach the official finaliser, then one succeeds and the other is rejected.
    #[test]
    fn concurrent_m12_finalisation_consumes_one_live_session_once() {
        let state = Arc::new(StateManager::new());
        state.set_game(game_with_spatial_fixture());
        prepare_session(&state);
        let prepared = prepared_replay(&state);
        let first_state = Arc::clone(&state);
        let second_state = Arc::clone(&state);
        let first_report = prepared.report.clone();
        let second_report = prepared.report;

        let first = std::thread::spawn(move || finish_spatial_match(&first_state, first_report));
        let second = std::thread::spawn(move || finish_spatial_match(&second_state, second_report));
        let outcomes = [
            first.join().expect("first request thread"),
            second.join().expect("second request thread"),
        ];
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_err()).count(),
            1
        );
        let game = state.get_game(Clone::clone).expect("active game");
        assert_eq!(game.competitions[0].standings[0].played, 1);
        assert_eq!(game.competitions[0].standings[1].played, 1);
    }

    /// Given an older save with no replay row and a replay row containing invalid JSON,
    /// when a fresh SaveManager reads them, then absence stays absent and corrupt bytes are
    /// returned unchanged for viewer validation rather than triggering a new simulation.
    #[test]
    fn old_and_corrupt_replays_are_loaded_without_simulation() {
        let directory = TempDir::new().expect("temporary saves directory");
        let game = game_with_spatial_fixture();
        let mut writer = SaveManager::init(directory.path()).expect("save manager");
        let save_id = writer
            .create_save(&game, "Replay compatibility")
            .expect("create save");
        assert_eq!(
            writer
                .load_match_replay(&save_id, "competition-m12-1", "fixture-m12-1")
                .expect("read old save without replay"),
            None
        );
        writer
            .save_match_replay(&save_id, "competition-m12-1", "fixture-m12-1", "{bad")
            .expect("persist corrupt test document");
        drop(writer);

        let mut reader = SaveManager::init(directory.path()).expect("fresh save manager");
        assert_eq!(
            reader
                .load_match_replay(&save_id, "competition-m12-1", "fixture-m12-1")
                .expect("load corrupt replay bytes"),
            Some("{bad".into())
        );
    }

    /// Given a career with a saved replay,
    /// when a replay write targets an unknown save id, then the write failure leaves the known
    /// replay byte-for-byte intact instead of partially replacing a document.
    #[test]
    fn a_failed_replay_write_leaves_existing_sqlite_data_unchanged() {
        let directory = TempDir::new().expect("temporary saves directory");
        let game = game_with_spatial_fixture();
        let mut manager = SaveManager::init(directory.path()).expect("save manager");
        let save_id = manager
            .create_save(&game, "Replay write failure")
            .expect("create save");
        manager
            .save_match_replay(&save_id, "competition-m12-1", "fixture-m12-1", "good")
            .expect("save baseline replay");

        let error = manager
            .save_match_replay("not-a-save", "competition-m12-1", "fixture-m12-1", "bad")
            .expect_err("unknown save write must fail");
        assert!(error.starts_with("be.error.saveNotFound"));
        assert_eq!(
            manager
                .load_match_replay(&save_id, "competition-m12-1", "fixture-m12-1")
                .expect("read baseline replay"),
            Some("good".into())
        );
    }
}
