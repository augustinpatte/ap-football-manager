use super::evaluate_transfer_market;
use crate::clock::GameClock;
use crate::game::Game;
use chrono::{TimeZone, Utc};
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use domain::team::Team;

fn make_team(id: &str, name: &str, reputation: u32) -> Team {
    let mut team = Team::new(
        id.to_string(),
        name.to_string(),
        name[..3].to_string(),
        "England".to_string(),
        "Testville".to_string(),
        format!("{} Ground", name),
        25_000,
    );
    team.reputation = reputation;
    team.finance = 5_000_000;
    team.transfer_budget = 5_000_000;
    team.wage_budget = 2_000_000;
    team
}

pub(super) fn sample_attributes() -> PlayerAttributes {
    PlayerAttributes {
        pace: 68,
        stamina: 66,
        strength: 64,
        agility: 67,
        passing: 65,
        shooting: 72,
        tackling: 38,
        dribbling: 69,
        defending: 35,
        positioning: 66,
        vision: 63,
        decisions: 61,
        composure: 62,
        aggression: 48,
        teamwork: 58,
        leadership: 44,
        handling: 12,
        reflexes: 14,
        aerial: 40,
    }
}

pub(super) fn make_game() -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 1, 12, 12, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr-user".to_string(),
        "Alex".to_string(),
        "Boss".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());

    let mut player = Player::new(
        "player-award".to_string(),
        "Golden".to_string(),
        "Golden Boot".to_string(),
        "1998-04-01".to_string(),
        "England".to_string(),
        Position::Forward,
        sample_attributes(),
    );
    player.team_id = Some("team1".to_string());
    player.market_value = 600_000;
    player.stage_wage(18_000);
    player.morale = 58;
    player.stage_contract_end(Some("2027-06-30".to_string()));
    player.stats.appearances = 6;
    player.stats.goals = 19;

    let mut game = Game::new(
        clock,
        manager,
        vec![
            make_team("team1", "Alpha FC", 620),
            make_team("team2", "Beta FC", 690),
        ],
        vec![player],
        vec![],
        vec![],
    );
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    game
}

#[test]
fn evaluate_transfer_market_targets_award_leaderboard_user_player() {
    let mut game = make_game();

    evaluate_transfer_market(&mut game);

    let player = game
        .players
        .iter()
        .find(|player| player.id == "player-award")
        .expect("award leaderboard player should exist");

    assert!(
        player.transfer_offers.iter().any(|offer| {
            offer.from_team_id == "team2" && offer.status == TransferOfferStatus::Pending
        }),
        "Award-leaderboard players should attract AI bids even when their base transfer-interest score is otherwise too low"
    );
    assert!(
        game.messages
            .iter()
            .any(|message| { message.context.player_id.as_deref() == Some("player-award") }),
        "The incoming bid should surface through the usual inbox flow"
    );
}

#[test]
fn dormant_clubs_outside_the_active_scope_skip_the_market() {
    use domain::league::{League, StandingEntry};

    let mut game = make_game();
    // team3 plays in the actively-simulated competition; team2 is moved into
    // a dormant competition the player isn't simulating in full.
    game.teams.push(make_team("team3", "Gamma FC", 700));

    let active = League {
        id: "active-league".to_string(),
        standings: vec![
            StandingEntry::new("team1".to_string()),
            StandingEntry::new("team3".to_string()),
        ],
        ..Default::default()
    };
    let dormant = League {
        id: "dormant-league".to_string(),
        standings: vec![StandingEntry::new("team2".to_string())],
        ..Default::default()
    };
    game.competitions = vec![active, dormant];
    game.active_competition_ids = vec!["active-league".to_string()];

    evaluate_transfer_market(&mut game);

    let player = game
        .players
        .iter()
        .find(|player| player.id == "player-award")
        .expect("award leaderboard player should exist");

    assert!(
        player
            .transfer_offers
            .iter()
            .any(|offer| offer.from_team_id == "team3"),
        "an active club should still bid on the user's standout player"
    );
    assert!(
        !player
            .transfer_offers
            .iter()
            .any(|offer| offer.from_team_id == "team2"),
        "a dormant club outside the active simulation scope must not shop the market"
    );
}

/// A dormant club that is short of forwards and is the only club able to buy, on a day that is
/// (`on_review_day`) or is not its own review weekday, with `days_remaining` until the close.
fn dormant_buyer_with_a_need(days_remaining: i64, on_review_day: bool) -> Game {
    use chrono::{Datelike, Duration};
    use domain::league::{League, StandingEntry};

    let mut game = make_game();
    game.teams.push(make_team("team3", "Gamma FC", 700));
    game.competitions = vec![
        League {
            id: "active-league".to_string(),
            standings: vec![
                StandingEntry::new("team1".to_string()),
                StandingEntry::new("team3".to_string()),
            ],
            ..Default::default()
        },
        League {
            id: "dormant-league".to_string(),
            standings: vec![StandingEntry::new("team2".to_string())],
            ..Default::default()
        },
    ];
    game.active_competition_ids = vec!["active-league".to_string()];
    // Two senior forwards is below the group's floor plus planning margin: a real depth need.
    for index in 0..2 {
        let mut forward = Player::new(
            format!("team2-forward-{index}"),
            "Squad".to_string(),
            "Forward".to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            Position::Forward,
            sample_attributes(),
        );
        forward.team_id = Some("team2".to_string());
        forward.stage_contract_end(Some("2028-06-30".to_string()));
        forward.stage_wage(10_000);
        game.players.push(forward);
    }
    // Only team2 can buy: the club inside the active scope has nothing to spend.
    for team in game.teams.iter_mut().filter(|team| team.id == "team3") {
        team.finance = 0;
        team.transfer_budget = 0;
    }
    // The window is early until the last week, so a need does not yet let a club skip its day.
    game.season_context.transfer_window.days_remaining = Some(days_remaining);
    let review = crate::ai_tactics::review_weekday("team2");
    let mut date = Utc.with_ymd_and_hms(2026, 1, 12, 12, 0, 0).unwrap();
    while (date.weekday().num_days_from_monday() == review) != on_review_day {
        date += Duration::days(1);
    }
    game.clock = GameClock::new(date);
    game
}

fn bids_from_team2(game: &Game) -> bool {
    game.players
        .iter()
        .find(|player| player.id == "player-award")
        .is_some_and(|player| {
            player
                .transfer_offers
                .iter()
                .any(|offer| offer.from_team_id == "team2")
        })
}

/// Given a club outside the player's active scope with a real hole in its squad, when the market
/// sweeps early in the window on a day that is not its review weekday, then it does not bid: a need
/// shapes what the club buys on its day, it does not make it shop every day.
#[test]
fn a_dormant_club_with_a_need_waits_for_its_review_day_early_in_the_window() {
    let mut game = dormant_buyer_with_a_need(60, false);
    evaluate_transfer_market(&mut game);
    assert!(
        !bids_from_team2(&game),
        "a club outside the active scope must not shop daily merely because it has a need"
    );
}

/// Given the same dormant club, when the sweep lands on its own review weekday, then it works its
/// squad: the rule delays the club's reaction, it does not silence it.
#[test]
fn a_dormant_club_with_a_need_shops_on_its_review_day() {
    let mut game = dormant_buyer_with_a_need(60, true);
    evaluate_transfer_market(&mut game);
    assert!(
        bids_from_team2(&game),
        "a dormant club still works its squad on its own review day"
    );
}

/// Given the same dormant club, when the window is in its last week, then a real need no longer
/// waits for the review day.
#[test]
fn a_dormant_club_with_a_need_shops_before_the_window_closes() {
    let mut game = dormant_buyer_with_a_need(5, false);
    evaluate_transfer_market(&mut game);
    assert!(
        bids_from_team2(&game),
        "in the last week a real need must not wait for the review day"
    );
}

// Shared fixtures for the acceptance routes, with enough depth to permit a departure.
pub(super) fn acceptance_game(owner: &str, deferred: bool) -> Game {
    let mut game = make_game();
    game.players[0].team_id = Some(owner.to_string());
    game.players[0].loan_listed = true;
    for (position, count) in [
        (Position::Goalkeeper, 3),
        (Position::Defender, 5),
        (Position::Midfielder, 5),
        (Position::Forward, 3),
    ] {
        for index in 0..count {
            let mut player = Player::new(
                format!("depth-{position:?}-{index}"),
                "Depth".into(),
                "Player".into(),
                "1998-01-01".into(),
                "England".into(),
                position.clone(),
                sample_attributes(),
            );
            player.team_id = Some(owner.to_string());
            game.players.push(player);
        }
    }
    if deferred {
        game.season_context.transfer_window.status = TransferWindowStatus::Closed;
        game.season_context.transfer_window.opens_on = Some("2026-07-02".into());
    }
    game
}

pub(super) fn incoming_transfer(game: &mut Game) -> String {
    super::bids::upsert_transfer_offer(
        &mut game.players[0],
        "team2",
        600_000,
        TransferOfferStatus::Pending,
        "2026-01-12",
        None,
        1,
        None,
        None,
    )
}

pub(super) fn incoming_loan(game: &mut Game) -> String {
    super::loans::upsert_loan_offer(
        &mut game.players[0],
        "team2",
        "team1",
        "2026-01-12",
        "2026-12-20",
        60,
        Some(600_000),
        domain::player::LoanOfferStatus::Pending,
        "2026-01-12",
    )
}

pub(super) fn assert_acceptance_notice<'a>(
    game: &'a Game,
    key: &str,
    start: &str,
) -> &'a domain::message::InboxMessage {
    assert_eq!(game.messages.len(), 1, "one notice for the agreed deal");
    let message = &game.messages[0];
    assert_eq!(message.body_key.as_deref(), Some(key));
    assert_eq!(message.context.player_id.as_deref(), Some("player-award"));
    assert_eq!(message.context.team_id.as_deref(), Some("team1"));
    assert_eq!(
        message.i18n_params.get("player").map(String::as_str),
        Some("Golden Boot")
    );
    if !start.is_empty() {
        assert_eq!(
            message.i18n_params.get("start").map(String::as_str),
            Some(start)
        );
    }
    assert_eq!(message.category, domain::message::MessageCategory::Transfer);
    assert_eq!(
        message.sender_key.as_deref(),
        Some("be.sender.transferCommittee")
    );
    assert_eq!(
        message.sender_role_key.as_deref(),
        Some("be.role.directorOfFootball")
    );
    assert!(game.emitted_events.contains(&message.id));
    message
}

fn squad_player(id: &str, club: &str, position: Position, wage: u32) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "1998-04-01".to_string(),
        "England".to_string(),
        position,
        sample_attributes(),
    );
    player.team_id = Some(club.to_string());
    player.stage_wage(wage);
    player.stage_contract_end(Some("2028-06-30".to_string()));
    player.market_value = 100_000;
    player
}

/// A senior squad that can spare one forward: two keepers, five defenders, five midfielders and
/// three forwards, each on a token wage.
fn full_squad(club: &str) -> Vec<Player> {
    [
        (Position::Goalkeeper, 2),
        (Position::Defender, 5),
        (Position::Midfielder, 5),
        (Position::Forward, 3),
    ]
    .into_iter()
    .flat_map(|(position, count)| (0..count).map(move |index| (position.clone(), index)))
    .map(|(position, index)| {
        squad_player(&format!("{club}-{position:?}{index}"), club, position, 100)
    })
    .collect()
}

/// A forward on the transfer list with a contract about to end: the AI clubs go for him.
fn bargain(id: &str, club: &str, wage: u32) -> Player {
    let mut player = squad_player(id, club, Position::Forward, wage);
    player.transfer_listed = true;
    player.stage_contract_end(Some("2026-02-20".to_string()));
    player.market_value = 1_200_000;
    // Better than the forwards any club here has: a buyer goes for a player who improves it.
    player.ovr = 85;
    player
}

/// Given buyer "b" signs club "a"'s expensive bargain first, and "a" is then over its wage budget
/// only if its bill is still the one from before the sale, When the sweep reaches "a", Then "a"
/// signs "c"'s bargain: the bill it is held to is the one after the sale. A bill left from before
/// the sweep would refuse him.
#[test]
fn a_club_that_just_sold_is_held_to_the_wage_bill_it_has_after_the_sale() {
    let mut game = make_game();
    game.players.clear();
    game.teams = vec![
        make_team("team1", "Alpha FC", 700),
        make_team("b", "Beta FC", 700),
        make_team("a", "Atlas FC", 700),
        make_team("c", "Cobra FC", 700),
    ];
    game.teams[2].wage_budget = 50_000;
    for club in ["b", "c"] {
        game.teams
            .iter_mut()
            .find(|t| t.id == club)
            .unwrap()
            .wage_budget = 50_000_000;
    }
    // Cash enough to carry the wages with no income to speak of: the books are not what this is about.
    for team in game.teams.iter_mut() {
        team.finance = 200_000_000;
    }
    for club in ["b", "a", "c"] {
        game.players.extend(full_squad(club));
    }
    game.players.push(bargain("sold-by-a", "a", 80_000));
    game.players.push(bargain("sold-by-c", "c", 20_000));

    evaluate_transfer_market(&mut game);

    let club_of = |id: &str| {
        game.players
            .iter()
            .find(|player| player.id == id)
            .and_then(|player| player.team_id.clone())
    };
    assert_eq!(
        club_of("sold-by-a").as_deref(),
        Some("b"),
        "the first sale did not happen"
    );
    assert_eq!(club_of("sold-by-c").as_deref(), Some("a"));
}

/// Three clubs in a window. Club "b" has two forwards rated 75 (born `starters_born`) and a third,
/// the cover, rated `cover`; every other club's forwards are rated 70 and aged 27. Club "c" has a
/// listed 20-year-old forward rated 58 with `potential`: worse than "b"'s cover today, and expected
/// a year on to be 58 plus 40 % of the way to his potential (a planned 66 at 78).
fn succession_market(starters_born: &str, cover: u8, potential: u8) -> Game {
    let mut game = make_game();
    game.players.clear();
    game.teams = vec![
        make_team("team1", "Alpha FC", 700),
        make_team("b", "Beta FC", 700),
        make_team("a", "Atlas FC", 700),
        make_team("c", "Cobra FC", 700),
    ];
    for team in game.teams.iter_mut() {
        team.finance = 200_000_000;
        team.wage_budget = 50_000_000;
    }
    for club in ["b", "a", "c"] {
        game.players.extend(full_squad(club));
    }
    for player in game.players.iter_mut() {
        if player.position == Position::Forward {
            player.ovr = 70;
            player.potential = 70;
        }
        if player.team_id.as_deref() == Some("b") && player.position == Position::Forward {
            let starter = player.id.ends_with("0") || player.id.ends_with("1");
            player.ovr = if starter { 75 } else { cover };
            player.potential = player.ovr;
            if starter {
                player.date_of_birth = starters_born.to_string();
            }
        }
    }
    let mut prospect = squad_player("prospect", "c", Position::Forward, 20_000);
    prospect.transfer_listed = true;
    prospect.ovr = 58;
    prospect.potential = potential;
    prospect.date_of_birth = "2005-04-01".to_string();
    prospect.market_value = 600_000;
    game.players.push(prospect);
    game
}

fn prospect_club(game: &Game) -> Option<String> {
    game.players
        .iter()
        .find(|player| player.id == "prospect")
        .and_then(|player| player.team_id.clone())
}

fn market_sells_the_prospect_to(starters_born: &str, cover: u8, potential: u8) -> Option<String> {
    let mut game = succession_market(starters_born, cover, potential);
    evaluate_transfer_market(&mut game);
    prospect_club(&game)
}

/// Given a club whose two first-choice forwards are 36, a cover rated 68, and a listed 20-year-old
/// rated 58 who will be 66 next year (short of the cover either way)
/// When the window's sweep runs
/// Then the club signs him: it is buying the successor to two players about to retire (the group
/// will be one short of its depth), whatever his rating.
#[test]
fn a_club_with_two_ageing_forwards_buys_the_young_forward_who_will_replace_them() {
    assert_eq!(
        market_sells_the_prospect_to("1990-04-01", 68, 78).as_deref(),
        Some("b")
    );
}

/// The same market with first-choice forwards of 27: nothing is leaving, the target falls short of
/// the cover today and a year on, and nobody buys him. The control that makes the case above about age.
#[test]
fn a_club_with_forwards_in_their_prime_does_not_buy_a_prospect_who_stays_short_of_its_cover() {
    assert_eq!(
        market_sells_the_prospect_to("1999-04-01", 68, 78).as_deref(),
        Some("c")
    );
}

/// Given a club with forwards in their prime and a cover rated 60, and a listed 20-year-old rated 58
/// with a potential of 78 (66 next year: six better than the cover)
/// When the window's sweep runs
/// Then nobody signs him: he will be a better bench player next year, and a bench player is needed
/// one match in ten. Squad Intelligence prices that at a few hundredths of a rating point across the
/// side, which is not worth a transfer for a club whose starters are 27 and 75. (Until F5.4 a club
/// bought him because he would beat its first cover; the succession case that does need him is
/// `a_club_with_two_ageing_forwards_buys_the_young_forward_who_will_replace_them`.)
#[test]
fn a_club_does_not_buy_a_prospect_who_will_only_be_better_than_its_bench() {
    assert_eq!(
        market_sells_the_prospect_to("1999-04-01", 60, 78).as_deref(),
        Some("c")
    );
}

/// The same market with a prospect whose potential is 60: next year he is 59, still short of the
/// 60 cover, and nobody buys him. The control for the case above: it is his growth that sells him.
#[test]
fn a_club_does_not_buy_a_prospect_who_has_no_room_to_grow_past_its_cover() {
    assert_eq!(
        market_sells_the_prospect_to("1999-04-01", 60, 60).as_deref(),
        Some("c")
    );
}

/// Makes `team_id` a club of `count` senior players, with room left for the deals a test adds.
fn register_seniors(game: &mut Game, team_id: &str, count: usize) {
    for index in 0..count {
        let mut player = Player::new(
            format!("{team_id}-senior-{index}"),
            "Squad".into(),
            "Player".into(),
            "1998-01-01".into(),
            "England".into(),
            Position::Midfielder,
            sample_attributes(),
        );
        player.team_id = Some(team_id.to_string());
        player.squad_role = domain::player::SquadRole::Senior;
        game.players.push(player);
    }
}

fn promise_the_loan(game: &mut Game) {
    incoming_loan(game);
    let offer = game.players[0]
        .loan_offers
        .last_mut()
        .expect("a loan offer");
    offer.status = domain::player::LoanOfferStatus::PendingRegistration;
}

fn promise_the_purchase(game: &mut Game) {
    incoming_transfer(game);
    let offer = game.players[0]
        .transfer_offers
        .last_mut()
        .expect("a transfer offer");
    offer.status = TransferOfferStatus::PendingRegistration;
}

/// Given a club with twenty-eight seniors and a loan agreed and waiting to register
/// When the gate counts its places and then another senior joins
/// Then the promised loan takes the last place: a loan that waits reserves one exactly as a
/// purchase does, and a deal registering itself is not blocked by its own promise.
#[test]
fn a_pending_loan_in_reserves_a_senior_place_as_a_purchase_does() {
    use super::registration::{
        pending_incoming_loans, pending_incoming_purchases, signing_place_refusal,
    };

    let mut game = acceptance_game("team1", false);
    register_seniors(&mut game, "team2", 28);
    promise_the_loan(&mut game);
    assert_eq!(pending_incoming_loans(&game, "team2", ""), 1);
    assert_eq!(pending_incoming_purchases(&game, "team2", ""), 0);
    assert!(signing_place_refusal(&game, "team2", "").is_none());

    register_seniors(&mut game, "team2", 1);

    assert!(signing_place_refusal(&game, "team2", "").is_some());
    let loaned = game.players[0].id.clone();
    assert!(
        signing_place_refusal(&game, "team2", &loaned).is_none(),
        "the loan registering does not block itself"
    );
}

/// Given a club with twenty-eight seniors, a purchase and a loan both agreed and waiting
/// When the gate counts its places
/// Then the two promises are counted together: there is no room for a third deal.
#[test]
fn a_pending_purchase_and_a_pending_loan_are_counted_together() {
    use super::registration::signing_place_refusal;

    let mut game = acceptance_game("team1", false);
    register_seniors(&mut game, "team2", 28);
    promise_the_purchase(&mut game);
    promise_the_loan(&mut game);

    assert!(signing_place_refusal(&game, "team2", "").is_some());
}

/// Given a club with twenty-eight seniors and one of its own seniors out on loan elsewhere
/// When the gate counts its places
/// Then the loanee's place is reserved: the club may fill only the place it actually has, and the
/// loan ending is not blocked by the place it is resuming.
#[test]
fn a_senior_out_on_loan_reserves_the_place_he_returns_to() {
    use super::registration::{loaned_out_seniors, signing_place_refusal};

    let mut game = acceptance_game("team1", false);
    register_seniors(&mut game, "team2", 28);
    let loanee = lend_a_senior_out(&mut game, "team2", "team1");
    assert_eq!(loaned_out_seniors(&game, "team2", ""), 1);

    // Twenty-eight registered, one away: two free places, one of them reserved for his return.
    assert_eq!(
        crate::contracts::squad_capacity(&game, "team2").free_places,
        2,
        "the fixture has two places free"
    );
    assert!(
        signing_place_refusal(&game, "team2", "").is_none(),
        "one place is still free after the return"
    );

    register_seniors(&mut game, "team2", 1);

    assert!(
        signing_place_refusal(&game, "team2", "").is_some(),
        "the only remaining place belongs to the player coming back"
    );
    assert!(
        signing_place_refusal(&game, "team2", &loanee).is_none(),
        "his own return takes the place it reserved, it does not need a second"
    );
}

/// A senior of `parent` playing at `borrower` on an active loan; returns his id.
fn lend_a_senior_out(game: &mut Game, parent: &str, borrower: &str) -> String {
    use domain::player::{ActiveLoan, SquadRole};

    let mut player = Player::new(
        format!("{parent}-loanee"),
        "Loaned".into(),
        "Player".into(),
        "1999-01-01".into(),
        "England".into(),
        Position::Defender,
        sample_attributes(),
    );
    player.team_id = Some(borrower.to_string());
    player.squad_role = SquadRole::Senior;
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
    });
    let id = player.id.clone();
    game.players.push(player);
    id
}

/// The registration gate as it was written before it became one pass: the three counts of places
/// promised, and the capacity of the club's list, each read in a pass of its own.
fn the_gate_as_four_passes(game: &Game, team_id: &str, incoming: &str) -> Option<String> {
    use super::registration::{
        loaned_out_seniors, pending_incoming_loans, pending_incoming_purchases,
    };
    let promised = pending_incoming_purchases(game, team_id, incoming)
        + pending_incoming_loans(game, team_id, incoming)
        + loaned_out_seniors(game, team_id, incoming);
    let capacity = crate::contracts::squad_capacity(game, team_id);
    (capacity.free_places <= promised).then(|| capacity.full_error())
}

/// Given worlds of players scattered over four clubs and free agents, with every way a player can
///       hold or promise a place (a registered senior, a youth, a retired player, a purchase or a
///       loan waiting to register in every state, a senior out on loan), at club sizes either side
///       of the senior limit
/// When the one-pass registration gate and the four-pass original are asked for every club, with
///      and without an incoming player excepted
/// Then they give the same answer, error text included, every time.
#[test]
fn the_one_pass_registration_gate_agrees_with_the_four_pass_original() {
    use domain::player::{ActiveLoan, LoanOfferStatus, SquadRole};

    let clubs = ["team1", "team2", "team3", "team4"];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move |bound: u64| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % bound
    };
    let (mut refused, mut allowed, mut exception_decided) = (0, 0, 0);
    let mut unexcepted = std::collections::HashMap::new();
    for world in 0..12u64 {
        let mut game = acceptance_game("team1", false);
        game.players.truncate(1);
        game.players[0].team_id = None;
        for (index, club) in clubs.iter().enumerate() {
            // Sizes straddling the limit of 30: some clubs full, some a place short, some roomy.
            register_seniors(
                &mut game,
                club,
                24 + ((world + index as u64 * 3) % 9) as usize,
            );
        }
        for n in 0..60u64 {
            let mut player = Player::new(
                format!("extra-{world}-{n}"),
                "X".into(),
                "Y".into(),
                "1999-01-01".into(),
                "England".into(),
                Position::Midfielder,
                sample_attributes(),
            );
            // Half of them belong to no club; the rest spread over the four.
            player.team_id = match next(10) {
                0..=4 => None,
                k => Some(clubs[(k - 5) as usize % 4].to_string()),
            };
            player.squad_role = if next(3) == 0 {
                SquadRole::Youth
            } else {
                SquadRole::Senior
            };
            player.retired = next(4) == 0;
            if next(3) == 0 {
                player.active_loan = Some(ActiveLoan {
                    parent_team_id: clubs[next(4) as usize].to_string(),
                    loan_team_id: clubs[next(4) as usize].to_string(),
                    loan_team_name: None,
                    start_date: "2026-07-01".to_string(),
                    end_date: "2027-06-30".to_string(),
                    wage_contribution_pct: 100,
                    buy_option_fee: None,
                    loan_start_minutes: 0,
                    loan_start_appearances: 0,
                    development_reported_minutes: 0,
                    development_reported_appearances: 0,
                });
            }
            // Most players promise nothing; one in six holds an offer or two in some state.
            for _ in 0..(if next(6) == 0 { 1 + next(2) } else { 0 }) {
                let status = match next(3) {
                    0 => TransferOfferStatus::Pending,
                    1 => TransferOfferStatus::PendingRegistration,
                    _ => TransferOfferStatus::Rejected,
                };
                super::bids::upsert_transfer_offer(
                    &mut player,
                    clubs[next(4) as usize],
                    500_000,
                    status,
                    "2026-01-12",
                    None,
                    1,
                    None,
                    None,
                );
            }
            // Most players promise nothing; one in six holds an offer or two in some state.
            for _ in 0..(if next(6) == 0 { 1 + next(2) } else { 0 }) {
                let status = match next(3) {
                    0 => LoanOfferStatus::Pending,
                    1 => LoanOfferStatus::PendingRegistration,
                    _ => LoanOfferStatus::Rejected,
                };
                super::loans::upsert_loan_offer(
                    &mut player,
                    clubs[next(4) as usize],
                    clubs[next(4) as usize],
                    "2026-01-12",
                    "2026-12-20",
                    60,
                    None,
                    status,
                    "2026-01-12",
                );
            }
            game.players.push(player);
        }
        // Put each club exactly at the edge: as many places left as places promised, so that
        // excepting any one promise (or not) decides whether the club is full.
        for club in clubs {
            use super::registration::{
                loaned_out_seniors, pending_incoming_loans, pending_incoming_purchases,
            };
            let promised = pending_incoming_purchases(&game, club, "")
                + pending_incoming_loans(&game, club, "")
                + loaned_out_seniors(&game, club, "");
            let capacity = crate::contracts::squad_capacity(&game, club);
            let target = capacity.senior_limit.saturating_sub(promised);
            let mut registered = capacity.registered_seniors;
            for player in game.players.iter_mut().filter(|p| {
                p.team_id.as_deref() == Some(club)
                    && !p.retired
                    && p.squad_role == domain::player::SquadRole::Senior
            }) {
                if registered <= target {
                    break;
                }
                player.squad_role = domain::player::SquadRole::Youth;
                registered -= 1;
            }
            for index in registered..target {
                let mut filler = game.players[0].clone();
                filler.id = format!("{club}-edge-{world}-{index}");
                filler.team_id = Some(club.to_string());
                filler.squad_role = domain::player::SquadRole::Senior;
                filler.retired = false;
                filler.active_loan = None;
                filler.transfer_offers.clear();
                filler.loan_offers.clear();
                game.players.push(filler);
            }
        }
        // Every player in turn is the one excepted, so each reservation is excepted somewhere.
        let ids: Vec<String> = std::iter::once(String::new())
            .chain(game.players.iter().map(|p| p.id.clone()))
            .collect();
        for club in clubs {
            for incoming in &ids {
                let fast = super::registration::signing_place_refusal(&game, club, incoming);
                let original = the_gate_as_four_passes(&game, club, incoming);
                assert_eq!(
                    fast, original,
                    "world {world}, {club}, excepting {incoming:?}"
                );
                // Cases where the exception is what decides the answer.
                if incoming.is_empty() {
                    unexcepted.insert((club, world), fast.is_some());
                } else if unexcepted.get(&(club, world)) != Some(&fast.is_some()) {
                    exception_decided += 1;
                }
                if fast.is_some() {
                    refused += 1
                } else {
                    allowed += 1
                }
            }
        }
    }
    assert!(
        exception_decided > 10,
        "the worlds must include cases the excepted player decides: {exception_decided}"
    );
    assert!(
        refused > 50 && allowed > 50,
        "the worlds must exercise both answers: {refused} refused, {allowed} allowed"
    );
}
