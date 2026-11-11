//! The external player registry, from the screen's side.
//!
//! Search, detail, preview and recruit all read the same local sidecar (see
//! `fm24_adapter::registry`). Nothing here loads the registry into the game: a search returns one
//! page, and only the player the manager actually signs becomes a `Game` player. The rule that
//! decides whether a source player may join lives in `ofm_core::external_registry`; this file only
//! joins the active game's clock and roster to it.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::NaiveDate;
use log::info;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use fm24_adapter::registry::{
    self, RegistryAvailability, RegistryPage, RegistryPlayerDetail, RegistryPositionFilter,
    RegistryQuery, RegistrySort,
};
use ofm_core::external_registry::{
    preview_external_recruitment as preview_promotion, recruit_external_player as promote_player,
    ExternalPlayer, ExternalRecruitOutcome, ExternalRecruitPreview,
};
use ofm_core::game::Game;
use ofm_core::state::StateManager;

use crate::commands::fm24_world::registry_path;
use crate::commands::util::{mutate_active_game, user_team_id};

const NO_ACTIVE_GAME: &str = "be.error.noActiveGameSession";
const REGISTRY_READ_FAILED: &str = "be.error.fm24.registryQueryFailed";
const PLAYER_INCOMPLETE: &str = "be.error.registry.playerIncomplete";

/// What a search screen sends. `today`, the roster and the page clamp are decided on this side, so a
/// stale or hostile screen cannot search a different clock or a different exclusion set.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RegistrySearchRequest {
    pub text: String,
    pub nation: Option<String>,
    pub position: RegistryPositionFilter,
    pub min_age: Option<u32>,
    pub max_age: Option<u32>,
    pub club: String,
    pub availability: RegistryAvailability,
    pub sort: RegistrySort,
    pub descending: bool,
    pub page: u32,
    pub page_size: u32,
    pub include_incomplete: bool,
}

/// The clock and the identities the active career already owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryContext {
    pub today: NaiveDate,
    pub excluded_ids: Vec<String>,
}

/// Every source identity already in the career, on a roster or in the retired archive.
///
/// The base world imports most of the perimeter as `fm-p-*`, so this is a few thousand ids; sending
/// them to the local SQLite query is far cheaper than a second copy of the roster in the sidecar.
/// The filter only ever hides a player the career genuinely has, which is the duplicate guard the
/// promotion path enforces again before mutating.
pub fn registry_context(game: &Game) -> RegistryContext {
    let mut excluded_ids: Vec<String> = game
        .players
        .iter()
        .map(|player| player.id.as_str())
        .chain(game.retired_players.iter().map(|player| player.id.as_str()))
        .filter(|id| id.starts_with("fm-p-"))
        .map(str::to_string)
        .collect();
    excluded_ids.sort_unstable();
    excluded_ids.dedup();
    RegistryContext {
        today: game.clock.current_date.date_naive(),
        excluded_ids,
    }
}

/// The adapter query for a screen request and the career context. Pure, so the wire contract and the
/// projection can be tested without a registry file.
pub fn registry_query(request: RegistrySearchRequest, context: &RegistryContext) -> RegistryQuery {
    RegistryQuery {
        text: request.text,
        nation: request.nation,
        position: request.position,
        min_age: request.min_age,
        max_age: request.max_age,
        club: request.club,
        availability: request.availability,
        sort: request.sort,
        descending: request.descending,
        page: request.page,
        page_size: request.page_size,
        include_incomplete: request.include_incomplete,
        today: context.today,
    }
}

/// Where the sidecar lives, resolved once per command.
fn registry_file(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map(|dir| registry_path(&dir))
        .map_err(|_| "be.error.fm24.writeFailed".to_string())
}

fn context_for(state: &StateManager) -> Result<RegistryContext, String> {
    state
        .get_game(registry_context)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub async fn search_external_players(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<StateManager>>,
    request: RegistrySearchRequest,
) -> Result<RegistryPage, String> {
    info!("[cmd] search_external_players");
    let path = registry_file(&app_handle)?;
    let context = context_for(&state)?;
    let query = registry_query(request, &context);
    let excluded = context.excluded_ids;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = registry::open_registry(&path)?;
        registry::query_registry(&conn, &query, &excluded)
    })
    .await
    .map_err(|_| REGISTRY_READ_FAILED.to_string())?
}

#[tauri::command]
pub async fn get_external_player(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Option<RegistryPlayerDetail>, String> {
    info!("[cmd] get_external_player: player_id={player_id}");
    let path = registry_file(&app_handle)?;
    let context = context_for(&state)?;
    let today = context.today;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = registry::open_registry(&path)?;
        registry::get_registry_player(&conn, &player_id, today)
    })
    .await
    .map_err(|_| REGISTRY_READ_FAILED.to_string())?
}

/// The promotion payload, or the key that says why the stored record cannot be one.
pub fn external_from_detail(detail: RegistryPlayerDetail) -> Result<ExternalPlayer, String> {
    detail
        .into_external_player()
        .ok_or_else(|| PLAYER_INCOMPLETE.to_string())
}

/// Preview a recruitment against the active game. No mutation, so a screen can show the fee, wage
/// and the one reason the move is blocked without changing anything.
pub fn preview_recruitment_internal(
    state: &StateManager,
    external: &ExternalPlayer,
) -> Result<ExternalRecruitPreview, String> {
    state
        .get_game(|game| preview_promotion(game, external))
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())?
}

/// Promote the player into the active game for the manager's club, returning the new game and what
/// the move cost.
pub fn recruit_internal(
    state: &StateManager,
    external: &ExternalPlayer,
) -> Result<(Game, ExternalRecruitOutcome), String> {
    let mut outcome = None;
    let game = mutate_active_game(state, |game| {
        outcome = Some(promote_player(game, external)?);
        Ok(())
    })?;
    let outcome = outcome.ok_or_else(|| NO_ACTIVE_GAME.to_string())?;
    Ok((game, outcome))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitExternalResponse {
    pub game: Game,
    pub outcome: ExternalRecruitOutcome,
}

#[tauri::command]
pub async fn preview_external_recruitment(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<ExternalRecruitPreview, String> {
    info!("[cmd] preview_external_recruitment: player_id={player_id}");
    let detail = load_detail(&app_handle, &state, &player_id).await?;
    let external = external_from_detail(detail)?;
    preview_recruitment_internal(&state, &external)
}

#[tauri::command]
pub async fn recruit_external_player(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<RecruitExternalResponse, String> {
    info!("[cmd] recruit_external_player: player_id={player_id}");
    // The managed club is checked before the registry is read so an unemployed manager gets the
    // meaningful key rather than a registry error.
    let _managed_team = state
        .get_game(user_team_id)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())??;
    let detail = load_detail(&app_handle, &state, &player_id).await?;
    let external = external_from_detail(detail)?;
    let (game, outcome) = recruit_internal(&state, &external)?;
    Ok(RecruitExternalResponse { game, outcome })
}

async fn load_detail(
    app_handle: &tauri::AppHandle,
    state: &State<'_, Arc<StateManager>>,
    player_id: &str,
) -> Result<RegistryPlayerDetail, String> {
    let path = registry_file(app_handle)?;
    let today = context_for(state)?.today;
    let player_id = player_id.to_string();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        let conn = registry::open_registry(&path)?;
        registry::get_registry_player(&conn, &player_id, today)
    })
    .await
    .map_err(|_| REGISTRY_READ_FAILED.to_string())??;
    detail.ok_or_else(|| "be.error.registry.playerNotAvailable".to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRegistryStatus {
    pub installed: bool,
    pub schema_version: Option<u32>,
    pub source_sha256: Option<String>,
    pub players_seen: u32,
    pub eligible: u32,
    pub free_agents: u32,
    pub active_world: u32,
    pub missing_identity: u32,
    pub missing_nation: u32,
    pub no_position: u32,
    pub outside_age_range: u32,
    pub bytes: u64,
}

#[tauri::command]
pub fn get_external_registry_status(
    app_handle: tauri::AppHandle,
) -> Result<ExternalRegistryStatus, String> {
    let path = registry_file(&app_handle)?;
    let installed = path.is_file();
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    let mut status = ExternalRegistryStatus {
        installed,
        schema_version: None,
        source_sha256: None,
        players_seen: 0,
        eligible: 0,
        free_agents: 0,
        active_world: 0,
        missing_identity: 0,
        missing_nation: 0,
        no_position: 0,
        outside_age_range: 0,
        bytes,
    };
    if !installed {
        return Ok(status);
    }
    let conn = registry::open_registry(&path)?;
    for (key, value) in registry::registry_meta(&conn)? {
        let number = value.parse::<u32>().ok();
        match key.as_str() {
            "schema_version" => status.schema_version = number,
            "source_sha256" => status.source_sha256 = Some(value),
            "players_seen" => status.players_seen = number.unwrap_or(0),
            "eligible" => status.eligible = number.unwrap_or(0),
            "free_agents" => status.free_agents = number.unwrap_or(0),
            "active_world" => status.active_world = number.unwrap_or(0),
            "missing_identity" => status.missing_identity = number.unwrap_or(0),
            "missing_nation" => status.missing_nation = number.unwrap_or(0),
            "no_position" => status.no_position = number.unwrap_or(0),
            "outside_age_range" => status.outside_age_range = number.unwrap_or(0),
            _ => {}
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{Footedness, PlayerAttributes, Position};
    use domain::team::Team;
    use ofm_core::clock::GameClock;
    use ofm_core::external_registry::ExternalClub;

    fn attrs() -> PlayerAttributes {
        PlayerAttributes {
            pace: 70,
            stamina: 70,
            strength: 70,
            agility: 70,
            passing: 70,
            shooting: 70,
            tackling: 60,
            dribbling: 70,
            defending: 55,
            positioning: 68,
            vision: 70,
            decisions: 70,
            composure: 68,
            aggression: 55,
            teamwork: 70,
            leadership: 60,
            handling: 20,
            reflexes: 20,
            aerial: 55,
        }
    }

    fn team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "Alpha FC".to_string(),
            "ALP".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Alpha Ground".to_string(),
            30_000,
        );
        team.reputation = 50;
        team.wage_budget = 500_000;
        team.finance = 50_000_000;
        team.transfer_budget = 20_000_000;
        team
    }

    fn game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Jane".to_string(),
            "Doe".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());
        let mut game = Game::new(clock, manager, vec![team()], vec![], vec![], vec![]);
        game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Open;
        game.players.push(domain::player::Player::new(
            "fm-p-1".to_string(),
            "Base".to_string(),
            "Base Player".to_string(),
            "2000-01-01".to_string(),
            "GB".to_string(),
            Position::Striker,
            attrs(),
        ));
        game.retired_players.push(retired("fm-p-2"));
        game
    }

    fn retired(id: &str) -> domain::retired_player::RetiredPlayer {
        domain::retired_player::RetiredPlayer {
            id: id.to_string(),
            match_name: "Retired".to_string(),
            full_name: "Retired Player".to_string(),
            date_of_birth: "1980-01-01".to_string(),
            nationality: "GB".to_string(),
            football_nation: "GB".to_string(),
            birth_country: None,
            media: domain::player::PlayerMedia::default(),
            natural_position: Position::Striker,
            alternate_positions: vec![],
            footedness: Footedness::Right,
            weak_foot: 3,
            attributes: attrs(),
            ovr: 70,
            potential: 70,
            career: vec![],
            movements: vec![],
            retired_on: "2025-06-30".to_string(),
            retired_season: 2025,
            last_team_id: Some("team-1".to_string()),
        }
    }

    fn free_agent(id: &str) -> ExternalPlayer {
        ExternalPlayer {
            id: id.to_string(),
            match_name: "J. Free".to_string(),
            full_name: "John Free".to_string(),
            date_of_birth: "2000-01-01".to_string(),
            nationality: "GB".to_string(),
            position: Position::Striker,
            natural_position: Position::Striker,
            alternate_positions: vec![],
            footedness: Footedness::Right,
            weak_foot: 3,
            attributes: attrs(),
            ovr: 70,
            potential: 80,
            market_value: 2_000_000,
            club: None,
        }
    }

    fn detail(id: &str) -> RegistryPlayerDetail {
        RegistryPlayerDetail {
            id: id.to_string(),
            name: "John Free".to_string(),
            first_name: "John".to_string(),
            last_name: "Free".to_string(),
            common_name: None,
            age: Some(26),
            birth_date: Some("2000-01-01".to_string()),
            nation: Some("England".to_string()),
            nationality: Some("GB".to_string()),
            position: Some("Striker".to_string()),
            position_group: Some("forward".to_string()),
            natural_position: Some(Position::Striker),
            alternate_positions: vec![],
            footedness: Footedness::Right,
            weak_foot: 3,
            attributes: Some(attrs()),
            ovr: 70,
            potential: 80,
            club_id: None,
            club_name: None,
            club_nation: None,
            club_reputation: 0,
            ownership: "free_agent".to_string(),
            contract_end: None,
            market_value: 2_000_000,
            weekly_wage: 0,
            value_source: "save".to_string(),
            wage_source: "save".to_string(),
            asking_price: 0,
            status: "eligible".to_string(),
            exclusion_reason: None,
            fm_uid: Some(12),
            fm_uid_status: "unique".to_string(),
            source_row_hash: "hash".to_string(),
        }
    }

    /// Given the active game, when the excluded set is built, then it names the roster and the
    /// retired archive by source identity only.
    #[test]
    fn the_context_excludes_roster_and_retired_source_identities() {
        let context = registry_context(&game());
        assert_eq!(context.excluded_ids, vec!["fm-p-1", "fm-p-2"]);
        assert_eq!(context.today, NaiveDate::from_ymd_opt(2026, 8, 1).unwrap());
    }

    /// Given a screen request, when it is mapped, then the clock and paging come from the career,
    /// never from the request.
    #[test]
    fn a_search_request_takes_its_clock_from_the_career() {
        let context = RegistryContext {
            today: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            excluded_ids: vec![],
        };
        let request = RegistrySearchRequest {
            text: "free".to_string(),
            page: 2,
            page_size: 10,
            ..RegistrySearchRequest::default()
        };
        let query = registry_query(request, &context);
        assert_eq!(query.text, "free");
        assert_eq!(query.page, 2);
        assert_eq!(query.page_size, 10);
        assert_eq!(query.today, context.today);
    }

    /// Given a stored record missing an attribute block, when it is promoted, then the refusal is
    /// the incomplete-player key rather than a fabricated player.
    #[test]
    fn an_incomplete_detail_is_refused_with_a_key() {
        let mut record = detail("fm-p-3");
        record.attributes = None;
        assert_eq!(
            external_from_detail(record).err(),
            Some("be.error.registry.playerIncomplete".to_string())
        );
    }

    /// Given a free agent from the registry, when the command promotes him, then the active game
    /// holds him under his source identity and the preview said a free signing.
    #[test]
    fn recruiting_through_the_command_mutates_the_active_game_once() {
        let state = StateManager::new();
        state.set_game(game());
        let external = free_agent("fm-p-3");

        let preview = preview_recruitment_internal(&state, &external).unwrap();
        assert!(preview.free_agent);
        assert_eq!(preview.blocker, None);

        let (game, outcome) = recruit_internal(&state, &external).unwrap();
        assert!(outcome.free_agent);
        assert!(game.players.iter().any(|player| player.id == "fm-p-3"));

        let error = recruit_internal(&state, &external).unwrap_err();
        assert_eq!(error, "be.error.registry.playerAlreadyInWorld");
    }

    /// Given a contracted external player, when he is bought through the command, then the outcome
    /// names the external club and the game carries the completed transfer.
    #[test]
    fn a_contracted_player_recruited_through_the_command_names_his_club() {
        let state = StateManager::new();
        state.set_game(game());
        let mut external = free_agent("fm-p-4");
        external.club = Some(ExternalClub {
            id: "fm-c-900".to_string(),
            name: "External United".to_string(),
            contract_end: Some("2028-06-30".to_string()),
            weekly_wage: 8_000,
        });

        let (game, outcome) = recruit_internal(&state, &external).unwrap();

        assert!(!outcome.free_agent);
        assert_eq!(outcome.from_club.as_deref(), Some("External United"));
        assert!(outcome.fee > 0);
        let player = game
            .players
            .iter()
            .find(|player| player.id == "fm-p-4")
            .expect("promoted");
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
    }
}
