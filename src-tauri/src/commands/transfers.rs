use domain::negotiation::NegotiationFeedback;
use domain::player::{LoanOfferStatus, Position};
use domain::transfer_terms::TransferTerms;
use log::info;
use std::sync::Arc;
use tauri::State;

use ofm_core::game::Game;
use ofm_core::game::{YouthScoutingObjective, YouthScoutingRegion};
use ofm_core::state::StateManager;
use ofm_core::transfers::{
    BidNegotiationStatus, LoanOfferDecision, LoanOfferOutcome, TransferBidFinancialProjection,
    TransferNegotiationDecision, TransferNegotiationOutcome,
};

use crate::commands::util::mutate_active_game;

const INVALID_YOUTH_SCOUTING_REGION_ERROR: &str = "be.error.transfers.invalidYouthScoutingRegion";
const INVALID_YOUTH_SCOUTING_OBJECTIVE_ERROR: &str =
    "be.error.transfers.invalidYouthScoutingObjective";
const INVALID_YOUTH_SCOUTING_TARGET_POSITION_ERROR: &str =
    "be.error.transfers.invalidYouthScoutingTargetPosition";
const ERR_PLAYER_NOT_OWNED_BY_USER: &str = "be.error.transfers.playerNotOwnedByUser";
const ERR_PLAYER_ALREADY_LOANED: &str = "be.error.transfers.playerAlreadyLoaned";

const NO_ACTIVE_GAME: &str = "be.error.noActiveGameSession";

/// Runs a command that moves money or players as one transaction (see `ofm_core::transactions`):
/// all-or-nothing under the game lock, and a repeat of `request_id` replays the first outcome.
fn transact<T>(
    state: &StateManager,
    request_id: Option<&str>,
    change: impl FnOnce(&mut Game) -> Result<T, String>,
) -> Result<T, String>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    state
        .run_transaction(request_id, change)
        .unwrap_or_else(|| Err(NO_ACTIVE_GAME.to_string()))
}

/// The game as the screen holds it, read after a transaction has committed.
fn ui_game(state: &StateManager) -> Result<Game, String> {
    state
        .get_game(Game::ui_clone)
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

/// What became of a request the screen sent but may not have heard back from: it ran and changed
/// the game, it ran and was refused, or it never ran. The screen asks before it tells the player a
/// transfer failed, and never resubmits a request whose answer it cannot confirm.
#[tauri::command]
pub fn get_transaction_status(
    state: State<'_, Arc<StateManager>>,
    request_id: String,
) -> ofm_core::transactions::TransactionStatus {
    state.transaction_status(&request_id)
}

fn player_has_active_or_pending_loan(player: &domain::player::Player) -> bool {
    player.active_loan.is_some()
        || player
            .loan_offers
            .iter()
            .any(|offer| offer.status == LoanOfferStatus::PendingRegistration)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferNegotiationCommandResponse {
    pub decision: TransferNegotiationDecision,
    pub suggested_fee: Option<u64>,
    pub is_terminal: bool,
    pub registration_date: Option<String>,
    pub feedback: NegotiationFeedback,
    /// The wage and length the player asks for, when he is the one holding the deal up.
    pub player_demand: Option<ofm_core::transfers::PlayerDemand>,
    /// What stood in his way, when he turned the move down.
    pub player_obstacle: Option<domain::free_agent_talk::FreeAgentTalkReason>,
    pub game: Game,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferBidFinancialProjectionCommandResponse {
    pub projection: TransferBidFinancialProjection,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoanOfferCommandResponse {
    pub decision: LoanOfferDecision,
    pub offer_id: String,
    pub suggested_wage_contribution_pct: Option<u8>,
    pub suggested_end_date: Option<String>,
    pub suggested_buy_option_fee: Option<u64>,
    pub is_terminal: bool,
    pub game: Game,
}

#[tauri::command]
pub fn toggle_transfer_list(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Game, String> {
    toggle_transfer_list_internal(&state, &player_id)
}

pub fn toggle_transfer_list_internal(
    state: &StateManager,
    player_id: &str,
) -> Result<Game, String> {
    info!("[cmd] toggle_transfer_list: player_id={}", player_id);
    mutate_active_game(state, |game| {
        if let Some(p) = game.players.iter_mut().find(|p| p.id == player_id) {
            p.transfer_listed = !p.transfer_listed;
            Ok(())
        } else {
            Err("be.error.playerNotFound".into())
        }
    })
}

#[tauri::command]
pub fn toggle_loan_list(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<Game, String> {
    toggle_loan_list_internal(&state, &player_id)
}

pub fn toggle_loan_list_internal(state: &StateManager, player_id: &str) -> Result<Game, String> {
    info!("[cmd] toggle_loan_list: player_id={}", player_id);
    mutate_active_game(state, |game| {
        let user_team_id = game
            .manager
            .team_id
            .clone()
            .ok_or_else(|| "be.error.noTeamAssigned".to_string())?;
        let player = game
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
            .ok_or_else(|| "be.error.playerNotFound".to_string())?;

        if player.team_id.as_deref() != Some(user_team_id.as_str()) {
            return Err(ERR_PLAYER_NOT_OWNED_BY_USER.to_string());
        }

        if player_has_active_or_pending_loan(player) {
            return Err(ERR_PLAYER_ALREADY_LOANED.to_string());
        }

        player.loan_listed = !player.loan_listed;
        Ok(())
    })
}

/// Players of the manager's club who cannot leave today without leaving it short of the squad
/// floor, each with the refusal a sale, loan or release of his would get (a translation key). The
/// screen reads it before the manager answers or counters an offer, so a sale that cannot happen is
/// said so up front instead of after he has submitted it.
#[tauri::command]
pub fn get_departure_blockers(
    state: State<'_, Arc<StateManager>>,
) -> Result<std::collections::HashMap<String, String>, String> {
    get_departure_blockers_internal(&state)
}

pub fn get_departure_blockers_internal(
    state: &StateManager,
) -> Result<std::collections::HashMap<String, String>, String> {
    state
        .get_game(|game| {
            game.manager
                .team_id
                .as_deref()
                .map(|team_id| ofm_core::squad_floor::departure_blockers(game, team_id))
                .unwrap_or_default()
                .into_iter()
                .collect()
        })
        .ok_or_else(|| NO_ACTIVE_GAME.to_string())
}

#[tauri::command]
pub fn make_transfer_bid(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    fee: u64,
    terms: Option<TransferTerms>,
    request_id: Option<String>,
) -> Result<TransferNegotiationCommandResponse, String> {
    make_transfer_bid_internal(
        &state,
        request_id.as_deref(),
        &player_id,
        fee,
        terms.unwrap_or_default(),
    )
}

#[tauri::command]
pub fn preview_transfer_bid_financial_impact(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    fee: u64,
    terms: Option<TransferTerms>,
) -> Result<TransferBidFinancialProjectionCommandResponse, String> {
    preview_transfer_bid_financial_impact_internal(
        &state,
        &player_id,
        fee,
        &terms.unwrap_or_default(),
    )
}

#[tauri::command]
pub fn get_transfer_deadline_summary(
    state: State<'_, Arc<StateManager>>,
) -> Result<ofm_core::transfers::DeadlineSummary, String> {
    get_transfer_deadline_summary_internal(&state)
}

pub fn get_transfer_deadline_summary_internal(
    state: &StateManager,
) -> Result<ofm_core::transfers::DeadlineSummary, String> {
    state
        .get_game(ofm_core::transfers::transfer_deadline_summary)
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())
}

#[tauri::command]
pub fn get_bid_negotiation_status(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
) -> Result<BidNegotiationStatus, String> {
    get_bid_negotiation_status_internal(&state, &player_id)
}

pub fn get_bid_negotiation_status_internal(
    state: &StateManager,
    player_id: &str,
) -> Result<BidNegotiationStatus, String> {
    info!("[cmd] get_bid_negotiation_status: player_id={}", player_id);
    state
        .get_game(|game| ofm_core::transfers::bid_negotiation_status(game, player_id))
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())?
}

pub fn make_transfer_bid_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    fee: u64,
    terms: TransferTerms,
) -> Result<TransferNegotiationCommandResponse, String> {
    info!(
        "[cmd] make_transfer_bid: player_id={}, fee={}, terms={:?}",
        player_id, fee, terms
    );
    let outcome = transact(state, request_id, |game| {
        ofm_core::transfers::make_manager_transfer_bid(game, player_id, fee, terms)
    })?;
    Ok(map_transfer_negotiation_response(outcome, ui_game(state)?))
}

#[tauri::command]
pub fn make_loan_offer(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    end_date: String,
    wage_contribution_pct: u8,
    buy_option_fee: Option<u64>,
    request_id: Option<String>,
) -> Result<LoanOfferCommandResponse, String> {
    make_loan_offer_internal(
        &state,
        request_id.as_deref(),
        &player_id,
        &end_date,
        wage_contribution_pct,
        buy_option_fee,
    )
}

pub fn make_loan_offer_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    end_date: &str,
    wage_contribution_pct: u8,
    buy_option_fee: Option<u64>,
) -> Result<LoanOfferCommandResponse, String> {
    info!(
        "[cmd] make_loan_offer: player_id={}, end_date={}, wage_contribution_pct={}, buy_option_fee={:?}",
        player_id, end_date, wage_contribution_pct, buy_option_fee
    );
    // Read, change and commit under one hold of the game lock: this used to clone the game, change
    // the clone and put it back, which lost whatever another command committed in between.
    let outcome = transact(state, request_id, |game| {
        ofm_core::transfers::make_loan_offer(
            game,
            player_id,
            end_date,
            wage_contribution_pct,
            buy_option_fee,
        )
    })?;
    Ok(map_loan_offer_response(outcome, ui_game(state)?))
}

#[tauri::command]
pub fn exercise_loan_buy_option(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    request_id: Option<String>,
) -> Result<Game, String> {
    exercise_loan_buy_option_internal(&state, request_id.as_deref(), &player_id)
}

pub fn exercise_loan_buy_option_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
) -> Result<Game, String> {
    info!("[cmd] exercise_loan_buy_option: player_id={}", player_id);
    transact(state, request_id, |game| {
        ofm_core::transfers::exercise_loan_buy_option(game, player_id)
    })?;
    ui_game(state)
}

pub fn preview_transfer_bid_financial_impact_internal(
    state: &StateManager,
    player_id: &str,
    fee: u64,
    terms: &TransferTerms,
) -> Result<TransferBidFinancialProjectionCommandResponse, String> {
    info!(
        "[cmd] preview_transfer_bid_financial_impact: player_id={}, fee={}, terms={:?}",
        player_id, fee, terms
    );

    let projection = state
        .get_game(|game| {
            ofm_core::transfers::project_transfer_bid_financial_impact_with_terms(
                game, player_id, fee, terms,
            )
        })
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())??;

    Ok(TransferBidFinancialProjectionCommandResponse { projection })
}

#[tauri::command]
pub fn respond_to_offer(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    offer_id: String,
    accept: bool,
    request_id: Option<String>,
) -> Result<Game, String> {
    respond_to_offer_internal(&state, request_id.as_deref(), &player_id, &offer_id, accept)
}

pub fn respond_to_offer_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    offer_id: &str,
    accept: bool,
) -> Result<Game, String> {
    info!(
        "[cmd] respond_to_offer: player_id={}, offer_id={}, accept={}",
        player_id, offer_id, accept
    );
    transact(state, request_id, |game| {
        ofm_core::transfers::respond_to_offer(game, player_id, offer_id, accept)
    })?;
    ui_game(state)
}

#[tauri::command]
pub fn respond_to_loan_offer(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    offer_id: String,
    accept: bool,
    request_id: Option<String>,
) -> Result<Game, String> {
    respond_to_loan_offer_internal(&state, request_id.as_deref(), &player_id, &offer_id, accept)
}

pub fn respond_to_loan_offer_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    offer_id: &str,
    accept: bool,
) -> Result<Game, String> {
    info!(
        "[cmd] respond_to_loan_offer: player_id={}, offer_id={}, accept={}",
        player_id, offer_id, accept
    );
    transact(state, request_id, |game| {
        ofm_core::transfers::respond_to_loan_offer(game, player_id, offer_id, accept)
    })?;
    ui_game(state)
}

#[tauri::command]
pub fn counter_loan_offer(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    offer_id: String,
    end_date: String,
    wage_contribution_pct: u8,
    buy_option_fee: Option<u64>,
    request_id: Option<String>,
) -> Result<LoanOfferCommandResponse, String> {
    counter_loan_offer_internal(
        &state,
        request_id.as_deref(),
        &player_id,
        &offer_id,
        &end_date,
        wage_contribution_pct,
        buy_option_fee,
    )
}

pub fn counter_loan_offer_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    offer_id: &str,
    end_date: &str,
    wage_contribution_pct: u8,
    buy_option_fee: Option<u64>,
) -> Result<LoanOfferCommandResponse, String> {
    info!(
        "[cmd] counter_loan_offer: player_id={}, offer_id={}, end_date={}, wage_contribution_pct={}, buy_option_fee={:?}",
        player_id, offer_id, end_date, wage_contribution_pct, buy_option_fee
    );
    let outcome = transact(state, request_id, |game| {
        ofm_core::transfers::counter_loan_offer(
            game,
            player_id,
            offer_id,
            end_date,
            wage_contribution_pct,
            buy_option_fee,
        )
    })?;
    Ok(map_loan_offer_response(outcome, ui_game(state)?))
}

#[tauri::command]
pub fn counter_offer(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    offer_id: String,
    requested_fee: u64,
    request_id: Option<String>,
) -> Result<TransferNegotiationCommandResponse, String> {
    counter_offer_internal(
        &state,
        request_id.as_deref(),
        &player_id,
        &offer_id,
        requested_fee,
    )
}

pub fn counter_offer_internal(
    state: &StateManager,
    request_id: Option<&str>,
    player_id: &str,
    offer_id: &str,
    requested_fee: u64,
) -> Result<TransferNegotiationCommandResponse, String> {
    info!(
        "[cmd] counter_offer: player_id={}, offer_id={}, requested_fee={}",
        player_id, offer_id, requested_fee
    );
    let outcome = transact(state, request_id, |game| {
        ofm_core::transfers::counter_offer(game, player_id, offer_id, requested_fee)
    })?;
    Ok(map_transfer_negotiation_response(outcome, ui_game(state)?))
}

fn map_transfer_negotiation_response(
    outcome: TransferNegotiationOutcome,
    game: Game,
) -> TransferNegotiationCommandResponse {
    TransferNegotiationCommandResponse {
        decision: outcome.decision,
        suggested_fee: outcome.suggested_fee,
        is_terminal: outcome.is_terminal,
        registration_date: outcome.registration_date,
        feedback: outcome.feedback,
        player_demand: outcome.player_demand,
        player_obstacle: outcome.player_obstacle,
        game,
    }
}

fn map_loan_offer_response(outcome: LoanOfferOutcome, game: Game) -> LoanOfferCommandResponse {
    LoanOfferCommandResponse {
        decision: outcome.decision,
        offer_id: outcome.offer_id,
        suggested_wage_contribution_pct: outcome.suggested_wage_contribution_pct,
        suggested_end_date: outcome.suggested_end_date,
        suggested_buy_option_fee: outcome.suggested_buy_option_fee,
        is_terminal: outcome.is_terminal,
        game: game.into_ui(),
    }
}

#[tauri::command]
pub fn send_scout(
    state: State<'_, Arc<StateManager>>,
    scout_id: String,
    player_id: String,
) -> Result<Game, String> {
    info!(
        "[cmd] send_scout: scout_id={}, player_id={}",
        scout_id, player_id
    );
    mutate_active_game(&state, |game| {
        ofm_core::scouting::send_scout(game, &scout_id, &player_id)
    })
}

#[tauri::command]
pub fn start_youth_scouting(
    state: State<'_, Arc<StateManager>>,
    scout_id: String,
    region: Option<String>,
    objective: Option<String>,
    target_position: Option<String>,
) -> Result<Game, String> {
    info!(
        "[cmd] start_youth_scouting: scout_id={}, region={:?}, objective={:?}, target_position={:?}",
        scout_id, region, objective, target_position
    );
    let region = parse_youth_region(region.as_deref())?;
    let objective = parse_youth_objective(objective.as_deref())?;
    let target_position = parse_youth_target_position(target_position.as_deref())?;

    mutate_active_game(&state, |game| {
        ofm_core::scouting::start_youth_scouting(
            game,
            &scout_id,
            region,
            objective,
            target_position,
        )
    })
}

#[tauri::command]
pub fn cancel_youth_scouting(
    state: State<'_, Arc<StateManager>>,
    assignment_id: String,
) -> Result<Game, String> {
    info!(
        "[cmd] cancel_youth_scouting: assignment_id={}",
        assignment_id
    );
    mutate_active_game(&state, |game| {
        ofm_core::scouting::cancel_youth_scouting(game, &assignment_id)
    })
}

#[tauri::command]
pub fn reassign_youth_scouting(
    state: State<'_, Arc<StateManager>>,
    assignment_id: String,
    scout_id: String,
) -> Result<Game, String> {
    info!(
        "[cmd] reassign_youth_scouting: assignment_id={}, scout_id={}",
        assignment_id, scout_id
    );
    mutate_active_game(&state, |game| {
        ofm_core::scouting::reassign_youth_scouting(game, &assignment_id, &scout_id)
    })
}

fn parse_youth_region(value: Option<&str>) -> Result<YouthScoutingRegion, String> {
    match value {
        None | Some("") | Some("Domestic") => Ok(YouthScoutingRegion::Domestic),
        Some("International") => Ok(YouthScoutingRegion::International),
        Some(_) => Err(INVALID_YOUTH_SCOUTING_REGION_ERROR.to_string()),
    }
}

fn parse_youth_objective(value: Option<&str>) -> Result<YouthScoutingObjective, String> {
    match value {
        None | Some("") | Some("Balanced") => Ok(YouthScoutingObjective::Balanced),
        Some("HighPotential") => Ok(YouthScoutingObjective::HighPotential),
        Some("ReadySoon") => Ok(YouthScoutingObjective::ReadySoon),
        Some(_) => Err(INVALID_YOUTH_SCOUTING_OBJECTIVE_ERROR.to_string()),
    }
}

fn parse_youth_target_position(value: Option<&str>) -> Result<Option<Position>, String> {
    match value {
        None | Some("") => Ok(None),
        Some("Goalkeeper") => Ok(Some(Position::Goalkeeper)),
        Some("Defender") => Ok(Some(Position::Defender)),
        Some("Midfielder") => Ok(Some(Position::Midfielder)),
        Some("Forward") => Ok(Some(Position::Forward)),
        Some(_) => Err(INVALID_YOUTH_SCOUTING_TARGET_POSITION_ERROR.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        counter_loan_offer_internal, counter_offer_internal, exercise_loan_buy_option_internal,
        get_departure_blockers_internal, get_transfer_deadline_summary_internal,
        make_loan_offer_internal, make_transfer_bid_internal,
        preview_transfer_bid_financial_impact_internal, respond_to_loan_offer_internal,
        respond_to_offer_internal, toggle_loan_list_internal, toggle_transfer_list_internal,
    };
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{
        ActiveLoan, LoanOffer, LoanOfferStatus, Player, PlayerAttributes, Position, TransferOffer,
        TransferOfferStatus,
    };
    use domain::season::TransferWindowStatus;
    use domain::team::Team;
    use ofm_core::clock::GameClock;
    use ofm_core::game::Game;
    use ofm_core::state::StateManager;
    use ofm_core::transfers::{LoanOfferDecision, TransferNegotiationDecision};

    fn default_attrs() -> PlayerAttributes {
        PlayerAttributes {
            pace: 60,
            stamina: 60,
            strength: 60,
            agility: 60,
            passing: 60,
            shooting: 60,
            tackling: 60,
            dribbling: 60,
            defending: 60,
            positioning: 60,
            vision: 60,
            decisions: 60,
            composure: 60,
            aggression: 60,
            teamwork: 60,
            leadership: 60,
            handling: 30,
            reflexes: 30,
            aerial: 60,
        }
    }

    fn make_user_team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "User FC".to_string(),
            "USR".to_string(),
            "England".to_string(),
            "London".to_string(),
            "User Ground".to_string(),
            25_000,
        );
        team.finance = 5_000_000;
        team.transfer_budget = 2_000_000;
        team.manager_id = Some("manager-1".to_string());
        team
    }

    fn make_buyer_team() -> Team {
        let mut team = Team::new(
            "team-2".to_string(),
            "Buyer FC".to_string(),
            "BUY".to_string(),
            "England".to_string(),
            "Liverpool".to_string(),
            "Buyer Ground".to_string(),
            28_000,
        );
        team.finance = 6_000_000;
        team.transfer_budget = 3_000_000;
        team
    }

    fn make_player_with_offer() -> Player {
        let mut player = Player::new(
            "player-1".to_string(),
            "P. One".to_string(),
            "Player One".to_string(),
            "2000-01-01".to_string(),
            "England".to_string(),
            Position::Forward,
            default_attrs(),
        );
        player.team_id = Some("team-1".to_string());
        player.stage_contract_end(Some("2028-06-30".to_string()));
        player.market_value = 1_000_000;
        player.transfer_offers.push(TransferOffer {
            id: "offer-1".to_string(),
            from_team_id: "team-2".to_string(),
            fee: 900_000,
            wage_offered: 0,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            status: TransferOfferStatus::Pending,
            date: "2026-08-01".to_string(),
            registration_date: None,
            closed_on: None,
            registration_failure_reason: None,
            terms: Default::default(),
        });
        player
    }

    /// Squad depth for both clubs, so one player leaving never takes either
    /// below the squad floor — these tests are about the command, not the
    /// floor. Inert: no wage and no market value, so no budget moves for it.
    fn give_both_clubs_squad_depth(game: &mut Game) {
        for team_id in ["team-1", "team-2"] {
            // One more than the minimum in every group, sixteen in all.
            let depth = [3, 5, 5, 3];
            for ((group, _), count) in ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
                .into_iter()
                .zip(depth)
            {
                for index in 0..count {
                    let mut player = Player::new(
                        format!("depth-{team_id}-{group:?}-{index}"),
                        format!("Depth {index}"),
                        format!("Depth {group:?} {index}"),
                        "1996-01-01".to_string(),
                        "England".to_string(),
                        group.clone(),
                        default_attrs(),
                    );
                    player.team_id = Some(team_id.to_string());
                    player.stage_contract_end(Some("2031-06-30".to_string()));
                    player.market_value = 0;
                    game.players.push(player);
                }
            }
        }
    }

    fn make_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut game = Game::new(
            clock,
            manager,
            vec![make_user_team(), make_buyer_team()],
            vec![make_player_with_offer()],
            vec![],
            vec![],
        );
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        give_both_clubs_squad_depth(&mut game);
        game
    }

    fn make_bid_target_player() -> Player {
        let mut player = Player::new(
            "player-2".to_string(),
            "P. Two".to_string(),
            "Player Two".to_string(),
            "2000-01-01".to_string(),
            "England".to_string(),
            Position::Forward,
            default_attrs(),
        );
        player.team_id = Some("team-2".to_string());
        player.stage_contract_end(Some("2028-06-30".to_string()));
        player.market_value = 1_000_000;
        player.stage_wage(1_000);
        player.morale = 35;
        player.stats.appearances = 1;
        player
    }

    fn make_bid_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut game = Game::new(
            clock,
            manager,
            vec![make_user_team(), make_buyer_team()],
            vec![make_bid_target_player()],
            vec![],
            vec![],
        );
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        game.teams[0].reputation = 700;
        game.teams[1].reputation = 350;
        give_both_clubs_squad_depth(&mut game);
        game
    }

    #[test]
    fn counter_offer_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = counter_offer_internal(&state, None, "player-1", "offer-1", 1_050_000)
            .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-2"));
        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::Accepted
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-1")
                .and_then(|player| player.team_id.clone())
                .as_deref(),
            Some("team-2")
        );
    }

    /// The manager's bid, carried through the player's step: the first answer on a fee the clubs
    /// agree is `PlayerTalks` with the contract he would sign (nothing completed); the second puts
    /// that contract to him.
    fn bid_through_the_player_step(
        state: &StateManager,
        player_id: &str,
        fee: u64,
    ) -> super::TransferNegotiationCommandResponse {
        let first = make_transfer_bid_internal(
            state,
            None,
            player_id,
            fee,
            domain::transfer_terms::TransferTerms::default(),
        )
        .expect("first bid");
        assert_eq!(first.decision, TransferNegotiationDecision::PlayerTalks);
        let shown = first.player_demand.expect("the contract he would sign");
        make_transfer_bid_internal(
            state,
            None,
            player_id,
            fee,
            domain::transfer_terms::TransferTerms {
                player_weekly_wage: shown.weekly_wage,
                player_contract_years: u8::try_from(shown.contract_years).expect("years"),
                ..domain::transfer_terms::TransferTerms::default()
            },
        )
        .expect("bid with his contract")
    }

    #[test]
    fn make_transfer_bid_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let first = make_transfer_bid_internal(
            &state,
            None,
            "player-2",
            1_050_000,
            domain::transfer_terms::TransferTerms::default(),
        )
        .expect("first bid");
        assert_eq!(first.decision, TransferNegotiationDecision::PlayerTalks);
        assert_eq!(
            first.game.players[0].team_id.as_deref(),
            Some("team-2"),
            "the clubs agreeing moves nobody"
        );
        let response = bid_through_the_player_step(&state, "player-2", 1_050_000);

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-1"));
        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::Accepted
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-2")
                .and_then(|player| player.team_id.clone())
                .as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn make_transfer_bid_internal_preserves_scheduled_registration_date() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.clock.current_date = Utc.with_ymd_and_hms(2026, 12, 20, 12, 0, 0).unwrap();
        game.season_context.transfer_window.status = TransferWindowStatus::Closed;
        game.season_context.transfer_window.opens_on = Some("2027-01-01".to_string());
        state.set_game(game);

        let response = bid_through_the_player_step(&state, "player-2", 1_050_000);

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert_eq!(response.registration_date.as_deref(), Some("2027-01-01"));
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-2"));
        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::PendingRegistration
        );
        assert_eq!(
            response.game.players[0].transfer_offers[0]
                .registration_date
                .as_deref(),
            Some("2027-01-01")
        );
    }

    #[test]
    fn make_loan_offer_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = true;
        state.set_game(game);

        let response = make_loan_offer_internal(&state, None, "player-2", "2027-01-01", 100, None)
            .expect("response");

        assert_eq!(response.decision, LoanOfferDecision::Accepted);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-1"));
        assert!(response.game.players[0].active_loan.is_some());
        assert_eq!(
            response.game.players[0].loan_offers[0].status,
            LoanOfferStatus::Accepted
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-2")
                .and_then(|player| player.active_loan.as_ref())
                .map(|loan| loan.parent_team_id.as_str()),
            Some("team-2")
        );
    }

    #[test]
    fn exercise_loan_buy_option_internal_updates_state() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = true;
        state.set_game(game);

        make_loan_offer_internal(&state, None, "player-2", "2027-01-01", 100, Some(1_000_000))
            .expect("loan-to-buy offer");
        let response =
            exercise_loan_buy_option_internal(&state, None, "player-2").expect("exercise option");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-2")
            .expect("player should exist");
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
        assert!(player.active_loan.is_none());

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-2")
                .and_then(|player| player.active_loan.as_ref()),
            None
        );
    }

    #[test]
    fn make_transfer_bid_internal_can_return_counter_offer_feedback() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let response = make_transfer_bid_internal(
            &state,
            None,
            "player-2",
            900_000,
            domain::transfer_terms::TransferTerms::default(),
        )
        .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::CounterOffer);
        assert_eq!(response.suggested_fee, Some(950_000));
        assert!(!response.is_terminal);
        assert_eq!(response.feedback.round, 1);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-2")
                .and_then(|player| player.team_id.clone())
                .as_deref(),
            Some("team-2")
        );
    }

    #[test]
    fn make_transfer_bid_internal_uses_existing_negotiation_round_on_follow_up_bid() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let first = make_transfer_bid_internal(
            &state,
            None,
            "player-2",
            900_000,
            domain::transfer_terms::TransferTerms::default(),
        )
        .expect("first bid");
        assert_eq!(first.decision, TransferNegotiationDecision::CounterOffer);
        assert_eq!(first.feedback.round, 1);

        let second = bid_through_the_player_step(&state, "player-2", 950_000);

        assert_eq!(second.decision, TransferNegotiationDecision::Accepted);
        assert_eq!(second.feedback.round, 2);
        assert_eq!(second.game.players[0].team_id.as_deref(), Some("team-1"));
    }

    #[test]
    fn respond_to_offer_internal_returns_game_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = respond_to_offer_internal(&state, None, "player-1", "offer-1", false)
            .expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
        assert_eq!(
            player.transfer_offers[0].status,
            TransferOfferStatus::Rejected
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert_eq!(stored_player.team_id.as_deref(), Some("team-1"));
        assert_eq!(
            stored_player.transfer_offers[0].status,
            TransferOfferStatus::Rejected
        );
    }

    #[test]
    fn respond_to_loan_offer_internal_returns_game_and_updates_state() {
        let state = StateManager::new();
        let mut game = make_game();
        game.players[0].loan_listed = true;
        game.players[0].loan_offers.push(LoanOffer {
            id: "loan-offer-1".to_string(),
            from_team_id: "team-2".to_string(),
            parent_team_id: "team-1".to_string(),
            start_date: "2026-08-01".to_string(),
            end_date: "2027-01-01".to_string(),
            wage_contribution_pct: 75,
            buy_option_fee: None,
            last_manager_wage_contribution_pct: None,
            last_manager_end_date: None,
            last_manager_buy_option_fee: None,
            negotiation_round: 1,
            suggested_wage_contribution_pct: None,
            suggested_end_date: None,
            suggested_buy_option_fee: None,
            status: LoanOfferStatus::Pending,
            date: "2026-08-01".to_string(),
            closed_on: None,
        });
        state.set_game(game);

        let response =
            respond_to_loan_offer_internal(&state, None, "player-1", "loan-offer-1", true)
                .expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert_eq!(player.team_id.as_deref(), Some("team-2"));
        assert_eq!(player.loan_offers[0].status, LoanOfferStatus::Accepted);
        assert!(player.active_loan.is_some());

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert_eq!(stored_player.team_id.as_deref(), Some("team-2"));
        assert!(stored_player.active_loan.is_some());
    }

    /// Given an affordable AI borrower, when its accepted counter passes through the command adapter, then the payload and stored game reflect the agreed loan.
    #[test]
    fn counter_loan_offer_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        let mut game = make_game();
        game.teams[1].wage_budget = 500_000;
        game.players[0].loan_listed = true;
        game.players[0].stage_wage(520_000);
        game.players[0].ovr = 68;
        game.players[0].potential = 78;
        game.teams[1].finance = 6_000_000;
        game.players[0].loan_offers.push(LoanOffer {
            id: "loan-offer-counter".to_string(),
            from_team_id: "team-2".to_string(),
            parent_team_id: "team-1".to_string(),
            start_date: "2026-08-01".to_string(),
            end_date: "2027-01-01".to_string(),
            wage_contribution_pct: 65,
            buy_option_fee: None,
            last_manager_wage_contribution_pct: None,
            last_manager_end_date: None,
            last_manager_buy_option_fee: None,
            negotiation_round: 1,
            suggested_wage_contribution_pct: None,
            suggested_end_date: None,
            suggested_buy_option_fee: None,
            status: LoanOfferStatus::Pending,
            date: "2026-08-01".to_string(),
            closed_on: None,
        });
        state.set_game(game);

        let response = counter_loan_offer_internal(
            &state,
            None,
            "player-1",
            "loan-offer-counter",
            "2027-01-01",
            85,
            None,
        )
        .expect("response");

        assert_eq!(response.decision, LoanOfferDecision::Accepted);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-2"));
        assert_eq!(
            response.game.players[0].loan_offers[0].last_manager_wage_contribution_pct,
            Some(85)
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert_eq!(stored_player.team_id.as_deref(), Some("team-2"));
        assert!(stored_player.active_loan.is_some());
    }

    #[test]
    fn toggle_transfer_list_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = toggle_transfer_list_internal(&state, "player-1").expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert!(player.transfer_listed);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(stored_player.transfer_listed);
    }

    #[test]
    fn toggle_loan_list_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = toggle_loan_list_internal(&state, "player-1").expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert!(player.loan_listed);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(stored_player.loan_listed);
    }

    #[test]
    fn toggle_loan_list_internal_rejects_other_club_player() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let error = toggle_loan_list_internal(&state, "player-2")
            .expect_err("loan listing another club's player should fail");

        assert_eq!(error, "be.error.transfers.playerNotOwnedByUser");
        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-2")
            .expect("stored player should exist");
        assert!(!stored_player.loan_listed);
    }

    #[test]
    fn toggle_loan_list_internal_rejects_active_loan_player() {
        let state = StateManager::new();
        let mut game = make_game();
        game.players[0].active_loan = Some(ActiveLoan {
            parent_team_id: "team-1".to_string(),
            loan_team_id: "team-2".to_string(),
            loan_team_name: None,
            start_date: "2026-08-01".to_string(),
            end_date: "2027-01-01".to_string(),
            wage_contribution_pct: 75,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
            terms_source: domain::player::LoanTermsSource::Agreed,
        });
        state.set_game(game);

        let error = toggle_loan_list_internal(&state, "player-1")
            .expect_err("active loan player should not be loan-listed");

        assert_eq!(error, "be.error.transfers.playerAlreadyLoaned");
        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(!stored_player.loan_listed);
    }

    #[test]
    fn toggle_loan_list_internal_rejects_pending_loan_registration_player() {
        let state = StateManager::new();
        let mut game = make_game();
        game.players[0].loan_offers.push(LoanOffer {
            id: "pending-loan-registration".to_string(),
            from_team_id: "team-2".to_string(),
            parent_team_id: "team-1".to_string(),
            start_date: "2026-09-01".to_string(),
            end_date: "2027-01-01".to_string(),
            wage_contribution_pct: 75,
            buy_option_fee: None,
            last_manager_wage_contribution_pct: None,
            last_manager_end_date: None,
            last_manager_buy_option_fee: None,
            negotiation_round: 1,
            suggested_wage_contribution_pct: None,
            suggested_end_date: None,
            suggested_buy_option_fee: None,
            status: LoanOfferStatus::PendingRegistration,
            date: "2026-08-01".to_string(),
            closed_on: None,
        });
        state.set_game(game);

        let error = toggle_loan_list_internal(&state, "player-1")
            .expect_err("pending loan registration player should not be loan-listed");

        assert_eq!(error, "be.error.transfers.playerAlreadyLoaned");
        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(!stored_player.loan_listed);
    }

    #[test]
    fn preview_transfer_bid_financial_impact_internal_returns_projection() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let response = preview_transfer_bid_financial_impact_internal(
            &state,
            "player-2",
            1_000_000,
            &domain::transfer_terms::TransferTerms::default(),
        )
        .expect("response");

        assert_eq!(response.projection.transfer_budget_before, 2_000_000);
        assert_eq!(response.projection.transfer_budget_after, 1_000_000);
        assert_eq!(response.projection.finance_before, 5_000_000);
        assert_eq!(response.projection.finance_after, 4_000_000);
        assert!(!response.projection.exceeds_transfer_budget);
        assert!(!response.projection.exceeds_finance);
        // Window is open in the fixture — the debit fires today, no deferred
        // registration date to surface.
        assert!(response.projection.pending_registration_date.is_none());
        assert_eq!(
            response.projection.current_weekly_wage_spend,
            response.projection.annual_wage_bill_before
        );
        assert_eq!(
            response.projection.projected_weekly_wage_spend,
            response.projection.annual_wage_bill_after
        );
        assert_eq!(
            response.projection.weekly_wage_budget,
            response.projection.annual_wage_budget
        );
        // He is shown at the wage the buyer would pay (its standard terms, rounded up to
        // the thousand), not the 1,000 he earns at his club.
        assert_ne!(response.projection.incoming_player_weekly_wage, 1_000);
        assert_eq!(
            response.projection.incoming_player_weekly_wage,
            response.projection.annual_wage_bill_after
                - response.projection.annual_wage_bill_before
        );
    }

    #[test]
    fn preview_transfer_bid_financial_impact_reports_pending_registration_date_when_window_closed()
    {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.clock.current_date = Utc.with_ymd_and_hms(2026, 12, 20, 12, 0, 0).unwrap();
        game.season_context.transfer_window.status = TransferWindowStatus::Closed;
        game.season_context.transfer_window.opens_on = Some("2027-01-02".to_string());
        state.set_game(game);

        let response = preview_transfer_bid_financial_impact_internal(
            &state,
            "player-2",
            1_000_000,
            &domain::transfer_terms::TransferTerms::default(),
        )
        .expect("response");

        assert_eq!(
            response.projection.pending_registration_date.as_deref(),
            Some("2027-01-02"),
        );
    }

    #[test]
    fn parse_youth_region_returns_backend_key_when_value_is_unsupported() {
        let result = super::parse_youth_region(Some("Intergalactic"));

        assert_eq!(
            result.unwrap_err(),
            super::INVALID_YOUTH_SCOUTING_REGION_ERROR
        );
    }

    #[test]
    fn parse_youth_objective_returns_backend_key_when_value_is_unsupported() {
        let result = super::parse_youth_objective(Some("WonderkidOnly"));

        assert_eq!(
            result.unwrap_err(),
            super::INVALID_YOUTH_SCOUTING_OBJECTIVE_ERROR
        );
    }

    #[test]
    fn parse_youth_target_position_returns_backend_key_when_value_is_unsupported() {
        let result = super::parse_youth_target_position(Some("Sweeper"));

        assert_eq!(
            result.unwrap_err(),
            super::INVALID_YOUTH_SCOUTING_TARGET_POSITION_ERROR
        );
    }

    #[test]
    fn parse_youth_target_position_accepts_goalkeeper() {
        let result = super::parse_youth_target_position(Some("Goalkeeper"));

        assert_eq!(result.unwrap(), Some(Position::Goalkeeper));
    }

    /// Given no active game, When the deadline summary is requested,
    /// Then the command fails with the shared translation key.
    #[test]
    fn deadline_summary_without_a_game_reports_the_no_session_key() {
        let state = StateManager::new();

        let error = get_transfer_deadline_summary_internal(&state).expect_err("no game");

        assert_eq!(error, "be.error.noActiveGameSession");
    }

    /// Given a buyer with one senior place left and a purchase already agreed and waiting to
    ///   register
    /// When the manager accepts an incoming offer from that buyer
    /// Then the promised place is not his: the command fails naming the buyer's list, not "your"
    ///      list, and the stored career is unchanged.
    #[test]
    fn respond_to_offer_internal_refuses_a_buyer_whose_last_place_is_promised() {
        let state = StateManager::new();
        let mut game = make_game();
        fill_senior_list(&mut game, "team-2", 29);
        promise_a_purchase(&mut game, "team-2");
        let before = snapshot(&game);
        state.set_game(game);

        let refused = respond_to_offer_internal(&state, None, "player-1", "offer-1", true);

        assert!(
            refused.as_ref().err().is_some_and(
                |error| error.starts_with("be.error.transfers.buyerSquadFull?club=Buyer%20FC")
            ),
            "{refused:?}"
        );
        let stored = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(snapshot(&stored), before, "nothing moved");
        let player = stored
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("the player");
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
        assert_eq!(
            player.transfer_offers[0].status,
            TransferOfferStatus::Pending
        );
    }

    /// Given a buyer with one senior place left and a purchase already agreed and waiting to
    ///   register
    /// When the manager counters an incoming offer at a fee the buyer would accept
    /// Then the promised place stops the sale: the command fails and the offer is still pending at
    ///      its original fee.
    #[test]
    fn counter_offer_internal_refuses_a_buyer_whose_last_place_is_promised() {
        let state = StateManager::new();
        let mut game = make_game();
        fill_senior_list(&mut game, "team-2", 29);
        promise_a_purchase(&mut game, "team-2");
        let before = snapshot(&game);
        state.set_game(game);

        let refused = counter_offer_internal(&state, None, "player-1", "offer-1", 1_050_000);

        assert!(
            refused.as_ref().err().is_some_and(
                |error| error.starts_with("be.error.transfers.buyerSquadFull?club=Buyer%20FC")
            ),
            "{refused:?}"
        );
        let stored = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(snapshot(&stored), before, "nothing moved");
        let player = stored
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("the player");
        assert_eq!(
            player.transfer_offers[0].status,
            TransferOfferStatus::Pending
        );
        assert_eq!(player.transfer_offers[0].fee, 900_000);
    }

    /// Given a buyer with no senior place left and a reservation
    /// When the manager counters at a fee the buyer would refuse
    /// Then the rejection is still recorded: a refusal is not a signing and needs no place.
    #[test]
    fn counter_offer_internal_rejects_a_fee_even_when_the_buyer_has_no_place() {
        let state = StateManager::new();
        let mut game = make_game();
        fill_senior_list(&mut game, "team-2", 30);
        state.set_game(game);

        let response = counter_offer_internal(&state, None, "player-1", "offer-1", 5_000_000)
            .expect("the negotiation can be ended");

        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::Rejected
        );
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-1"));
    }

    /// Brings `club` up to `seniors` senior players on no wage.
    fn fill_senior_list(game: &mut Game, club: &str, seniors: usize) {
        let mut index = 0;
        while ofm_core::contracts::squad_capacity(game, club).registered_seniors < seniors {
            let mut filler = Player::new(
                format!("filler-{club}-{index}"),
                format!("Filler {index}"),
                format!("Filler {index}"),
                "1998-01-01".to_string(),
                "England".to_string(),
                Position::Midfielder,
                default_attrs(),
            );
            filler.team_id = Some(club.to_string());
            filler.stage_contract_end(Some("2031-06-30".to_string()));
            filler.stage_wage(0);
            filler.market_value = 0;
            game.players.push(filler);
            index += 1;
        }
    }

    /// An agreed purchase waiting to register, at another club, from `buyer_id`.
    fn promise_a_purchase(game: &mut Game, buyer_id: &str) {
        let mut promised = Player::new(
            "promised-purchase".to_string(),
            "P. Romised".to_string(),
            "Promised Purchase".to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            Position::Midfielder,
            default_attrs(),
        );
        promised.team_id = Some("elsewhere".to_string());
        promised.stage_contract_end(Some("2028-06-30".to_string()));
        promised.transfer_offers.push(TransferOffer {
            id: "offer-promised-purchase".to_string(),
            from_team_id: buyer_id.to_string(),
            fee: 1_000_000,
            wage_offered: 0,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            status: TransferOfferStatus::PendingRegistration,
            date: "2026-08-01".to_string(),
            registration_date: Some("2027-01-01".to_string()),
            closed_on: None,
            registration_failure_reason: None,
            terms: Default::default(),
        });
        game.players.push(promised);
    }

    fn snapshot(game: &Game) -> serde_json::Value {
        serde_json::to_value(game).expect("a game serialises")
    }

    /// Loan-listed players of team-2, each a separate target for a loan offer the owner turns down.
    fn game_with_many_loan_targets(count: usize) -> Game {
        let mut game = make_bid_game();
        let template = game.players[0].clone();
        game.players.clear();
        for index in 0..count {
            let mut target = template.clone();
            target.id = format!("loan-target-{index}");
            target.loan_listed = true;
            target.transfer_offers.clear();
            target.loan_offers.clear();
            game.players.push(target);
        }
        game.messages.clear();
        game
    }

    fn posted_messages(state: &StateManager) -> usize {
        state.get_game(|game| game.messages.len()).expect("game")
    }

    /// Given a loan offer being processed while another command posts messages
    /// When both run for a while on separate threads
    /// Then every message is still in the game: the loan command does not put an old copy of the
    ///      game back over what the other command committed.
    #[test]
    fn a_loan_offer_does_not_overwrite_what_another_command_commits() {
        use std::sync::Arc;
        let state = Arc::new(StateManager::new());
        state.set_game(game_with_many_loan_targets(120));
        let poster = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                for index in 0..400 {
                    state.update_game(|game| {
                        game.messages.push(domain::message::InboxMessage::new(
                            format!("concurrent-{index}"),
                            "s".to_string(),
                            "b".to_string(),
                            "f".to_string(),
                            "2026-08-01".to_string(),
                        ));
                    });
                }
            })
        };
        for index in 0..120 {
            let target = format!("loan-target-{index}");
            // Wage share 0: the owner turns it down, which is committed (the offer row) and returns.
            let outcome = make_loan_offer_internal(&state, None, &target, "2027-01-01", 0, None);
            assert!(outcome.is_ok(), "{outcome:?}");
        }
        poster.join().expect("poster");

        let concurrent = state
            .get_game(|game| {
                game.messages
                    .iter()
                    .filter(|message| message.id.starts_with("concurrent-"))
                    .count()
            })
            .expect("game");
        assert_eq!(concurrent, 400, "messages posted meanwhile were lost");
    }

    /// Given a loan offer delivered twice with the same request id (a double click, a retry)
    /// When both deliveries are processed
    /// Then there is one loan, both get the same answer, and the second is not an error.
    #[test]
    fn a_loan_offer_delivered_twice_is_one_loan_with_one_answer() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = true;
        state.set_game(game);

        let first = make_loan_offer_internal(
            &state,
            Some("req-loan"),
            "player-2",
            "2027-01-01",
            100,
            None,
        )
        .expect("first delivery");
        let second = make_loan_offer_internal(
            &state,
            Some("req-loan"),
            "player-2",
            "2027-01-01",
            100,
            None,
        )
        .expect("the repeat replays the first answer, it does not fail");

        assert_eq!(first.decision, LoanOfferDecision::Accepted);
        assert_eq!(second.decision, first.decision);
        assert_eq!(second.offer_id, first.offer_id);
        let (offers, active) = state
            .get_game(|game| {
                let player = game.players.iter().find(|p| p.id == "player-2").unwrap();
                (player.loan_offers.len(), player.active_loan.is_some())
            })
            .expect("game");
        assert_eq!((offers, active), (1, true));
        assert_eq!(
            state.transaction_status("req-loan"),
            ofm_core::transactions::TransactionStatus::Committed
        );
    }

    /// Given a loan offer that is refused (the player is not on the loan list)
    /// When it is sent
    /// Then the game is byte-for-byte what it was, and the request is recorded as refused.
    #[test]
    fn a_refused_loan_offer_leaves_no_trace() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = false;
        state.set_game(game);
        let before = state
            .get_game(|game| serde_json::to_string(game).expect("serialises"))
            .expect("game");

        let refused = make_loan_offer_internal(
            &state,
            Some("req-refused"),
            "player-2",
            "2027-01-01",
            100,
            None,
        );

        assert!(refused.is_err());
        let after = state
            .get_game(|game| serde_json::to_string(game).expect("serialises"))
            .expect("game");
        assert_eq!(before, after);
        assert!(matches!(
            state.transaction_status("req-refused"),
            ofm_core::transactions::TransactionStatus::Refused(_)
        ));
    }

    /// Given an accepted purchase option exercised twice with one request id
    /// When both deliveries are processed
    /// Then the club is charged once, and the repeat is not an error.
    #[test]
    fn exercising_a_buy_option_twice_with_one_request_id_charges_once() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = true;
        state.set_game(game);
        make_loan_offer_internal(&state, None, "player-2", "2027-01-01", 100, Some(2_000_000))
            .expect("a loan with a purchase option");
        let finance_before = state
            .get_game(|game| {
                game.teams
                    .iter()
                    .find(|t| t.id == "team-1")
                    .unwrap()
                    .finance
            })
            .expect("game");

        let first = exercise_loan_buy_option_internal(&state, Some("req-buy"), "player-2");
        let second = exercise_loan_buy_option_internal(&state, Some("req-buy"), "player-2");

        assert!(first.is_ok(), "{:?}", first.err());
        assert!(second.is_ok(), "{:?}", second.err());
        let finance_after = state
            .get_game(|game| {
                game.teams
                    .iter()
                    .find(|t| t.id == "team-1")
                    .unwrap()
                    .finance
            })
            .expect("game");
        assert_eq!(finance_before - finance_after, 2_000_000);
    }

    /// Given a loan agreed while the window is shut, so it waits to register
    /// When the game is saved and loaded as JSON in between and the window opens
    /// Then the loan is still pending after the reload, registers once, and is not duplicated.
    #[test]
    fn a_loan_pending_registration_survives_a_reload_and_registers_once() {
        let state = StateManager::new();
        let mut game = make_bid_game();
        game.players[0].loan_listed = true;
        game.season_context.transfer_window.status = TransferWindowStatus::Closed;
        game.season_context.transfer_window.opens_on = Some("2026-09-01".to_string());
        state.set_game(game);

        let response = make_loan_offer_internal(&state, None, "player-2", "2027-01-01", 100, None)
            .expect("agreed, pending registration");
        assert_eq!(response.decision, LoanOfferDecision::Accepted);
        let json = state
            .get_game(|game| serde_json::to_string(game).expect("serialises"))
            .expect("game");
        let mut reloaded: Game = serde_json::from_str(&json).expect("reloads");
        assert_eq!(
            reloaded.players[0].loan_offers[0].status,
            LoanOfferStatus::PendingRegistration
        );
        assert!(reloaded.players[0].active_loan.is_none());

        reloaded.clock.current_date = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        reloaded.season_context.transfer_window.status = TransferWindowStatus::Open;
        ofm_core::transfers::process_pending_loan_registrations(&mut reloaded);
        ofm_core::transfers::process_pending_loan_registrations(&mut reloaded);

        assert!(reloaded.players[0].active_loan.is_some());
        assert_eq!(reloaded.players[0].loan_offers.len(), 1);
        assert_eq!(reloaded.players[0].team_id.as_deref(), Some("team-1"));
    }

    /// Given no request id and an unknown one
    /// When the status of the unknown one is asked
    /// Then nothing happened for it.
    #[test]
    fn an_unseen_request_has_no_effect_to_report() {
        let state = StateManager::new();
        state.set_game(make_bid_game());
        assert_eq!(
            state.transaction_status("never-sent"),
            ofm_core::transactions::TransactionStatus::Unknown
        );
        assert_eq!(posted_messages(&state), 0);
    }

    /// Given a club with a surplus midfielder and only two goalkeepers
    /// When the manager's departure blockers are read through the command
    /// Then the keepers are named with the key a sale of either would return, the midfielder is not,
    ///      and an unemployed manager has none
    #[test]
    fn departure_blockers_name_the_players_a_sale_would_be_refused_for() {
        let state = StateManager::new();
        let mut game = make_game();
        // Two senior keepers, and outfield depth so only the keepers are held.
        game.players.retain(|p| {
            p.position != Position::Goalkeeper || p.team_id.as_deref() != Some("team-1")
        });
        for index in 0..2 {
            let mut keeper = game.players[0].clone();
            keeper.id = format!("keeper-{index}");
            keeper.position = Position::Goalkeeper;
            keeper.team_id = Some("team-1".to_string());
            keeper.transfer_offers.clear();
            game.players.push(keeper);
        }
        state.set_game(game);

        let blockers = get_departure_blockers_internal(&state).expect("blockers");

        assert!(
            blockers
                .get("keeper-0")
                .is_some_and(|key| key.starts_with("be.error.squadFloor.")),
            "{blockers:?}"
        );
        state.update_game(|game| game.manager.team_id = None);
        assert!(get_departure_blockers_internal(&state)
            .expect("blockers")
            .is_empty());
    }
}
