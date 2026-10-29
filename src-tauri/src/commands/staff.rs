use log::info;
use std::sync::Arc;
use tauri::State;

use domain::delegation::{DelegationState, Duty};
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
        has_assistant: members.iter().any(|s| s.post() == StaffPost::AssistantManager),
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
    mutate_active_game(&state, |game| set_duty(game, duty, delegated, confirm_major))
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
    use domain::delegation::Duty;
    use domain::staff::StaffPost;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
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
        assert!(stored.staff.iter().find(|s| s.id == "staff-1").unwrap().team_id.is_none());

        state.set_game(with_dangling_team_id(make_game_with_employed_staff()));
        assert_eq!(
            release_staff_internal(&state, "staff-1").err(),
            Some("be.error.teamNotFound".into())
        );
        let stored = state.get_game(|g| g.clone()).unwrap();
        assert_eq!(
            stored.staff.iter().find(|s| s.id == "staff-1").unwrap().team_id.as_deref(),
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
            physios.iter().map(|c| c.staff.id.as_str()).collect::<Vec<_>>(),
            ["p1", "p2"]
        );
        assert!(physios.iter().all(|c| c.blocker.is_none()));
        let strong = search_candidates_of(&game, None, 60).unwrap();
        assert_eq!(strong.len(), 1);

        let mut broke = game.clone();
        broke.teams[0].finance = 0;
        let blocked = search_candidates_of(&broke, Some(StaffPost::Physio), 0).unwrap();
        assert_eq!(blocked[0].blocker.as_deref(), Some("be.error.staff.notEnoughCash"));
    }

    #[test]
    fn delegating_needs_an_assistant_and_taking_back_always_works() {
        let mut game = with_posts(
            make_game(),
            &[("p1", StaffPost::Physio, 50, 2_000, true)],
        );
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

    #[test]
    fn the_assistant_without_a_squad_has_nothing_to_apply_but_the_gap() {
        let game = with_posts(make_game(), &[("p1", StaffPost::Physio, 50, 2_000, true)]);
        let recs = assistant::recommendations(&game, "team-1");
        assert!(recs.iter().any(|r| r.id == "no-assistant"));
    }
}
