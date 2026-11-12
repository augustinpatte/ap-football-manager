//! The external registry (D7B2) against the transfer rules of P5, on one game.
//!
//! D7B2 promotes a source player into `Game.players` in one step; P5 reserves the money and the
//! wage room of deals that are agreed and wait to register. Each test below is a place where the
//! two meet, played through the public API of both, so a reservation P5 made cannot be spent a
//! second time by an external signing and an external signing cannot be spent a second time by a
//! bid.

use super::*;
use domain::player::Footedness;
use ofm_core::contracts::squad_capacity;
use ofm_core::external_registry::{
    ERR_NEGOTIATION_NOT_AGREED, ERR_OFFER_REJECTED, ERR_PLAYER_ALREADY_IN_WORLD, ExternalClub,
    ExternalPlayer, preview_external_recruitment, preview_external_recruitment_with_offer,
    recruit_external_player, recruit_external_player_at_offer,
};

const BOARD_WAGE_POLICY: &str = "be.error.contracts.boardWagePolicy";
const BUDGET_TOO_LOW: &str = "be.error.transfers.transferBudgetTooLow";
const NO_FUNDS: &str = "be.error.transfers.insufficientFunds";
const SQUAD_FULL: &str = "be.error.freeAgents.squadFull";

fn far_away_club() -> ExternalClub {
    ExternalClub {
        id: "fm-c-900".to_string(),
        name: "Far Away FC".to_string(),
        contract_end: Some("2029-06-30".to_string()),
        weekly_wage: 8_000,
        reputation: 600,
    }
}

/// A source player of 26, mid-table level. A club makes him a contracted player; none, a free agent.
fn external(id: &str, club: Option<ExternalClub>) -> ExternalPlayer {
    ExternalPlayer {
        id: id.to_string(),
        match_name: "E. Xternal".to_string(),
        full_name: format!("External {id}"),
        date_of_birth: "2000-01-01".to_string(),
        nationality: "England".to_string(),
        position: Position::Midfielder,
        natural_position: Position::Midfielder,
        alternate_positions: vec![],
        footedness: Footedness::Right,
        weak_foot: 3,
        attributes: default_attrs(),
        ovr: 60,
        potential: 66,
        market_value: 1_500_000,
        club,
    }
}

fn contracted(id: &str) -> ExternalPlayer {
    external(id, Some(far_away_club()))
}

/// The user's club with `cash` and `budget`, the window open, two internal targets at the seller.
fn open_game(cash: i64, budget: i64) -> Game {
    let mut game = make_game_with_player(make_player("target-a"), vec![], cash, budget);
    game.players.push(make_player("target-b"));
    give_every_club_squad_depth(&mut game);
    game
}

/// The window shut with a reopening date: a bid the clubs agree is scheduled, not paid.
fn closed_window_game(cash: i64, budget: i64) -> Game {
    let mut game = open_game(cash, budget);
    game.clock.current_date = Utc.with_ymd_and_hms(2026, 12, 20, 12, 0, 0).unwrap();
    game.season_context.transfer_window.status = TransferWindowStatus::Closed;
    game.season_context.transfer_window.opens_on = Some("2027-01-01".to_string());
    game
}

/// The whole game as the save would see it, to prove a refusal changed nothing at all.
fn snapshot(game: &Game) -> serde_json::Value {
    serde_json::to_value(game).expect("a game serialises")
}

/// The fee and wage an external player would cost this club, read before any deal exists.
fn price_of(external: &ExternalPlayer) -> (u64, u32) {
    let preview = preview_external_recruitment(&open_game(900_000_000, 900_000_000), external)
        .expect("a preview");
    (preview.fee, preview.weekly_wage)
}

fn open_the_window(game: &mut Game) {
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
}

/// Given an open window and a free agent from the registry
/// When the manager signs him, and then tries to sign him again
/// Then he joins once, on the club's contract and with no fee, and the second attempt is refused
///      with nothing changed
#[test]
fn a_free_agent_is_signed_once_and_cannot_be_signed_again() {
    let mut game = open_game(5_000_000, 5_000_000);
    let free = external("fm-p-9001", None);
    let cash = game.teams[0].finance;

    let outcome = recruit_external_player(&mut game, &free).expect("a free agent signs");

    let joined: Vec<&Player> = game
        .players
        .iter()
        .filter(|p| p.id == "fm-p-9001")
        .collect();
    assert_eq!(joined.len(), 1, "promoted exactly once");
    assert_eq!(joined[0].team_id.as_deref(), Some("team-1"));
    assert_eq!(joined[0].wage(), outcome.weekly_wage);
    assert_eq!(game.teams[0].finance, cash, "no fee for a free agent");
    let after = snapshot(&game);

    let again = recruit_external_player(&mut game, &free);

    assert_eq!(again.err().as_deref(), Some(ERR_PLAYER_ALREADY_IN_WORLD));
    assert_eq!(snapshot(&game), after, "a refusal mutates nothing");
}

/// Given an open window and a Newgen identity (`fm-n-*`) from the registry
/// When the manager signs him twice
/// Then the Newgen is promoted once under his own stable id
#[test]
fn a_newgen_identity_is_promoted_once_under_its_own_id() {
    let mut game = open_game(5_000_000, 5_000_000);
    let newgen = external("fm-n-2027-0042", None);

    recruit_external_player(&mut game, &newgen).expect("a Newgen signs");
    let again = recruit_external_player(&mut game, &newgen);

    assert_eq!(
        game.players
            .iter()
            .filter(|p| p.id == "fm-n-2027-0042")
            .count(),
        1
    );
    assert_eq!(again.err().as_deref(), Some(ERR_PLAYER_ALREADY_IN_WORLD));
}

/// Given an open window and a contracted player at a club outside the game
/// When the manager buys him
/// Then the fee leaves both the cash and the transfer budget once, and his ledger names the club
///      he came from and the fee paid
#[test]
fn a_contracted_player_is_bought_with_one_fee_and_a_named_origin() {
    let mut game = open_game(20_000_000, 20_000_000);
    let target = contracted("fm-p-9002");

    let outcome = recruit_external_player(&mut game, &target).expect("he is bought");

    assert!(outcome.fee > 0, "a contracted player costs something");
    assert_eq!(game.teams[0].finance, 20_000_000 - outcome.fee as i64);
    assert_eq!(
        game.teams[0].transfer_budget,
        20_000_000 - outcome.fee as i64
    );
    let bought = game.players.iter().find(|p| p.id == "fm-p-9002").unwrap();
    let movement = bought
        .movement_history
        .last()
        .expect("his move is recorded");
    assert_eq!(movement.kind, PlayerMovementKind::PermanentTransfer);
    assert_eq!(movement.from_team_id.as_deref(), Some("fm-c-900"));
    assert_eq!(movement.fee, Some(outcome.fee));
}

/// Given a price the selling club would take
/// When the manager offers 70 % of the club's floor
/// Then the club rejects it by name and nothing in the game changes
#[test]
fn a_rejected_offer_changes_nothing() {
    let mut game = open_game(20_000_000, 20_000_000);
    let target = contracted("fm-p-9003");
    let floor = preview_external_recruitment(&game, &target)
        .unwrap()
        .minimum_fee;
    let before = snapshot(&game);

    let lowball = floor * 70 / 100;
    let preview = preview_external_recruitment_with_offer(&game, &target, Some(lowball)).unwrap();
    let result = recruit_external_player_at_offer(&mut game, &target, Some(lowball));

    assert_eq!(preview.club_decision, "rejected");
    assert_eq!(result.err().as_deref(), Some(ERR_OFFER_REJECTED));
    assert_eq!(
        snapshot(&game),
        before,
        "cash, budget, roster and messages are untouched"
    );
}

/// Given an offer between 75 % and 100 % of the floor
/// When the club counters, and the manager can pay the counter / is one unit short of it
/// Then the counter is not a signature: he must offer it back; paying it succeeds, and being short
///      of it is refused with nothing changed
#[test]
fn a_counter_is_paid_at_the_floor_and_only_if_the_club_can_afford_it() {
    let target = contracted("fm-p-9004");
    let rich = open_game(20_000_000, 20_000_000);
    let floor = preview_external_recruitment(&rich, &target)
        .unwrap()
        .minimum_fee;
    let offer = floor * 80 / 100;

    let mut game = rich.clone();
    let countered = preview_external_recruitment_with_offer(&game, &target, Some(offer)).unwrap();
    assert_eq!(countered.club_decision, "countered");
    let players = game.players.len();
    let not_yet = recruit_external_player_at_offer(&mut game, &target, Some(offer)).unwrap_err();
    assert_eq!(not_yet, ERR_NEGOTIATION_NOT_AGREED);
    assert_eq!(game.players.len(), players, "a counter signs nobody");

    let outcome = recruit_external_player_at_offer(&mut game, &target, Some(floor)).unwrap();
    assert_eq!(outcome.fee, floor, "a counter is the floor, not the offer");
    assert_eq!(game.teams[0].finance, 20_000_000 - floor as i64);

    let mut short = open_game(20_000_000, floor as i64 - 1);
    let before = snapshot(&short);
    let refused = recruit_external_player_at_offer(&mut short, &target, Some(floor));
    assert_eq!(refused.err().as_deref(), Some(BUDGET_TOO_LOW));
    assert_eq!(snapshot(&short), before);
}

/// Given a deal the clubs agreed in the shut window that uses the whole transfer budget, and the
///       window then opening before the deal registers
/// When the manager buys an external player
/// Then the budget the agreed deal will take is not his to spend: he is refused for the budget,
///      nothing changes, and the same purchase still goes through with the room the deal leaves
#[test]
fn an_external_purchase_cannot_spend_the_budget_an_agreed_deal_is_waiting_to_pay() {
    let target = contracted("fm-p-9005");
    let (fee, _) = price_of(&target);

    let mut game = closed_window_game(900_000_000, 2_000_000 + fee as i64 - 1);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first deal is agreed");
    open_the_window(&mut game);
    let before = snapshot(&game);

    let preview = preview_external_recruitment(&game, &target).unwrap();
    let result = recruit_external_player(&mut game, &target);

    assert_eq!(
        preview.blocker.as_deref(),
        Some(BUDGET_TOO_LOW),
        "{preview:?}"
    );
    assert!(!preview.can_afford_fee);
    assert_eq!(result.err().as_deref(), Some(BUDGET_TOO_LOW));
    assert_eq!(snapshot(&game), before);

    let mut roomy = closed_window_game(900_000_000, 2_000_000 + fee as i64);
    make_transfer_bid(&mut roomy, "target-a", 2_000_000).expect("the first deal is agreed");
    open_the_window(&mut roomy);
    assert!(
        recruit_external_player(&mut roomy, &target).is_ok(),
        "the reservation holds back only what the deal will take"
    );
}

/// Given the same agreed deal and cash for one of the two purchases, not the budget
/// When the manager buys an external player
/// Then the cash an agreed deal will take is not his to spend either
#[test]
fn an_external_purchase_cannot_spend_the_cash_an_agreed_deal_is_waiting_to_pay() {
    let target = contracted("fm-p-9006");
    let (fee, _) = price_of(&target);

    let mut game = closed_window_game(2_000_000 + fee as i64 - 1, 900_000_000);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first deal is agreed");
    open_the_window(&mut game);
    let before = snapshot(&game);

    let result = recruit_external_player(&mut game, &target);

    assert_eq!(result.err().as_deref(), Some(NO_FUNDS));
    assert_eq!(snapshot(&game), before);
}

/// Given an agreed deal waiting to register and a wage budget with room for one more player only
/// When the manager signs an external free agent
/// Then the agreed player's wage is already in the bill the board judges him against, so he is
///      refused; with no agreed deal, the same signing is allowed
#[test]
fn an_external_signing_is_judged_against_the_wages_of_agreed_deals() {
    let free = external("fm-p-9007", None);
    let (_, wage_external) = price_of(&free);
    let mut game = closed_window_game(900_000_000, 900_000_000);
    let wage_agreed =
        ofm_core::transfers::project_transfer_bid_financial_impact(&game, "target-a", 1)
            .unwrap()
            .incoming_player_weekly_wage;
    let room = wage_external.max(wage_agreed as u32) as i64;
    // The soft cap is 110 % of the wage budget: room for the dearer wage, not for both.
    game.teams[0].wage_budget = room * 100 / 110 + 1;
    assert!(
        preview_external_recruitment(&game, &free)
            .unwrap()
            .wage_policy_allows,
        "control: alone, he fits the room"
    );
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first deal fits the room");
    let before = snapshot(&game);

    let preview = preview_external_recruitment(&game, &free).unwrap();
    let result = recruit_external_player(&mut game, &free);

    assert!(!preview.wage_policy_allows, "{preview:?}");
    assert!(
        result
            .as_deref_err()
            .is_some_and(|e| e.starts_with(BOARD_WAGE_POLICY)),
        "{result:?}"
    );
    assert_eq!(snapshot(&game), before);
}

/// Given an agreed deal that registers when the window opens
/// When the manager then signs an external free agent into the room left for exactly both wages
/// Then the registered wage is counted once, so he is allowed
#[test]
fn a_registered_deal_is_not_reserved_again_against_an_external_signing() {
    let free = external("fm-p-9008", None);
    let (_, wage_external) = price_of(&free);
    let mut game = closed_window_game(900_000_000, 900_000_000);
    let wage_agreed =
        ofm_core::transfers::project_transfer_bid_financial_impact(&game, "target-a", 1)
            .unwrap()
            .incoming_player_weekly_wage;
    let both = i64::from(wage_external) + wage_agreed;
    game.teams[0].wage_budget = both * 100 / 110 + 1;
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the first deal is agreed");
    game.clock.current_date = Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap();
    open_the_window(&mut game);
    process_pending_transfer_registrations(&mut game);
    assert_eq!(
        game.players
            .iter()
            .find(|p| p.id == "target-a")
            .and_then(|p| p.team_id.as_deref()),
        Some("team-1"),
        "the deal registered"
    );

    let preview = preview_external_recruitment(&game, &free).unwrap();

    assert!(preview.wage_policy_allows, "one wage, once: {preview:?}");
    assert!(recruit_external_player(&mut game, &free).is_ok());
}

/// Given an external purchase that took nearly the whole transfer budget
/// When the manager then bids for another player
/// Then the money is already gone: the bid is refused, not left to overdraw
#[test]
fn a_bid_cannot_spend_what_an_external_purchase_already_took() {
    let target = contracted("fm-p-9009");
    let (fee, _) = price_of(&target);
    let mut game = open_game(900_000_000, fee as i64 + 100_000);

    recruit_external_player(&mut game, &target).expect("he is bought");
    let bid = make_transfer_bid(&mut game, "target-a", 2_000_000);

    assert_eq!(bid.err().as_deref(), Some(BUDGET_TOO_LOW));
}

/// Given a club whose senior list is one place short of full, and one that is full
/// When the manager signs an external free agent
/// Then he joins the first and is refused by the second with the registration limit's own key,
///      nothing changed, exactly as any other free agent would be
#[test]
fn an_external_free_agent_respects_the_senior_registration_limit() {
    let free = external("fm-p-9010", None);
    let mut game = open_game(5_000_000, 5_000_000);
    let mut index = 0;
    while squad_capacity(&game, "team-1").free_places > 1 {
        let mut filler = make_player(&format!("filler-{index}"));
        filler.team_id = Some("team-1".to_string());
        filler.stage_wage(0);
        game.players.push(filler);
        index += 1;
    }
    let mut full = game.clone();
    let mut filler = make_player("last-filler");
    filler.team_id = Some("team-1".to_string());
    full.players.push(filler);
    assert!(squad_capacity(&full, "team-1").blocks_signing);
    let before = snapshot(&full);

    let refused = recruit_external_player(&mut full, &free);

    assert!(
        refused
            .as_deref_err()
            .is_some_and(|e| e.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
    assert_eq!(snapshot(&full), before);
    assert!(
        recruit_external_player(&mut game, &free).is_ok(),
        "one place is enough"
    );
}

/// Fills `team-1`'s senior list until exactly one place is free.
fn leave_one_senior_place(game: &mut Game) {
    let mut index = 0;
    while squad_capacity(game, "team-1").free_places > 1 {
        let mut filler = make_player(&format!("filler-{index}"));
        filler.team_id = Some("team-1".to_string());
        filler.stage_wage(0);
        game.players.push(filler);
        index += 1;
    }
}

/// Given a senior list with one free place, and an in-game purchase agreed in the shut window that
///       will take it when it registers
/// When the window opens and the manager buys a contracted player from the global market
/// Then the place is not his to take: he is refused with the registration limit's key and nothing
///      changes, and the agreed deal still registers into the place it held, filling the list to 30
#[test]
fn an_agreed_in_game_purchase_holds_its_place_against_a_global_market_purchase() {
    let target = contracted("fm-p-9011");
    let mut game = closed_window_game(900_000_000, 900_000_000);
    leave_one_senior_place(&mut game);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the in-game deal is agreed");
    game.clock.current_date = Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap();
    open_the_window(&mut game);
    let before = snapshot(&game);

    let preview = preview_external_recruitment(&game, &target).unwrap();
    let refused = recruit_external_player(&mut game, &target);

    assert!(
        preview
            .blocker
            .as_deref()
            .is_some_and(|e| e.starts_with(SQUAD_FULL)),
        "{preview:?}"
    );
    assert!(
        refused
            .as_deref_err()
            .is_some_and(|e| e.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
    assert_eq!(snapshot(&game), before, "a refusal mutates nothing");

    process_pending_transfer_registrations(&mut game);

    let registered = game.players.iter().find(|p| p.id == "target-a").unwrap();
    assert_eq!(
        registered.team_id.as_deref(),
        Some("team-1"),
        "the agreed deal registered"
    );
    assert_eq!(
        squad_capacity(&game, "team-1").registered_seniors,
        squad_capacity(&game, "team-1").senior_limit
    );
}

/// Given the same agreed in-game purchase holding the last senior place
/// When a global-market FREE AGENT is signed first (a free agent is judged against the live list,
///      as every free-agent signing is) and the deal's registration date then arrives
/// Then the list never exceeds the limit: the deal fails to register, is withdrawn with a reason,
///      and the fee it would have paid is not taken
///
/// Characterizes a known gap, not a desired rule: the free agent takes the place the deal was
/// promised. It fails cleanly; see docs/APFM_I2_INTEGRATION.md.
#[test]
fn a_free_agent_can_take_the_place_an_agreed_purchase_was_promised_and_the_deal_fails_cleanly() {
    let free = external("fm-p-9012", None);
    let mut game = closed_window_game(900_000_000, 900_000_000);
    leave_one_senior_place(&mut game);
    make_transfer_bid(&mut game, "target-a", 2_000_000).expect("the in-game deal is agreed");
    game.clock.current_date = Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap();
    open_the_window(&mut game);
    recruit_external_player(&mut game, &free).expect("a free agent meets the live list only");
    let cash = game.teams[0].finance;

    process_pending_transfer_registrations(&mut game);

    let capacity = squad_capacity(&game, "team-1");
    assert_eq!(
        capacity.registered_seniors, capacity.senior_limit,
        "never above the limit"
    );
    let target = game.players.iter().find(|p| p.id == "target-a").unwrap();
    assert_ne!(
        target.team_id.as_deref(),
        Some("team-1"),
        "the deal did not register"
    );
    assert!(
        target
            .transfer_offers
            .iter()
            .all(|o| o.status != TransferOfferStatus::PendingRegistration),
        "nothing is left pending"
    );
    assert_eq!(game.teams[0].finance, cash, "the fee was not taken");
}

/// `Result::as_deref_err` is not in std; the tests only need to look at the error text.
trait ErrText {
    fn as_deref_err(&self) -> Option<&str>;
}

impl<T> ErrText for Result<T, String> {
    fn as_deref_err(&self) -> Option<&str> {
        self.as_ref().err().map(String::as_str)
    }
}
