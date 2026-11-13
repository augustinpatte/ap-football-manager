//! P6: the club's promises are kept in every route that spends them.
//!
//! P5 reserved the fee and the wage of an agreed deal that waits to register against a second
//! *bid*; a free agent, a renewal and a delegated decision read only the books. And the senior
//! list's limit of thirty held for a free agent but not for a purchase.

use super::*;
use ofm_core::contract_wage_policy::{WagePolicyVerdict, wage_policy_verdict};
use ofm_core::contracts::squad_capacity;

/// The closed-window game of the bid-money tests: two willing targets at the seller and the user
/// holding what they cost.
fn two_targets(finance: i64, transfer_budget: i64) -> Game {
    let mut game = make_game_with_player(make_player("target-a"), vec![], finance, transfer_budget);
    game.players.push(make_player("target-b"));
    give_every_club_squad_depth(&mut game);
    game.clock.current_date = Utc.with_ymd_and_hms(2026, 12, 20, 12, 0, 0).unwrap();
    game.season_context.transfer_window.status = TransferWindowStatus::Closed;
    game.season_context.transfer_window.opens_on = Some("2027-01-01".to_string());
    game
}

fn a_free_forward() -> Player {
    let mut player = make_player("free-agent");
    player.team_id = None;
    player.stage_contract_end(None);
    player.stage_wage(0);
    player
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

/// Given an agreed deal waiting for the window to open, and a wage budget with room for one
///   more player's wage only
/// When a free agent is judged against the board's wage policy, before the deal and after it
/// Then he fits before it, and not after: the agreed player's wage is as good as paid.
#[test]
fn a_free_agent_is_judged_against_the_wages_of_agreed_deals() {
    let mut game = two_targets(90_000_000, 90_000_000);
    let wage = ofm_core::transfers::project_transfer_bid_financial_impact(&game, "target-a", 1)
        .expect("the projection is available")
        .incoming_player_weekly_wage;
    assert!(wage > 0, "the fixture's players have a wage");
    // The soft cap is 110 % of the wage budget: room for one and a half such wages.
    game.teams[0].wage_budget = (wage * 3 / 2) * 100 / 110 + 1;
    let wage = u32::try_from(wage).expect("a wage");
    let free_agent = a_free_forward();
    let judged = |game: &Game| wage_policy_verdict(game, &game.teams[0], &free_agent, wage);
    assert_eq!(
        judged(&game),
        WagePolicyVerdict::WithinPolicy,
        "room for one"
    );

    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the deal is agreed");

    assert_eq!(
        judged(&game),
        WagePolicyVerdict::OverPolicy,
        "the room is promised to the agreed deal"
    );
}

/// Given a club whose senior list holds thirty
/// When it bids for a player of another club
/// Then the bid is refused with the refusal a free agent gets, and nothing is agreed; with one
///      place free it is agreed.
#[test]
fn a_purchase_into_a_full_senior_list_is_refused_as_a_free_agent_is() {
    let mut game = two_targets(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 30);

    let refused = make_transfer_bid(&mut game, "target-a", 2_000_000);

    assert_eq!(
        refused.err().as_deref(),
        Some("be.error.freeAgents.squadFull?count=30&max=30&release=1")
    );
    assert!(
        game.players
            .iter()
            .find(|player| player.id == "target-a")
            .is_some_and(|player| player.transfer_offers.is_empty()),
        "nothing was agreed"
    );

    let mut roomy = two_targets(90_000_000, 90_000_000);
    fill_senior_list(&mut roomy, "team-1", 29);
    assert!(make_transfer_bid(&mut roomy, "target-a", 2_000_000).is_ok());
}

/// Given a club with one place left on its senior list and one agreed purchase waiting to register
/// When it bids for a second player
/// Then the place is already taken: the second bid is refused, not left to fail at registration.
#[test]
fn an_agreed_purchase_holds_its_place_against_a_second_bid() {
    let mut game = two_targets(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first bid takes the place");

    let second = make_transfer_bid(&mut game, "target-b", 2_000_000);

    assert!(
        second
            .as_ref()
            .err()
            .is_some_and(|error| error.starts_with("be.error.freeAgents.squadFull")),
        "{second:?}"
    );
}

/// Given a purchase agreed while the club had a place, and a free agent signed into that place
///   before the window opens
/// When the window opens and the purchase comes to register
/// Then it fails to register: the player stays where he was and the list is not over thirty.
#[test]
fn an_agreed_purchase_does_not_register_into_a_list_that_filled_in_the_meantime() {
    let mut game = two_targets(90_000_000, 90_000_000);
    fill_senior_list(&mut game, "team-1", 29);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the bid is agreed");
    fill_senior_list(&mut game, "team-1", 30);

    game.clock.current_date = Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap();
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    process_pending_transfer_registrations(&mut game);

    assert_eq!(
        game.players
            .iter()
            .find(|player| player.id == "target-a")
            .and_then(|player| player.team_id.as_deref()),
        Some("team-2"),
        "he stayed at his club"
    );
    assert_eq!(squad_capacity(&game, "team-1").registered_seniors, 30);
}
