//! The playable-career journey, driven through the same application-layer functions the Tauri
//! commands call (no window, no IPC): choose a squad and tactics, advance the calendar, play the
//! user's match, bid for a player, answer the assistant, save, reload and play on.
//!
//! This is a backend integration test of the journey's seams. It is **not** a GUI test: nothing
//! here renders a component or clicks a button, and it says nothing about whether a screen is
//! usable (see `docs/APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`).

use db::save_manager::SaveManager;
use domain::delegation::{DelegationMode, ProposalAction};
use domain::player::Player;
use domain::stats::StatsState;
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use ofm_core::interactive_match::{AdvanceRequest, AdvanceTarget, StopReason};
use ofm_core::state::StateManager;
use season_harness::fingerprint::{fingerprint, first_difference};
use season_harness::worlds::WorldSpec;
use tempfile::TempDir;

use crate::application::career::install_career;
use crate::application::interactive_match::{
    advance_interactive_match_internal, finish_interactive_match_internal,
    start_interactive_match_internal,
};
use crate::application::saving::persist_active_game;
use crate::application::time_advancement::advance_time_with_mode;
use crate::commands::season::advance_to_next_season_internal;
use crate::commands::squad::{
    apply_tactic_preset_internal, set_formation_internal, set_starting_xi_internal,
};
use crate::commands::staff::set_mode;
use crate::commands::transfers::make_transfer_bid_internal;
use crate::commands::util::mutate_active_game;

const SEED: u64 = 1;
/// More days than any season has, so a loop that never finds its target fails instead of hanging.
const DAY_GUARD: u32 = 460;

/// One career on disk: the state a player's session holds, and the folder their saves live in.
struct Session {
    state: StateManager,
    saves: SaveManager,
    dir: TempDir,
}

impl Session {
    /// A new career with a generated world and the first save, as "new game" leaves it.
    fn begin() -> Self {
        let game = WorldSpec::gate(SEED).build().expect("a generated world");
        let dir = tempfile::tempdir().expect("a temporary folder");
        let mut saves = SaveManager::init(dir.path()).expect("a save folder");
        let save_id = saves
            .create_save_with_stats(&game, &StatsState::default(), "journey")
            .expect("the opening save");
        let state = StateManager::new();
        install_career(&state, game, StatsState::default(), Some(save_id));
        Self { state, saves, dir }
    }

    fn game(&self) -> Game {
        self.state.get_game(|g| g.clone()).expect("an active game")
    }

    /// The save button, then closing the session and opening the save again as "load game" does.
    /// Returns the game exactly as the file held it, before the load command's own refresh.
    fn save_close_and_reopen(&mut self) -> Game {
        persist_active_game(&self.state, &mut self.saves).expect("the save succeeds");
        let save_id = self.state.get_save_id().expect("the career has a save");
        let before = fingerprint(&self.game());
        let mut reopened_saves = SaveManager::init(self.dir.path()).expect("the folder reopens");
        let stored = reopened_saves.load_game(&save_id).expect("the save loads");
        let after = fingerprint(&stored);
        assert!(
            before == after,
            "the reloaded career differs from the saved one: {}",
            first_difference(&before, &after)
        );
        let stats = reopened_saves
            .load_stats_state(&save_id)
            .expect("stats load");
        let mut installed = stored.clone();
        ofm_core::ai_hiring::seed_ai_managers(&mut installed);
        ofm_core::season_context::refresh_game_context(&mut installed);
        let state = StateManager::new();
        install_career(&state, installed, stats, Some(save_id));
        self.state = state;
        self.saves = reopened_saves;
        stored
    }
}

fn user_team_id(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn user_squad(game: &Game) -> Vec<&Player> {
    let team = user_team_id(game);
    game.players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team.as_str()))
        .collect()
}

/// Matches the club's own table rows say it has played, summed over its competitions. Friendlies
/// and cups do not count for a table, so this is not "matches played".
fn own_played(game: &Game) -> u32 {
    let team = user_team_id(game);
    game.competitions
        .iter()
        .flat_map(|c| c.standings.iter())
        .filter(|s| s.team_id == team)
        .map(|s| s.played)
        .sum()
}

/// The club's fixtures that carry a result, in every competition and friendly.
fn own_completed(game: &Game) -> usize {
    let team = user_team_id(game);
    game.competitions
        .iter()
        .flat_map(|c| c.fixtures.iter())
        .filter(|f| f.home_team_id == team || f.away_team_id == team)
        .filter(|f| f.status == domain::league::FixtureStatus::Completed && f.result.is_some())
        .count()
}

/// A full eleven for `formation`, in slot order: for each slot, the playable player who fits it
/// best among those not yet used. Slot geometry and fit are the backend's own
/// (`player_rating`), so the eleven is one a manager could have set by hand.
fn eleven_for(game: &Game, formation: &str) -> Vec<String> {
    use ofm_core::player_rating::{effective_rating_for_assignment, formation_slots};
    let squad = user_squad(game);
    let mut eleven: Vec<String> = Vec::new();
    for slot in formation_slots(formation) {
        let best = squad
            .iter()
            .filter(|p| p.can_play() && !eleven.contains(&p.id))
            .max_by(|a, b| {
                effective_rating_for_assignment(a, &slot)
                    .total_cmp(&effective_rating_for_assignment(b, &slot))
            })
            .expect("the squad can fill the formation");
        eleven.push(best.id.clone());
    }
    eleven
}

/// Advances one day at a time in `mode` until the user's match is the day's action.
fn advance_until_user_match(state: &StateManager, mode: &str) {
    for _ in 0..DAY_GUARD {
        let response = advance_time_with_mode(state, mode).expect("a day advances");
        if response.action == "live_match" {
            return;
        }
    }
    panic!("no user match in {DAY_GUARD} days");
}

fn play_interactive_match(state: &StateManager) {
    start_interactive_match_internal(state).expect("the interactive match starts");
    loop {
        let out = advance_interactive_match_internal(
            state,
            AdvanceRequest {
                target: AdvanceTarget::ToEnd,
                stop_at_key_events: false,
            },
            0,
        )
        .expect("the match advances");
        if out.stop == StopReason::Finished {
            break;
        }
    }
    finish_interactive_match_internal(state).expect("the match is finalised");
}

/// Plays days in `delegate` mode until the season is complete.
fn play_season_delegated(state: &StateManager) {
    for day in 1..=DAY_GUARD {
        if state.get_game(is_season_complete).expect("an active game") {
            return;
        }
        advance_time_with_mode(state, "delegate")
            .unwrap_or_else(|error| panic!("day {day} of the season failed: {error}"));
    }
    panic!("the season did not complete in {DAY_GUARD} days");
}

/// Given a new career,
/// When the manager picks a formation and a preset, then swaps a bench player into the eleven,
/// Then the club holds exactly those choices, and they are still there after the save is closed and
///      reopened.
#[test]
fn squad_and_tactic_choices_are_kept_and_survive_a_reload() {
    let mut session = Session::begin();
    apply_tactic_preset_internal(&session.state, "4-4-2", "HighPress").expect("a preset applies");
    let game = session.game();
    let mut eleven = eleven_for(&game, "4-4-2");
    set_starting_xi_internal(&session.state, eleven.clone()).expect("an eleven is set");
    let squad = user_squad(&game);
    let (slot, bench_player) = eleven
        .iter()
        .enumerate()
        .find_map(|(slot, id)| {
            let starter = squad.iter().find(|p| &p.id == id)?;
            squad
                .iter()
                .find(|p| p.position == starter.position && p.can_play() && !eleven.contains(&p.id))
                .map(|bench| (slot, bench.id.clone()))
        })
        .expect("a bench player shares a position with a starter");
    eleven[slot] = bench_player;
    set_starting_xi_internal(&session.state, eleven.clone()).expect("the swap is saved");

    let stored = session.save_close_and_reopen();
    let team = stored
        .teams
        .iter()
        .find(|t| t.id == user_team_id(&stored))
        .unwrap();
    assert_eq!(team.formation, "4-4-2");
    assert_eq!(team.play_style, domain::team::PlayStyle::HighPress);
    assert_eq!(team.starting_xi_ids, eleven, "the swap holds its slot");
}

/// Given a formation the game does not know,
/// When the manager sets it,
/// Then it is refused with a translation key and the club's formation is untouched.
#[test]
fn an_unknown_formation_is_refused_without_changing_the_club() {
    let session = Session::begin();
    let before = session.game();
    let error = set_formation_internal(&session.state, "9-9-9").expect_err("refused");
    assert!(error.starts_with("be.error."), "{error}");
    let after = session.game();
    let formation = |g: &Game| {
        g.teams
            .iter()
            .find(|t| t.id == user_team_id(g))
            .unwrap()
            .formation
            .clone()
    };
    assert_eq!(formation(&before), formation(&after));
}

/// Given the day of the user's match, started live,
/// When the calendar is advanced again, or the match is finished twice,
/// Then the second advance and the second finish are refused, and the match counts once in the
///      club's results.
#[test]
fn a_live_match_cannot_be_advanced_past_or_finalised_twice() {
    let session = Session::begin();
    advance_until_user_match(&session.state, "live");
    let completed_before = own_completed(&session.game());

    let blocked = advance_time_with_mode(&session.state, "live").expect_err("advance refused");
    assert_eq!(blocked, "be.error.liveMatch.inProgress");

    play_interactive_match(&session.state);
    assert_eq!(own_completed(&session.game()), completed_before + 1);
    finish_interactive_match_internal(&session.state).expect_err("a second finish is refused");
    assert_eq!(own_completed(&session.game()), completed_before + 1);
}

/// Given the day of the user's match, with the match delegated to the simulation,
/// When the days are advanced one at a time,
/// Then the match is finalised inside that one advance (one more result for the club, no live
///      session left open) and no other day adds a result for the club.
#[test]
fn a_delegated_match_day_finalises_the_match_once() {
    let session = Session::begin();
    let mut results_seen = 0;
    for _ in 0..DAY_GUARD {
        let before = own_completed(&session.game());
        advance_time_with_mode(&session.state, "delegate").expect("a day advances");
        let gained = own_completed(&session.game()) - before;
        assert!(gained <= 1, "a club cannot play twice in a day");
        assert!(
            session.state.with_live_match(|_| ()).is_none(),
            "a live session was left open"
        );
        results_seen += gained;
        if results_seen == 2 {
            return;
        }
    }
    panic!("fewer than two user matches in {DAY_GUARD} days");
}

/// Given a listed player at another club and a fee the club can afford,
/// When the manager bids, the clubs agree, and the calendar reaches the registration date,
/// Then the player is at the club only from that date (not before), the squad grew by one and the
///      club paid no more than the fee plus a wage-sized margin.
#[test]
fn a_transfer_is_scheduled_on_acceptance_and_completes_on_its_registration_date() {
    use domain::transfer_terms::TransferTerms;
    use ofm_core::transfers::TransferNegotiationDecision as Decision;

    let session = Session::begin();
    let game = session.game();
    let user = user_team_id(&game);
    let budget = game
        .teams
        .iter()
        .find(|t| t.id == user)
        .unwrap()
        .transfer_budget;
    let targets: Vec<_> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref().is_some_and(|t| t != user) && p.transfer_listed)
        .map(|p| (p.id.clone(), p.market_value.max(100_000) * 13 / 10))
        .filter(|(_, fee)| (*fee as i64) <= budget)
        .take(15)
        .collect();
    assert!(
        !targets.is_empty(),
        "the world lists players the club can afford"
    );

    let cash = |g: &Game| g.teams.iter().find(|t| t.id == user).unwrap().finance;
    let (cash_before, squad_before) = (cash(&game), user_squad(&game).len());
    let mut signed = None;
    for (target, fee) in targets {
        let mut response = match make_transfer_bid_internal(
            &session.state,
            &target,
            fee,
            TransferTerms::default(),
        ) {
            Ok(response) => response,
            Err(error) => {
                assert!(error.starts_with("be.error."), "{error}");
                continue;
            }
        };
        if let Some(demand) = response.player_demand.clone() {
            let terms = TransferTerms {
                player_weekly_wage: demand.weekly_wage,
                player_contract_years: demand.contract_years as u8,
                ..TransferTerms::default()
            };
            response = make_transfer_bid_internal(&session.state, &target, fee, terms)
                .expect("the contract terms are answered");
        }
        if response.decision == Decision::Accepted {
            let date = response
                .registration_date
                .clone()
                .expect("an accepted deal is dated");
            signed = Some((target, fee, date));
            break;
        }
        let unchanged = session.game();
        assert_eq!(
            user_squad(&unchanged).len(),
            squad_before,
            "a bid not accepted moved a player"
        );
        assert_eq!(
            cash(&unchanged),
            cash_before,
            "a bid not accepted charged the club"
        );
    }
    let (target, fee, date) = signed.expect("no bid was accepted");

    let at_club = |g: &Game| user_squad(g).iter().any(|p| p.id == target);
    assert!(
        !at_club(&session.game())
            || date
                <= session
                    .game()
                    .clock
                    .current_date
                    .format("%Y-%m-%d")
                    .to_string()
    );
    for _ in 0..30 {
        if at_club(&session.game()) {
            break;
        }
        advance_time_with_mode(&session.state, "delegate").expect("a day advances");
    }
    let after = session.game();
    assert!(
        at_club(&after),
        "the player never arrived (registration date {date})"
    );
    assert_eq!(
        user_squad(&after).len(),
        squad_before + 1,
        "exactly one player arrived"
    );
    assert!(
        cash(&after) < cash_before,
        "the signing cost the club nothing"
    );
    assert!(
        cash_before - cash(&after) <= (fee as i64) * 2,
        "the club paid far more than the fee"
    );
}

/// Given a fee larger than the club's transfer budget,
/// When the manager bids,
/// Then the bid is refused with a translation key and the club's books and squad are untouched.
#[test]
fn a_bid_above_the_transfer_budget_is_refused_and_changes_nothing() {
    use domain::transfer_terms::TransferTerms;

    let session = Session::begin();
    let before = session.game();
    let user = user_team_id(&before);
    let team = before.teams.iter().find(|t| t.id == user).unwrap();
    let fee = (team.transfer_budget.max(0) as u64) + 1_000_000;
    let target = before
        .players
        .iter()
        .find(|p| p.team_id.as_deref().is_some_and(|t| t != user))
        .expect("a player at another club")
        .id
        .clone();

    let error = make_transfer_bid_internal(&session.state, &target, fee, TransferTerms::default())
        .expect_err("the bid is beyond the budget");

    assert_eq!(error, "be.error.transfers.transferBudgetTooLow");
    let after = session.game();
    let team_after = after.teams.iter().find(|t| t.id == user).unwrap();
    assert_eq!(team_after.finance, team.finance);
    assert_eq!(team_after.transfer_budget, team.transfer_budget);
    assert_eq!(user_squad(&after).len(), user_squad(&before).len());
}

/// Given the assistant has left a proposal waiting,
/// When the manager rejects it,
/// Then no proposal is pending (what the dashboard alert counts) and the player the proposal
///      concerned is still at the club.
#[test]
fn a_pending_proposal_is_counted_until_the_manager_answers_it() {
    let session = Session::begin();
    let game = session.game();
    let player_id = user_squad(&game)[0].id.clone();
    mutate_active_game(&session.state, |game| {
        let queued = game.delegation.propose(
            domain::delegation::Duty::Offers,
            "2033-07-01",
            ProposalAction::ListForSale {
                player_id: player_id.clone(),
            },
            "delegation.proposal.listForSale",
            vec![],
            "delegation.reason.surplus",
        );
        assert!(queued);
        Ok(())
    })
    .expect("a proposal is queued");
    let pending = session.game().delegation.proposals.clone();
    assert_eq!(pending.len(), 1);

    mutate_active_game(&session.state, |game| {
        ofm_core::staff_system::delegation::resolve_proposal(game, &pending[0].id, false)
    })
    .expect("the proposal is answered");

    let game = session.game();
    assert!(game.delegation.proposals.is_empty());
    assert!(user_squad(&game).iter().any(|p| p.id == player_id));
}

/// Given a club with no assistant manager,
/// When the manager asks for an assistant to act with approval,
/// Then it is refused (nobody to delegate to), while taking everything back (manual) is allowed.
#[test]
fn delegation_needs_an_assistant_but_taking_control_back_does_not() {
    let session = Session::begin();
    mutate_active_game(&session.state, |game| {
        let team = user_team_id(game);
        game.staff.retain(|s| {
            !(s.team_id.as_deref() == Some(team.as_str())
                && s.post() == domain::staff::StaffPost::AssistantManager)
        });
        Ok(())
    })
    .expect("the assistant leaves");
    let refused = mutate_active_game(&session.state, |game| {
        set_mode(game, DelegationMode::Approval)
    })
    .expect_err("no assistant to delegate to");
    assert_eq!(refused, "be.error.delegation.noAssistant");
    mutate_active_game(&session.state, |game| {
        set_mode(game, DelegationMode::Manual)
    })
    .expect("manual control is always available");
}

/// Given a career saved and reopened mid-way through the first season,
/// When it is played to the end of two seasons through the calendar, rolling over in between,
/// Then both seasons complete, the second season starts a year later with fixtures to play, and the
///      career is still the same world after the save is closed and reopened at the end.
#[test]
#[ignore = "plays two full seasons; run with --release -- --ignored"]
fn a_reloaded_career_completes_two_seasons() {
    let mut session = Session::begin();
    for _ in 0..30 {
        advance_time_with_mode(&session.state, "delegate").expect("a day advances");
    }
    session.save_close_and_reopen();

    let first_year = session.game().clock.current_date.format("%Y").to_string();
    for season in 1..=2 {
        play_season_delegated(&session.state);
        assert!(
            own_played(&session.game()) > 0,
            "season {season} played no match"
        );
        session.save_close_and_reopen();
        advance_to_next_season_internal(&session.state).expect("the season rolls over");
        let game = session.game();
        assert!(game.manager.team_id.is_some(), "the manager was dismissed");
        assert!(
            !is_season_complete(&game),
            "the new season starts with fixtures to play"
        );
        assert_eq!(
            own_played(&game),
            0,
            "the new season starts with an empty table"
        );
    }
    let last_year = session.game().clock.current_date.format("%Y").to_string();
    assert!(last_year > first_year);
    session.save_close_and_reopen();
}
