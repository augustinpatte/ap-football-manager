//! Run F2: an agreed paid transfer is processed before it is registered.
//!
//! Before, a deal the clubs and the player agreed while the window was open completed in the same
//! call: the player left the day the selling club said yes. Now the agreement waits for an
//! administrative registration date (same-day for a small domestic deal, a few days for a large or
//! international one, never past the closing day), and the player is still at his club until then.

#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use chrono::{Datelike, NaiveDate};
use domain::player::TransferOfferStatus;
use domain::season::TransferWindowStatus;
use domain::transfer_terms::TransferTerms;
use market_fixtures::{PLAYER_ID, game_with, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, make_manager_transfer_bid, process_pending_transfer_registrations,
};

const FEE: u64 = 12_000_000;

/// The F1 "fair move" with an open window that has `days_left` days to run.
fn fair_move(days_left: i64) -> Game {
    let mut owned = market_fixtures::player("1999-01-01");
    owned.team_id = Some("small-club".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 70;
    owned.potential = 70;
    owned.market_value = 8_000_000;
    owned.morale = 75;
    let mut game = game_with(owned);
    game.teams[0].reputation = 500;
    game.teams[0].transfer_budget = 60_000_000;
    game.teams[0].finance = 80_000_000;
    game.teams[0].wage_budget = 60_000_000;
    for team in &mut game.teams {
        if team.id == "small-club" {
            team.reputation = 90;
        }
    }
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(days_left);
    game
}

/// The contract the player showed, put back to him: the move that completes the deal.
fn agreed_bid(game: &mut Game) -> ofm_core::transfers::TransferNegotiationOutcome {
    let shown = make_manager_transfer_bid(game, PLAYER_ID, FEE, TransferTerms::default())
        .unwrap()
        .player_demand
        .expect("his contract");
    let terms = TransferTerms {
        player_weekly_wage: shown.weekly_wage,
        player_contract_years: u8::try_from(shown.contract_years).unwrap(),
        ..TransferTerms::default()
    };
    make_manager_transfer_bid(game, PLAYER_ID, FEE, terms).unwrap()
}

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

/// Scenario 1. Given a large paid deal the clubs and the player agree, with a month of window left
/// When the manager puts the contract
/// Then the answer is "accepted, registered on a later date", the offer waits as pending
/// registration, and the player is still at his club with no cash moved.
#[test]
fn an_agreed_deal_waits_for_its_registration_date_while_the_window_is_open() {
    let mut game = fair_move(30);
    let cash = game.teams[0].finance;

    let outcome = agreed_bid(&mut game);

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    let date = outcome
        .registration_date
        .expect("a registration date after today");
    let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap();
    assert!(date > today(&game), "{date} is not after {}", today(&game));
    assert!(date <= today(&game) + chrono::Duration::days(5));
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
    assert_eq!(game.teams[0].finance, cash);
    assert!(
        prospect(&game)
            .transfer_offers
            .iter()
            .any(|offer| offer.status == TransferOfferStatus::PendingRegistration)
    );
}

/// Scenario 2. Given the deal waits for its date
/// When the days pass
/// Then the player does not move before the date and does on it, once.
#[test]
fn the_player_moves_on_the_registration_date_and_not_before() {
    let mut game = fair_move(30);
    let outcome = agreed_bid(&mut game);
    let due = NaiveDate::parse_from_str(&outcome.registration_date.unwrap(), "%Y-%m-%d").unwrap();
    let start = today(&game);

    let mut day = start;
    while day < due {
        process_pending_transfer_registrations(&mut game);
        assert_eq!(
            prospect(&game).team_id.as_deref(),
            Some("small-club"),
            "left early on {day}"
        );
        day += chrono::Duration::days(1);
        set_date(&mut game, day.year(), day.month(), day.day());
    }
    process_pending_transfer_registrations(&mut game);

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    process_pending_transfer_registrations(&mut game);
    assert_eq!(
        prospect(&game)
            .movement_history
            .iter()
            .filter(|entry| entry.kind == domain::player::PlayerMovementKind::PermanentTransfer)
            .count(),
        1,
        "registered exactly once"
    );
}

/// Scenario 3. Given the same deal, built twice
/// When it is agreed
/// Then the registration date is the same: a reload or a fast-forward cannot move it.
#[test]
fn the_registration_date_is_deterministic() {
    let first = agreed_bid(&mut fair_move(30)).registration_date;
    let second = agreed_bid(&mut fair_move(30)).registration_date;
    assert_eq!(first, second);
}

/// Scenario 4. Given the window closes in two days
/// When a large deal is agreed
/// Then it registers no later than the closing day, never after the window shut.
#[test]
fn a_deal_near_the_close_is_registered_by_the_closing_day() {
    let mut game = fair_move(2);
    let outcome = agreed_bid(&mut game);
    let latest = today(&game) + chrono::Duration::days(2);
    match outcome.registration_date {
        Some(date) => {
            assert!(NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap() <= latest);
        }
        None => assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1")),
    }
}

/// Scenario 5. Given deadline day
/// When a deal is agreed
/// Then it completes at once: there is no day left to wait.
#[test]
fn on_deadline_day_the_deal_completes_at_once() {
    let mut game = fair_move(0);
    game.season_context.transfer_window.status = TransferWindowStatus::DeadlineDay;

    let outcome = agreed_bid(&mut game);

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert!(outcome.registration_date.is_none());
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
}

/// The manager's own player, wanted by the bigger club at `FEE`, with `days_left` days of window.
fn sale(days_left: i64) -> Game {
    let mut owned = market_fixtures::player("1999-01-01");
    owned.team_id = Some("team-1".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 70;
    owned.potential = 70;
    owned.market_value = 8_000_000;
    owned.morale = 75;
    let mut game = game_with(owned);
    game.teams[1].transfer_budget = 60_000_000;
    game.teams[1].finance = 80_000_000;
    game.teams[1].wage_budget = 60_000_000;
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(days_left);
    market_fixtures::pending_offer(&mut game, "offer-1", "big-club", FEE, "2026-08-01");
    game
}

fn move_date(game: &Game) -> Option<NaiveDate> {
    prospect(game)
        .transfer_offers
        .iter()
        .find(|offer| offer.status == TransferOfferStatus::PendingRegistration)
        .and_then(|offer| offer.registration_date.as_deref())
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

/// Scenario 7. Given an offer from a club for the manager's player, with the window open
/// When the manager accepts
/// Then the player is still at the club, no money has moved, and the move waits for its date.
#[test]
fn accepting_an_offer_for_my_player_does_not_move_him_that_day() {
    let mut game = sale(30);
    let cash = (game.teams[0].finance, game.teams[1].finance);

    ofm_core::transfers::respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap();

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!((game.teams[0].finance, game.teams[1].finance), cash);
    let date = move_date(&game).expect("the move has a registration date");
    assert!(date > today(&game));
}

/// Scenario 8. Given the sale is waiting for its date
/// When the date comes
/// Then he leaves, once, and the fee is paid once.
#[test]
fn the_sold_player_leaves_on_the_registration_date() {
    let mut game = sale(30);
    ofm_core::transfers::respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap();
    let due = move_date(&game).unwrap();
    let before = game.teams[0].finance;

    set_date(&mut game, due.year(), due.month(), due.day());
    process_pending_transfer_registrations(&mut game);

    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert!(game.teams[0].finance > before, "the fee came in");
    let after = game.teams[0].finance;
    process_pending_transfer_registrations(&mut game);
    assert_eq!(game.teams[0].finance, after, "paid once");
}

/// Scenario 9. Given a sale that was agreed but whose buyer can no longer pay by the date
/// When the date comes
/// Then the player stays, nothing is paid, and the agreement is withdrawn with a reason.
#[test]
fn a_sale_whose_buyer_cannot_pay_by_the_date_falls_through() {
    let mut game = sale(30);
    ofm_core::transfers::respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap();
    let due = move_date(&game).unwrap();
    game.teams[1].finance = 0;
    game.teams[1].transfer_budget = 0;

    set_date(&mut game, due.year(), due.month(), due.day());
    process_pending_transfer_registrations(&mut game);

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    let offer = &prospect(&game).transfer_offers[0];
    assert_eq!(offer.status, TransferOfferStatus::Withdrawn);
    assert!(offer.registration_failure_reason.is_some());
}

/// Scenario 10. Given a sale waiting for its date
/// When the manager tries to accept a second offer for the same player
/// Then it is refused: a player cannot be sold twice.
#[test]
fn a_player_with_a_sale_pending_cannot_be_sold_again() {
    let mut game = sale(30);
    market_fixtures::pending_offer(&mut game, "offer-2", "small-club", FEE, "2026-08-01");
    ofm_core::transfers::respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap();

    let second = ofm_core::transfers::respond_to_offer(&mut game, PLAYER_ID, "offer-2", true);

    assert!(second.is_err());
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
}
