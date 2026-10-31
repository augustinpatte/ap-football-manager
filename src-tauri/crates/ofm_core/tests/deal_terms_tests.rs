//! Signing bonuses, fee instalments, sell-on clauses and the player's own say on a permanent
//! transfer: what moves, when, and that it moves once.

mod market_fixtures;

use domain::finance::CashKind;
use domain::player::{Player, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use domain::transfer_terms::{TransferObligation, TransferTerms};
use market_fixtures::{PLAYER_ID, game_with, pending_offer, player, prospect, set_date};
use ofm_core::contracts::{RenewalDecision, RenewalOffer, offer_free_agent_contract_with_terms};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, make_transfer_bid_with_terms,
    process_pending_transfer_registrations, process_transfer_obligations, respond_to_offer,
};

const FEE: u64 = 3_000_000;

fn team<'a>(game: &'a Game, id: &str) -> &'a domain::team::Team {
    game.teams.iter().find(|team| team.id == id).unwrap()
}

fn posts_of(game: &Game, kind: CashKind) -> Vec<(String, i64)> {
    game.cash_journal
        .iter()
        .filter(|post| post.kind == kind)
        .map(|post| (post.club_id.clone(), post.amount))
        .collect()
}

/// The fixture player at `owner`, ready to be bought by the user's club.
fn market_game(owner: &str, date_of_birth: &str) -> Game {
    let mut owned = player(date_of_birth);
    owned.team_id = Some(owner.to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 70;
    owned.potential = 70;
    game_with(owned)
}

fn terms(instalments: u8, sell_on_pct: u8, signing_bonus: u64) -> TransferTerms {
    TransferTerms {
        instalments,
        sell_on_pct,
        signing_bonus,
        promise_stepping_stone: false,
    }
}

fn bid(game: &mut Game, fee: u64, terms: TransferTerms) -> TransferNegotiationDecision {
    make_transfer_bid_with_terms(game, PLAYER_ID, fee, terms)
        .expect("bid resolves")
        .decision
}

fn obligations(game: &Game) -> Vec<&TransferObligation> {
    game.transfer_obligations.iter().collect()
}

fn free_agent_offer(
    game: &mut Game,
    wage: u32,
    bonus: u64,
) -> Result<ofm_core::contracts::RenewalOutcome, String> {
    offer_free_agent_contract_with_terms(
        game,
        PLAYER_ID,
        RenewalOffer {
            weekly_wage: wage,
            contract_years: 4,
        },
        false,
        bonus,
    )
}

#[test]
fn a_signing_bonus_tips_a_free_agent_who_would_otherwise_only_counter() {
    // Given his usual ask is 6 400 a week, and 6 000 is only countered.
    let mut plain = game_with(player("2005-01-01"));
    assert_eq!(
        free_agent_offer(&mut plain, 6_000, 0).unwrap().decision,
        RenewalDecision::CounterOffer
    );

    // When the same wage comes with a bonus worth the difference over the contract.
    let mut game = game_with(player("2005-01-01"));
    let cash_before = team(&game, "team-1").finance;
    let budget_before = team(&game, "team-1").transfer_budget;
    let outcome = free_agent_offer(&mut game, 6_000, 100_000).unwrap();

    // Then he signs at the wage, and the bonus left the club's cash and transfer budget once.
    assert_eq!(outcome.decision, RenewalDecision::Accepted);
    assert_eq!(prospect(&game).wage(), 6_000);
    assert_eq!(team(&game, "team-1").finance, cash_before - 100_000);
    assert_eq!(
        team(&game, "team-1").transfer_budget,
        budget_before - 100_000
    );
    assert_eq!(
        posts_of(&game, CashKind::SigningBonus),
        vec![("team-1".to_string(), -100_000)]
    );
}

#[test]
fn a_bonus_the_club_cannot_afford_refuses_the_whole_signing() {
    // Given a club with 50 000 in cash.
    let mut game = game_with(player("2005-01-01"));
    game.teams[0].finance = 50_000;
    let journal_before = game.cash_journal.len();

    // When a 100 000 bonus is offered, then the deal fails with the funds key and nothing moves.
    let error = free_agent_offer(&mut game, 6_000, 100_000).unwrap_err();

    assert_eq!(error, "be.error.transfers.insufficientFunds");
    assert!(prospect(&game).team_id.is_none());
    assert_eq!(game.teams[0].finance, 50_000);
    assert_eq!(game.cash_journal.len(), journal_before);
}

#[test]
fn a_bonus_beyond_the_wages_of_the_contract_is_not_a_valid_offer() {
    let mut game = game_with(player("2005-01-01"));

    let error = free_agent_offer(&mut game, 6_000, 6_000 * 52 * 4 + 1).unwrap_err();

    assert_eq!(error, "be.error.transfers.invalidDealTerms");
    assert!(prospect(&game).team_id.is_none());
}

#[test]
fn an_instalment_bid_pays_the_first_part_now_and_schedules_the_rest_a_year_apart() {
    // Given a bid for the whole fee in three instalments.
    let mut game = market_game("small-club", "1999-01-01");
    let (buyer_cash, seller_cash) = (
        team(&game, "team-1").finance,
        team(&game, "small-club").finance,
    );
    let (buyer_budget, seller_budget) = (
        team(&game, "team-1").transfer_budget,
        team(&game, "small-club").transfer_budget,
    );

    // When the clubs agree.
    assert_eq!(
        bid(&mut game, FEE, terms(3, 0, 0)),
        TransferNegotiationDecision::Accepted
    );

    // Then only the first instalment moved (the remainder goes in the first), the buyer's budget
    // committed the whole fee, and two payments are scheduled a year apart.
    let first = FEE - 2 * (FEE / 3);
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(team(&game, "team-1").finance, buyer_cash - first as i64);
    assert_eq!(
        team(&game, "small-club").finance,
        seller_cash + first as i64
    );
    assert_eq!(
        team(&game, "team-1").transfer_budget,
        buyer_budget - FEE as i64
    );
    assert_eq!(
        team(&game, "small-club").transfer_budget,
        seller_budget + first as i64
    );
    let dues: Vec<(&str, u64)> = obligations(&game)
        .into_iter()
        .filter_map(|o| match o {
            TransferObligation::Instalment { due, amount, .. } => Some((due.as_str(), *amount)),
            TransferObligation::SellOn { .. } => None,
        })
        .collect();
    assert_eq!(dues, vec![("2027-08-01", FEE / 3), ("2028-08-01", FEE / 3)]);
}

#[test]
fn instalments_fall_due_once_each_across_seasons_and_add_up_to_the_fee() {
    // Given a completed three-instalment deal.
    let mut game = market_game("small-club", "1999-01-01");
    bid(&mut game, FEE, terms(3, 0, 0));
    let buyer_cash_after_first = team(&game, "team-1").finance;

    // When a day before the second falls due, nothing is paid.
    set_date(&mut game, 2027, 7, 31);
    process_transfer_obligations(&mut game);
    assert_eq!(team(&game, "team-1").finance, buyer_cash_after_first);

    // On the due day it is paid, and a second run the same day pays nothing more.
    set_date(&mut game, 2027, 8, 1);
    process_transfer_obligations(&mut game);
    process_transfer_obligations(&mut game);
    assert_eq!(
        team(&game, "team-1").finance,
        buyer_cash_after_first - (FEE / 3) as i64
    );
    assert!(
        game.messages.iter().any(|m| {
            m.subject_key.as_deref() == Some("be.msg.transferInstalment.paid.subject")
        })
    );

    // A season later the last is paid; in all the seller received the fee exactly.
    set_date(&mut game, 2028, 8, 1);
    process_transfer_obligations(&mut game);
    process_transfer_obligations(&mut game);
    let received: i64 = posts_of(&game, CashKind::TransferFeeIn)
        .into_iter()
        .filter(|(club, _)| club == "small-club")
        .map(|(_, amount)| amount)
        .sum();
    assert_eq!(received, FEE as i64);
    assert!(obligations(&game).iter().all(|o| matches!(
        o,
        TransferObligation::Instalment {
            paid_on: Some(_),
            ..
        }
    )));
}

#[test]
fn a_settled_instalment_is_forgotten_after_its_retention() {
    let mut game = market_game("small-club", "1999-01-01");
    bid(&mut game, FEE, terms(2, 0, 0));
    set_date(&mut game, 2027, 8, 1);
    process_transfer_obligations(&mut game);
    assert_eq!(game.transfer_obligations.len(), 1);

    set_date(&mut game, 2027, 12, 1);
    process_transfer_obligations(&mut game);

    assert!(game.transfer_obligations.is_empty());
}

#[test]
fn a_bid_whose_terms_exceed_the_budget_is_refused_before_anything_moves() {
    // Given 3 000 000 of budget and a fee that fits, but a bonus that does not.
    let mut game = market_game("small-club", "1999-01-01");
    game.teams[0].transfer_budget = FEE as i64;
    let journal_before = game.cash_journal.len();

    let error =
        make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms(1, 0, 500_000)).unwrap_err();

    assert_eq!(error, "be.error.transfers.transferBudgetTooLow");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
    assert_eq!(game.cash_journal.len(), journal_before);
    assert!(game.transfer_obligations.is_empty());
}

#[test]
fn terms_outside_the_allowed_range_are_rejected_outright() {
    let mut game = market_game("small-club", "1999-01-01");

    for bad in [terms(0, 0, 0), terms(5, 0, 0), terms(1, 31, 0)] {
        let error = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, bad).unwrap_err();
        assert_eq!(error, "be.error.transfers.invalidDealTerms");
    }
    assert!(game.transfer_obligations.is_empty());
}

/// The lowest fee (in steps of 50 000) the seller accepts for these terms.
fn lowest_accepted_fee(terms: &TransferTerms) -> u64 {
    (1..=100u64)
        .map(|step| step * 50_000)
        .find(|fee| {
            let mut game = market_game("small-club", "1999-01-01");
            make_transfer_bid_with_terms(&mut game, PLAYER_ID, *fee, terms.clone())
                .is_ok_and(|outcome| outcome.decision == TransferNegotiationDecision::Accepted)
        })
        .expect("some fee is accepted")
}

#[test]
fn the_seller_charges_for_waiting_and_discounts_a_sell_on_clause() {
    let plain = lowest_accepted_fee(&TransferTerms::default());
    let instalments = lowest_accepted_fee(&terms(4, 0, 0));
    let clause = lowest_accepted_fee(&terms(1, 30, 0));

    assert!(instalments > plain, "{instalments} should exceed {plain}");
    assert!(clause < plain, "{clause} should undercut {plain}");
}

#[test]
fn a_sell_on_clause_pays_the_original_seller_from_the_next_sale_only() {
    // Given the user bought the player from the small club with a 20% sell-on clause.
    let mut game = market_game("small-club", "1999-01-01");
    bid(&mut game, FEE, terms(1, 20, 0));
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    let small_cash = team(&game, "small-club").finance;
    let small_budget = team(&game, "small-club").transfer_budget;

    // When the user sells him on to the big club for 4 000 000.
    set_date(&mut game, 2027, 2, 1);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    pending_offer(&mut game, "resale", "big-club", 4_000_000, "2027-02-01");
    respond_to_offer(&mut game, PLAYER_ID, "resale", true).unwrap();

    // Then the small club got 20% of that fee, in cash and in budget, and the clause is spent.
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert_eq!(team(&game, "small-club").finance, small_cash + 800_000);
    assert_eq!(
        team(&game, "small-club").transfer_budget,
        small_budget + 800_000
    );
    assert!(matches!(
        obligations(&game)[0],
        TransferObligation::SellOn {
            settled_on: Some(_),
            ..
        }
    ));

    // And selling him again pays the clause nothing further.
    let mut resold = game.clone();
    resold.players[0].team_id = Some("team-1".to_string());
    let cash_before = team(&resold, "small-club").finance;
    pending_offer(&mut resold, "again", "small-club", 1_000_000, "2027-02-02");
    respond_to_offer(&mut resold, PLAYER_ID, "again", true).unwrap();
    assert_eq!(team(&resold, "small-club").finance, cash_before - 1_000_000);
}

#[test]
fn a_clause_the_seller_cannot_fund_blocks_the_sale_without_moving_money() {
    // Given a clause for the small club, and a user club that cannot pay 20% out of a cheap sale.
    let mut game = market_game("small-club", "1999-01-01");
    bid(&mut game, FEE, terms(1, 30, 0));
    set_date(&mut game, 2027, 2, 1);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.teams[0].finance = 0;
    pending_offer(&mut game, "resale", "big-club", 4_000_000, "2027-02-01");
    // The sale itself brings 4 000 000 in; make the buyer unable to pay so only the clause matters.
    let journal_before = game.cash_journal.len();
    game.teams[0].finance = -10_000_000;

    let result = respond_to_offer(&mut game, PLAYER_ID, "resale", true);

    assert_eq!(result.unwrap_err(), "be.error.transfers.sellOnUnaffordable");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    // The offer was not marked agreed: the manager can still deal with it.
    assert_eq!(
        prospect(&game)
            .transfer_offers
            .iter()
            .find(|o| o.id == "resale")
            .unwrap()
            .status,
        TransferOfferStatus::Pending
    );
    assert_eq!(game.cash_journal.len(), journal_before);
}

#[test]
fn an_ambitious_player_refuses_to_step_down_unless_promised_a_way_out() {
    // Given a promising player at the big club and a smaller club bidding for him.
    let promising = |promise: bool, bonus: u64| {
        let mut game = market_game("big-club", "2005-01-01");
        game.players
            .iter_mut()
            .find(|p| p.id == PLAYER_ID)
            .map(|p: &mut Player| p.potential = 80);
        let outcome = make_transfer_bid_with_terms(
            &mut game,
            PLAYER_ID,
            8_000_000,
            TransferTerms {
                promise_stepping_stone: promise,
                signing_bonus: bonus,
                ..TransferTerms::default()
            },
        );
        (game, outcome)
    };

    // When no promise is made, the player turns the move down and nothing is spent.
    let (refused, outcome) = promising(false, 0);
    let outcome = outcome.unwrap();
    assert_eq!(outcome.decision, TransferNegotiationDecision::Rejected);
    assert!(!outcome.is_terminal);
    assert_eq!(
        outcome.feedback.headline_key,
        "transfers.transferFeedbackPlayerRefusesHeadline"
    );
    assert_eq!(prospect(&refused).team_id.as_deref(), Some("big-club"));

    // With the promise he accepts, and the promise is recorded against the buyer.
    let (signed, outcome) = promising(true, 0);
    assert_eq!(
        outcome.unwrap().decision,
        TransferNegotiationDecision::Accepted
    );
    assert_eq!(prospect(&signed).team_id.as_deref(), Some("team-1"));
    let promise = prospect(&signed)
        .morale_core
        .stepping_stone_promise
        .clone()
        .expect("promise recorded");
    assert_eq!(promise.club_id, "team-1");

    // A bonus worth half a year of his wage also wins him over, with no promise.
    let (bought, outcome) = promising(false, 10_000 * 26);
    assert_eq!(
        outcome.unwrap().decision,
        TransferNegotiationDecision::Accepted
    );
    assert!(
        prospect(&bought)
            .morale_core
            .stepping_stone_promise
            .is_none()
    );
}

#[test]
fn a_promise_to_a_player_who_does_not_want_one_is_refused_up_front() {
    // Given a veteran, who has no use for a way up.
    let mut game = market_game("small-club", "1990-01-01");

    let error = make_transfer_bid_with_terms(
        &mut game,
        PLAYER_ID,
        FEE,
        TransferTerms {
            promise_stepping_stone: true,
            ..TransferTerms::default()
        },
    )
    .unwrap_err();

    assert_eq!(error, "be.error.contracts.promiseNotWanted");
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
}

#[test]
fn terms_survive_a_closed_window_and_apply_when_the_deal_registers() {
    // Given a window that is closed until August 2027, and a bid with instalments and a bonus.
    let mut game = market_game("small-club", "1999-01-01");
    set_date(&mut game, 2027, 3, 1);
    let window = &mut game.season_context.transfer_window;
    window.status = TransferWindowStatus::Closed;
    window.opens_on = Some("2027-07-01".to_string());
    let outcome = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms(2, 10, 200_000))
        .expect("bid resolves");
    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);

    // Then he waits at the seller, with the terms kept on the agreed offer.
    assert_eq!(prospect(&game).team_id.as_deref(), Some("small-club"));
    let offer = prospect(&game)
        .transfer_offers
        .iter()
        .find(|o| o.status == TransferOfferStatus::PendingRegistration)
        .expect("agreed offer");
    assert_eq!(offer.terms, terms(2, 10, 200_000));
    assert!(game.transfer_obligations.is_empty());

    // When the window opens and the registration runs, the money and obligations follow.
    set_date(&mut game, 2027, 7, 1);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    process_pending_transfer_registrations(&mut game);

    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    assert_eq!(
        posts_of(&game, CashKind::SigningBonus),
        vec![("team-1".to_string(), -200_000)]
    );
    assert_eq!(
        game.transfer_obligations.len(),
        2,
        "one instalment, one clause"
    );
}
