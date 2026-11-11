#![cfg(test)]

//! The delegation scenarios across both modes: what the assistant does with offers, listings and
//! signings under Prudent (every major decision is a proposal) and under Total (he conducts the
//! whole strategy within the club's finances), and what he never does in either.
//!
//! A club of twenty-odd in a 4-4-2 with real positions and ratings, a buyer club with cash, an
//! assistant of a chosen quality. Each test names its situation (Given), the days played (When) and
//! what the manager sees (Then).

use chrono::Duration;
use domain::delegation::{DecisionOutcome, DelegationMode, Duty, ProposalAction};
use domain::player::{Player, Position, TransferOffer, TransferOfferStatus};
use domain::season::TransferWindowStatus;

use super::assistant::tests::game;
use super::delegation::{resolve_proposal, run_after_day};
use crate::game::Game;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::transfers::suggested_incoming_fee;

fn member(id: &str, team: &str, position: Position, ovr: u8, value: u64, age_born: &str) -> Player {
    let mut player = crate::world::test_fixtures::player_at(id, team, "2030-06-30");
    player.position = position.clone();
    player.natural_position = position;
    player.ovr = ovr;
    player.potential = ovr;
    player.market_value = value;
    player.date_of_birth = age_born.to_string();
    player.stage_wage(8_000);
    player.condition = 90;
    player
}

/// The club's squad by position, best first within each: `(id prefix, position, ratings)`.
type Spec<'a> = &'a [(&'a str, Position, &'a [u8])];

fn add_squad(g: &mut Game, team: &str, spec: Spec<'_>) {
    for (prefix, position, ratings) in spec {
        for (i, ovr) in ratings.iter().enumerate() {
            g.players.push(member(
                &format!("{prefix}{i}"),
                team,
                position.clone(),
                *ovr,
                u64::from(*ovr) * 100_000,
                "1998-01-01",
            ));
        }
    }
}

/// "team1" (the manager's): two elite keepers and a cheap third (the squad floor keeps two, so a
/// club can only sell one of the elite pair with a third behind them), a sound back four and
/// midfield, and one striker of 61 with nobody behind him. "team2" is a rich buyer with a full
/// squad of its own.
fn world(assistant_rating: u8) -> Game {
    let mut g = game(Some(assistant_rating), 20);
    g.players.clear();
    add_squad(
        &mut g,
        "team1",
        &[
            ("gk", Position::Goalkeeper, &[85, 83, 55]),
            ("lb", Position::LeftBack, &[72, 66]),
            ("rb", Position::RightBack, &[72, 66]),
            ("cb", Position::CenterBack, &[74, 72, 68, 66]),
            ("lm", Position::LeftMidfielder, &[72, 66]),
            ("rm", Position::RightMidfielder, &[72, 66]),
            ("cm", Position::CentralMidfielder, &[74, 72, 68, 66]),
            ("st", Position::Striker, &[61]),
        ],
    );
    add_squad(
        &mut g,
        "team2",
        &[
            ("bgk", Position::Goalkeeper, &[60, 58, 56]),
            ("bdf", Position::Defender, &[60; 6]),
            ("bmf", Position::Midfielder, &[60; 6]),
            ("bfw", Position::Forward, &[60; 4]),
        ],
    );
    g.season_context.transfer_window.status = TransferWindowStatus::Open;
    g.season_context.transfer_window.days_remaining = Some(30);
    for team in &mut g.teams {
        team.finance = 200_000_000;
        team.transfer_budget = 100_000_000;
        team.wage_budget = 100_000_000;
        team.formation = "4-4-2".to_string();
    }
    g.teams[0].starting_xi_ids.clear();
    g
}

fn offer(id: &str, fee: u64) -> TransferOffer {
    TransferOffer {
        id: id.to_string(),
        from_team_id: "team2".to_string(),
        fee,
        wage_offered: 5_000,
        last_manager_fee: None,
        negotiation_round: 1,
        suggested_counter_fee: None,
        status: TransferOfferStatus::Pending,
        date: "2026-08-01".to_string(),
        registration_date: None,
        registration_failure_reason: None,
        closed_on: None,
        terms: Default::default(),
    }
}

fn fair(g: &Game, player_id: &str) -> u64 {
    let player = g.players.iter().find(|p| p.id == player_id).unwrap();
    suggested_incoming_fee(g.clock.current_date.date_naive(), player)
}

fn offer_for(g: &mut Game, player_id: &str, id: &str, fee: u64) {
    g.players
        .iter_mut()
        .find(|p| p.id == player_id)
        .unwrap()
        .transfer_offers
        .push(offer(id, fee));
}

fn status(g: &Game, player_id: &str, offer_id: &str) -> TransferOfferStatus {
    g.players
        .iter()
        .find(|p| p.id == player_id)
        .unwrap()
        .transfer_offers
        .iter()
        .find(|o| o.id == offer_id)
        .unwrap()
        .status
        .clone()
}

fn is_sold_or_agreed(g: &Game, player_id: &str, offer_id: &str) -> bool {
    matches!(
        status(g, player_id, offer_id),
        TransferOfferStatus::Accepted | TransferOfferStatus::PendingRegistration
    )
}

fn days(g: &mut Game, count: i64) {
    for _ in 0..count {
        run_after_day(g);
        g.clock.current_date += Duration::days(1);
    }
}

fn logged(g: &Game, key: &str) -> usize {
    g.delegation
        .log
        .iter()
        .filter(|e| e.title_key == key)
        .count()
}

fn proposals_of(g: &Game, duty: Duty) -> Vec<&domain::delegation::Proposal> {
    g.delegation
        .proposals
        .iter()
        .filter(|p| p.duty == duty)
        .collect()
}

fn param<'a>(proposal: &'a domain::delegation::Proposal, key: &str) -> Option<&'a str> {
    proposal
        .params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Scenario 5. Given a club with two elite keepers and a weak attack, and an offer far above the
/// second keeper's worth
/// When the assistant has Total authority and a day is played
/// Then he accepts it: the money mends the attack by more than the spare keeper is worth to the
/// side, and the history says why.
#[test]
fn total_sells_the_second_goalkeeper_for_a_fortune_to_fund_the_attack() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Full);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);

    days(&mut g, 1);

    assert!(is_sold_or_agreed(&g, "gk1", "fortune"));
    let entry = g
        .delegation
        .log
        .iter()
        .find(|e| e.title_key == "delegation.log.offerAcceptedFunds")
        .expect("the sale is in the history");
    assert_eq!(entry.outcome, DecisionOutcome::Executed);
    assert!(entry.params.iter().any(|(k, v)| k == "why" && v == "funds"));
    assert!(entry.params.iter().any(|(k, _)| k == "loss"));
}

/// Scenario 6. Given a club whose second keeper is poor, so that its first is the only reliable
/// one, and an offer far above his worth
/// When the assistant has Total authority and a day is played
/// Then he turns it down, at any price: the side could not replace him.
#[test]
fn nobody_sells_the_only_reliable_goalkeeper_at_any_price() {
    let mut g = world(80);
    g.players.retain(|p| p.id != "gk1");
    g.players.push(member(
        "gk1",
        "team1",
        Position::Goalkeeper,
        45,
        100_000,
        "1998-01-01",
    ));
    g.delegation.apply_mode(DelegationMode::Full);
    let fee = fair(&g, "gk0") * 2;
    offer_for(&mut g, "gk0", "too-good", fee);

    days(&mut g, 1);

    assert_eq!(status(&g, "gk0", "too-good"), TransferOfferStatus::Rejected);
    assert_eq!(logged(&g, "delegation.log.offerKeepsStarter"), 1);
}

/// Scenario 8. Given two clubs with the same squad and the same formation, one managed by an AI and
/// one by the manager with a sharp assistant
/// When each reads its squad
/// Then they find the same first weakness: one definition of a need, two decision policies.
#[test]
fn the_assistant_and_a_rival_club_find_the_same_weakness() {
    let g = world(100);
    let mut mirror = g.clone();
    mirror
        .players
        .retain(|p| p.team_id.as_deref() != Some("team2"));
    for p in g
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some("team1"))
    {
        let mut copy = p.clone();
        copy.id = format!("rival-{}", p.id);
        copy.team_id = Some("team2".to_string());
        mirror.players.push(copy);
    }
    let rival = ClubSquad::of(&mirror, "team2", Appetite::NEUTRAL).unwrap();
    let assistant =
        ClubSquad::of(&mirror, "team1", Appetite::NEUTRAL.seen_by_assistant(100.0)).unwrap();
    let groups = |squad: &ClubSquad| {
        squad
            .needs()
            .into_iter()
            .fold(Vec::new(), |mut groups, need| {
                if !groups.contains(&need.group) {
                    groups.push(need.group);
                }
                groups
            })
    };
    let (a, b) = (groups(&assistant), groups(&rival));
    assert_eq!(a.first(), Some(&3), "the attack");
    assert_eq!(a, b);
    // An assistant of 20 looks less far ahead, but finds the same place to look first.
    let poor = ClubSquad::of(&mirror, "team1", Appetite::NEUTRAL.seen_by_assistant(20.0)).unwrap();
    assert_eq!(groups(&poor).first(), Some(&3));
}

/// Scenario 9. Given Prudent delegation and an offer the club should take
/// When days are played
/// Then nothing is sold: the assistant brings the sale to the manager, with the fee, what it
/// costs the side and why; accepting it makes the sale, and it is recorded as approved.
#[test]
fn prudent_brings_the_sale_to_the_manager_with_the_figures() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);

    days(&mut g, 3);

    assert_eq!(status(&g, "gk1", "fortune"), TransferOfferStatus::Pending);
    let proposals = proposals_of(&g, Duty::Offers);
    let sale = proposals
        .iter()
        .find(|p| matches!(p.action, ProposalAction::AcceptOffer { .. }))
        .expect("the sale is proposed");
    assert_eq!(param(sale, "fee").unwrap(), fee.to_string());
    assert_eq!(param(sale, "why"), Some("funds"));
    assert!(param(sale, "loss").is_some() && param(sale, "club").is_some());
    assert_eq!(sale.reason_key, "delegation.reason.saleFunds");
    let id = sale.id.clone();

    resolve_proposal(&mut g, &id, true).unwrap();

    assert!(is_sold_or_agreed(&g, "gk1", "fortune"));
    assert!(
        g.delegation
            .log
            .iter()
            .any(|e| e.outcome == DecisionOutcome::Approved)
    );
}

/// Scenario 9b. Given the same proposal
/// When the manager declines it and days pass with the offer still open
/// Then the assistant does not bring the same sale again.
#[test]
fn a_declined_sale_is_not_proposed_again() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);
    days(&mut g, 2);
    let id = proposals_of(&g, Duty::Offers)
        .iter()
        .find(|p| matches!(p.action, ProposalAction::AcceptOffer { .. }))
        .unwrap()
        .id
        .clone();

    resolve_proposal(&mut g, &id, false).unwrap();
    days(&mut g, 10);

    assert!(
        proposals_of(&g, Duty::Offers)
            .iter()
            .all(|p| !matches!(p.action, ProposalAction::AcceptOffer { .. }))
    );
    assert_eq!(status(&g, "gk1", "fortune"), TransferOfferStatus::Pending);
}

/// Scenario 10. Given a spare player and an offer at his worth
/// When the assistant has Total authority
/// Then he accepts it himself, once, however many days pass.
#[test]
fn total_accepts_a_valid_offer_for_a_spare_player_once() {
    let mut g = world(80);
    // A fifth keeper nobody needs.
    g.players.push(member(
        "gk9",
        "team1",
        Position::Goalkeeper,
        60,
        1_000_000,
        "1998-01-01",
    ));
    g.players.push(member(
        "gk8",
        "team1",
        Position::Goalkeeper,
        61,
        1_000_000,
        "1998-01-01",
    ));
    g.delegation.apply_mode(DelegationMode::Full);
    let fee = fair(&g, "gk9");
    offer_for(&mut g, "gk9", "fair", fee);

    days(&mut g, 6);

    assert!(is_sold_or_agreed(&g, "gk9", "fair"));
    assert_eq!(logged(&g, "delegation.log.offerAcceptedSurplus"), 1);
}

/// Scenario 11. Given an offer a little short of a spare player's worth
/// When the assistant has Total authority
/// Then he asks for the club's price, within the counters he allows himself, and the history
/// shows what he asked and what the buyer said.
#[test]
fn total_counters_an_offer_a_little_short_of_the_price() {
    let mut g = world(80);
    g.players.push(member(
        "gk9",
        "team1",
        Position::Goalkeeper,
        60,
        4_000_000,
        "1998-01-01",
    ));
    g.players.push(member(
        "gk8",
        "team1",
        Position::Goalkeeper,
        61,
        4_000_000,
        "1998-01-01",
    ));
    g.delegation.apply_mode(DelegationMode::Full);
    let price = fair(&g, "gk9");
    offer_for(&mut g, "gk9", "short", price * 88 / 100);

    days(&mut g, 1);

    let countered = |key: &str| logged(&g, key);
    assert_eq!(
        countered("delegation.log.offerCounteredAccepted")
            + countered("delegation.log.offerCounteredAgain")
            + countered("delegation.log.offerCounteredRefused"),
        1
    );
    let entry = g
        .delegation
        .log
        .iter()
        .find(|e| e.title_key.starts_with("delegation.log.offerCountered"))
        .unwrap();
    assert!(
        entry
            .params
            .iter()
            .any(|(k, v)| k == "asked" && *v == price.to_string())
    );
    assert!(entry.params.iter().any(|(k, _)| k == "answer"));
    let counters = g.delegation.sales.record_for("short").unwrap().counters;
    assert_eq!(counters, 1);
}

/// Scenario 12. Given a squad with more midfielders than it plans for
/// When the assistant has Total authority and a week passes
/// Then he lists the spares himself (at most two), and an offer for a listed spare is accepted.
#[test]
fn total_lists_surplus_and_sells_a_listed_spare() {
    let mut g = world(80);
    add_squad(
        &mut g,
        "team1",
        &[("xm", Position::CentralMidfielder, &[58, 57, 56, 55])],
    );
    g.delegation.apply_mode(DelegationMode::Full);

    days(&mut g, 2);

    let listed: Vec<&Player> = g
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some("team1") && p.transfer_listed)
        .collect();
    assert!(
        !listed.is_empty() && listed.len() <= 2,
        "{} listed",
        listed.len()
    );
    assert_eq!(g.delegation.sales.listed.len(), listed.len());
    let spare = listed[0].id.clone();
    let fee = fair(&g, &spare);
    offer_for(&mut g, &spare, "for-the-listed", fee);

    days(&mut g, 2);

    assert!(is_sold_or_agreed(&g, &spare, "for-the-listed"));
}

/// Scenario 12b. Given the same squad under Prudent
/// When a week passes
/// Then nobody is listed: the listing is proposed, with the reason.
#[test]
fn prudent_proposes_listing_and_lists_nobody() {
    let mut g = world(80);
    add_squad(
        &mut g,
        "team1",
        &[("xm", Position::CentralMidfielder, &[58, 57, 56, 55])],
    );
    g.delegation.apply_mode(DelegationMode::Approval);

    days(&mut g, 9);

    assert!(
        g.players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
            .all(|p| !p.transfer_listed)
    );
    let listings: Vec<_> = proposals_of(&g, Duty::Offers)
        .into_iter()
        .filter(|p| matches!(p.action, ProposalAction::ListForSale { .. }))
        .collect();
    assert!(!listings.is_empty() && listings.len() <= 4);
    assert!(listings.iter().all(|p| param(p, "why").is_some()));
}

/// Scenario 13. Given a protected player
/// When offers arrive for him and the assistant has the squad to review, in either mode
/// Then every offer is turned down, he is never listed and his sale is never proposed.
#[test]
fn a_protected_player_is_never_sold_listed_or_proposed() {
    for mode in [DelegationMode::Approval, DelegationMode::Full] {
        let mut g = world(80);
        add_squad(
            &mut g,
            "team1",
            &[("xm", Position::CentralMidfielder, &[58, 57, 56, 55])],
        );
        for id in ["xm0", "xm1", "xm2", "xm3"] {
            g.delegation.set_protected(id, true);
        }
        g.delegation.apply_mode(mode);
        let fee = fair(&g, "xm0") * 2;
        offer_for(&mut g, "xm0", "tempting", fee);

        days(&mut g, 10);

        assert_eq!(
            status(&g, "xm0", "tempting"),
            TransferOfferStatus::Rejected,
            "{mode:?}"
        );
        assert_eq!(logged(&g, "delegation.log.offerDeclinedProtected"), 1);
        assert!(
            g.players
                .iter()
                .filter(|p| p.id.starts_with("xm"))
                .all(|p| !p.transfer_listed)
        );
        assert!(
            proposals_of(&g, Duty::Offers)
                .iter()
                .all(|p| { param(p, "player_id").is_none_or(|id| !id.starts_with("xm")) }),
            "{mode:?}"
        );
    }
}

/// A protection given after a sale was proposed stops the sale from being made on approval.
#[test]
fn protecting_a_player_after_a_proposal_stops_the_approved_sale() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);
    days(&mut g, 2);
    let id = proposals_of(&g, Duty::Offers)
        .iter()
        .find(|p| matches!(p.action, ProposalAction::AcceptOffer { .. }))
        .unwrap()
        .id
        .clone();
    g.delegation.set_protected("gk1", true);

    let result = resolve_proposal(&mut g, &id, true);

    assert_eq!(
        result,
        Err("be.error.delegation.playerProtected".to_string())
    );
    assert_eq!(status(&g, "gk1", "fortune"), TransferOfferStatus::Pending);
}

/// Scenario 14. Given Prudent delegation with offers and needs everywhere and weeks of play
/// When nothing is approved
/// Then no player moves and no money moves: only proposals exist.
#[test]
fn prudent_never_commits_anything_by_itself() {
    let mut g = world(80);
    add_squad(
        &mut g,
        "team1",
        &[("xm", Position::CentralMidfielder, &[58, 57, 56, 55])],
    );
    g.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);
    let cash = g.teams[0].finance;
    let before: Vec<(String, Option<String>, bool)> = g
        .players
        .iter()
        .map(|p| (p.id.clone(), p.team_id.clone(), p.transfer_listed))
        .collect();

    days(&mut g, 30);

    let after: Vec<(String, Option<String>, bool)> = g
        .players
        .iter()
        .map(|p| (p.id.clone(), p.team_id.clone(), p.transfer_listed))
        .collect();
    assert_eq!(before, after);
    assert_eq!(g.teams[0].finance, cash);
    assert!(
        g.delegation
            .log
            .iter()
            .all(|e| e.outcome != DecisionOutcome::Approved)
    );
}

/// Scenario 15. Given the same world and the same days under Total, twice
/// When the two runs are compared
/// Then every decision is the same: the history, the proposals, who is where.
#[test]
fn the_assistants_decisions_are_deterministic() {
    let run = || {
        let mut g = world(65);
        add_squad(
            &mut g,
            "team1",
            &[("xm", Position::CentralMidfielder, &[58, 57, 56, 55])],
        );
        g.delegation.apply_mode(DelegationMode::Full);
        let fee = fair(&g, "gk1") * 13 / 10;
        offer_for(&mut g, "gk1", "fortune", fee);
        days(&mut g, 20);
        let players: Vec<(String, Option<String>, bool)> = g
            .players
            .iter()
            .map(|p| (p.id.clone(), p.team_id.clone(), p.transfer_listed))
            .collect();
        (
            serde_json::to_string(&g.delegation).unwrap(),
            players,
            g.teams[0].finance,
        )
    };
    assert_eq!(run(), run());
}

/// Scenario 16. Given a day's play, a save written in the middle of the assistant's negotiations,
/// and the same game played on without a reload
/// When the saved delegation is read back and the game played on from it
/// Then the two games end up the same: a reload carries the negotiation on.
#[test]
fn a_reload_in_the_middle_of_delegated_negotiations_changes_nothing() {
    let mut g = world(80);
    g.players.push(member(
        "gk9",
        "team1",
        Position::Goalkeeper,
        60,
        4_000_000,
        "1998-01-01",
    ));
    g.players.push(member(
        "gk8",
        "team1",
        Position::Goalkeeper,
        61,
        4_000_000,
        "1998-01-01",
    ));
    g.delegation.apply_mode(DelegationMode::Full);
    let price = fair(&g, "gk9");
    offer_for(&mut g, "gk9", "short", price * 88 / 100);
    days(&mut g, 1);
    assert_eq!(g.delegation.sales.record_for("short").unwrap().counters, 1);

    let mut reloaded = g.clone();
    reloaded.delegation =
        serde_json::from_str(&serde_json::to_string(&g.delegation).unwrap()).unwrap();
    days(&mut g, 8);
    days(&mut reloaded, 8);

    assert_eq!(
        serde_json::to_string(&g.delegation).unwrap(),
        serde_json::to_string(&reloaded.delegation).unwrap()
    );
    assert_eq!(
        status(&g, "gk9", "short"),
        status(&reloaded, "gk9", "short")
    );
}

/// Scenario 17. Given an offer and a squad with spares, days played one after another with
/// nothing else changing
/// When the assistant has Prudent or Total authority
/// Then each offer is answered once and each sale proposed once: no duplicates, ever.
#[test]
fn nothing_is_done_twice_however_many_days_are_processed() {
    let mut prudent = world(80);
    prudent.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&prudent, "gk1") * 13 / 10;
    offer_for(&mut prudent, "gk1", "fortune", fee);
    days(&mut prudent, 12);
    let sales = proposals_of(&prudent, Duty::Offers)
        .iter()
        .filter(|p| matches!(p.action, ProposalAction::AcceptOffer { .. }))
        .count();
    assert_eq!(sales, 1);

    let mut total = world(80);
    total.delegation.apply_mode(DelegationMode::Full);
    let fee = fair(&total, "gk1") * 13 / 10;
    offer_for(&mut total, "gk1", "fortune", fee);
    offer_for(&mut total, "st0", "insult", 100_000);
    days(&mut total, 12);
    assert_eq!(logged(&total, "delegation.log.offerAcceptedFunds"), 1);
    assert_eq!(logged(&total, "delegation.log.offerDeclined"), 1);
}

/// Scenario 18. Given Total authority and a club whose wage budget is spent
/// When the assistant looks for a signing
/// Then he proposes it instead of making it, whatever the fee ceiling says: the wage room is the
/// F5.3 rule and Total does not lift it.
#[test]
fn total_does_not_lift_the_wage_room() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Full);
    let bill = crate::finances::calc_wages(&g, "team1");
    g.teams[0].wage_budget = bill;
    add_squad(&mut g, "team2", &[("fw", Position::Striker, &[80, 79])]);
    let cash = g.teams[0].finance;

    days(&mut g, 12);

    assert_eq!(g.teams[0].finance, cash, "nothing was paid");
    assert!(
        g.players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
            .all(|p| !p.id.starts_with("fw"))
    );
}

/// Given Total delegation in which the assistant sells a player and signs nobody
/// When a fortnight passes
/// Then the inbox holds one digest, in the body that names signings and sales, with the counts.
#[test]
fn the_digest_names_the_sales_and_signings_made() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Full);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);

    days(&mut g, 16);

    // One digest a fortnight, not one per decision: two in sixteen days at most, and exactly one of
    // them (the one after the sale) names it.
    let digests: Vec<_> = g
        .messages
        .iter()
        .filter(|m| m.id.starts_with("assistant_digest_"))
        .collect();
    assert!(
        (1..=2).contains(&digests.len()),
        "{} digests",
        digests.len()
    );
    let counted = |param: &str| -> usize {
        digests
            .iter()
            .filter_map(|m| m.i18n_params.get(param))
            .filter_map(|n| n.parse::<usize>().ok())
            .sum()
    };
    // The one sale is named once; the signings Total made after it (a striker the side needed) are
    // named as many times as they were made, and every digest that has any uses the body that says so.
    assert_eq!(counted("sales"), 1);
    assert_eq!(
        counted("signings"),
        logged(&g, "delegation.log.recruitScheduled")
    );
    for digest in &digests {
        let names_transfers =
            digest.i18n_params["sales"] != "0" || digest.i18n_params["signings"] != "0";
        assert_eq!(
            digest.body_key.as_deref() == Some("be.msg.assistantDigest.bodyTransfers"),
            names_transfers
        );
    }
}

/// Given a sale proposed under Prudent delegation and left unanswered
/// When the manager switches delegation off and days pass
/// Then nothing new is proposed or done, the proposal is still his to answer, and answering it
/// makes the sale: a decision pending when delegation is disabled is not lost and is not made for him.
#[test]
fn a_decision_pending_when_delegation_is_switched_off_stays_the_managers() {
    let mut g = world(80);
    g.delegation.apply_mode(DelegationMode::Approval);
    let fee = fair(&g, "gk1") * 13 / 10;
    offer_for(&mut g, "gk1", "fortune", fee);
    days(&mut g, 2);
    let waiting = proposals_of(&g, Duty::Offers).len();
    assert!(waiting >= 1);
    let id = proposals_of(&g, Duty::Offers)
        .iter()
        .find(|p| matches!(p.action, ProposalAction::AcceptOffer { .. }))
        .unwrap()
        .id
        .clone();

    g.delegation.apply_mode(DelegationMode::Manual);
    days(&mut g, 10);

    assert_eq!(status(&g, "gk1", "fortune"), TransferOfferStatus::Pending);
    assert!(
        g.delegation.proposals.iter().any(|p| p.id == id),
        "still waiting"
    );

    resolve_proposal(&mut g, &id, true).unwrap();
    assert!(is_sold_or_agreed(&g, "gk1", "fortune"));
}

/// Given a counter-offer the assistant has made and the buyer has not settled
/// When the assistant leaves and a new one is hired, with days in between
/// Then nothing happens while the post is empty, and the new assistant carries on from what was
/// recorded: the offer is never countered more often than the limit, however many assistants there are.
#[test]
fn replacing_the_assistant_in_the_middle_of_a_negotiation_carries_on_without_repeats() {
    let mut g = world(80);
    g.players.push(member(
        "gk9",
        "team1",
        Position::Goalkeeper,
        60,
        4_000_000,
        "1998-01-01",
    ));
    g.players.push(member(
        "gk8",
        "team1",
        Position::Goalkeeper,
        61,
        4_000_000,
        "1998-01-01",
    ));
    g.delegation.apply_mode(DelegationMode::Full);
    let price = fair(&g, "gk9");
    offer_for(&mut g, "gk9", "short", price * 88 / 100);
    days(&mut g, 1);
    let counters = |g: &Game| {
        g.delegation
            .sales
            .record_for("short")
            .map_or(0, |r| r.counters)
    };
    let after_first = counters(&g);
    assert_eq!(after_first, 1);

    let assistant = g.staff.iter().find(|s| s.id == "assist").unwrap().clone();
    g.staff.retain(|s| s.id != "assist");
    let log_before = g.delegation.log.len();
    days(&mut g, 5);
    assert_eq!(counters(&g), after_first, "nobody to counter");
    assert_eq!(g.delegation.log.len(), log_before);

    let mut successor = assistant;
    successor.id = "assist-2".to_string();
    g.staff.push(successor);
    days(&mut g, 12);

    assert!(
        counters(&g) <= 2,
        "never more than the limit: {}",
        counters(&g)
    );
}
