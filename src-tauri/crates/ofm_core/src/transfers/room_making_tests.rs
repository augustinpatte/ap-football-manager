#![cfg(test)]

//! Scenarios for a club that makes room on a full senior list before it signs.
//!
//! The world is small and written in football terms. `buyer` has a full list (30 seniors) and a
//! weak right-back; `seller` has a right-back of 80 on the transfer list; `taker` is short of
//! goalkeepers; and the manager's own club, `user`, is elsewhere. The one thing that stands between
//! `buyer` and the right-back is a place on its list: its fourth goalkeeper is the player it can let
//! go. Every scenario changes that one fact.

use super::{evaluate_transfer_market, signing_place_refusal};
use crate::clock::GameClock;
use crate::game::Game;
use crate::squad_intelligence::test_squads::{Squad, player as squad_player};
use chrono::{Datelike, Duration, TimeZone, Utc};
use domain::league::{League, StandingEntry};
use domain::manager::Manager;
use domain::player::{
    ActiveLoan, LoanTermsSource, Player, PlayerMovementKind, Position, SquadRole,
    TransferOfferStatus,
};
use domain::season::TransferWindowStatus;
use domain::team::Team;

const BUYER: &str = "buyer";
const SELLER: &str = "seller";
const TAKER: &str = "taker";
const SIGNING: &str = "seller-rb-80";
const SURPLUS_KEEPER: &str = "buyer-gk-3";

fn club(id: &str, reputation: u32, cash: i64) -> Team {
    let mut team = crate::squad_intelligence::test_squads::team("4-4-2", cash, 3_000_000);
    team.id = id.to_string();
    team.name = format!("{id} FC");
    team.short_name = id.chars().take(3).collect::<String>().to_uppercase();
    team.reputation = reputation;
    team
}

/// A senior of `club_id`, on a plain contract.
fn senior(club_id: &str, name: &str, position: Position, ovr: u8, age: i32) -> Player {
    let mut player = squad_player(&format!("{club_id}-{name}"), position, ovr, age);
    player.team_id = Some(club_id.to_string());
    player.market_value = 400_000;
    player.morale = 80;
    player.stage_wage(6_000);
    player.stage_contract_end(Some("2028-06-30".to_string()));
    player
}

/// A squad of twenty for `club_id` for a 4-4-2, a right-back of about `right_back` as its first
/// choice with cover far behind him, plus the goalkeepers and filler that make up `seniors`.
fn squad(club_id: &str, right_back: u8, extra_goalkeepers: &[u8], seniors: usize) -> Vec<Player> {
    let mut players: Vec<Player> = Squad::new()
        .add(Position::Goalkeeper, &[74, 66])
        .add(Position::LeftBack, &[72, 66])
        .add(Position::RightBack, &[right_back, 50])
        .add(Position::CenterBack, &[74, 72, 68, 66])
        .add(Position::LeftMidfielder, &[72, 66])
        .add(Position::RightMidfielder, &[72, 66])
        .add(Position::CentralMidfielder, &[74, 72, 68, 66])
        .add(Position::Striker, &[75, 73, 67])
        .players
        .into_iter()
        .map(|mut player| {
            player.id = format!("{club_id}-{}", player.id);
            player.team_id = Some(club_id.to_string());
            player.market_value = 400_000;
            player.morale = 80;
            player.stage_wage(6_000);
            player.stage_contract_end(Some("2028-06-30".to_string()));
            player
        })
        .collect();
    for (index, ovr) in extra_goalkeepers.iter().enumerate() {
        players.push(senior(
            club_id,
            &format!("gk-{}", index + 3),
            Position::Goalkeeper,
            *ovr,
            28,
        ));
    }
    let mut filler = 0;
    while players.len() < seniors {
        filler += 1;
        players.push(senior(
            club_id,
            &format!("bench-{filler}"),
            Position::CentralMidfielder,
            60,
            26,
        ));
    }
    players
}

/// The club's best right-back.
fn best_right_back<'a>(players: &'a mut [Player], club_id: &str) -> &'a mut Player {
    players
        .iter_mut()
        .filter(|player| {
            player.team_id.as_deref() == Some(club_id) && player.position == Position::RightBack
        })
        .max_by_key(|player| player.ovr)
        .expect("a right-back")
}

/// The world: `buyer` full (30) and weak at right-back, with `SURPLUS_KEEPER` the one player it
/// could spare; `seller` with the right-back it wants on the list; `taker` short of goalkeepers.
fn world() -> Game {
    let mut manager = Manager::new(
        "mgr-user".to_string(),
        "Alex".to_string(),
        "Boss".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("user".to_string());
    let mut players = squad(BUYER, 60, &[52], 30);
    // The third goalkeeper is the club's surplus: cheap, nobody the side misses.
    let spare = players
        .iter_mut()
        .find(|player| player.id == SURPLUS_KEEPER)
        .expect("a third keeper");
    spare.market_value = 300_000;
    players.extend(squad(SELLER, 80, &[], 22));
    let signing = best_right_back(&mut players, SELLER);
    signing.id = SIGNING.to_string();
    signing.transfer_listed = true;
    signing.market_value = 900_000;
    signing.morale = 60;
    // The taker has two goalkeepers of 40: a spare of 52 would be its first choice.
    players.extend(squad(TAKER, 70, &[], 24));
    for keeper in players.iter_mut().filter(|player| {
        player.team_id.as_deref() == Some(TAKER) && player.position == Position::Goalkeeper
    }) {
        keeper.ovr = 40;
        keeper.potential = 40;
    }
    // The manager's club: a few players, none of them for sale.
    players.extend(squad("user", 70, &[], 20));

    let date = Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap();
    let mut game = Game::new(
        GameClock::new(date),
        manager,
        vec![
            club(BUYER, 700, 30_000_000),
            club(SELLER, 600, 5_000_000),
            club(TAKER, 650, 30_000_000),
            club("user", 500, 5_000_000),
        ],
        players,
        vec![],
        vec![],
    );
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game.season_context.transfer_window.days_remaining = Some(30);
    // `buyer` plays in the competition the manager is simulating and shops every day; the other two
    // clubs are dormant and shop on their own review day only. The `taker` therefore takes the
    // surplus keeper because the club asked it to, not because it went shopping for him itself.
    game.competitions = vec![
        League {
            id: "active-league".to_string(),
            standings: vec![
                StandingEntry::new(BUYER.to_string()),
                StandingEntry::new("user".to_string()),
            ],
            ..Default::default()
        },
        League {
            id: "dormant-league".to_string(),
            standings: vec![
                StandingEntry::new(SELLER.to_string()),
                StandingEntry::new(TAKER.to_string()),
            ],
            ..Default::default()
        },
    ];
    game.active_competition_ids = vec!["active-league".to_string()];
    // The sweep is the buyer's review day: a full club looks for room once a week.
    let review = crate::ai_tactics::review_weekday(BUYER);
    let mut day = date;
    while day.weekday().num_days_from_monday() != review {
        day += Duration::days(1);
    }
    game.clock = GameClock::new(day);
    for dormant in [SELLER, TAKER] {
        assert_ne!(
            crate::ai_tactics::review_weekday(dormant),
            review,
            "the fixture needs {dormant} to be off its review day"
        );
    }
    game
}

fn seniors_at(game: &Game, club: &str) -> usize {
    game.players
        .iter()
        .filter(|player| {
            player.team_id.as_deref() == Some(club)
                && player.squad_role == SquadRole::Senior
                && !player.retired
        })
        .count()
}

fn club_of<'a>(game: &'a Game, player_id: &str) -> Option<&'a str> {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .and_then(|player| player.team_id.as_deref())
}

fn role_of(game: &Game, player_id: &str) -> SquadRole {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .map(|player| player.squad_role)
        .expect("player exists")
}

/// One day's market sweep.
fn sweep(game: &mut Game) {
    evaluate_transfer_market(game);
}

/// Given a club with a full list that wants a right-back, and a surplus keeper another club wants
/// When the market sweeps on the club's review day
/// Then the keeper is sold first and the right-back signed after, and the list is still 30.
#[test]
fn a_full_club_sells_its_surplus_player_and_then_signs_the_one_it_wanted() {
    let mut game = world();
    assert_eq!(seniors_at(&game, BUYER), 30);
    assert!(signing_place_refusal(&game, BUYER, SIGNING).is_some());

    sweep(&mut game);

    assert_eq!(
        club_of(&game, SURPLUS_KEEPER),
        Some(TAKER),
        "the keeper went to the club that needed him"
    );
    assert_eq!(
        club_of(&game, SIGNING),
        Some(BUYER),
        "and the right-back came"
    );
    assert_eq!(
        seniors_at(&game, BUYER),
        30,
        "the list never went over its limit"
    );
    let departed = game
        .players
        .iter()
        .find(|player| player.id == SURPLUS_KEEPER)
        .expect("keeper");
    assert_eq!(
        departed
            .movement_history
            .last()
            .map(|entry| entry.kind.clone()),
        Some(PlayerMovementKind::PermanentTransfer),
        "the departure went through the transfer, not around it"
    );
}

// --- Helpers for the scenarios below -----------------------------------------------------------

fn player_mut<'a>(game: &'a mut Game, player_id: &str) -> &'a mut Player {
    game.players
        .iter_mut()
        .find(|player| player.id == player_id)
        .expect("player exists")
}

fn team_mut<'a>(game: &'a mut Game, club_id: &str) -> &'a mut Team {
    game.teams
        .iter_mut()
        .find(|team| team.id == club_id)
        .expect("club exists")
}

/// What a deal moves: every player's club and role, every club's money, and the journal's length.
fn books(game: &Game) -> Books {
    Books {
        players: game
            .players
            .iter()
            .map(|player| (player.id.clone(), player.team_id.clone(), player.squad_role))
            .collect(),
        deals: game
            .players
            .iter()
            .map(|player| {
                (
                    player.transfer_offers.len(),
                    player.movement_history.len(),
                    player.wage(),
                )
            })
            .collect(),
        money: game
            .teams
            .iter()
            .map(|team| (team.finance, team.transfer_budget))
            .collect(),
        journal_rows: game.cash_journal.len(),
    }
}

/// Every player's club and role.
type Placements = Vec<(String, Option<String>, SquadRole)>;

#[derive(Debug, PartialEq)]
struct Books {
    players: Placements,
    /// Each player's offers, movement ledger length and wage: a partly applied deal would show here.
    deals: Vec<(usize, usize, u32)>,
    /// Each club's cash and transfer budget.
    money: Vec<(i64, i64)>,
    journal_rows: usize,
}

/// A sale of `player_id` to `buyer`, agreed and waiting for its registration date.
fn agree_sale_awaiting_registration(game: &mut Game, player_id: &str, buyer: &str) {
    let offer = serde_json::from_value(serde_json::json!({
        "id": "agreed-sale",
        "from_team_id": buyer,
        "fee": 200_000,
        "wage_offered": 6_000,
        "status": "PendingRegistration",
        "date": "2026-08-03",
        "registration_date": "2026-08-05",
    }))
    .expect("a transfer offer");
    player_mut(game, player_id).transfer_offers.push(offer);
}

/// `player_id` on loan from `parent` to `borrower`, as an AI club's loan leaves him.
fn lend(game: &mut Game, player_id: &str, parent: &str, borrower: &str) {
    let player = player_mut(game, player_id);
    player.team_id = Some(borrower.to_string());
    player.active_loan = Some(ActiveLoan {
        parent_team_id: parent.to_string(),
        loan_team_id: borrower.to_string(),
        loan_team_name: None,
        start_date: "2026-07-01".to_string(),
        end_date: "2027-06-30".to_string(),
        wage_contribution_pct: 100,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
        terms_source: LoanTermsSource::default(),
    });
}

/// Fills `club_id`'s senior list with benchwarmers nobody would buy until it holds `seniors`.
fn pad(game: &mut Game, club_id: &str, seniors: usize) {
    let mut index = 0;
    while seniors_at(game, club_id) < seniors {
        index += 1;
        let mut filler = senior(
            club_id,
            &format!("pad-{index}"),
            Position::CentralMidfielder,
            55,
            31,
        );
        filler.market_value = 50_000;
        game.players.push(filler);
    }
}

// --- The principal constraint ------------------------------------------------------------------

/// Given the same club with nobody it could let go that another club would take (the one club
///       that wants a keeper has a full list of its own)
/// When the market sweeps on the club's review day
/// Then nothing moves: the right-back stays where he was, nobody is sold, no money changes hands.
#[test]
fn a_full_club_with_no_acceptable_departure_makes_no_signing() {
    let mut game = world();
    pad(&mut game, TAKER, 30);
    let before = books(&game);

    sweep(&mut game);

    assert_eq!(club_of(&game, SIGNING), Some(SELLER));
    assert_eq!(books(&game), before, "no player, role or money moved");
    assert_eq!(seniors_at(&game, BUYER), 30);
}

/// Given the surplus keeper is protected by the manager's delegation settings
/// When the market sweeps on the club's review day
/// Then he is not sold, and with nobody else to let go the right-back is not signed.
#[test]
fn a_protected_player_is_never_the_one_who_makes_room() {
    let mut game = world();
    game.delegation.set_protected(SURPLUS_KEEPER, true);

    sweep(&mut game);

    assert_eq!(club_of(&game, SURPLUS_KEEPER), Some(BUYER));
    assert_eq!(club_of(&game, SIGNING), Some(SELLER));
    assert_eq!(seniors_at(&game, BUYER), 30);
}

/// Given a club whose only buyer-wanted players are its first choices
/// When the market sweeps on its review day
/// Then none of the eleven it fields is sold to make room: the club does not give up a starter
///      to buy a slightly better player.
#[test]
fn the_club_does_not_sell_a_first_choice_to_buy_a_better_one() {
    let mut game = world();
    // Nobody is spare: the surplus keeper is gone and the filler is as good as the starters.
    game.players.retain(|player| player.id != SURPLUS_KEEPER);
    pad(&mut game, BUYER, 30);
    let starters: Vec<String> = ["Goalkeeper0-74", "CenterBack3-74", "Striker27-75"]
        .iter()
        .map(|name| format!("{BUYER}-{name}"))
        .collect();
    let first_choices: Vec<String> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(BUYER) && player.ovr >= 72)
        .map(|player| player.id.clone())
        .collect();
    assert!(!first_choices.is_empty() && starters.len() == 3);

    sweep(&mut game);

    for id in first_choices {
        assert_eq!(
            club_of(&game, &id),
            Some(BUYER),
            "{id} was sold to make room"
        );
        assert_eq!(role_of(&game, &id), SquadRole::Senior);
    }
}

/// Given a club with 29 registered seniors and one senior out on loan, whose place is held
/// When the market sweeps on the club's review day
/// Then the loan did not free a place: one departure cannot make room (two places are missing), so
///      nobody is sold and the right-back is not signed; the loanee's place is still held.
#[test]
fn a_loan_out_keeps_its_place_and_does_not_make_room() {
    let mut game = world();
    lend(&mut game, SURPLUS_KEEPER, BUYER, TAKER);
    assert_eq!(seniors_at(&game, BUYER), 29);
    assert!(
        signing_place_refusal(&game, BUYER, SIGNING).is_some(),
        "29 registered and one place held for the loanee: the list is full for a signing"
    );
    assert_eq!(
        super::places_to_free_for_a_signing(&game, BUYER, SIGNING),
        1
    );
    let before = books(&game);

    // A second spare keeper, whom the taker would gladly have: with the loanee's place held, one
    // sale would still leave the list a place short, so he must not be sold for it.
    let mut second_spare = senior(BUYER, "gk-4", Position::Goalkeeper, 50, 28);
    second_spare.market_value = 300_000;
    game.players.push(second_spare);
    assert_eq!(seniors_at(&game, BUYER), 30);
    assert_eq!(
        super::places_to_free_for_a_signing(&game, BUYER, SIGNING),
        2
    );
    let before_two_short = books(&game);
    sweep(&mut game);

    assert_eq!(
        club_of(&game, SIGNING),
        Some(SELLER),
        "the signing did not happen"
    );
    assert_eq!(
        club_of(&game, "buyer-gk-4"),
        Some(BUYER),
        "nobody was sold for a place that one sale cannot free"
    );
    assert_eq!(books(&game), before_two_short);
    assert_ne!(
        before, before_two_short,
        "the second keeper changed the list; the sweep did not"
    );
    let loanee = game
        .players
        .iter()
        .find(|player| player.id == SURPLUS_KEEPER)
        .expect("loanee");
    assert_eq!(
        loanee
            .active_loan
            .as_ref()
            .map(|loan| loan.parent_team_id.as_str()),
        Some(BUYER),
        "he is still the club's player, out on loan"
    );
}

/// Given a club with 29 registered seniors and one senior out on loan (one place short once his
///       place is held), and a second spare keeper the taker wants
/// When the market sweeps on the club's review day
/// Then the club sells the spare keeper for the one place it is short, signs the right-back, and the
///      loanee's place is never counted as free: registered plus held is still 30.
#[test]
fn a_club_one_place_short_because_of_a_loan_makes_that_one_place_by_a_sale() {
    let mut game = world();
    lend(&mut game, SURPLUS_KEEPER, BUYER, TAKER);
    game.players
        .retain(|player| player.id != format!("{BUYER}-bench-1"));
    let mut second_spare = senior(BUYER, "gk-4", Position::Goalkeeper, 50, 28);
    second_spare.market_value = 300_000;
    game.players.push(second_spare);
    assert_eq!(seniors_at(&game, BUYER), 29);
    assert_eq!(
        super::places_to_free_for_a_signing(&game, BUYER, SIGNING),
        1
    );

    sweep(&mut game);

    assert_eq!(
        club_of(&game, "buyer-gk-4"),
        Some(TAKER),
        "the spare was sold"
    );
    assert_eq!(
        club_of(&game, SIGNING),
        Some(BUYER),
        "and the right-back signed"
    );
    assert_eq!(seniors_at(&game, BUYER), 29, "28 left plus the right-back");
    let loanee = game
        .players
        .iter()
        .find(|player| player.id == SURPLUS_KEEPER)
        .expect("loanee");
    assert_eq!(
        loanee
            .active_loan
            .as_ref()
            .map(|loan| loan.parent_team_id.as_str()),
        Some(BUYER),
        "he is still on loan, and his place is still held: the list is full for any further signing"
    );
    assert!(signing_place_refusal(&game, BUYER, "another-signing").is_some());
}

/// Given a full list
/// When one senior is sold through the transfer and another is lent out
/// Then the sale frees a place and the loan does not.
#[test]
fn a_completed_permanent_sale_frees_one_place_and_a_loan_does_not() {
    let mut game = world();
    assert!(signing_place_refusal(&game, BUYER, SIGNING).is_some());

    lend(&mut game, SURPLUS_KEEPER, BUYER, TAKER);
    assert!(
        signing_place_refusal(&game, BUYER, SIGNING).is_some(),
        "a loan-out leaves the place held"
    );
    // Undo the loan, then sell him for real.
    let keeper = player_mut(&mut game, SURPLUS_KEEPER);
    keeper.active_loan = None;
    keeper.team_id = Some(BUYER.to_string());
    super::execute_transfer_with_terms(
        &mut game,
        SURPLUS_KEEPER,
        TAKER,
        BUYER,
        150_000,
        &Default::default(),
    )
    .expect("the sale completes");

    assert_eq!(seniors_at(&game, BUYER), 29);
    assert_eq!(
        signing_place_refusal(&game, BUYER, SIGNING),
        None,
        "one place is free"
    );
    assert_eq!(
        super::places_to_free_for_a_signing(&game, BUYER, SIGNING),
        0
    );
}

/// Given the surplus keeper has been sold to the taker, a sale that waits for its registration date
/// When the market sweeps
/// Then the pending sale frees no place and he is not sold a second time to make room, so the
///      right-back is not signed on the strength of a departure that has not happened.
#[test]
fn a_pending_sale_does_not_free_a_place_and_is_not_sold_twice() {
    let mut game = world();
    agree_sale_awaiting_registration(&mut game, SURPLUS_KEEPER, TAKER);
    assert!(
        signing_place_refusal(&game, BUYER, SIGNING).is_some(),
        "he is still registered at the club until the sale registers"
    );

    sweep(&mut game);

    assert_eq!(
        club_of(&game, SURPLUS_KEEPER),
        Some(BUYER),
        "nobody moved him early"
    );
    assert_eq!(club_of(&game, SIGNING), Some(SELLER));
    let keeper = game
        .players
        .iter()
        .find(|player| player.id == SURPLUS_KEEPER)
        .expect("keeper");
    assert_eq!(
        keeper.transfer_offers.len(),
        1,
        "no second negotiation was opened for the same player"
    );
    assert_eq!(
        keeper.transfer_offers[0].status,
        TransferOfferStatus::PendingRegistration
    );
}

/// Given the right-back costs more than the club has
/// When the market sweeps on the club's review day
/// Then the club makes no departure for a signing it cannot afford: the keeper stays.
#[test]
fn a_replacement_the_club_cannot_afford_makes_no_departure() {
    // The control: the same club with its money signs him (and sells the keeper to do it).
    let mut funded = world();
    sweep(&mut funded);
    assert_eq!(club_of(&funded, SIGNING), Some(BUYER));
    assert_eq!(club_of(&funded, SURPLUS_KEEPER), Some(TAKER));

    let mut game = world();
    {
        let buyer = team_mut(&mut game, BUYER);
        buyer.finance = 50_000;
        buyer.transfer_budget = 50_000;
    }
    let before = books(&game);

    sweep(&mut game);

    assert_eq!(club_of(&game, SURPLUS_KEEPER), Some(BUYER));
    assert_eq!(club_of(&game, SIGNING), Some(SELLER));
    assert_eq!(books(&game), before);
}

/// Given a taker whose senior list is full
/// When a sale to it is executed
/// Then the transfer is refused and neither club, nor the player, nor the journal has changed.
#[test]
fn a_transfer_refused_for_want_of_a_place_leaves_both_clubs_unchanged() {
    let mut game = world();
    pad(&mut game, TAKER, 30);
    let before = books(&game);

    let refused = super::execute_transfer_with_terms(
        &mut game,
        SURPLUS_KEEPER,
        TAKER,
        BUYER,
        150_000,
        &Default::default(),
    );

    assert!(
        refused
            .as_ref()
            .is_err_and(|reason| reason.starts_with("be.error.transfers.buyerSquadFull")),
        "{refused:?}"
    );
    assert_eq!(books(&game), before);
}

/// Given a seller that would be left below the squad floor by the sale
/// When the sale is executed
/// Then it is refused whole: no player and no money moved. The floor is not the room-making's to
///      break, and a refusal never leaves half a deal.
#[test]
fn a_transfer_that_would_break_the_squad_floor_leaves_both_clubs_unchanged() {
    let mut game = world();
    // Two keepers is the floor: with the spare gone, selling either leaves the club short.
    game.players.retain(|player| player.id != SURPLUS_KEEPER);
    let first_choice = game
        .players
        .iter()
        .find(|p| p.team_id.as_deref() == Some(BUYER) && p.position == Position::Goalkeeper)
        .map(|p| p.id.clone())
        .expect("a keeper");
    let before = books(&game);

    let refused = super::execute_transfer_with_terms(
        &mut game,
        &first_choice,
        TAKER,
        BUYER,
        150_000,
        &Default::default(),
    );

    assert!(
        refused
            .as_ref()
            .is_err_and(|reason| reason.starts_with("be.error.squadFloor")),
        "{refused:?}"
    );
    assert_eq!(books(&game), before);
}

/// Given two full clubs that both want the same right-back
/// When the market sweeps
/// Then exactly one signs him, and the other sells nobody for a signing it did not get.
#[test]
fn two_full_clubs_after_the_same_player_make_one_swap_not_two() {
    let mut game = world();
    let review = crate::ai_tactics::review_weekday(BUYER);
    let rival = (0..)
        .map(|n| format!("rival{n}"))
        .find(|id| crate::ai_tactics::review_weekday(id) == review)
        .expect("some id shares the review day");
    game.teams.push(club(&rival, 700, 30_000_000));
    game.competitions[0]
        .standings
        .push(StandingEntry::new(rival.clone()));
    let mut rival_squad = squad(&rival, 60, &[52], 30);
    rival_squad
        .iter_mut()
        .find(|player| player.id == format!("{rival}-gk-3"))
        .expect("the rival's spare keeper")
        .market_value = 300_000;
    game.players.extend(rival_squad);
    // Room in the taker for both keepers, and nothing else for them to buy.
    for index in 0..3 {
        game.players
            .retain(|player| player.id != format!("{TAKER}-bench-{}", index + 1));
    }

    sweep(&mut game);

    let signed_by: Vec<&str> = [BUYER, rival.as_str()]
        .into_iter()
        .filter(|club| club_of(&game, SIGNING) == Some(*club))
        .collect();
    assert_eq!(
        signed_by.len(),
        1,
        "exactly one club got him: {signed_by:?}"
    );
    let (winner, loser) = if signed_by[0] == BUYER {
        (BUYER, rival.as_str())
    } else {
        (rival.as_str(), BUYER)
    };
    // The winner made room by selling its spare keeper; the loser, which did not get its signing,
    // sold nobody for it.
    assert_eq!(club_of(&game, &format!("{winner}-gk-3")), Some(TAKER));
    assert_eq!(club_of(&game, &format!("{loser}-gk-3")), Some(loser));
    assert_eq!(seniors_at(&game, loser), 30);
}

/// Given the manager's club has room on its list and two weak keepers, so that it is the club most
///       likely to take the spare keeper (the usual taker's list is full)
/// When the market sweeps
/// Then nothing of the manager's is sold, reassigned or bought, and the spare keeper is not given
///      to it: the manager's club takes no part in room-making, as seller or as taker.
#[test]
fn the_managers_club_takes_no_part_in_making_room() {
    let mut game = world();
    for keeper in game.players.iter_mut().filter(|player| {
        player.team_id.as_deref() == Some("user") && player.position == Position::Goalkeeper
    }) {
        keeper.ovr = 40;
        keeper.potential = 40;
    }
    // The taker is out of the picture, so the only club that could use the keeper is the manager's,
    // and it is of a stature to want him.
    pad(&mut game, TAKER, 30);
    {
        let managers = team_mut(&mut game, "user");
        managers.reputation = 650;
        managers.finance = 30_000_000;
        managers.transfer_budget = 30_000_000;
    }
    pad(&mut game, "user", 24);
    let managers_players = |game: &Game| -> Placements {
        books(game)
            .players
            .into_iter()
            .filter(|(id, _, _)| id.starts_with("user-"))
            .collect()
    };
    let before = managers_players(&game);
    let team_before = game
        .teams
        .iter()
        .find(|team| team.id == "user")
        .map(|team| (team.finance, team.transfer_budget));

    sweep(&mut game);

    assert_eq!(managers_players(&game), before);
    assert_eq!(
        club_of(&game, SURPLUS_KEEPER).filter(|club| *club == "user"),
        None
    );
    assert_eq!(
        game.teams
            .iter()
            .find(|team| team.id == "user")
            .map(|team| (team.finance, team.transfer_budget)),
        team_before
    );
}

/// Given an agreed sale waiting for its registration date, saved and reloaded
/// When the places and the market are read again
/// Then the held place and the sweep's decisions are the same as before the save.
#[test]
fn a_negotiation_in_progress_survives_save_and_reload() {
    let mut game = world();
    agree_sale_awaiting_registration(&mut game, SURPLUS_KEEPER, TAKER);
    let mut loaded: Game =
        serde_json::from_str(&serde_json::to_string(&game).expect("saves")).expect("loads");

    assert_eq!(
        super::places_to_free_for_a_signing(&loaded, BUYER, SIGNING),
        1
    );
    assert_eq!(
        super::places_to_free_for_a_signing(&game, BUYER, SIGNING),
        1
    );
    sweep(&mut game);
    sweep(&mut loaded);
    assert_eq!(
        books(&loaded),
        books(&game),
        "the reloaded game decides as the live one does"
    );
    assert_eq!(club_of(&loaded, SURPLUS_KEEPER), Some(BUYER));
    assert_eq!(
        loaded
            .players
            .iter()
            .find(|p| p.id == SURPLUS_KEEPER)
            .map(|p| p.transfer_offers.len()),
        Some(1),
        "the agreed sale is still the only negotiation"
    );

    // And a world in which a swap does happen, saved before the sweep, makes the same swap.
    let fresh = world();
    let mut reloaded: Game =
        serde_json::from_str(&serde_json::to_string(&fresh).expect("saves")).expect("loads");
    let mut live = fresh;
    sweep(&mut live);
    sweep(&mut reloaded);
    assert_eq!(club_of(&live, SIGNING), Some(BUYER));
    assert_eq!(
        books(&reloaded),
        books(&live),
        "the same swap, from the saved game"
    );
}

/// Given a club one place short of full, because a departure was already completed
/// When the market sweeps
/// Then it signs without selling anyone else: the departure that already happened is the one that
///      made room.
#[test]
fn a_departure_already_completed_makes_the_room_and_nobody_else_is_sold() {
    let mut game = world();
    game.players
        .retain(|player| player.id != format!("{BUYER}-bench-1"));
    assert_eq!(seniors_at(&game, BUYER), 29);

    sweep(&mut game);

    assert_eq!(club_of(&game, SIGNING), Some(BUYER));
    assert_eq!(
        club_of(&game, SURPLUS_KEEPER),
        Some(BUYER),
        "no further sale was made"
    );
    assert_eq!(seniors_at(&game, BUYER), 30);
}

/// Given a surplus youngster the academy's rules allow and no club that wants him
/// When the market sweeps
/// Then he is registered with the academy, which is a permitted reassignment and not a sale, and
///      the right-back is signed; a surplus player past the academy's age limit is never reassigned.
#[test]
fn a_surplus_youngster_with_no_taker_is_reassigned_to_the_academy() {
    let mut game = world();
    pad(&mut game, TAKER, 30);
    let youngster = player_mut(&mut game, SURPLUS_KEEPER);
    youngster.date_of_birth = "2006-03-01".to_string();

    sweep(&mut game);

    assert_eq!(club_of(&game, SIGNING), Some(BUYER));
    assert_eq!(
        club_of(&game, SURPLUS_KEEPER),
        Some(BUYER),
        "he stays at the club"
    );
    assert_eq!(role_of(&game, SURPLUS_KEEPER), SquadRole::Youth);
    assert_eq!(seniors_at(&game, BUYER), 30);
    assert!(
        game.players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some(BUYER))
            .filter(|player| player.squad_role == SquadRole::Youth)
            .all(|player| crate::roster::YOUTH_ACADEMY_MAX_AGE
                >= domain::age::age_on_or_unknown(
                    game.clock.current_date.date_naive(),
                    &player.date_of_birth
                )),
        "only players the academy may hold were reassigned"
    );
}

/// Given the surplus keeper is named in the club's stored eleven
/// When the market sweeps on the club's review day
/// Then he is not the one who makes room: a player in the side the club has picked stays.
#[test]
fn a_player_in_the_stored_eleven_is_not_the_one_who_makes_room() {
    let mut game = world();
    team_mut(&mut game, BUYER).starting_xi_ids = vec![SURPLUS_KEEPER.to_string()];

    sweep(&mut game);

    assert_eq!(club_of(&game, SURPLUS_KEEPER), Some(BUYER));
    assert_eq!(club_of(&game, SIGNING), Some(SELLER));
}

/// Given a buyer with a place on its list and purchases that must be refused for different reasons
/// When the pre-flight a club makes before it lets a player go, and the transfer itself, judge each
/// Then they agree on whether it is refused, and on the reason: a club never sells a player to make
///      room for a signing the executor was going to refuse. (If a check is added to the executor
///      and not to the pre-flight, this goes red.)
#[test]
fn the_pre_flight_refuses_exactly_what_the_transfer_refuses() {
    use domain::transfer_terms::TransferTerms;
    type Scenario = (&'static str, fn(&mut Game), TransferTerms);
    let scenarios: Vec<Scenario> = vec![
        ("a plain purchase", |_| {}, TransferTerms::default()),
        (
            "the player is on loan",
            |game| {
                lend(game, SIGNING, SELLER, "user");
                player_mut(game, SIGNING).team_id = Some(SELLER.to_string());
            },
            TransferTerms::default(),
        ),
        (
            "terms the rules do not allow",
            |_| {},
            TransferTerms {
                sell_on_pct: 200,
                ..Default::default()
            },
        ),
        (
            "the seller would be left below the floor",
            |game| {
                game.players.retain(|player| {
                    !(player.team_id.as_deref() == Some(SELLER)
                        && player.position == Position::RightBack
                        && player.id != SIGNING)
                });
                // One defender fewer than the floor allows once he leaves.
                let defenders: Vec<String> = game
                    .players
                    .iter()
                    .filter(|p| {
                        p.team_id.as_deref() == Some(SELLER) && p.position != Position::Goalkeeper
                    })
                    .filter(|p| p.position.to_group_position() == Position::Defender)
                    .map(|p| p.id.clone())
                    .collect();
                let keep = crate::squad_floor::MIN_PLAYERS_PER_GROUP[1].1;
                for id in defenders
                    .iter()
                    .filter(|id| id.as_str() != SIGNING)
                    .skip(keep - 1)
                {
                    game.players.retain(|player| &player.id != id);
                }
            },
            TransferTerms::default(),
        ),
    ];
    let mut refused = Vec::new();
    for (name, arrange, terms) in scenarios {
        let mut game = world();
        arrange(&mut game);
        let preflight =
            super::purchase_refusal_but_for_a_place(&game, SIGNING, TAKER, SELLER, 500_000, &terms);
        let executed =
            super::execute_transfer_with_terms(&mut game, SIGNING, TAKER, SELLER, 500_000, &terms);
        assert_eq!(
            preflight, executed,
            "{name}: the pre-flight and the transfer disagree"
        );
        if preflight.is_err() {
            refused.push(name);
        }
    }
    // The scenarios discriminate: the plain purchase goes through, the other three are refused.
    assert_eq!(refused.len(), 3, "{refused:?}");
}

/// Given a buyer whose senior list is full
/// When the pre-flight and the transfer judge the same purchase
/// Then the pre-flight says it would go through but for the place and the transfer refuses for
///      exactly that place: the pre-flight leaves the place to the caller and nothing else.
#[test]
fn the_pre_flight_leaves_only_the_place_to_the_transfer() {
    let mut game = world();
    let terms = Default::default();
    assert!(signing_place_refusal(&game, BUYER, SIGNING).is_some());

    let preflight =
        super::purchase_refusal_but_for_a_place(&game, SIGNING, BUYER, SELLER, 500_000, &terms);
    let executed =
        super::execute_transfer_with_terms(&mut game, SIGNING, BUYER, SELLER, 500_000, &terms);

    assert_eq!(preflight, Ok(()));
    assert!(
        executed
            .as_ref()
            .is_err_and(|reason| reason.starts_with("be.error.transfers.buyerSquadFull")),
        "{executed:?}"
    );
}
