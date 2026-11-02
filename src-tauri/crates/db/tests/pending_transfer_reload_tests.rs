//! Run E: a paid transfer that waits for the window, and the assistant's recruitment memory,
//! saved to a real `.db` file and read back by a fresh `SaveManager`.
//!
//! What must hold across the reload is that the career reaches the same place it would have
//! reached without it: the agreement with the player's own wage and length is still on the offer,
//! it registers once, at that wage, and the money moves once.

#[path = "../../ofm_core/tests/market_fixtures/mod.rs"]
#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use db::save_manager::SaveManager;
use domain::delegation::{ActiveApproach, Duty, ProposalAction, ShelveReason};
use domain::player::{Player, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use domain::transfer_terms::TransferTerms;
use market_fixtures::{PLAYER_ID, game_with, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, make_transfer_bid_with_terms,
    process_pending_transfer_registrations,
};

const USER: &str = "team-1";
const FEE: u64 = 40_000_000;

/// A star of a bigger club, hesitant to drop down,, whom the user's club can sign only if it pays him
/// what he asks, with the window shut until the new year.
fn closed_window_market() -> Game {
    let mut owned = player("1998-01-01");
    owned.team_id = Some("big-club".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 86;
    owned.potential = 86;
    owned.market_value = 8_000_000;
    owned.morale = 75;
    let mut game = game_with(owned);
    game.teams[0].reputation = 350;
    game.teams[1].reputation = 700;
    for team in &mut game.teams {
        team.transfer_budget = 80_000_000;
        team.finance = 90_000_000;
        team.wage_budget = 80_000_000;
    }
    let window = &mut game.season_context.transfer_window;
    window.status = TransferWindowStatus::Closed;
    window.opens_on = Some("2027-01-01".to_string());
    game
}

fn open_the_window(game: &mut Game) {
    set_date(game, 2027, 1, 1);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
}

/// Given an agreement struck with the player's own terms while the window is shut
/// When the career is saved, reloaded from disk, and the window opens
/// Then the reloaded career registers the same transfer at the same wage as the one that was never
///      saved, and the buyer pays once.
#[test]
fn a_pending_transfer_with_the_players_own_terms_registers_the_same_after_a_reload() {
    let mut game = closed_window_market();
    let first = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, TransferTerms::default())
        .expect("the bid resolves");
    assert_eq!(first.decision, TransferNegotiationDecision::Rejected);
    let demand = first
        .player_demand
        .expect("the unsettled star names his terms");
    let terms = TransferTerms {
        player_weekly_wage: demand.weekly_wage,
        player_contract_years: u8::try_from(demand.contract_years).unwrap(),
        ..TransferTerms::default()
    };
    let agreed = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms.clone())
        .expect("the bid resolves");
    assert_eq!(agreed.decision, TransferNegotiationDecision::Accepted);
    assert!(
        agreed.registration_date.is_some(),
        "he waits for the window"
    );
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Pending")
        .unwrap();
    let mut reloaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    let offer = |g: &Game| {
        prospect(g)
            .transfer_offers
            .iter()
            .find(|o| o.status == TransferOfferStatus::PendingRegistration)
            .cloned()
            .expect("the agreement is still on the player")
    };
    assert_eq!(
        offer(&reloaded).terms,
        terms,
        "his wage and length survived"
    );
    assert_eq!(offer(&reloaded).id, offer(&game).id);
    assert_eq!(offer(&reloaded).fee, offer(&game).fee);
    assert_eq!(
        offer(&reloaded).registration_date,
        offer(&game).registration_date
    );

    for career in [&mut game, &mut reloaded] {
        open_the_window(career);
        process_pending_transfer_registrations(career);
    }

    for (name, career) in [("never saved", &game), ("reloaded", &reloaded)] {
        assert_eq!(prospect(career).team_id.as_deref(), Some(USER), "{name}");
        assert_eq!(prospect(career).wage(), demand.weekly_wage, "{name}: wage");
        let moves = prospect(career)
            .movement_history
            .iter()
            .filter(|entry| entry.kind == domain::player::PlayerMovementKind::PermanentTransfer)
            .count();
        assert_eq!(moves, 1, "{name}: one move in the history");
    }
    let cash = |g: &Game| {
        g.teams
            .iter()
            .map(|t| (t.id.clone(), t.finance))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        cash(&game),
        cash(&reloaded),
        "the money moved the same way, once"
    );
}

/// Given the assistant mid-negotiation with a target set aside and a proposal waiting
/// When the career is saved and reloaded twice
/// Then everything he had in hand is exactly what it was.
#[test]
fn the_assistants_recruitment_memory_survives_two_reloads() {
    let mut owned: Player = player("1999-01-01");
    owned.team_id = Some("small-club".to_string());
    let mut game = game_with(owned);
    game.delegation.set(Duty::PlayerRecruitment, true, false);
    game.delegation.limits.max_transfer_fee = 7_000_000;
    game.delegation.recruitment.replace(ActiveApproach {
        player_id: PLAYER_ID.to_string(),
        started: "2026-08-01".to_string(),
        need: "Defender".to_string(),
        max_fee: 3_300_000,
        next_fee: 3_150_000,
        wage: 41_000,
        years: 4,
        rounds: 2,
    });
    game.delegation
        .recruitment
        .shelve("someone-else", "2026-10-01", ShelveReason::PlayerRefused);
    game.delegation
        .recruitment
        .record_approach(PLAYER_ID, "2026-08-01");
    game.delegation.propose(
        Duty::PlayerRecruitment,
        "2026-08-02",
        ProposalAction::SignPlayer {
            player_id: "third".to_string(),
            fee: 9_000_000,
            wage: 52_000,
            years: 5,
        },
        "delegation.proposal.signPlayer",
        vec![("player".to_string(), "Third".to_string())],
        "delegation.reason.recruitAboveLimit",
    );

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Memory")
        .unwrap();
    let first = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(first.delegation, game.delegation);

    SaveManager::init(&saves)
        .unwrap()
        .save_game(&first, &id)
        .unwrap();
    let second = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(second.delegation, game.delegation);
    assert_eq!(second.delegation.recruitment.active[0].wage, 41_000);
    assert!(
        second
            .delegation
            .recruitment
            .is_shelved("someone-else", "2026-09-01")
    );
}

/// A fair move up for a player of a small club, with a month of open window left.
fn open_window_market() -> Game {
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
    for team in &mut game.teams {
        if team.id == "small-club" {
            team.reputation = 90;
        }
    }
    game.teams[0].transfer_budget = 60_000_000;
    game.teams[0].finance = 80_000_000;
    game.teams[0].wage_budget = 60_000_000;
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(30);
    game
}

/// Given a deal the clubs and the player agreed with the window open, still being processed
/// When the career is saved and reloaded before the registration date
/// Then the reloaded career still has the player at his club, the same date, and registers him on
///      that date exactly as the career that was never saved does.
#[test]
fn a_deal_being_processed_in_an_open_window_survives_a_reload() {
    let mut game = open_window_market();
    let shown = ofm_core::transfers::make_manager_transfer_bid(
        &mut game,
        PLAYER_ID,
        12_000_000,
        TransferTerms::default(),
    )
    .unwrap();
    let terms = TransferTerms {
        player_weekly_wage: shown.player_demand.as_ref().map_or(0, |d| d.weekly_wage),
        player_contract_years: shown
            .player_demand
            .as_ref()
            .map_or(0, |d| u8::try_from(d.contract_years).unwrap()),
        ..TransferTerms::default()
    };
    let agreed =
        ofm_core::transfers::make_manager_transfer_bid(&mut game, PLAYER_ID, 12_000_000, terms)
            .unwrap();
    assert_eq!(agreed.decision, TransferNegotiationDecision::Accepted);
    let date = agreed.registration_date.expect("a registration date");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Processing")
        .unwrap();
    let mut reloaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(prospect(&reloaded).team_id.as_deref(), Some("small-club"));

    let due = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap();
    for career in [&mut game, &mut reloaded] {
        use chrono::Datelike;
        set_date(career, due.year(), due.month(), due.day());
        // The loader derives the window from the real calendar, which the fixture's dates do not
        // follow; the existing reload test opens it by hand for the same reason.
        career.season_context.transfer_window.status = TransferWindowStatus::Open;
        process_pending_transfer_registrations(career);
    }
    for (name, career) in [("never saved", &game), ("reloaded", &reloaded)] {
        assert_eq!(prospect(career).team_id.as_deref(), Some(USER), "{name}");
    }
    assert_eq!(game.teams[0].finance, reloaded.teams[0].finance);
}
