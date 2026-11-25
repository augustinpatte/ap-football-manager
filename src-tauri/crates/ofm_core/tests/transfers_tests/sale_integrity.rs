//! ALPHA6: selling a player is decided by three separate rules, and each says so on its own.
//!
//! The buyer's willingness (price and means), the seller's squad floor (position cover and the
//! fifteen-senior minimum) and the buyer's senior list (the 30-place limit) are different reasons a
//! sale does not happen. A sale that is blocked leaves the game exactly as it was, and it never
//! costs the manager a negotiation round he could not have used.

use super::*;
use domain::player::SquadRole;
use ofm_core::contracts::squad_capacity;

const FLOOR_GROUP: &str = "be.error.squadFloor.wouldLeaveShort";
const BUYER_FULL: &str = "be.error.transfers.buyerSquadFull";
const SQUAD_FULL: &str = "be.error.freeAgents.squadFull";

fn user_player_at(id: &str, position: Position, incoming_fee: u64) -> Player {
    let mut player = make_user_player(id);
    player.position = position;
    player.market_value = 1_000_000;
    player
        .transfer_offers
        .push(make_pending_incoming_offer("offer-1", incoming_fee));
    player
}

/// A game in which the AI club "team-2" can pay for the player and the user has depth.
fn sale_game(player: Player) -> Game {
    let mut game = make_game_with_player(player, vec![], 5_000_000, 2_000_000);
    game.teams[1].finance = 6_000_000;
    game.teams[1].transfer_budget = 3_000_000;
    game
}

/// Leaves `team` with exactly `keep` registered senior goalkeepers, the named player among them.
fn leave_goalkeepers(game: &mut Game, team: &str, keep: usize, except: &str) {
    let mut seen = 0;
    for player in game.players.iter_mut() {
        if player.team_id.as_deref() == Some(team) && player.position == Position::Goalkeeper {
            seen += 1;
            if player.id != except && seen > keep {
                player.team_id = None;
            }
        }
    }
    // The named player may have been counted after the cut; drop surplus depth until `keep` remain.
    let mut remaining = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team) && p.position == Position::Goalkeeper)
        .count();
    for player in game.players.iter_mut() {
        if remaining <= keep {
            break;
        }
        if player.team_id.as_deref() == Some(team)
            && player.position == Position::Goalkeeper
            && player.id != except
        {
            player.team_id = None;
            remaining -= 1;
        }
    }
}

fn seniors(game: &Game, team: &str) -> usize {
    squad_capacity(game, team).registered_seniors
}

fn fill_to(game: &mut Game, team: &str, target: usize) {
    let mut index = 0;
    while seniors(game, team) < target {
        let mut filler = make_player(&format!("fill-{team}-{index}"));
        filler.team_id = Some(team.to_string());
        filler.position = Position::Midfielder;
        filler.stage_wage(0);
        filler.market_value = 0;
        game.players.push(filler);
        index += 1;
    }
}

fn status_of(game: &Game, player: &str) -> TransferOfferStatus {
    game.players
        .iter()
        .find(|p| p.id == player)
        .and_then(|p| p.transfer_offers.first())
        .map(|offer| offer.status.clone())
        .expect("an offer")
}

fn club_of(game: &Game, player: &str) -> Option<String> {
    game.players
        .iter()
        .find(|p| p.id == player)
        .and_then(|p| p.team_id.clone())
}

/// Given a goalkeeper with adequate cover behind him (three senior keepers)
/// When the manager counters within the buyer's means
/// Then the sale is agreed and the keeper leaves.
#[test]
fn a_counter_for_a_keeper_with_cover_completes_the_sale() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));

    let outcome = counter_offer(&mut game, "gk", "offer-1", 1_050_000).expect("evaluated");

    assert_eq!(outcome.decision, TransferNegotiationDecision::Accepted);
    assert_eq!(club_of(&game, "gk").as_deref(), Some("team-2"));
}

/// Given a club with only two senior goalkeepers and an offer for one of them
/// When the manager counters at a price the buyer would pay
/// Then the sale is refused for the squad floor, by name, and nothing at all has changed.
#[test]
fn a_counter_that_would_leave_two_keepers_short_is_refused_and_changes_nothing() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));
    leave_goalkeepers(&mut game, "team-1", 2, "gk");
    let before = serde_json::to_string(&game).expect("serialises");

    let refused = counter_offer(&mut game, "gk", "offer-1", 1_050_000);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| e.starts_with(FLOOR_GROUP)),
        "{refused:?}"
    );
    assert_eq!(serde_json::to_string(&game).expect("serialises"), before);
}

/// Given the same two-keeper club
/// When the manager counters at a price the buyer would refuse
/// Then the squad floor is still what he is told, and the buyer's offer stays open: a counter for
///      a sale he could never make must not spend the negotiation.
#[test]
fn a_counter_for_a_player_who_cannot_leave_does_not_close_the_negotiation() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));
    leave_goalkeepers(&mut game, "team-1", 2, "gk");

    let refused = counter_offer(&mut game, "gk", "offer-1", 1_400_000);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| e.starts_with(FLOOR_GROUP)),
        "{refused:?}"
    );
    assert_eq!(status_of(&game, "gk"), TransferOfferStatus::Pending);
}

/// Given the two-keeper club
/// When the manager turns the offer down
/// Then he can: refusing a sale never depends on the floor.
#[test]
fn declining_an_offer_for_a_player_the_club_cannot_spare_still_works() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));
    leave_goalkeepers(&mut game, "team-1", 2, "gk");

    respond_to_offer(&mut game, "gk", "offer-1", false).expect("declining is always allowed");

    assert_eq!(status_of(&game, "gk"), TransferOfferStatus::Rejected);
    assert_eq!(club_of(&game, "gk").as_deref(), Some("team-1"));
}

/// Given the two-keeper club
/// When the manager accepts the offer as it stands
/// Then it is refused for the floor, by name, with the game untouched.
#[test]
fn accepting_an_offer_for_a_player_the_club_cannot_spare_is_refused_untouched() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));
    leave_goalkeepers(&mut game, "team-1", 2, "gk");
    let before = serde_json::to_string(&game).expect("serialises");

    let refused = respond_to_offer(&mut game, "gk", "offer-1", true);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| e.starts_with(FLOOR_GROUP)),
        "{refused:?}"
    );
    assert_eq!(serde_json::to_string(&game).expect("serialises"), before);
}

/// Given a counter just above what the buyer will go to, on a player who can leave
/// When the buyer names its own price and the manager accepts it
/// Then the second round is agreed: the repeated negotiation reaches a sale at the named price.
#[test]
fn accepting_the_buyers_named_price_after_a_counter_completes_the_sale() {
    let mut game = sale_game(user_player_at("fw", Position::Forward, 900_000));

    let first = counter_offer(&mut game, "fw", "offer-1", 1_250_000).expect("first round");
    assert_eq!(first.decision, TransferNegotiationDecision::CounterOffer);
    let named = first.suggested_fee.expect("the buyer names a price");
    assert_eq!(status_of(&game, "fw"), TransferOfferStatus::Pending);
    let finance_before = game.teams[0].finance;

    respond_to_offer(&mut game, "fw", "offer-1", true).expect("the named price is accepted");

    assert_eq!(club_of(&game, "fw").as_deref(), Some("team-2"));
    assert_eq!(game.teams[0].finance, finance_before + named as i64);
}

/// Given the buyer's named price
/// When the manager pushes again for more than the buyer is willing to pay
/// Then the buyer breaks off and the offer closes: repeated rounds end.
#[test]
fn pushing_past_the_buyers_named_price_ends_the_talks() {
    let mut game = sale_game(user_player_at("fw", Position::Forward, 900_000));
    let first = counter_offer(&mut game, "fw", "offer-1", 1_250_000).expect("first round");
    let named = first.suggested_fee.expect("the buyer names a price");

    let second = counter_offer(&mut game, "fw", "offer-1", named * 2).expect("second round");

    assert_eq!(second.decision, TransferNegotiationDecision::Rejected);
    assert_eq!(status_of(&game, "fw"), TransferOfferStatus::Rejected);
}

/// Given an offer the manager's counter has already closed
/// When he counters again
/// Then the talks are over: the offer is no longer pending and nothing moves.
#[test]
fn a_counter_after_the_negotiation_was_closed_is_refused() {
    let mut game = sale_game(user_player_at("fw", Position::Forward, 900_000));
    let closed = counter_offer(&mut game, "fw", "offer-1", 3_000_000).expect("rejected outright");
    assert_eq!(closed.decision, TransferNegotiationDecision::Rejected);

    let again = counter_offer(&mut game, "fw", "offer-1", 1_000_000);

    assert_eq!(
        again.err().as_deref(),
        Some("be.error.transfers.offerNotPending")
    );
}

/// Given a club of 31 seniors against a limit of 30
/// When it sells a replaceable midfielder
/// Then the sale completes and the list falls to 30: leaving never needs a free place.
#[test]
fn a_club_of_thirty_one_can_sell_a_midfielder_and_falls_to_thirty() {
    let mut game = sale_game(user_player_at("mid", Position::Midfielder, 900_000));
    fill_to(&mut game, "team-1", 31);
    assert_eq!(seniors(&game, "team-1"), 31);

    respond_to_offer(&mut game, "mid", "offer-1", true).expect("the sale goes through");

    assert_eq!(club_of(&game, "mid").as_deref(), Some("team-2"));
    assert_eq!(seniors(&game, "team-1"), 30);
    assert!(!squad_capacity(&game, "team-1").blocks_signing || seniors(&game, "team-1") == 30);
}

/// Given a club of 31 seniors
/// When it signs a player
/// Then the signing is refused for the club's own list, in the "your list" words with the real
///      numbers; the departure rule and the arrival rule are separate.
#[test]
fn a_club_of_thirty_one_cannot_sign_another_player() {
    let target = make_player("target");
    let mut game = make_game_with_player(target, vec![], 90_000_000, 90_000_000);
    fill_to(&mut game, "team-1", 31);

    let refused = make_transfer_bid(&mut game, "target", 2_000_000);

    assert!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| e.starts_with(SQUAD_FULL)),
        "{refused:?}"
    );
}

/// Given a buyer whose own senior list is full
/// When the manager accepts its offer
/// Then the refusal names the buyer's club and its numbers, not "your" list, and the manager's
///      squad is untouched.
#[test]
fn a_sale_to_a_buyer_with_a_full_list_names_the_buyers_list() {
    let mut game = sale_game(user_player_at("mid", Position::Midfielder, 900_000));
    fill_to(&mut game, "team-2", 30);
    let before = serde_json::to_string(&game).expect("serialises");

    let refused = respond_to_offer(&mut game, "mid", "offer-1", true);

    let error = refused.err().expect("refused");
    assert!(error.starts_with(BUYER_FULL), "{error}");
    assert!(error.contains("club=Seller%20FC"), "{error}");
    assert!(
        error.contains("count=30") && error.contains("max=30"),
        "{error}"
    );
    assert_eq!(serde_json::to_string(&game).expect("serialises"), before);
}

/// Given a player of the 31-senior club who is academy (youth), not senior
/// When the squad is counted
/// Then he holds no place: moving a player to the youth squad frees one.
#[test]
fn a_youth_player_holds_no_senior_place() {
    let mut game = sale_game(user_player_at("mid", Position::Midfielder, 900_000));
    fill_to(&mut game, "team-1", 31);
    let senior_before = seniors(&game, "team-1");
    game.players
        .iter_mut()
        .find(|p| p.id == "fill-team-1-0")
        .expect("a filler")
        .squad_role = SquadRole::Youth;

    assert_eq!(seniors(&game, "team-1"), senior_before - 1);
}

const SQUAD_FULL_HELD: &str = "be.error.freeAgents.squadFullHeld";

/// Sends `count` of `club`'s seniors out on loan to team-2, where the borrower registers them.
fn lend_out(game: &mut Game, club: &str, count: usize) {
    for index in 0..count {
        let mut lent = make_player(&format!("lent-{club}-{index}"));
        lent.team_id = Some("team-2".to_string());
        lent.stage_wage(0);
        lent.market_value = 0;
        lent.active_loan = Some(ActiveLoan {
            parent_team_id: club.to_string(),
            loan_team_id: "team-2".to_string(),
            loan_team_name: None,
            start_date: "2026-08-01".to_string(),
            end_date: "2027-06-30".to_string(),
            wage_contribution_pct: 100,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
            terms_source: domain::player::LoanTermsSource::Agreed,
        });
        game.players.push(lent);
    }
}

/// Given a club with 23 seniors registered and 7 more out on loan, who return to their places
/// When it bids for a player
/// Then it is refused, and the refusal says the places are held (7), not that the list is full:
///      the old "23 of 30 taken, free 0" was a sentence nobody could act on
#[test]
fn a_club_with_loanees_out_is_told_its_places_are_held() {
    let mut game = make_game_with_player(make_player("target"), vec![], 90_000_000, 90_000_000);
    fill_to(&mut game, "team-1", 23);
    lend_out(&mut game, "team-1", 7);

    let refused = make_transfer_bid(&mut game, "target", 2_000_000);

    let error = refused.err().expect("refused");
    assert!(error.starts_with(SQUAD_FULL_HELD), "{error}");
    assert!(
        error.contains("count=23") && error.contains("held=7") && error.contains("release=1"),
        "{error}"
    );
}

/// Given the same club with one place more free than its loanees will take back
/// When it bids for a player
/// Then it is agreed: the held places are exactly that, held, and one more is open
#[test]
fn a_club_with_one_place_beyond_its_loanees_can_sign() {
    let mut game = make_game_with_player(make_player("target"), vec![], 90_000_000, 90_000_000);
    fill_to(&mut game, "team-1", 22);
    lend_out(&mut game, "team-1", 7);

    let agreed = make_transfer_bid(&mut game, "target", 2_000_000);

    assert!(agreed.is_ok(), "{agreed:?}");
}

/// Given a club with nobody out on loan and a full list of 30
/// When it bids for a player
/// Then the refusal is the plain "full" one with its figures, exactly as before
#[test]
fn a_full_list_with_no_held_places_keeps_the_plain_refusal() {
    let mut game = make_game_with_player(make_player("target"), vec![], 90_000_000, 90_000_000);
    fill_to(&mut game, "team-1", 30);

    let refused = make_transfer_bid(&mut game, "target", 2_000_000);

    assert_eq!(
        refused.err().as_deref(),
        Some("be.error.freeAgents.squadFull?count=30&max=30&release=1")
    );
}

/// Given a club with only two senior goalkeepers
/// When the blockers are asked
/// Then both keepers are listed with the refusal a sale of either would get, and no outfield
///      player is: the warning on screen is the answer the command would give
#[test]
fn the_blockers_list_exactly_the_players_who_cannot_leave() {
    let mut game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));
    leave_goalkeepers(&mut game, "team-1", 2, "gk");
    // Depth elsewhere, so the fifteen-senior minimum is not what holds the outfield players.
    fill_to(&mut game, "team-1", 22);

    let blockers = ofm_core::squad_floor::departure_blockers(&game, "team-1");

    let ids: Vec<&str> = blockers.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"gk"));
    assert_eq!(ids.len(), 2, "{ids:?}");
    let refusal = counter_offer(&mut game, "gk", "offer-1", 1_050_000)
        .err()
        .unwrap();
    assert_eq!(
        blockers.iter().find(|(id, _)| id == "gk").unwrap().1,
        refusal
    );
}

/// Given a club with cover in every position
/// When the blockers are asked
/// Then nobody is blocked
#[test]
fn a_club_with_cover_everywhere_has_no_blockers() {
    let game = sale_game(user_player_at("gk", Position::Goalkeeper, 900_000));

    assert!(ofm_core::squad_floor::departure_blockers(&game, "team-1").is_empty());
}
