//! Deadline day: the summary the interface shows and the sweep that ends unanswered talks.

mod market_fixtures;

use domain::player::{LoanOfferStatus, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use market_fixtures::{game_with, pending_offer, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    DeadlineItemState, expire_negotiations_at_deadline, transfer_deadline_summary,
};

/// A user-owned player with a bid on the table from the bigger club, on deadline day.
fn deadline_game() -> Game {
    let mut owned = player("1999-01-01");
    owned.team_id = Some("team-1".to_string());
    let mut game = game_with(owned);
    set_date(&mut game, 2026, 9, 1);
    let window = &mut game.season_context.transfer_window;
    window.status = TransferWindowStatus::DeadlineDay;
    window.days_remaining = Some(0);
    window.closes_on = Some("2026-09-01".to_string());
    pending_offer(&mut game, "offer-1", "big-club", 900_000, "2026-08-28");
    game
}

fn offer_status(game: &Game, id: &str) -> TransferOfferStatus {
    prospect(game)
        .transfer_offers
        .iter()
        .find(|o| o.id == id)
        .unwrap()
        .status
        .clone()
}

fn deadline_messages(game: &Game) -> usize {
    game.messages
        .iter()
        .filter(|m| m.subject_key.as_deref() == Some("be.msg.deadlineExpired.subject"))
        .count()
}

#[test]
fn the_summary_surfaces_the_deadline_and_the_offer_that_needs_an_answer() {
    // Given an unanswered bid on deadline day.
    let game = deadline_game();

    // When the summary is read.
    let summary = transfer_deadline_summary(&game);

    // Then the window is urgent, and the bid is the one thing to answer.
    assert!(summary.window_open);
    assert!(summary.urgent);
    assert_eq!(summary.phase, "deadline_day");
    assert_eq!(summary.closes_on.as_deref(), Some("2026-09-01"));
    assert_eq!(summary.urgent_response_count, 1);
    assert_eq!(summary.items[0].state, DeadlineItemState::IncomingOffer);
    assert_eq!(summary.items[0].counterparty_team_name, "big-club FC");
    assert_eq!(summary.items[0].fee, Some(900_000));
}

#[test]
fn the_summary_is_calm_early_in_the_window() {
    let mut game = deadline_game();
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(20);

    let summary = transfer_deadline_summary(&game);

    assert!(!summary.urgent);
    assert_eq!(summary.phase, "mid");
}

#[test]
fn an_agreed_deal_is_scheduled_not_urgent() {
    // Given a deal agreed for a later registration.
    let mut game = deadline_game();
    game.players[0].transfer_offers[0].status = TransferOfferStatus::PendingRegistration;
    game.players[0].transfer_offers[0].registration_date = Some("2027-01-01".to_string());

    let summary = transfer_deadline_summary(&game);

    assert_eq!(summary.urgent_response_count, 0);
    assert_eq!(
        summary.items[0].state,
        DeadlineItemState::ScheduledRegistration
    );
    assert_eq!(
        summary.items[0].registration_date.as_deref(),
        Some("2027-01-01")
    );
}

#[test]
fn unanswered_talks_end_when_the_last_day_is_over_and_the_user_is_told_once() {
    // Given an unanswered bid and a loan approach on deadline day.
    let mut game = deadline_game();
    game.players[0].loan_offers.push(domain::player::LoanOffer {
        id: "loan-1".to_string(),
        from_team_id: "big-club".to_string(),
        parent_team_id: "team-1".to_string(),
        start_date: "2026-09-01".to_string(),
        end_date: "2027-01-01".to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        last_manager_wage_contribution_pct: None,
        last_manager_end_date: None,
        last_manager_buy_option_fee: None,
        negotiation_round: 1,
        suggested_wage_contribution_pct: None,
        suggested_end_date: None,
        suggested_buy_option_fee: None,
        status: LoanOfferStatus::Pending,
        date: "2026-08-30".to_string(),
        closed_on: None,
    });

    // When the closing sweep runs, twice.
    expire_negotiations_at_deadline(&mut game);
    expire_negotiations_at_deadline(&mut game);

    // Then both lapse, once, with one message.
    assert_eq!(
        offer_status(&game, "offer-1"),
        TransferOfferStatus::Withdrawn
    );
    assert_eq!(
        game.players[0].loan_offers[0].status,
        LoanOfferStatus::Withdrawn
    );
    assert_eq!(deadline_messages(&game), 1);
    let message = game
        .messages
        .iter()
        .find(|m| m.subject_key.as_deref() == Some("be.msg.deadlineExpired.subject"))
        .unwrap();
    assert_eq!(message.i18n_params.get("count"), Some(&"2".to_string()));
}

#[test]
fn the_sweep_spares_agreed_deals_and_offers_that_arrived_today() {
    // Given an agreed deal, and a fresh bid that arrived today.
    let mut game = deadline_game();
    game.players[0].transfer_offers[0].status = TransferOfferStatus::PendingRegistration;
    pending_offer(&mut game, "offer-2", "small-club", 800_000, "2026-09-01");

    expire_negotiations_at_deadline(&mut game);

    // Then neither is touched, and nobody is told anything lapsed.
    assert_eq!(
        offer_status(&game, "offer-1"),
        TransferOfferStatus::PendingRegistration
    );
    assert_eq!(offer_status(&game, "offer-2"), TransferOfferStatus::Pending);
    assert_eq!(deadline_messages(&game), 0);
}

#[test]
fn the_sweep_does_nothing_before_the_last_day() {
    let mut game = deadline_game();
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(1);

    expire_negotiations_at_deadline(&mut game);

    assert_eq!(offer_status(&game, "offer-1"), TransferOfferStatus::Pending);
    assert_eq!(deadline_messages(&game), 0);
}
