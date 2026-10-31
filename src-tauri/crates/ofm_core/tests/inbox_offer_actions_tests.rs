//! Answering a transfer offer from its inbox message: same rules as the Transfers tab, once.

mod market_fixtures;

use domain::player::TransferOfferStatus;
use market_fixtures::{PLAYER_ID, game_with, pending_offer, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::messages::transfer_interest_digest_message;
use ofm_core::transfers::apply_transfer_offer_response;

const MESSAGE_ID: &str = "transfer_interest_prospect-1";
const ACTION_ID: &str = "respond_offer:offer-1";

/// A user-owned player with a 3 000 000 bid from the big club and the inbox thread describing it.
fn game_with_offer_message() -> Game {
    let mut owned = player("1999-01-01");
    owned.team_id = Some("team-1".to_string());
    owned.market_value = 2_500_000;
    let mut game = game_with(owned);
    set_date(&mut game, 2027, 3, 1);
    pending_offer(&mut game, "offer-1", "big-club", 3_000_000, "2027-03-01");
    game.messages.push(transfer_interest_digest_message(
        PLAYER_ID,
        "Pat Prospect",
        1,
        "big-club FC",
        3_000_000,
        "offer-1",
        "2027-03-01",
    ));
    game
}

fn action_resolved(game: &Game) -> bool {
    game.messages
        .iter()
        .find(|m| m.id == MESSAGE_ID)
        .and_then(|m| m.actions.iter().find(|a| a.id == ACTION_ID))
        .expect("action present")
        .resolved
}

fn offer_status(game: &Game) -> TransferOfferStatus {
    prospect(game).transfer_offers[0].status.clone()
}

#[test]
fn accepting_from_the_inbox_sells_the_player_and_resolves_the_action_once() {
    // Given an offer message with an accept action.
    let mut game = game_with_offer_message();
    let seller_cash = game.teams[0].finance;

    // When the manager accepts from the inbox.
    let effect =
        apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "accept").expect("handled");

    // Then he is sold, the money moved, the action is resolved and the player is told.
    assert_eq!(effect.i18n_key, "be.msg.transferInterest.effect.accepted");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert_eq!(game.teams[0].finance, seller_cash + 3_000_000);
    assert!(action_resolved(&game));

    // And replaying the same answer does nothing: no second sale, no second payment.
    let cash_after = game.teams[0].finance;
    assert!(apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "accept").is_none());
    assert_eq!(game.teams[0].finance, cash_after);
}

#[test]
fn rejecting_from_the_inbox_closes_the_offer_and_keeps_the_player() {
    let mut game = game_with_offer_message();

    let effect =
        apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "reject").expect("handled");

    assert_eq!(effect.i18n_key, "be.msg.transferInterest.effect.rejected");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(offer_status(&game), TransferOfferStatus::Rejected);
    assert!(action_resolved(&game));
}

#[test]
fn an_offer_already_answered_elsewhere_closes_the_action_with_an_explanation() {
    // Given the offer was withdrawn before the manager used the inbox.
    let mut game = game_with_offer_message();
    game.players[0].transfer_offers[0].status = TransferOfferStatus::Withdrawn;

    let effect =
        apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "accept").expect("handled");

    assert_eq!(effect.i18n_key, "be.msg.transferInterest.effect.expired");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert!(action_resolved(&game));
}

#[test]
fn a_refused_sale_leaves_everything_untouched_and_the_action_open() {
    // Given a squad that cannot spare him: everybody else has been released, so selling him
    // would break the squad floor.
    let mut game = game_with_offer_message();
    for other in game.players.iter_mut().filter(|p| p.id != PLAYER_ID) {
        if other.team_id.as_deref() == Some("team-1") {
            other.team_id = None;
        }
    }
    let journal_before = game.cash_journal.len();
    let cash_before = game.teams[0].finance;

    // When the manager accepts from the inbox.
    let effect =
        apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "accept").expect("handled");

    // Then the reason is reported, nothing moved, and the action can still be used elsewhere.
    assert!(
        effect.i18n_key.starts_with("be.error."),
        "{}",
        effect.i18n_key
    );
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(game.teams[0].finance, cash_before);
    assert_eq!(game.cash_journal.len(), journal_before);
    assert_eq!(offer_status(&game), TransferOfferStatus::Pending);
    assert!(!action_resolved(&game));
}

#[test]
fn actions_and_options_that_are_not_ours_are_ignored() {
    let mut game = game_with_offer_message();

    assert!(apply_transfer_offer_response(&mut game, MESSAGE_ID, ACTION_ID, "maybe").is_none());
    assert!(
        apply_transfer_offer_response(&mut game, MESSAGE_ID, "view_transfers", "accept").is_none()
    );
    assert!(apply_transfer_offer_response(&mut game, "missing", ACTION_ID, "accept").is_none());
    assert_eq!(offer_status(&game), TransferOfferStatus::Pending);
}
