//! The stepping-stone promise: made when a club signs a free agent, judged on measurable terms
//! when an offer for him arrives, resolved by sale, refusal or the passing of time.

mod market_fixtures;

use domain::player::{
    Player, RenewalSessionStatus, SteppingStonePromise, SteppingStonePromiseStatus,
};
use market_fixtures::{
    PLAYER_ID, USER_REPUTATION, game_with, pending_offer, player, prospect, set_date,
};
use ofm_core::contracts::{RenewalDecision, RenewalOffer, offer_free_agent_contract_with_promise};
use ofm_core::game::Game;
use ofm_core::transfers::{
    ERR_PROMISE_NOT_WANTED, process_stepping_stone_promises, respond_to_offer,
};

fn promise(game: &Game) -> &SteppingStonePromise {
    prospect(game)
        .morale_core
        .stepping_stone_promise
        .as_ref()
        .expect("promise recorded")
}

/// A signed prospect on the user's books, bound by an active promise from the given date.
fn game_with_signed_prospect() -> Game {
    let mut game = game_with(player("2005-01-01"));
    let outcome = offer_free_agent_contract_with_promise(
        &mut game,
        PLAYER_ID,
        RenewalOffer {
            weekly_wage: 20_000,
            contract_years: 4,
        },
        true,
    )
    .expect("signing resolves");
    assert_eq!(outcome.decision, RenewalDecision::Accepted);
    game
}

fn incoming_offer(game: &mut Game, from: &str, fee: u64) {
    pending_offer(game, "offer-1", from, fee, "2027-03-01");
}

#[test]
fn signing_with_a_promise_records_measurable_terms() {
    // Given a young player with room to grow, a club that promises a way out.
    let game = game_with_signed_prospect();

    // Then the signing is recorded, with terms checkable against the game state.
    assert_eq!(prospect(&game).team_id.as_deref(), Some("team-1"));
    let promise = promise(&game);
    assert_eq!(promise.club_id, "team-1");
    assert_eq!(promise.status, SteppingStonePromiseStatus::Active);
    assert_eq!(promise.min_buyer_reputation, USER_REPUTATION + 50);
    assert_eq!(promise.made_on, "2026-08-01");
    assert_eq!(promise.protected_from, "2027-01-28");
    assert_eq!(promise.expires_on, "2029-08-01");
}

#[test]
fn the_promise_buys_a_lower_wage_than_the_player_would_otherwise_accept() {
    let offer = |weekly_wage, promise| {
        let mut game = game_with(player("2005-01-01"));
        let outcome = offer_free_agent_contract_with_promise(
            &mut game,
            PLAYER_ID,
            RenewalOffer {
                weekly_wage,
                contract_years: 4,
            },
            promise,
        )
        .unwrap();
        (game, outcome)
    };
    // Given his full ask, read from a near-miss counter.
    let (_, near_miss) = offer(6_000, false);
    let ask = near_miss
        .suggested_wage
        .expect("a near miss is countered with his ask");
    let under = ask * 95 / 100;

    // When 5% under the ask is offered with and without the promise.
    let (without, plain) = offer(under, false);
    let (with, promised) = offer(under, true);

    // Then only the promise gets the deal done.
    assert_eq!(plain.decision, RenewalDecision::CounterOffer);
    assert!(prospect(&without).team_id.is_none());
    assert_eq!(promised.decision, RenewalDecision::Accepted);
    assert_eq!(prospect(&with).wage(), under);
}

#[test]
fn a_veteran_has_no_use_for_the_promise_and_is_not_signed_on_it() {
    // Given a thirty-six-year-old, who is not stepping anywhere.
    let mut game = game_with(player("1990-01-01"));

    // When the club tries to promise him a way out.
    let result = offer_free_agent_contract_with_promise(
        &mut game,
        PLAYER_ID,
        RenewalOffer {
            weekly_wage: 20_000,
            contract_years: 1,
        },
        true,
    );

    // Then the offer is refused outright: nothing is recorded, he is still a free agent.
    assert_eq!(result.unwrap_err(), ERR_PROMISE_NOT_WANTED);
    assert!(prospect(&game).team_id.is_none());
    assert!(prospect(&game).morale_core.stepping_stone_promise.is_none());
}

#[test]
fn refusing_a_suitable_offer_breaches_the_promise() {
    // Given a bound promise and a big club's fair offer.
    let mut game = game_with_signed_prospect();
    set_date(&mut game, 2027, 3, 1);
    incoming_offer(&mut game, "big-club", 900_000);
    let trust_before = prospect(&game).morale_core.manager_trust;
    let morale_before = prospect(&game).morale;

    // When the club turns it down.
    respond_to_offer(&mut game, PLAYER_ID, "offer-1", false).unwrap();

    // Then the promise is breached: trust and morale drop and he will not talk renewal for a while.
    assert_eq!(promise(&game).status, SteppingStonePromiseStatus::Breached);
    assert_eq!(promise(&game).resolved_on.as_deref(), Some("2027-03-01"));
    assert!(prospect(&game).morale_core.manager_trust < trust_before);
    assert!(prospect(&game).morale < morale_before);
    let state = prospect(&game).morale_core.renewal_state.as_ref().unwrap();
    assert_eq!(state.status, RenewalSessionStatus::Blocked);
    assert_eq!(state.manager_blocked_until.as_deref(), Some("2027-08-28"));
    assert!(
        game.messages
            .iter()
            .any(|m| { m.subject_key.as_deref() == Some("be.msg.steppingStone.breached.subject") })
    );
}

#[test]
fn refusals_the_promise_does_not_cover_are_not_a_breach() {
    // A club that is no step up, a lowball, and an offer inside the protected period.
    let cases = [
        ("small club", "small-club", 900_000, (2027, 3, 1)),
        ("lowball fee", "big-club", 500_000, (2027, 3, 1)),
        ("still protected", "big-club", 900_000, (2026, 10, 1)),
    ];
    for (label, bidder, fee, (y, m, d)) in cases {
        // Given a bound promise and an offer outside its terms.
        let mut game = game_with_signed_prospect();
        set_date(&mut game, y, m, d);
        incoming_offer(&mut game, bidder, fee);

        // When the club turns it down.
        respond_to_offer(&mut game, PLAYER_ID, "offer-1", false).unwrap();

        // Then the promise stands untouched.
        assert_eq!(
            promise(&game).status,
            SteppingStonePromiseStatus::Active,
            "{label}"
        );
        assert!(
            !game
                .messages
                .iter()
                .any(|m| m.subject_key.as_deref() == Some("be.msg.steppingStone.breached.subject")),
            "{label}"
        );
    }
}

#[test]
fn selling_to_a_suitable_club_fulfils_the_promise() {
    // Given a bound promise and a big club's fair offer.
    let mut game = game_with_signed_prospect();
    set_date(&mut game, 2027, 3, 1);
    incoming_offer(&mut game, "big-club", 900_000);

    // When the club lets him go.
    respond_to_offer(&mut game, PLAYER_ID, "offer-1", true).unwrap();

    // Then he is at the big club and the promise is fulfilled.
    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert_eq!(promise(&game).status, SteppingStonePromiseStatus::Fulfilled);
    assert!(
        game.messages.iter().any(|m| {
            m.subject_key.as_deref() == Some("be.msg.steppingStone.fulfilled.subject")
        })
    );
}

#[test]
fn a_promise_that_runs_out_unbroken_lapses_once_to_the_clubs_credit() {
    // Given a promise past its term with the player still at the club.
    let mut game = game_with_signed_prospect();
    set_date(&mut game, 2029, 8, 2);
    let trust_before = prospect(&game).morale_core.manager_trust;

    // When the day is processed twice.
    process_stepping_stone_promises(&mut game);
    let messages_after_first = game.messages.len();
    process_stepping_stone_promises(&mut game);

    // Then it lapsed once, trust rose once, and the second pass did nothing.
    assert_eq!(promise(&game).status, SteppingStonePromiseStatus::Lapsed);
    assert_eq!(prospect(&game).morale_core.manager_trust, trust_before + 5);
    assert_eq!(game.messages.len(), messages_after_first);
    assert!(
        game.messages
            .iter()
            .any(|m| { m.subject_key.as_deref() == Some("be.msg.steppingStone.lapsed.subject") })
    );
}

#[test]
fn a_promise_still_in_term_is_left_alone_by_daily_upkeep() {
    let mut game = game_with_signed_prospect();
    set_date(&mut game, 2028, 1, 1);

    process_stepping_stone_promises(&mut game);

    assert_eq!(promise(&game).status, SteppingStonePromiseStatus::Active);
}

#[test]
fn a_promise_round_trips_through_serde_and_old_saves_read_as_none() {
    // Given a signed prospect.
    let game = game_with_signed_prospect();
    let json = serde_json::to_string(prospect(&game)).unwrap();

    // When reloaded, then the promise is intact.
    let reloaded: Player = serde_json::from_str(&json).unwrap();
    assert_eq!(
        reloaded.morale_core.stepping_stone_promise,
        prospect(&game).morale_core.stepping_stone_promise
    );

    // And a save written before the field existed reads it as absent.
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["morale_core"]
        .as_object_mut()
        .unwrap()
        .remove("stepping_stone_promise");
    let old: Player = serde_json::from_value(value).unwrap();
    assert!(old.morale_core.stepping_stone_promise.is_none());
}
