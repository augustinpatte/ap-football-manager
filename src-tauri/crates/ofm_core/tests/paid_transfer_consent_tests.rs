//! A paid transfer needs three agreements: the clubs', the player's and the books'. These tests
//! pin the player's, and that it is told apart from the other two.

#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use chrono::Datelike;
use domain::player::{Player, TransferOfferStatus};
use domain::transfer_terms::TransferTerms;
use market_fixtures::{PLAYER_ID, game_with, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, TransferNegotiationOutcome, evaluate_transfer_market,
    make_transfer_bid_with_terms, project_transfer_bid_financial_impact_with_terms,
    respond_to_offer,
};

const REFUSES: &str = "transfers.transferFeedbackPlayerRefusesHeadline";
const WANTS_MORE: &str = "transfers.transferFeedbackPlayerWantsMoreHeadline";

struct Setup {
    owner: &'static str,
    owner_reputation: u32,
    buyer_reputation: u32,
    ovr: u8,
    born: &'static str,
}

/// The fixture player at `owner`, ready to be bought by the user's club (team-1).
fn market(setup: &Setup) -> Game {
    let mut owned = player(setup.born);
    owned.team_id = Some(setup.owner.to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = setup.ovr;
    owned.potential = setup.ovr;
    owned.market_value = 8_000_000;
    owned.morale = 75;
    let mut game = game_with(owned);
    game.teams[0].reputation = setup.buyer_reputation;
    game.teams[0].transfer_budget = 60_000_000;
    game.teams[0].finance = 80_000_000;
    game.teams[0].wage_budget = 60_000_000;
    for team in &mut game.teams {
        if team.id == setup.owner {
            team.reputation = setup.owner_reputation;
        }
    }
    game
}

fn elite_at_a_giant() -> Setup {
    Setup {
        owner: "big-club",
        owner_reputation: 900,
        buyer_reputation: 150,
        ovr: 86,
        born: "1998-01-01",
    }
}

fn bid(game: &mut Game, fee: u64) -> TransferNegotiationOutcome {
    make_transfer_bid_with_terms(game, PLAYER_ID, fee, TransferTerms::default())
        .expect("the bid is valid")
}

fn headline(outcome: &TransferNegotiationOutcome) -> &str {
    &outcome.feedback.headline_key
}

/// Scenario 1. Given a modest club that can easily afford a fee well above his price
/// When it bids for the elite player of the most prestigious club on standard terms
/// Then he refuses, nothing is spent, and he is still where he was.
#[test]
fn a_modest_club_cannot_buy_an_elite_player_just_by_paying() {
    let mut game = market(&elite_at_a_giant());
    let cash = game.teams[0].finance;

    let outcome = bid(&mut game, 40_000_000);

    assert_eq!(outcome.decision, TransferNegotiationDecision::Rejected);
    assert!(!outcome.is_terminal, "the manager may come back with more");
    assert_eq!(headline(&outcome), REFUSES);
    assert!(
        outcome.player_demand.is_none(),
        "no wage brings him round from a giant to this club"
    );
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert_eq!(game.teams[0].finance, cash);
}

/// Scenario 2. Given the same bid
/// When the clubs would agree (the same fee buys him when the buyer is as prestigious as his club)
/// Then it is the player, not the price, who stood in the way.
#[test]
fn the_selling_club_accepts_but_the_player_refuses() {
    let mut refused = market(&elite_at_a_giant());
    let outcome = bid(&mut refused, 40_000_000);
    assert_eq!(headline(&outcome), REFUSES);

    // Control: identical in every respect but the buyer's standing.
    let mut control = market(&Setup {
        buyer_reputation: 900,
        ..elite_at_a_giant()
    });
    let outcome = bid(&mut control, 40_000_000);
    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&control).team_id.as_deref(), Some("team-1"));
}

/// Scenario 2b. Given a refused move
/// When the player's club has listed him, he is unhappy and his contract is nearly out
/// Then a lesser club can sign him: exceptional circumstances are credible, so the door is open.
#[test]
fn credible_circumstances_make_a_step_down_possible() {
    let setup = Setup {
        owner_reputation: 700,
        buyer_reputation: 350,
        ..elite_at_a_giant()
    };
    let mut stays = market(&setup);
    let outcome = bid(&mut stays, 40_000_000);
    assert_ne!(
        outcome.decision,
        TransferNegotiationDecision::Accepted,
        "settled and content, he stays"
    );

    let mut leaves = market(&setup);
    let player: &mut Player = leaves
        .players
        .iter_mut()
        .find(|p| p.id == PLAYER_ID)
        .unwrap();
    player.transfer_listed = true;
    player.morale = 30;
    player.stage_contract_end(Some("2026-12-31".to_string()));
    let outcome = bid(&mut leaves, 40_000_000);
    if outcome.decision != TransferNegotiationDecision::Accepted {
        // He asks for something: meeting it closes the deal.
        let demand = outcome
            .player_demand
            .expect("a hesitant player names his price");
        let terms = TransferTerms {
            player_weekly_wage: demand.weekly_wage,
            player_contract_years: u8::try_from(demand.contract_years).unwrap(),
            ..TransferTerms::default()
        };
        let outcome =
            make_transfer_bid_with_terms(&mut leaves, PLAYER_ID, 40_000_000, terms).expect("valid");
        assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    }
    assert_eq!(prospect(&leaves).team_id.as_deref(), Some("team-1"));
}

/// Scenario 3. Given a player who would be glad to move up
/// When the bid is far below his club's price
/// Then it is the selling club that says no, and the player's side is never raised.
#[test]
fn the_player_is_keen_but_the_selling_club_refuses() {
    let mut game = market(&Setup {
        owner: "small-club",
        owner_reputation: 90,
        buyer_reputation: 500,
        ovr: 70,
        born: "1999-01-01",
    });

    let outcome = bid(&mut game, 100_000);

    assert_ne!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert!(outcome.player_demand.is_none());
    assert_ne!(headline(&outcome), REFUSES);
    assert_ne!(headline(&outcome), WANTS_MORE);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
}

/// Scenario 4. Given a fair move up
/// When the first bid is countered and the manager meets the counter
/// Then the player moves once, the fee is paid once and his contract is the buyer's.
#[test]
fn a_realistic_transfer_succeeds_after_negotiation() {
    let mut game = market(&Setup {
        owner: "small-club",
        owner_reputation: 90,
        buyer_reputation: 500,
        ovr: 70,
        born: "1999-01-01",
    });
    let buyer_cash = game.teams[0].finance;
    let seller_cash = game.teams[2].finance;

    let first = bid(&mut game, 9_400_000);
    let fee = match first.decision {
        TransferNegotiationDecision::Accepted => 9_400_000,
        TransferNegotiationDecision::CounterOffer => first.suggested_fee.expect("a price"),
        TransferNegotiationDecision::Rejected => panic!("a fair bid is not thrown out"),
        TransferNegotiationDecision::PlayerTalks => {
            panic!("the plain path assumes the standard contract and never stops for the player")
        }
    };
    if first.decision == TransferNegotiationDecision::CounterOffer {
        let second = bid(&mut game, fee);
        assert_eq!(second.decision, TransferNegotiationDecision::Accepted);
    }

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(game.teams[0].finance, buyer_cash - fee as i64);
    assert_eq!(game.teams[2].finance, seller_cash + fee as i64);
    let log = prospect(&game)
        .movement_history
        .iter()
        .filter(|entry| entry.kind == domain::player::PlayerMovementKind::PermanentTransfer)
        .count();
    assert_eq!(log, 1, "one move in the history");
    let accepted = prospect(&game)
        .transfer_offers
        .iter()
        .filter(|o| o.status == TransferOfferStatus::Accepted)
        .count();
    assert_eq!(accepted, 1);
}

/// Scenario 5. Given a hesitant player (a step down that pay can buy back)
/// When the manager meets the wage and length he names
/// Then he signs exactly that contract; and a longer, better paid deal never scores lower.
#[test]
fn the_player_negotiates_his_wage_and_length() {
    let setup = Setup {
        owner_reputation: 700,
        buyer_reputation: 350,
        ..elite_at_a_giant()
    };
    let mut game = market(&setup);
    let first = bid(&mut game, 40_000_000);
    assert_eq!(first.decision, TransferNegotiationDecision::Rejected);
    assert_eq!(
        headline(&first),
        WANTS_MORE,
        "he names terms, he does not slam the door"
    );
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    let demand = first.player_demand.expect("his ask");
    let standard = project_transfer_bid_financial_impact_with_terms(
        &game,
        PLAYER_ID,
        40_000_000,
        &TransferTerms::default(),
    )
    .unwrap()
    .player_outlook
    .expect("the preview carries his view");
    assert!(
        demand.weekly_wage > standard.expected_wage,
        "his ask is above the standard"
    );

    let terms = TransferTerms {
        player_weekly_wage: demand.weekly_wage,
        player_contract_years: u8::try_from(demand.contract_years).unwrap(),
        ..TransferTerms::default()
    };
    let signed = make_transfer_bid_with_terms(&mut game, PLAYER_ID, 40_000_000, terms).unwrap();
    assert_eq!(signed.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(prospect(&game).wage(), demand.weekly_wage);
    let end = prospect(&game).contract_end().expect("a contract");
    let end_year: i32 = end[..4].parse().expect("a date");
    assert!(
        end_year
            >= game.clock.current_date.year() + i32::try_from(demand.contract_years).unwrap() - 1,
        "a {}-year deal ends in {end}",
        demand.contract_years
    );

    // Pay is monotone: more never makes him less willing, on the same player.
    let rank_at = |wage: u32| {
        let game = market(&setup);
        let outlook = project_transfer_bid_financial_impact_with_terms(
            &game,
            PLAYER_ID,
            40_000_000,
            &TransferTerms {
                player_weekly_wage: wage,
                ..TransferTerms::default()
            },
        )
        .unwrap()
        .player_outlook
        .unwrap();
        rank(&outlook)
    };
    let ranks: Vec<u8> = [6_000, 9_000, 12_000, 13_000, 20_000]
        .into_iter()
        .map(rank_at)
        .collect();
    assert!(ranks.windows(2).all(|pair| pair[0] <= pair[1]), "{ranks:?}");
    assert!(ranks[0] < ranks[4], "pay changes his mind: {ranks:?}");
}

fn rank(outlook: &ofm_core::transfers::PlayerOutlook) -> u8 {
    match outlook.interest {
        ofm_core::contracts::InterestLevel::Keen => 2,
        ofm_core::contracts::InterestLevel::Hesitant => 1,
        ofm_core::contracts::InterestLevel::Unlikely => 0,
    }
}

/// Scenario 5b. Given the same player and two destination squads
/// When one would make him a backup and the other a regular
/// Then the one that gives him a place is the one he likes better.
#[test]
fn the_part_he_would_play_counts() {
    let setup = Setup {
        owner: "small-club",
        owner_reputation: 400,
        buyer_reputation: 400,
        ovr: 66,
        born: "1999-01-01",
    };
    let interest = |depth_ovr: u8| {
        let mut game = market(&setup);
        for p in game
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team-1"))
        {
            p.ovr = depth_ovr;
        }
        // Enough forwards at his level to push him down the order: the fixture's squads are thin.
        for index in 0..4 {
            let mut forward = player("1996-01-01");
            forward.id = format!("extra-forward-{index}");
            forward.team_id = Some("team-1".to_string());
            forward.ovr = depth_ovr;
            game.players.push(forward);
        }
        project_transfer_bid_financial_impact_with_terms(
            &game,
            PLAYER_ID,
            8_000_000,
            &TransferTerms::default(),
        )
        .unwrap()
        .player_outlook
        .unwrap()
    };
    let into_a_weak_side = interest(55);
    let into_a_strong_side = interest(80);
    assert_eq!(format!("{:?}", into_a_weak_side.place_in_new_side), "Key");
    assert_eq!(
        format!("{:?}", into_a_strong_side.place_in_new_side),
        "Backup"
    );
    assert!(rank(&into_a_weak_side) > rank(&into_a_strong_side));
}

/// Scenario 6. Given a bid the buyer cannot pay
/// When it is made
/// Then it is refused in Rust before anything is agreed or spent.
#[test]
fn a_financially_impossible_deal_is_refused_untouched() {
    let mut game = market(&Setup {
        owner: "small-club",
        owner_reputation: 90,
        buyer_reputation: 500,
        ovr: 70,
        born: "1999-01-01",
    });
    game.teams[0].finance = 500_000;
    game.teams[0].transfer_budget = 500_000;
    let before = serde_json::to_string(&game.players).unwrap();

    let error =
        make_transfer_bid_with_terms(&mut game, PLAYER_ID, 9_000_000, TransferTerms::default())
            .unwrap_err();

    assert!(error.starts_with("be.error."), "a translation key: {error}");
    assert_eq!(serde_json::to_string(&game.players).unwrap(), before);
    assert!(game.cash_journal.is_empty());
}

/// Scenario 7. Given a star that three AI clubs could each afford
/// When the market sweeps for a month
/// Then he moves at most once per day he moves, is on exactly one club's books, and the money
///      that left the buyers equals the money that reached the sellers.
#[test]
fn competing_ai_interest_never_duplicates_a_transfer() {
    let mut star = player("1999-01-01");
    star.team_id = Some("small-club".to_string());
    star.stage_contract_end(Some("2028-06-30".to_string()));
    star.stage_wage(10_000);
    star.ovr = 78;
    star.potential = 82;
    star.market_value = 3_000_000;
    star.transfer_listed = true;
    let mut game = game_with(star);
    game.manager.team_id = None;
    for team in &mut game.teams {
        team.transfer_budget = 40_000_000;
        team.finance = 60_000_000;
        team.wage_budget = 60_000_000;
    }
    let total_cash: i64 = game.teams.iter().map(|t| t.finance).sum();

    for day in 0..30 {
        set_date(&mut game, 2026, 8, 1 + day);
        evaluate_transfer_market(&mut game);
    }

    let owners: Vec<_> = game
        .players
        .iter()
        .filter(|p| p.id == PLAYER_ID)
        .map(|p| p.team_id.clone())
        .collect();
    assert_eq!(owners.len(), 1, "one player record");
    let moves: Vec<_> = game
        .competitions
        .iter()
        .flat_map(|c| &c.transfer_log)
        .chain(game.league.iter().flat_map(|l| &l.transfer_log))
        .filter(|t| t.player_id == PLAYER_ID)
        .collect();
    let mut dated: Vec<_> = moves.iter().map(|t| t.date.clone()).collect();
    dated.sort();
    dated.dedup();
    assert_eq!(dated.len(), moves.len(), "never two moves in a day");
    assert_eq!(
        game.teams.iter().map(|t| t.finance).sum::<i64>(),
        total_cash,
        "money only changes hands"
    );
}

/// Given an offer from a club the player would not join, for one of the manager's players
/// When the manager tries to accept it
/// Then the player's veto stands, with a translation-key error and nothing moved.
#[test]
fn a_player_can_refuse_to_join_a_club_that_bids_for_him() {
    let mut mine = player("1998-01-01");
    mine.team_id = Some("team-1".to_string());
    mine.stage_contract_end(Some("2030-06-30".to_string()));
    mine.stage_wage(10_000);
    mine.ovr = 86;
    mine.potential = 86;
    mine.morale = 80;
    mine.market_value = 20_000_000;
    let mut game = game_with(mine);
    game.teams[0].reputation = 900;
    game.teams[2].reputation = 120;
    market_fixtures::pending_offer(&mut game, "offer-1", "small-club", 25_000_000, "2026-08-01");

    let error = respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap_err();

    assert_eq!(error, ofm_core::transfers::ERR_PLAYER_REFUSES_MOVE);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(
        prospect(&game).transfer_offers[0].status,
        TransferOfferStatus::Pending
    );
}
