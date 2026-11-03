//! Loans in the FM24 world, from the proposal to the player's return, through the production day.
//!
//! OFM's own tests cover each step of a loan on a synthetic world. These play the whole loan in the
//! world a career is played in — 240 clubs, the opening loan listings, the transfer window and the
//! daily sweep — so a path that only breaks there (a window that is shut, a squad floor, a wage
//! bill read from another club) is found.
//!
//! The FM24 world is the player's own file (`APFM_WORLD_JSON`), never in the repository: each test
//! returns at once when it is not set.

use chrono::{Duration, NaiveDate};
use db::save_manager::SaveManager;
use domain::player::{LoanOfferStatus, Player};
use domain::season::TransferWindowStatus;
use domain::stats::StatsState;
use ofm_core::finances::calc_wages;
use ofm_core::game::Game;
use ofm_core::squad_floor::{MIN_PLAYERS_PER_GROUP, MIN_SENIOR_PLAYERS};
use ofm_core::transfers::{
    LoanOfferDecision, make_loan_offer, process_pending_loan_registrations, respond_to_loan_offer,
};
use season_harness::driver::{RunOptions, play_day};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

const LENS: &str = "fm-c-501";
/// A market value just under the line (1.5 million) from which a player counts as a regular of his
/// club and so has a say in where he is lent. Lens has only two such players with a contract long
/// enough to lend, both valued near 1.2 million.
const FRINGE_PROSPECT_VALUE: u64 = 1_400_000;

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

fn window_is_open(game: &Game) -> bool {
    matches!(
        game.season_context.transfer_window.status,
        TransferWindowStatus::Open | TransferWindowStatus::DeadlineDay
    )
}

/// Play days until the window is open; returns the day count so far.
fn play_to_the_window(game: &mut Game, invariants: &mut Invariants, day: &mut u32) {
    while !window_is_open(game) {
        *day += 1;
        assert!(*day < 120, "the window never opened");
        play_day(game, RunOptions::seasons(1), 1, *day, invariants).expect("a day plays");
    }
}

fn play_days(game: &mut Game, invariants: &mut Invariants, day: &mut u32, days: u32) {
    for _ in 0..days {
        *day += 1;
        play_day(game, RunOptions::seasons(1), 1, *day, invariants).expect("a day plays");
    }
}

/// An agreed loan waits for the day it starts (a plain loan takes up to a day to register, one
/// with a purchase option up to two). Moves to that day and runs the registration the turn runs,
/// so what follows is judged on the first day of the loan and not on a day of other business.
fn start_the_agreed_loan(game: &mut Game, id: &str) {
    let start = player(game, id)
        .loan_offers
        .iter()
        .find(|offer| offer.status == LoanOfferStatus::PendingRegistration)
        .and_then(|offer| NaiveDate::parse_from_str(&offer.start_date, "%Y-%m-%d").ok());
    if let Some(start) = start {
        game.clock.current_date += Duration::days((start - today(game)).num_days());
        process_pending_loan_registrations(game);
    }
}

/// Plays whole days, as the career does, until the loan the manager agreed has started. The
/// registration runs in the turn of its start date, so a loan due in two days is on the third
/// day's end; a bounded wait keeps a loan that never starts from hanging the test.
fn play_until_the_agreed_loan_starts(
    game: &mut Game,
    invariants: &mut Invariants,
    day: &mut u32,
    id: &str,
) {
    for _ in 0..5 {
        if player(game, id).active_loan.is_some() {
            return;
        }
        play_days(game, invariants, day, 1);
    }
}

fn player<'a>(game: &'a Game, id: &str) -> &'a Player {
    game.players
        .iter()
        .find(|p| p.id == id)
        .expect("the player")
}

/// The senior players registered to `team_id`: the ones the squad floor counts.
fn seniors_of<'a>(game: &'a Game, team_id: &str) -> Vec<&'a Player> {
    game.players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team_id)
                && p.squad_role == domain::player::SquadRole::Senior
        })
        .collect()
}

/// Whether the club could lend `player` and still keep the squad floor: more seniors in his
/// position group than the group's minimum, and more seniors in all than the club's minimum. The
/// production refusal (`be.error.squadFloor.*`) is correct; a test that lends a player must pick
/// one the club can spare, as a manager would.
fn can_leave_without_breaking_the_floor(seniors: &[&Player], player: &Player) -> bool {
    let group = player.position.to_group_position();
    let in_group = seniors
        .iter()
        .filter(|p| p.position.to_group_position() == group)
        .count();
    let group_floor = MIN_PLAYERS_PER_GROUP
        .iter()
        .find(|(position, _)| *position == group)
        .map_or(0, |(_, floor)| *floor);
    in_group > group_floor && seniors.len() > MIN_SENIOR_PLAYERS
}

/// An AI player on the loan list whose contract outlasts a loan of `days` by two months, best
/// rated first.
fn loan_listed_target(game: &Game, days: i64) -> String {
    let horizon = today(game) + Duration::days(days + 60);
    game.players
        .iter()
        .filter(|p| p.loan_listed && p.team_id.as_deref() != game.manager.team_id.as_deref())
        .filter(|p| p.active_loan.is_none() && p.squad_role == domain::player::SquadRole::Senior)
        .filter(|p| {
            p.contract_end()
                .and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
                .is_some_and(|end| end > horizon)
        })
        .max_by_key(|p| (p.ovr, std::cmp::Reverse(p.id.clone())))
        .map(|p| p.id.clone())
        .expect("a loan-listed AI player with a long contract")
}

fn on_loan_to(game: &Game, id: &str, borrower: &str) -> bool {
    let p = player(game, id);
    p.team_id.as_deref() == Some(borrower) && p.active_loan.is_some()
}

/// Given the opening of an FM24 career,
/// When the player asks for an AI club's loan-listed player,
/// Then there are AI loan listings to ask for, a derisory share of the wages is refused and a
///      full one accepted, and the refusal leaves the player where he was.
#[test]
fn an_ai_club_refuses_a_stingy_loan_and_accepts_a_fair_one() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut day = 0;
    assert!(
        game.players
            .iter()
            .filter(|p| p.loan_listed && p.team_id.as_deref() != Some(LENS))
            .count()
            >= 100,
        "the opening loan market is seeded"
    );
    play_to_the_window(&mut game, &mut invariants, &mut day);
    let id = loan_listed_target(&game, 120);
    let parent = player(&game, &id).team_id.clone().unwrap();
    let end = (today(&game) + Duration::days(120))
        .format("%Y-%m-%d")
        .to_string();

    let refused = make_loan_offer(&mut game, &id, &end, 0, None).expect("the offer is judged");
    assert_eq!(refused.decision, LoanOfferDecision::Rejected);
    assert_eq!(player(&game, &id).team_id.as_deref(), Some(parent.as_str()));
    assert!(player(&game, &id).active_loan.is_none());
    assert_eq!(
        player(&game, &id)
            .loan_offers
            .last()
            .map(|o| o.status.clone()),
        Some(LoanOfferStatus::Rejected)
    );

    let accepted = make_loan_offer(&mut game, &id, &end, 100, None).expect("the offer is judged");
    assert_eq!(accepted.decision, LoanOfferDecision::Accepted);
    start_the_agreed_loan(&mut game, &id);
    assert!(on_loan_to(&game, &id, LENS));
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given a loan the player asked for,
/// When it runs through the production day to its end date and beyond,
/// Then the wage is split as agreed from the first day (the borrower's bill rises by the share, the
///      parent's falls by it), the player plays for the borrower while it lasts, and he is back
///      with his parent club once it ends, the loan cleared, his contract untouched.
#[test]
fn a_loan_runs_its_course_and_the_player_goes_home() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut day = 0;
    play_to_the_window(&mut game, &mut invariants, &mut day);
    let id = loan_listed_target(&game, 120);
    let parent = player(&game, &id).team_id.clone().unwrap();
    let wage = i64::from(player(&game, &id).wage());
    let contract_end = player(&game, &id).contract_end().map(str::to_string);
    let (parent_bill, my_bill) = (calc_wages(&game, &parent), calc_wages(&game, LENS));
    let end = (today(&game) + Duration::days(120))
        .format("%Y-%m-%d")
        .to_string();

    let share = 60_u8;
    let outcome = make_loan_offer(&mut game, &id, &end, share, None).expect("judged");
    if outcome.decision != LoanOfferDecision::Accepted {
        // The owner wanted more than 60 %: ask for all of it.
        let outcome = make_loan_offer(&mut game, &id, &end, 100, None).expect("judged");
        assert_eq!(outcome.decision, LoanOfferDecision::Accepted);
    }
    start_the_agreed_loan(&mut game, &id);
    let agreed = player(&game, &id)
        .active_loan
        .as_ref()
        .expect("on loan")
        .wage_contribution_pct;
    let borrowed_share = wage * i64::from(agreed) / 100;
    assert_eq!(calc_wages(&game, LENS), my_bill + borrowed_share);
    assert_eq!(calc_wages(&game, &parent), parent_bill - borrowed_share);
    let loan = player(&game, &id).active_loan.clone().unwrap();
    assert_eq!(loan.parent_team_id, parent);
    assert_eq!(loan.loan_team_id, LENS);
    assert_eq!(loan.end_date, end);

    // Halfway through he is still the borrower's.
    play_days(&mut game, &mut invariants, &mut day, 60);
    assert!(on_loan_to(&game, &id, LENS), "recalled before the end date");

    // Past the end date he is home, the loan cleared.
    play_days(&mut game, &mut invariants, &mut day, 62);
    let home = player(&game, &id);
    assert_eq!(home.team_id.as_deref(), Some(parent.as_str()));
    assert!(home.active_loan.is_none(), "the loan was not cleared");
    assert_eq!(home.contract_end().map(str::to_string), contract_end);
    assert!(
        !game
            .players
            .iter()
            .any(|p| p.active_loan.as_ref().is_some_and(|l| l.end_date < end)),
        "a loan outlived its end date"
    );
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given a player of the user's club put on the loan list before the window,
/// When the window runs,
/// Then AI clubs make offers for him, the user accepts one, the player goes to that club on loan
///      with the share agreed, and he comes back when it ends.
#[test]
fn ai_clubs_offer_for_a_loan_listed_player_and_he_comes_back() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut day = 0;
    // A fringe prospect the club can spare: the player a manager lends. A first-eleven player or a
    // regular worth over a million and a half refuses a loan to a smaller club (the player's say
    // in a loan), and a club cannot lend a player it needs to keep its squad floor.
    let first_eleven = game
        .teams
        .iter()
        .find(|t| t.id == LENS)
        .map(|t| t.starting_xi_ids.clone())
        .unwrap_or_default();
    let seniors = seniors_of(&game, LENS);
    let mine = seniors
        .iter()
        .filter(|p| p.contract_end().is_some_and(|e| e > "2028-01-01"))
        .filter(|p| !first_eleven.contains(&p.id) && p.market_value < FRINGE_PROSPECT_VALUE)
        .filter(|p| can_leave_without_breaking_the_floor(&seniors, p))
        .max_by_key(|p| p.potential.saturating_sub(p.ovr))
        .map(|p| p.id.clone())
        .expect("a player to lend");
    game.players
        .iter_mut()
        .find(|p| p.id == mine)
        .unwrap()
        .loan_listed = true;

    play_to_the_window(&mut game, &mut invariants, &mut day);
    play_days(&mut game, &mut invariants, &mut day, 30);

    let offers: Vec<_> = player(&game, &mine)
        .loan_offers
        .iter()
        .filter(|o| o.status == LoanOfferStatus::Pending)
        .map(|o| (o.id.clone(), o.from_team_id.clone()))
        .collect();
    assert!(
        !offers.is_empty(),
        "no club offered for a loan-listed player in the first month of the window"
    );
    // The newest offer: one that is a fortnight old is withdrawn by the first thing the response
    // does, the sweep that expires stale talks.
    let (offer_id, borrower) = offers.last().expect("an offer").clone();
    let wage = i64::from(player(&game, &mine).wage());
    let borrower_bill = calc_wages(&game, &borrower);
    // By the end of August about one AI club in ten is over its wage budget (the same in the world
    // before the roster audit); the lifecycle is what this test follows, so the borrower of the
    // offer is given room for the loan rather than the test depending on which club happened to
    // bid.
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == borrower) {
        team.wage_budget = team.wage_budget.max((borrower_bill + wage) * 2);
    }
    respond_to_loan_offer(&mut game, &mine, &offer_id, true).expect("the offer is accepted");
    play_until_the_agreed_loan_starts(&mut game, &mut invariants, &mut day, &mine);

    let p = player(&game, &mine);
    let loan = p.active_loan.clone().expect("on loan");
    assert_eq!(p.team_id.as_deref(), Some(borrower.as_str()));
    assert_eq!(loan.parent_team_id, LENS);
    // The borrower pays the agreed share of his wage on top of whatever else its bill is on the
    // day the loan starts: compared with the same game without the loan, not with a bill read
    // days earlier, because the club signs and releases players in between.
    let mut without_the_loan = game.clone();
    let lent = without_the_loan
        .players
        .iter_mut()
        .find(|p| p.id == mine)
        .expect("the player");
    lent.team_id = Some(LENS.to_string());
    lent.active_loan = None;
    assert_eq!(
        calc_wages(&game, &borrower),
        calc_wages(&without_the_loan, &borrower)
            + wage * i64::from(loan.wage_contribution_pct) / 100
    );
    let end = NaiveDate::parse_from_str(&loan.end_date, "%Y-%m-%d").unwrap();
    let days_left = (end - today(&game)).num_days() as u32;

    play_days(&mut game, &mut invariants, &mut day, days_left + 2);
    let home = player(&game, &mine);
    assert_eq!(home.team_id.as_deref(), Some(LENS), "he did not come home");
    assert!(home.active_loan.is_none());
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given a loan in progress,
/// When the game is saved and read back,
/// Then the loan is the same: borrower, parent, end date, share, and the borrower's squad.
#[test]
fn a_loan_in_progress_survives_a_save_and_a_reload() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut day = 0;
    play_to_the_window(&mut game, &mut invariants, &mut day);
    let id = loan_listed_target(&game, 120);
    let end = (today(&game) + Duration::days(120))
        .format("%Y-%m-%d")
        .to_string();
    let outcome = make_loan_offer(&mut game, &id, &end, 100, None).expect("judged");
    assert_eq!(outcome.decision, LoanOfferDecision::Accepted);
    play_days(&mut game, &mut invariants, &mut day, 10);

    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let save = manager
        .create_save_with_stats(&game, &StatsState::default(), "loan")
        .expect("the save");
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&save)
        .unwrap();

    let before = player(&game, &id);
    let after = player(&reloaded, &id);
    assert_eq!(after.team_id, before.team_id);
    assert_eq!(after.active_loan, before.active_loan);
    assert_eq!(after.wage(), before.wage());
    assert_eq!(
        calc_wages(&reloaded, LENS),
        calc_wages(&game, LENS),
        "the borrower's bill changed in the save"
    );
}
