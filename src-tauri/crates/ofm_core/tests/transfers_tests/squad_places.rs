//! I2.1: a place promised to an agreed purchase is reserved on every route that signs a senior.
//!
//! The reservation is derived from a `PendingRegistration` offer, so a status change — register,
//! withdraw, expire — releases it without a second source of truth. These tests play the boundary
//! (29 with and without a promise, 30, a place freed) through the production paths: an in-game bid,
//! a free-agent talk, a registration loop and a save round trip.

use super::*;
use ofm_core::contracts::{OfferTerms, squad_capacity, submit_free_agent_offer};

const SQUAD_FULL: &str = "be.error.freeAgents.squadFull";

/// The user's club and a seller, window shut with a reopening date, two willing targets.
fn two_targets_in_the_shut_window(finance: i64, transfer_budget: i64) -> Game {
    let mut game = make_game_with_player(make_player("target-a"), vec![], finance, transfer_budget);
    game.players.push(make_player("target-b"));
    give_every_club_squad_depth(&mut game);
    game.clock.current_date = Utc.with_ymd_and_hms(2026, 12, 20, 12, 0, 0).unwrap();
    game.season_context.transfer_window.status = TransferWindowStatus::Closed;
    game.season_context.transfer_window.opens_on = Some("2027-01-01".to_string());
    game
}

/// Brings `club` up to `seniors` senior players on no wage, as the depth fixture does.
fn fill_senior_list(game: &mut Game, club: &str, seniors: usize) {
    let mut index = 0;
    while squad_capacity(game, club).registered_seniors < seniors {
        let mut filler = make_player(&format!("filler-{club}-{index}"));
        filler.team_id = Some(club.to_string());
        filler.stage_wage(0);
        filler.market_value = 0;
        game.players.push(filler);
        index += 1;
    }
}

/// An unattached player the user's club can talk to.
fn a_free_forward(id: &str) -> Player {
    let mut player = make_player(id);
    player.team_id = None;
    player.stage_contract_end(None);
    player.stage_wage(0);
    player.market_value = 0;
    player
}

fn offer(wage: u32) -> OfferTerms {
    OfferTerms {
        weekly_wage: wage,
        contract_years: 2,
        signing_bonus: 0,
        promise_stepping_stone: false,
    }
}

fn open_the_window(game: &mut Game) {
    game.clock.current_date = Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap();
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
}

/// Given 29 seniors and no agreed purchase
/// When the manager bids for a player
/// Then the last place is his: the bid is agreed.
#[test]
fn a_purchase_can_be_agreed_into_the_last_free_place() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);

    let agreed = make_transfer_bid(&mut game, "target-a", 2_000_000);

    assert!(agreed.is_ok(), "{agreed:?}");
}

/// Given 29 seniors and one agreed purchase waiting to register
/// When the manager bids for a second player
/// Then the last place is promised: the second bid is refused, and nothing is agreed for him.
#[test]
fn a_purchase_is_refused_when_the_last_place_is_promised() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first bid takes the place");

    let second = make_transfer_bid(&mut game, "target-b", 2_000_000);

    assert!(
        second
            .as_ref()
            .err()
            .is_some_and(|error| error.starts_with(SQUAD_FULL)),
        "{second:?}"
    );
    assert!(
        game.players
            .iter()
            .find(|player| player.id == "target-b")
            .is_some_and(|player| player.transfer_offers.is_empty()),
        "nothing was agreed for the second player"
    );
}

/// Given a list of 30 that also has an agreed purchase waiting to register
/// When the manager bids for another player
/// Then the list is full without the reservation being counted twice: the bid is refused with the
///      same limit, and the list still holds 30.
#[test]
fn a_full_list_with_a_pending_purchase_never_exceeds_the_limit() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 30);
    // A purchase agreed while there was still a place, then the list refilled to 30: the deal is
    // still pending, so no place is free and the promised one must not be subtracted from the
    // live count a second time.
    let mut pending = make_player("pending-target");
    pending
        .transfer_offers
        .push(pending_offer("offer-pending", "team-1"));
    game.players.push(pending);

    let refused = make_transfer_bid(&mut game, "target-a", 2_000_000);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|error| error.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
    assert_eq!(squad_capacity(&game, "team-1").registered_seniors, 30);
}

/// Given 29 seniors and an agreed purchase that took the last place
/// When the window opens and the deal comes to register
/// Then its own reservation does not block it: the deal registers and the list holds 30.
#[test]
fn a_pending_purchase_does_not_block_its_own_registration() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    open_the_window(&mut game);

    process_pending_transfer_registrations(&mut game);

    assert_eq!(
        game.players
            .iter()
            .find(|player| player.id == "target-a")
            .and_then(|player| player.team_id.as_deref()),
        Some("team-1"),
        "the deal registers into the place it reserved"
    );
    assert_eq!(squad_capacity(&game, "team-1").registered_seniors, 30);
}

/// Given 29 seniors and an agreed purchase holding the last place
/// When a senior leaves before the deal's registration date
/// Then the deal registers into a place that is now free and the list never passes 30.
#[test]
fn a_pending_purchase_registers_once_a_place_frees() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    game.players
        .iter_mut()
        .find(|player| player.team_id.as_deref() == Some("team-1"))
        .expect("a senior leaves")
        .team_id = None;
    open_the_window(&mut game);

    process_pending_transfer_registrations(&mut game);

    assert_eq!(
        game.players
            .iter()
            .find(|player| player.id == "target-a")
            .and_then(|player| player.team_id.as_deref()),
        Some("team-1"),
        "the promised deal registered"
    );
    assert_eq!(squad_capacity(&game, "team-1").registered_seniors, 29);
}

/// Given 29 seniors and an agreed purchase holding the last place
/// When the manager offers a free agent a contract
/// Then the free agent meets the promise: the talk is refused with the registration limit's key
///      and no talk is opened.
#[test]
fn a_free_agent_talk_is_refused_when_the_last_place_is_promised() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    game.players.push(a_free_forward("free-agent"));

    let refused = submit_free_agent_offer(&mut game, "free-agent", offer(1_000));

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|error| error.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
    assert!(
        game.players
            .iter()
            .find(|player| player.id == "free-agent")
            .is_some_and(|player| player.morale_core.free_agent_talks.is_empty()),
        "no talk was opened"
    );
}

/// Given the same promise and 29 seniors
/// When one senior leaves and the manager offers the same free agent a contract
/// Then there is a place for him and the talk opens.
#[test]
fn a_free_agent_talk_is_accepted_once_a_place_frees() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    game.players
        .iter_mut()
        .find(|player| player.team_id.as_deref() == Some("team-1"))
        .expect("a senior leaves")
        .team_id = None;
    game.players.push(a_free_forward("free-agent"));

    let opened = submit_free_agent_offer(&mut game, "free-agent", offer(1_000));

    assert!(opened.is_ok(), "{opened:?}");
}

/// Given 29 seniors and an agreed purchase holding the last place
/// When the deal cannot register (the club can no longer pay) and is withdrawn
/// Then its place is released: a later bid for another player is agreed.
#[test]
fn a_withdrawn_pending_purchase_releases_its_place() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    // Spend the money before the registration date: the deal lapses when it falls due.
    game.teams[0].finance = 0;
    game.teams[0].transfer_budget = 0;
    open_the_window(&mut game);

    process_pending_transfer_registrations(&mut game);

    let lapsed = game
        .players
        .iter()
        .find(|player| player.id == "target-a")
        .expect("the target still exists");
    assert_ne!(lapsed.team_id.as_deref(), Some("team-1"), "he did not join");
    assert!(
        lapsed
            .transfer_offers
            .iter()
            .all(|offer| offer.status != TransferOfferStatus::PendingRegistration),
        "the agreement was withdrawn"
    );

    game.teams[0].finance = 90_000_000;
    game.teams[0].transfer_budget = 90_000_000;
    let next = make_transfer_bid(&mut game, "target-b", 2_000_000);

    assert!(
        next.is_ok(),
        "the withdrawn promise released its place: {next:?}"
    );
}

/// Given 29 seniors and an agreed purchase holding the last place, saved and loaded
/// When the loaded career bids for another player
/// Then the reservation survived the save: the second bid is still refused.
#[test]
fn a_promised_place_survives_a_save_and_reload() {
    let mut game = two_targets_in_the_shut_window(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");
    let saved = serde_json::to_string(&game).expect("the career serialises");

    let mut loaded: Game = serde_json::from_str(&saved).expect("the career loads");
    let refused = make_transfer_bid(&mut loaded, "target-b", 2_000_000);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|error| error.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
}

/// A `PendingRegistration` offer from `buyer_id` for a player at another club.
fn pending_offer(id: &str, buyer_id: &str) -> TransferOffer {
    TransferOffer {
        id: id.to_string(),
        from_team_id: buyer_id.to_string(),
        fee: 2_000_000,
        wage_offered: 0,
        last_manager_fee: None,
        negotiation_round: 1,
        suggested_counter_fee: None,
        status: TransferOfferStatus::PendingRegistration,
        date: "2026-12-20".to_string(),
        registration_date: Some("2027-01-01".to_string()),
        closed_on: None,
        registration_failure_reason: None,
        terms: Default::default(),
    }
}
