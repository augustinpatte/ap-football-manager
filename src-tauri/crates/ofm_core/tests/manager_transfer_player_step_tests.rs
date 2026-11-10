//! Run F1, anomaly 3. A paid transfer the manager makes has a visible step with the player: once
//! the clubs agree, nothing moves until the contract has been put to him and he has answered.
//!
//! Before, a plain fee that the clubs agreed and that the player would take on the buyer's
//! standard contract completed on the spot: Jordan Henderson was signed after a negotiation
//! between clubs and no step with the player was ever shown.

#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use domain::player::TransferOfferStatus;
use domain::transfer_terms::TransferTerms;
use market_fixtures::{PLAYER_ID, game_with, pending_offer, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, TransferNegotiationOutcome, make_manager_transfer_bid,
    make_transfer_bid_with_terms,
};

const FEE: u64 = 12_000_000;
const WANTS_MORE: &str = "transfers.transferFeedbackPlayerWantsMoreHeadline";

/// A fair move up: a 70-rated player of a small club, bought by a club of 500 reputation. The
/// clubs agree at `FEE` and the player is keen on the standard contract.
fn fair_move() -> Game {
    let mut owned = player("1999-01-01");
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
    game
}

fn plain() -> TransferTerms {
    TransferTerms::default()
}

fn with_contract(wage: u32, years: u8) -> TransferTerms {
    TransferTerms {
        player_weekly_wage: wage,
        player_contract_years: years,
        ..plain()
    }
}

/// A bid the clubs accept at `FEE` and that goes on to the player's step.
fn first_bid(game: &mut Game) -> TransferNegotiationOutcome {
    make_manager_transfer_bid(game, PLAYER_ID, FEE, plain()).expect("the bid is valid")
}

/// Scenario 1. Given a fee the selling club accepts and a player who is keen
/// When the manager bids without naming a contract
/// Then the clubs' agreement is reported, the contract the player would sign is shown, and
/// nothing has moved: same club, same cash, no offer marked agreed, no movement recorded.
#[test]
fn the_clubs_agreeing_does_not_complete_the_transfer() {
    let mut game = fair_move();
    let cash = (game.teams[0].finance, game.teams[2].finance);

    let outcome = first_bid(&mut game);

    assert_eq!(outcome.decision, TransferNegotiationDecision::PlayerTalks);
    assert!(!outcome.is_terminal, "the manager is still negotiating");
    let shown = outcome.player_demand.expect("the contract he would sign");
    assert!(shown.weekly_wage > 0 && shown.contract_years > 0);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
    assert_eq!((game.teams[0].finance, game.teams[2].finance), cash);
    assert!(
        prospect(&game)
            .transfer_offers
            .iter()
            .all(|offer| offer.status != TransferOfferStatus::Accepted
                && offer.status != TransferOfferStatus::PendingRegistration),
        "no offer is agreed yet"
    );
    assert_eq!(
        prospect(&game)
            .movement_history
            .iter()
            .filter(|entry| entry.kind == domain::player::PlayerMovementKind::PermanentTransfer)
            .count(),
        0
    );
}

/// Scenario 2. Given the player's step is shown
/// When the manager puts exactly the contract he showed
/// Then the player accepts, the transfer completes once, and the contract written is that one.
#[test]
fn the_player_accepts_the_contract_put_to_him_and_only_then_the_transfer_completes() {
    let mut game = fair_move();
    let shown = first_bid(&mut game).player_demand.expect("his contract");
    let years = u8::try_from(shown.contract_years).unwrap();

    let outcome = make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(shown.weekly_wage, years),
    )
    .unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(prospect(&game).wage(), shown.weekly_wage);
}

/// Scenario 3. Given a contract the player finds too low
/// When the manager puts it to him
/// Then he answers with his own terms, as a non-final answer, and stays where he is.
#[test]
fn a_contract_below_his_expectations_gets_a_counter_proposal_not_a_signature() {
    let mut game = fair_move();
    let shown = first_bid(&mut game).player_demand.expect("his contract");

    let outcome = make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(
            shown.weekly_wage * 60 / 100,
            u8::try_from(shown.contract_years).unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::Rejected);
    assert!(!outcome.is_terminal);
    assert_eq!(outcome.feedback.headline_key, WANTS_MORE);
    let ask = outcome.player_demand.expect("his counter-proposal");
    assert!(ask.weekly_wage > shown.weekly_wage * 60 / 100);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));

    // The counter-proposal can be taken: that completes the transfer on his terms.
    let years = u8::try_from(ask.contract_years).unwrap();
    let done = make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(ask.weekly_wage, years),
    )
    .unwrap();
    assert_eq!(done.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&game).wage(), ask.weekly_wage);
}

/// Scenario 4. Given a move the player will not make at any wage
/// When the manager bids
/// Then he refuses straight away (no step is staged for a move he has ruled out).
#[test]
fn a_player_who_will_not_move_refuses_instead_of_being_offered_a_contract() {
    let mut game = fair_move();
    // The player's club is far more prestigious than the buyer and he is settled there.
    game.teams[0].reputation = 100;
    for team in &mut game.teams {
        if team.id == "small-club" {
            team.reputation = 900;
        }
    }
    let player_mut = game.players.iter_mut().find(|p| p.id == PLAYER_ID).unwrap();
    player_mut.ovr = 86;
    player_mut.potential = 86;

    let outcome = first_bid(&mut game);

    assert_ne!(outcome.decision, TransferNegotiationDecision::PlayerTalks);
    assert_ne!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
}

/// Scenario 5. Given the same agreed fee
/// When it is bid through the path the assistant coach uses, which acts inside limits the manager
/// set and has no screen
/// Then the standard contract is still assumed and judged, so delegated recruitment is unchanged.
#[test]
fn the_assistants_path_still_judges_the_standard_contract_on_the_spot() {
    let mut game = fair_move();

    let outcome = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, plain()).unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
}

/// Scenario 6. Given a fee below what the selling club accepts
/// When the manager bids
/// Then the clubs' negotiation is untouched by the player's step: a counter-offer on the fee.
#[test]
fn the_clubs_still_negotiate_the_fee_before_the_player_is_heard() {
    let mut game = fair_move();

    let outcome = make_manager_transfer_bid(&mut game, PLAYER_ID, 9_400_000, plain()).unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::CounterOffer);
    assert!(outcome.suggested_fee.is_some());
    assert!(outcome.player_demand.is_none());
}

/// Scenario 7. Given a contract far below what he would sign
/// When the manager puts it to him
/// Then he refuses it outright, without a counter-proposal, and stays.
#[test]
fn an_insulting_contract_is_refused_without_a_counter() {
    let mut game = fair_move();
    let shown = first_bid(&mut game).player_demand.expect("his contract");
    let years = u8::try_from(shown.contract_years).unwrap();

    let outcome = make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(shown.weekly_wage / 10, years),
    )
    .unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::Rejected);
    assert_eq!(
        outcome.feedback.headline_key,
        "transfers.transferFeedbackPlayerRefusesHeadline"
    );
    assert!(outcome.player_demand.is_none());
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
}

/// Scenario 8. Given a transfer window that is closed, so an agreed deal waits for the day
/// registration opens
/// When the player accepts the contract put to him
/// Then the agreement is stored with that contract (not the buyer's standard one), the player has
/// not moved yet, and nothing is paid before the registration date.
#[test]
fn the_contract_he_accepted_is_kept_until_the_registration_day() {
    let mut game = fair_move();
    set_date(&mut game, 2026, 12, 20);
    game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Closed;
    game.season_context.transfer_window.opens_on = Some("2027-01-01".to_string());
    let cash = game.teams[0].finance;
    let shown = first_bid(&mut game).player_demand.expect("his contract");
    let years = u8::try_from(shown.contract_years).unwrap();

    let outcome = make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(shown.weekly_wage, years),
    )
    .unwrap();

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(outcome.registration_date.as_deref(), Some("2027-01-01"));
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
    assert_eq!(
        game.teams[0].finance, cash,
        "nothing is paid before registration"
    );
    let stored = prospect(&game)
        .transfer_offers
        .iter()
        .find(|offer| offer.status == TransferOfferStatus::PendingRegistration)
        .expect("the agreement waits for the window");
    assert_eq!(stored.terms.player_weekly_wage, shown.weekly_wage);
    assert_eq!(
        u32::from(stored.terms.player_contract_years),
        shown.contract_years
    );
}

/// Scenario 9. Given another club's offer for the same player
/// When the clubs agree and the player's step is shown
/// Then that offer is untouched (nothing is agreed yet); once the player accepts and the transfer
/// completes, it is withdrawn so he is not sold twice.
#[test]
fn another_clubs_offer_survives_the_players_step_and_is_withdrawn_on_completion() {
    let mut game = fair_move();
    pending_offer(&mut game, "rival-1", "big-club", 7_000_000, "2026-08-01");
    let shown = first_bid(&mut game).player_demand.expect("his contract");
    let rival_status = |game: &Game| {
        prospect(game)
            .transfer_offers
            .iter()
            .find(|offer| offer.id == "rival-1")
            .map(|offer| offer.status.clone())
    };
    assert_eq!(rival_status(&game), Some(TransferOfferStatus::Pending));

    make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        FEE,
        with_contract(
            shown.weekly_wage,
            u8::try_from(shown.contract_years).unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_ne!(rival_status(&game), Some(TransferOfferStatus::Pending));
}
