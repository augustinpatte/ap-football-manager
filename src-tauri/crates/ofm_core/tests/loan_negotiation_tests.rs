//! Run F2: the loan lifecycle end to end, from the manager's proposal to the player's return.
//!
//! A loan proposal is answered by the owner, then by the player, then waits for its start date like
//! any other registration, then runs until its end date and returns the player to the club that
//! owns him. The loan fee is not modelled: a loan costs the borrower his wage share only.

#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use chrono::{Datelike, NaiveDate};
use domain::player::{ActiveLoan, LoanOfferStatus};
use domain::season::TransferWindowStatus;
use market_fixtures::{PLAYER_ID, game_with, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    LoanOfferDecision, make_loan_offer, process_loan_returns, process_pending_loan_registrations,
};

const END: &str = "2027-06-30";
/// A purchase option the owner accepts: a loan with one has a contract to draw up, so it is never
/// started the day it is agreed.
const OPTION: Option<u64> = Some(2_000_000);

/// A loan-listed player of the bigger club, on the fringe there, and a month of open window.
fn listed_fringe_player() -> Game {
    let mut owned = market_fixtures::player("2003-01-01");
    owned.team_id = Some("big-club".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 62;
    owned.potential = 75;
    owned.market_value = 500_000;
    owned.morale = 70;
    owned.loan_listed = true;
    let mut game = game_with(owned);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(30);
    game
}

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

fn start_of_pending_loan(game: &Game) -> Option<NaiveDate> {
    prospect(game)
        .loan_offers
        .iter()
        .find(|offer| offer.status == LoanOfferStatus::PendingRegistration)
        .and_then(|offer| NaiveDate::parse_from_str(&offer.start_date, "%Y-%m-%d").ok())
}

fn go_to(game: &mut Game, day: NaiveDate) {
    set_date(game, day.year(), day.month(), day.day());
}

/// Scenario 1. Given a loan-listed fringe player and an owner satisfied with the terms
/// When the manager proposes a loan with a purchase option
/// Then it is agreed but not started: the player is still at his club and the start is a later date.
#[test]
fn an_agreed_loan_waits_for_its_start_date() {
    let mut game = listed_fringe_player();

    let outcome = make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION).unwrap();

    assert_eq!(outcome.decision, LoanOfferDecision::Accepted);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert!(prospect(&game).active_loan.is_none());
    let start = start_of_pending_loan(&game).expect("pending, with a start date");
    assert!(
        start > today(&game),
        "{start} is not after {}",
        today(&game)
    );
}

/// Scenario 2. Given an agreed loan
/// When the days before its start pass, and then its start date comes
/// Then he is not loaned early, and on the date he is at the borrower, owned by his parent club.
#[test]
fn the_loan_starts_on_its_date_and_keeps_the_owner() {
    let mut game = listed_fringe_player();
    make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION).unwrap();
    let start = start_of_pending_loan(&game).unwrap();

    process_pending_loan_registrations(&mut game);
    assert!(prospect(&game).active_loan.is_none(), "not before {start}");
    go_to(&mut game, start);
    process_pending_loan_registrations(&mut game);

    let loaned = prospect(&game);
    assert_eq!(loaned.team_id.as_deref(), Some("team-1"));
    let loan = loaned.active_loan.as_ref().expect("on loan");
    assert_eq!(loan.parent_team_id, "big-club");
    assert_eq!(loan.end_date, END);
    assert_eq!(loan.buy_option_fee, OPTION);
}

/// Scenario 3. Given a player who is out on loan to another club, with a known end date
/// When the manager proposes a loan for him
/// Then it is refused as "already on loan", and the date he becomes available is the loan's end
/// date, which the player record carries.
#[test]
fn a_player_on_loan_elsewhere_cannot_be_loaned_and_his_availability_is_known() {
    let mut game = listed_fringe_player();
    game.players[0].team_id = Some("small-club".to_string());
    game.players[0].active_loan = Some(ActiveLoan {
        parent_team_id: "big-club".to_string(),
        loan_team_id: "small-club".to_string(),
        loan_team_name: None,
        start_date: "2026-07-01".to_string(),
        end_date: "2027-01-15".to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
    });

    let result = make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION);

    assert_eq!(
        result.unwrap_err(),
        "be.error.transfers.playerAlreadyLoaned"
    );
    assert_eq!(
        prospect(&game)
            .active_loan
            .as_ref()
            .map(|l| l.end_date.as_str()),
        Some("2027-01-15")
    );
}

/// Scenario 4. Given a loan that has run its course
/// When its end date comes
/// Then the player is back at the club that owns him, and is no longer on loan.
#[test]
fn the_player_returns_to_his_club_when_the_loan_ends() {
    let mut game = listed_fringe_player();
    make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION).unwrap();
    let start = start_of_pending_loan(&game).unwrap();
    go_to(&mut game, start);
    process_pending_loan_registrations(&mut game);
    assert!(prospect(&game).active_loan.is_some());

    go_to(
        &mut game,
        NaiveDate::parse_from_str(END, "%Y-%m-%d").unwrap(),
    );
    process_loan_returns(&mut game);

    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert!(prospect(&game).active_loan.is_none());
}

/// Scenario 5. Given a first-eleven player of a much bigger club
/// When a small club proposes a loan the owner accepts
/// Then the player refuses, and nothing is agreed or stored as agreed.
#[test]
fn a_key_player_refuses_a_loan_to_a_much_smaller_club() {
    let mut game = listed_fringe_player();
    game.teams[1].reputation = 700;
    game.players[0].market_value = 3_000_000;
    game.teams[1].starting_xi_ids = vec![PLAYER_ID.to_string()];

    let result = make_loan_offer(&mut game, PLAYER_ID, END, 100, None);

    assert_eq!(result.unwrap_err(), "be.error.transfers.playerRefusesLoan");
    assert!(
        prospect(&game)
            .loan_offers
            .iter()
            .all(|offer| offer.status != LoanOfferStatus::Accepted
                && offer.status != LoanOfferStatus::PendingRegistration)
    );
    assert!(prospect(&game).active_loan.is_none());
}

/// Scenario 6. Given an owner who wants the whole wage covered
/// When the manager offers a small share
/// Then the owner rejects and the player stays.
#[test]
fn an_owner_refuses_a_loan_with_too_small_a_wage_share() {
    let mut game = listed_fringe_player();
    game.players[0].market_value = 5_000_000;

    let outcome = make_loan_offer(&mut game, PLAYER_ID, END, 0, None).unwrap();

    assert_eq!(outcome.decision, LoanOfferDecision::Rejected);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert!(prospect(&game).active_loan.is_none());
    assert!(
        prospect(&game)
            .loan_offers
            .iter()
            .any(|offer| offer.status == LoanOfferStatus::Rejected)
    );
}

/// Scenario 7. Given a borrower whose books cannot carry the wage share
/// When the manager proposes the loan
/// Then it is refused before anything is agreed.
#[test]
fn a_loan_the_borrower_cannot_afford_is_refused() {
    let mut game = listed_fringe_player();
    game.teams[0].wage_budget = 0;
    game.teams[0].finance = 0;

    let result = make_loan_offer(&mut game, PLAYER_ID, END, 100, None);

    let error = result.unwrap_err();
    assert!(
        error.contains("wage") || error.contains("Wage"),
        "a wage-policy refusal, got {error}"
    );
    assert!(prospect(&game).active_loan.is_none());
    assert!(
        prospect(&game)
            .loan_offers
            .iter()
            .all(|offer| offer.status != LoanOfferStatus::Accepted)
    );
}

/// Scenario 8. Given a loan agreed and waiting for its start date
/// When the manager proposes another for the same player
/// Then it is refused: pending loans do not overlap.
#[test]
fn a_pending_loan_blocks_a_second_proposal() {
    let mut game = listed_fringe_player();
    make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION).unwrap();
    assert!(start_of_pending_loan(&game).is_some());

    let second = make_loan_offer(&mut game, PLAYER_ID, END, 100, OPTION);

    assert_eq!(
        second.unwrap_err(),
        "be.error.transfers.playerAlreadyLoaned"
    );
}
