use log::info;
use std::sync::Arc;
use tauri::State;

use domain::delegation::{DelegationLimits, DelegationMode, DelegationState, Duty};
use domain::staff::{Staff, StaffPost};
use ofm_core::game::Game;
use ofm_core::staff_system::assistant::{self, Recommendation};
use ofm_core::staff_system::market::{self, OfferResponse, PostStatus};
use ofm_core::staff_system::{delegation, profile};
use ofm_core::state::StateManager;
use serde::Serialize;

use crate::commands::util::{mutate_active_game, user_team_id};

const NO_ACTIVE_GAME: &str = "be.error.noActiveGameSession";
const CANDIDATE_LIMIT: usize = 40;

/// What the manager sees of one contracted member beyond the record itself.
#[derive(Debug, Clone, Serialize)]
pub struct MemberView {
    pub id: String,
    pub post: StaffPost,
    pub overall: u8,
    /// `None` for an open-ended contract.
    pub days_left: Option<i64>,
    /// What releasing them costs today.
    pub severance: i64,
    /// What they ask to stay.
    pub renewal_ask: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaffBudgetView {
    pub budget: i64,
    pub weekly_bill: i64,
    pub headroom: i64,
    pub signing_fee_weeks: i64,
    pub max_years: u8,
    pub severance_pct: i64,
}

/// The staff of the managed club: seats, members' contract terms, budget and delegation.
#[derive(Debug, Clone, Serialize)]
pub struct StaffOverview {
    pub team_id: String,
    pub posts: Vec<PostStatus>,
    pub members: Vec<MemberView>,
    pub budget: StaffBudgetView,
    pub delegation: DelegationState,
    /// The preset the duty settings amount to (derived: it is not stored).
    pub delegation_mode: DelegationMode,
    pub has_assistant: bool,
}

/// A free agent as a candidate for one of the club's seats.
#[derive(Debug, Clone, Serialize)]
pub struct CandidateView {
    pub staff: Staff,
    pub post: StaffPost,
    pub overall: u8,
    pub asking_wage: u32,
    /// The translation key of what stops the club from signing them at their ask, if anything.
    pub blocker: Option<String>,
}

/// The answer to an offer, with the game as it now stands.
#[derive(Debug, Clone, Serialize)]
pub struct OfferOutcome {
    pub response: OfferResponse,
    pub game: Game,
}

fn with_game<R>(
    state: &StateManager,
    read: impl FnOnce(&Game) -> Result<R, String>,
) -> Result<R, String> {
    state
        .get_game(read)
        .unwrap_or_else(|| Err(NO_ACTIVE_GAME.to_string()))
}

pub fn staff_overview_of(game: &Game) -> Result<StaffOverview, String> {
    let team_id = user_team_id(game)?;
    let members = profile::team_staff(game, &team_id);
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.reputation)
        .ok_or("be.error.teamNotFound")?;
    Ok(StaffOverview {
        posts: market::structure_of(&members),
        members: members
            .iter()
            .map(|s| MemberView {
                id: s.id.clone(),
                post: s.post(),
                overall: s.overall(),
                days_left: market::days_left(game, s),
                severance: market::severance(game, s),
                renewal_ask: market::renewal_ask(s, reputation),
            })
            .collect(),
        budget: StaffBudgetView {
            budget: market::staff_budget(game, &team_id),
            weekly_bill: market::weekly_staff_bill(game, &team_id),
            headroom: market::staff_headroom(game, &team_id),
            signing_fee_weeks: market::SIGNING_FEE_WEEKS,
            max_years: market::MAX_YEARS,
            severance_pct: market::SEVERANCE_PCT,
        },
        delegation: game.delegation.clone(),
        delegation_mode: game.delegation.mode(),
        has_assistant: members
            .iter()
            .any(|s| s.post() == StaffPost::AssistantManager),
        team_id,
    })
}

pub fn search_candidates_of(
    game: &Game,
    post: Option<StaffPost>,
    min_overall: u8,
) -> Result<Vec<CandidateView>, String> {
    let team_id = user_team_id(game)?;
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.reputation)
        .ok_or("be.error.teamNotFound")?;
    let mut found: Vec<CandidateView> = game
        .staff
        .iter()
        .filter(|s| s.team_id.is_none())
        .filter(|s| post.is_none_or(|p| s.post() == p))
        .filter(|s| s.overall() >= min_overall)
        .map(|s| {
            let ask = market::asking_wage(s, reputation);
            CandidateView {
                post: s.post(),
                overall: s.overall(),
                asking_wage: ask,
                blocker: market::hire_blocker(game, &team_id, &s.id, ask).map(str::to_string),
                staff: s.clone(),
            }
        })
        .collect();
    found.sort_by(|a, b| b.overall.cmp(&a.overall).then(a.staff.id.cmp(&b.staff.id)));
    found.truncate(CANDIDATE_LIMIT);
    Ok(found)
}

#[tauri::command]
pub fn get_staff_overview(state: State<'_, Arc<StateManager>>) -> Result<StaffOverview, String> {
    with_game(&state, staff_overview_of)
}

#[tauri::command]
pub fn search_staff_candidates(
    state: State<'_, Arc<StateManager>>,
    post: Option<StaffPost>,
    min_overall: Option<u8>,
) -> Result<Vec<CandidateView>, String> {
    with_game(&state, |game| {
        search_candidates_of(game, post, min_overall.unwrap_or(0))
    })
}

/// Offers `wage` per week for `years` to a free agent. Accepted: signed. Counter: nothing
/// changes, the answer carries the wage that would be accepted; accepting a counter-offer is
/// the same call with that wage, refusing it is not calling again.
#[tauri::command]
pub fn propose_staff_contract(
    state: State<'_, Arc<StateManager>>,
    staff_id: String,
    wage: u32,
    years: u8,
) -> Result<OfferOutcome, String> {
    propose_staff_contract_internal(&state, &staff_id, wage, years)
}

pub fn propose_staff_contract_internal(
    state: &StateManager,
    staff_id: &str,
    wage: u32,
    years: u8,
) -> Result<OfferOutcome, String> {
    info!("[cmd] propose_staff_contract: {staff_id} wage={wage} years={years}");
    offer(state, |game, team_id| {
        market::negotiate_hire(game, team_id, staff_id, wage, years)
    })
}

/// Offers a renewal to a member of the club.
#[tauri::command]
pub fn renew_staff_contract(
    state: State<'_, Arc<StateManager>>,
    staff_id: String,
    wage: u32,
    years: u8,
) -> Result<OfferOutcome, String> {
    info!("[cmd] renew_staff_contract: {staff_id} wage={wage} years={years}");
    offer(&state, |game, team_id| {
        market::negotiate_renewal(game, team_id, &staff_id, wage, years)
    })
}

fn offer(
    state: &StateManager,
    act: impl FnOnce(&mut Game, &str) -> Result<OfferResponse, String>,
) -> Result<OfferOutcome, String> {
    state
        .update_game(|game| {
            let team_id = user_team_id(game)?;
            let response = act(game, &team_id)?;
            Ok(OfferOutcome {
                response,
                game: game.ui_clone(),
            })
        })
        .unwrap_or_else(|| Err(NO_ACTIVE_GAME.to_string()))
}

/// Signs a free agent at their asking wage for two years (the MCP tool's simple path).
#[cfg_attr(not(feature = "mcp"), allow(dead_code))]
pub fn hire_staff_internal(state: &StateManager, staff_id: &str) -> Result<Game, String> {
    info!("[cmd] hire_staff: staff_id={}", staff_id);
    mutate_active_game(state, |game| {
        let team_id = user_team_id(game)?;
        let reputation = game
            .teams
            .iter()
            .find(|t| t.id == team_id)
            .map(|t| t.reputation)
            .ok_or("be.error.teamNotFound")?;
        let wage = game
            .staff
            .iter()
            .find(|s| s.id == staff_id)
            .map(|s| market::asking_wage(s, reputation))
            .ok_or("be.error.staffMemberNotFound")?;
        market::hire(game, &team_id, staff_id, wage, 2)
    })
}

/// Releases a member of the club, paying the severance.
#[tauri::command]
pub fn release_staff(
    state: State<'_, Arc<StateManager>>,
    staff_id: String,
) -> Result<Game, String> {
    release_staff_internal(&state, &staff_id)
}

pub fn release_staff_internal(state: &StateManager, staff_id: &str) -> Result<Game, String> {
    info!("[cmd] release_staff: staff_id={}", staff_id);
    mutate_active_game(state, |game| {
        let team_id = user_team_id(game)?;
        market::release(game, &team_id, staff_id).map(|_| ())
    })
}

/// Hands a responsibility to the assistant (or takes it back).
#[tauri::command]
pub fn set_duty_delegation(
    state: State<'_, Arc<StateManager>>,
    duty: Duty,
    delegated: bool,
    confirm_major: bool,
) -> Result<Game, String> {
    info!("[cmd] set_duty_delegation: {duty:?} delegated={delegated} confirm={confirm_major}");
    mutate_active_game(&state, |game| {
        set_duty(game, duty, delegated, confirm_major)
    })
}

pub fn set_duty(
    game: &mut Game,
    duty: Duty,
    delegated: bool,
    confirm_major: bool,
) -> Result<(), String> {
    let team_id = user_team_id(game)?;
    let has_assistant = profile::team_staff(game, &team_id)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    if delegated && !has_assistant {
        return Err("be.error.delegation.noAssistant".to_string());
    }
    game.delegation.set(duty, delegated, confirm_major);
    Ok(())
}

/// Hands the whole job to the assistant (or takes it all back) in one of the three presets.
#[tauri::command]
pub fn set_delegation_mode(
    state: State<'_, Arc<StateManager>>,
    mode: DelegationMode,
) -> Result<Game, String> {
    info!("[cmd] set_delegation_mode: {mode:?}");
    mutate_active_game(&state, |game| set_mode(game, mode))
}

/// Applies a preset. Delegating needs an assistant; taking everything back never does. `Custom`
/// only describes duties set one by one, so it is not something to apply.
pub fn set_mode(game: &mut Game, mode: DelegationMode) -> Result<(), String> {
    let team_id = user_team_id(game)?;
    if mode == DelegationMode::Custom {
        return Err("be.error.delegation.invalidMode".to_string());
    }
    let has_assistant = profile::team_staff(game, &team_id)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    if mode != DelegationMode::Manual && !has_assistant {
        return Err("be.error.delegation.noAssistant".to_string());
    }
    game.delegation.apply_mode(mode);
    Ok(())
}

/// Sets what the assistant may commit without asking.
#[tauri::command]
pub fn set_delegation_limits(
    state: State<'_, Arc<StateManager>>,
    limits: DelegationLimits,
) -> Result<Game, String> {
    info!("[cmd] set_delegation_limits: {limits:?}");
    mutate_active_game(&state, |game| set_limits(game, limits))
}

pub fn set_limits(game: &mut Game, mut limits: DelegationLimits) -> Result<(), String> {
    user_team_id(game)?;
    if limits.max_commitment < 0 || limits.max_transfer_fee < 0 {
        return Err("be.error.delegation.invalidLimits".to_string());
    }
    // Total authority is given and taken back by choosing a mode, never by editing a ceiling: a
    // caller that omits the flag must not revoke it, and one that sets it must not grant it.
    limits.finance_bound = game.delegation.limits.finance_bound;
    game.delegation.limits = limits;
    Ok(())
}

/// Tells the assistant a player is not to be sold (or that he may be again). Enforced in the
/// backend: the assistant turns down every offer for a protected player and never lists him.
#[tauri::command]
pub fn set_player_protected(
    state: State<'_, Arc<StateManager>>,
    player_id: String,
    protected: bool,
) -> Result<Game, String> {
    info!("[cmd] set_player_protected: {player_id} protected={protected}");
    mutate_active_game(&state, |game| set_protected(game, &player_id, protected))
}

pub fn set_protected(game: &mut Game, player_id: &str, protected: bool) -> Result<(), String> {
    let team_id = user_team_id(game)?;
    let at_club = game
        .players
        .iter()
        .any(|p| p.id == player_id && p.team_id.as_deref() == Some(team_id.as_str()));
    if !at_club {
        return Err("be.error.playerNotInClub".to_string());
    }
    game.delegation.set_protected(player_id, protected);
    Ok(())
}

/// The manager answers a proposal the assistant left for them.
#[tauri::command]
pub fn resolve_delegation_proposal(
    state: State<'_, Arc<StateManager>>,
    proposal_id: String,
    accept: bool,
) -> Result<Game, String> {
    info!("[cmd] resolve_delegation_proposal: {proposal_id} accept={accept}");
    mutate_active_game(&state, |game| {
        delegation::resolve_proposal(game, &proposal_id, accept)
    })
}

#[tauri::command]
pub fn get_assistant_recommendations(
    state: State<'_, Arc<StateManager>>,
) -> Result<Vec<Recommendation>, String> {
    with_game(&state, |game| {
        let team_id = user_team_id(game)?;
        Ok(assistant::recommendations(game, &team_id))
    })
}

/// Carries out one recommendation. It is looked up again by id rather than trusted from the
/// caller: the action is what the assistant would say today.
#[tauri::command]
pub fn apply_assistant_recommendation(
    state: State<'_, Arc<StateManager>>,
    recommendation_id: String,
) -> Result<Game, String> {
    info!("[cmd] apply_assistant_recommendation: {recommendation_id}");
    mutate_active_game(&state, |game| {
        let team_id = user_team_id(game)?;
        let rec = assistant::recommendations(game, &team_id)
            .into_iter()
            .find(|r| r.id == recommendation_id)
            .ok_or("be.error.assistant.recommendationGone")?;
        assistant::apply_action(game, &team_id, &rec.action)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::delegation::Duty;
    use domain::manager::Manager;
    use domain::staff::StaffPost;
    use domain::staff::{Staff, StaffAttributes, StaffRole};
    use domain::team::Team;
    use ofm_core::clock::GameClock;
    use ofm_core::game::Game;
    use ofm_core::state::StateManager;

    fn make_team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "User FC".to_string(),
            "USR".to_string(),
            "England".to_string(),
            "London".to_string(),
            "User Ground".to_string(),
            25_000,
        );
        team.manager_id = Some("manager-1".to_string());
        team
    }

    fn make_staff() -> Staff {
        let mut staff = Staff::new(
            "staff-1".to_string(),
            "Alex".to_string(),
            "Coach".to_string(),
            "1985-01-01".to_string(),
            StaffRole::Coach,
            StaffAttributes {
                coaching: 70,
                judging_ability: 50,
                judging_potential: 50,
                physiotherapy: 30,
            },
        );
        staff.wage = 12_000;
        staff
    }

    fn make_employed_staff() -> Staff {
        let mut staff = make_staff();
        staff.team_id = Some("team-1".to_string());
        staff
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

        Game::new(
            clock,
            manager,
            vec![make_team()],
            vec![],
            vec![make_staff()],
            vec![],
        )
    }

    fn make_game_with_employed_staff() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        Game::new(
            clock,
            manager,
            vec![make_team()],
            vec![],
            vec![make_employed_staff()],
            vec![],
        )
    }

    /// The manager holds a team id that no club in the world has. Both commands
    /// used to swallow that: the wage adjustment sat behind `if let Some(team)`
    /// with no else, so the staff record changed and the club's books did not.
    fn with_dangling_team_id(mut game: Game) -> Game {
        game.teams[0].id = "team-elsewhere".to_string();
        game
    }

    fn with_posts(mut game: Game, members: &[(&str, StaffPost, u8, u32, bool)]) -> Game {
        game.staff.clear();
        for (id, post, level, wage, employed) in members {
            let mut s = make_staff();
            s.id = (*id).to_string();
            s.display_name = (*id).to_string();
            s.role = post.role();
            s.specialization = post.specialization();
            s.post = Some(*post);
            s.attributes.coaching = *level;
            s.attributes.judging_ability = *level;
            s.attributes.judging_potential = *level;
            s.attributes.physiotherapy = *level;
            s.wage = *wage;
            s.team_id = employed.then(|| "team-1".to_string());
            game.staff.push(s);
        }
        game.teams[0].finance = 1_000_000;
        game.teams[0].wage_budget = 200_000;
        game.teams[0].reputation = 700;
        game
    }

    #[test]
    fn hiring_and_releasing_report_a_team_id_that_matches_no_club() {
        let state = StateManager::new();
        state.set_game(with_dangling_team_id(make_game()));
        assert_eq!(
            hire_staff_internal(&state, "staff-1").err(),
            Some("be.error.teamNotFound".into())
        );
        let stored = state.get_game(|g| g.clone()).unwrap();
        assert!(stored
            .staff
            .iter()
            .find(|s| s.id == "staff-1")
            .unwrap()
            .team_id
            .is_none());

        state.set_game(with_dangling_team_id(make_game_with_employed_staff()));
        assert_eq!(
            release_staff_internal(&state, "staff-1").err(),
            Some("be.error.teamNotFound".into())
        );
        let stored = state.get_game(|g| g.clone()).unwrap();
        assert_eq!(
            stored
                .staff
                .iter()
                .find(|s| s.id == "staff-1")
                .unwrap()
                .team_id
                .as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn an_accepted_offer_signs_with_the_contract_and_pays_the_fee() {
        let state = StateManager::new();
        state.set_game(with_posts(
            make_game(),
            &[("p1", StaffPost::Physio, 50, 2_000, false)],
        ));
        let before = state.get_game(|g| g.teams[0].finance).unwrap();
        let out = propose_staff_contract_internal(&state, "p1", 2_000, 3).unwrap();
        assert_eq!(out.response, OfferResponse::Accepted);
        let hired = out.game.staff.iter().find(|s| s.id == "p1").unwrap();
        assert_eq!(hired.team_id.as_deref(), Some("team-1"));
        assert_eq!(hired.wage, 2_000);
        assert_eq!(hired.contract_end.as_deref(), Some("2029-08-01"));
        let after = state.get_game(|g| g.teams[0].finance).unwrap();
        assert_eq!(before - after, 2_000);
    }

    #[test]
    fn a_counter_offer_changes_nothing_until_it_is_accepted_at_its_wage() {
        let state = StateManager::new();
        state.set_game(with_posts(
            make_game(),
            &[("p1", StaffPost::Physio, 50, 2_000, false)],
        ));
        let out = propose_staff_contract_internal(&state, "p1", 1_900, 2).unwrap();
        let OfferResponse::Counter { wage } = out.response else {
            panic!("expected a counter-offer, got {:?}", out.response);
        };
        assert_eq!(wage, 2_000);
        assert!(state.get_game(|g| g.staff[0].team_id.is_none()).unwrap());
        // Refusing is not calling again; accepting is calling with the counter's wage.
        let accepted = propose_staff_contract_internal(&state, "p1", wage, 2).unwrap();
        assert_eq!(accepted.response, OfferResponse::Accepted);
        let refused = propose_staff_contract_internal(&state, "p1", 100, 2);
        assert!(refused.is_err(), "already employed by now");
    }

    #[test]
    fn a_low_offer_is_refused_and_the_budget_blocks_a_signing() {
        let state = StateManager::new();
        state.set_game(with_posts(
            make_game(),
            &[("p1", StaffPost::Physio, 50, 7_000, false)],
        ));
        let out = propose_staff_contract_internal(&state, "p1", 500, 2).unwrap();
        assert_eq!(out.response, OfferResponse::Refused);
        state.update_game(|g| g.teams[0].wage_budget = 0).unwrap();
        let blocked = propose_staff_contract_internal(&state, "p1", 7_000, 2);
        assert_eq!(blocked.err(), Some("be.error.staff.overBudget".into()));
    }

    #[test]
    fn releasing_pays_the_severance_and_frees_the_seat() {
        let state = StateManager::new();
        state.set_game(with_posts(
            make_game(),
            &[("p1", StaffPost::Physio, 50, 2_000, true)],
        ));
        let before = state.get_game(|g| g.teams[0].finance).unwrap();
        let severance = state
            .get_game(|g| market::severance(g, &g.staff[0]))
            .unwrap();
        let game = release_staff_internal(&state, "p1").unwrap();
        assert!(game.staff[0].team_id.is_none());
        let after = state.get_game(|g| g.teams[0].finance).unwrap();
        assert_eq!(before - after, severance);
        assert!(severance >= 2_000 * market::SEVERANCE_MIN_WEEKS);
    }

    #[test]
    fn the_overview_and_candidate_search_describe_seats_budget_and_blockers() {
        let game = with_posts(
            make_game(),
            &[
                ("a1", StaffPost::AssistantManager, 60, 3_000, true),
                ("p1", StaffPost::Physio, 70, 2_000, false),
                ("p2", StaffPost::Physio, 40, 1_000, false),
                ("s1", StaffPost::Scout, 55, 1_500, false),
            ],
        );
        let overview = staff_overview_of(&game).unwrap();
        assert_eq!(overview.posts.len(), StaffPost::ALL.len());
        assert!(overview.has_assistant);
        assert_eq!(overview.members.len(), 1);
        assert_eq!(overview.budget.weekly_bill, 3_000);
        assert_eq!(
            overview.budget.headroom,
            overview.budget.budget - overview.budget.weekly_bill
        );

        let physios = search_candidates_of(&game, Some(StaffPost::Physio), 0).unwrap();
        assert_eq!(
            physios
                .iter()
                .map(|c| c.staff.id.as_str())
                .collect::<Vec<_>>(),
            ["p1", "p2"]
        );
        assert!(physios.iter().all(|c| c.blocker.is_none()));
        let strong = search_candidates_of(&game, None, 60).unwrap();
        assert_eq!(strong.len(), 1);

        let mut broke = game.clone();
        broke.teams[0].finance = 0;
        let blocked = search_candidates_of(&broke, Some(StaffPost::Physio), 0).unwrap();
        assert_eq!(
            blocked[0].blocker.as_deref(),
            Some("be.error.staff.notEnoughCash")
        );
    }

    #[test]
    fn delegating_needs_an_assistant_and_taking_back_always_works() {
        let mut game = with_posts(make_game(), &[("p1", StaffPost::Physio, 50, 2_000, true)]);
        assert_eq!(
            set_duty(&mut game, Duty::Training, true, false),
            Err("be.error.delegation.noAssistant".to_string())
        );
        assert!(!game.delegation.is_delegated(Duty::Training));
        let mut game = with_posts(
            game,
            &[("a1", StaffPost::AssistantManager, 60, 3_000, true)],
        );
        set_duty(&mut game, Duty::Training, true, true).unwrap();
        assert!(game.delegation.is_delegated(Duty::Training));
        assert!(game.delegation.needs_confirmation(Duty::Training));
        game.staff.clear();
        set_duty(&mut game, Duty::Training, false, false).unwrap();
        assert!(!game.delegation.is_delegated(Duty::Training));
    }

    /// Given a club with no assistant, when the manager picks a preset, then delegating is
    /// refused and taking back is not; with one, each preset is applied to every duty and the
    /// overview reads it back.
    #[test]
    fn presets_need_an_assistant_to_delegate_and_never_to_take_back() {
        let mut game = with_posts(make_game(), &[("p1", StaffPost::Physio, 50, 2_000, true)]);
        assert_eq!(
            set_mode(&mut game, DelegationMode::Full),
            Err("be.error.delegation.noAssistant".to_string())
        );
        assert_eq!(game.delegation.mode(), DelegationMode::Manual);
        let mut game = with_posts(
            game,
            &[("a1", StaffPost::AssistantManager, 60, 3_000, true)],
        );
        set_mode(&mut game, DelegationMode::Approval).unwrap();
        assert_eq!(
            staff_overview_of(&game).unwrap().delegation_mode,
            DelegationMode::Approval
        );
        set_mode(&mut game, DelegationMode::Full).unwrap();
        assert!(Duty::ALL.iter().all(|d| game.delegation.is_delegated(*d)));
        assert!(Duty::ALL
            .iter()
            .all(|d| !game.delegation.needs_confirmation(*d)));
        assert_eq!(
            set_mode(&mut game, DelegationMode::Custom),
            Err("be.error.delegation.invalidMode".to_string())
        );
        game.staff.clear();
        set_mode(&mut game, DelegationMode::Manual).unwrap();
        assert_eq!(game.delegation.mode(), DelegationMode::Manual);
    }

    /// Given spending limits, when they are set, then they are kept; a negative ceiling is
    /// refused and leaves the old limits alone.
    #[test]
    fn spending_limits_are_kept_and_nonsense_is_refused() {
        let mut game = make_game();
        let limits = DelegationLimits {
            max_staff_wage: 2_500,
            max_player_wage: 9_000,
            max_commitment: 1_000_000,
            max_transfer_fee: 250_000,
            finance_bound: false,
        };
        set_limits(&mut game, limits.clone()).unwrap();
        assert_eq!(game.delegation.limits, limits);
        let bad = DelegationLimits {
            max_commitment: -1,
            ..limits.clone()
        };
        assert_eq!(
            set_limits(&mut game, bad),
            Err("be.error.delegation.invalidLimits".to_string())
        );
        assert_eq!(game.delegation.limits, limits);
    }

    #[test]
    fn the_assistant_without_a_squad_has_nothing_to_apply_but_the_gap() {
        let game = with_posts(make_game(), &[("p1", StaffPost::Physio, 50, 2_000, true)]);
        let recs = assistant::recommendations(&game, "team-1");
        assert!(recs.iter().any(|r| r.id == "no-assistant"));
    }

    fn game_with_an_assistant_and_a_player() -> Game {
        let mut game = with_posts(
            make_game(),
            &[("assist", StaffPost::AssistantManager, 70, 1_000, true)],
        );
        game.players.push(a_player("p1", "team-1"));
        game
    }

    fn a_player(id: &str, club: &str) -> domain::player::Player {
        let attributes = domain::player::PlayerAttributes {
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
            handling: 60,
            reflexes: 60,
            aerial: 60,
        };
        let mut player = domain::player::Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            domain::player::Position::Midfielder,
            attributes,
        );
        player.team_id = Some(club.to_string());
        player.ovr = 70;
        player
    }

    /// Given Total authority
    /// When the manager edits the ceilings with a limits value that leaves the flag off (an older
    /// screen, or any caller)
    /// Then the authority is untouched: ceilings never revoke or grant it.
    #[test]
    fn editing_the_ceilings_never_changes_the_total_authority() {
        let mut game = game_with_an_assistant_and_a_player();
        set_mode(&mut game, DelegationMode::Full).unwrap();
        assert!(game.delegation.limits.finance_bound);

        let mut edited = game.delegation.limits.clone();
        edited.finance_bound = false;
        edited.max_transfer_fee = 5_000_000;
        set_limits(&mut game, edited).unwrap();
        assert!(game.delegation.limits.finance_bound, "not revoked");
        assert_eq!(game.delegation.limits.max_transfer_fee, 5_000_000);

        set_mode(&mut game, DelegationMode::Approval).unwrap();
        let mut grab = game.delegation.limits.clone();
        grab.finance_bound = true;
        set_limits(&mut game, grab).unwrap();
        assert!(!game.delegation.limits.finance_bound, "not granted");
    }

    /// Given the manager's player and another club's
    /// When the manager protects each
    /// Then only his own can be protected, and protecting twice is one entry.
    #[test]
    fn only_a_player_of_the_managers_club_can_be_protected() {
        let mut game = game_with_an_assistant_and_a_player();
        game.players.push(a_player("p2", "elsewhere"));
        set_protected(&mut game, "p1", true).unwrap();
        set_protected(&mut game, "p1", true).unwrap();
        assert_eq!(game.delegation.protected_players, vec!["p1".to_string()]);
        assert_eq!(
            set_protected(&mut game, "p2", true).err(),
            Some("be.error.playerNotInClub".to_string())
        );
        set_protected(&mut game, "p1", false).unwrap();
        assert!(game.delegation.protected_players.is_empty());
    }
}
