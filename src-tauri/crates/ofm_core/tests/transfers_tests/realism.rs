//! Mission 10: the market's realism, scenario by scenario. Each test builds a small world and
//! plays one day (or a few) of the production sweep; nothing here is random.

use super::*;
use ofm_core::transfers::{LoanOfferDecision, TransferNegotiationDecision};

/// A club of `country`, with cash and a wage budget that is never the obstacle unless a test makes
/// it one. Its squad is the inert depth `give_every_club_squad_depth` adds.
fn club(id: &str, country: &str, reputation: u32, cash: i64) -> Team {
    let mut team = make_ai_team(id, id, cash, cash);
    team.country = country.to_string();
    team.reputation = reputation;
    team.wage_budget = 500_000_000;
    team
}

/// A forward of the given rating who is for sale at `club`.
fn forward(id: &str, club: &str, ovr: u8, value: u64) -> Player {
    let mut player = make_player(id);
    player.team_id = Some(club.to_string());
    player.ovr = ovr;
    player.potential = ovr;
    player.market_value = value;
    player.transfer_listed = true;
    player.date_of_birth = "1999-01-01".to_string();
    player.stage_wage(10_000);
    player
}

/// The user's club and a seller of a worthless player, plus whatever clubs the test adds. Only the
/// clubs the test funds can buy: the rest have nothing to spend.
fn world(clubs: Vec<Team>, players: Vec<Player>) -> Game {
    let mut dummy = make_player("dummy");
    dummy.market_value = 0;
    dummy.transfer_listed = false;
    let mut game = make_game_with_player(dummy, vec![], 0, 0);
    game.teams[0].wage_budget = 500_000_000;
    game.teams[1].finance = 0;
    game.teams[1].transfer_budget = 0;
    game.teams.extend(clubs);
    game.players.extend(players);
    give_every_club_squad_depth(&mut game);
    // Their cover is the rating this module assumes: 60.
    for player in &mut game.players {
        if player.id.starts_with("depth-") {
            player.ovr = 60;
        }
    }
    attach_transfer_log_league(&mut game);
    game
}

fn set_days_left(game: &mut Game, days: i64) {
    let date = game.clock.current_date.date_naive() + chrono::Duration::days(days);
    game.season_context.transfer_window.days_remaining = Some(days);
    game.season_context.transfer_window.closes_on = Some(date.format("%Y-%m-%d").to_string());
    game.season_context.transfer_window.status = if days == 0 {
        TransferWindowStatus::DeadlineDay
    } else {
        TransferWindowStatus::Open
    };
}

fn owner(game: &Game, id: &str) -> Option<String> {
    game.players
        .iter()
        .find(|player| player.id == id)
        .and_then(|player| player.team_id.clone())
}

fn only_buyer_funded(game: &mut Game, buyer: &str) {
    for team in &mut game.teams {
        if team.id != buyer && team.id != "team-1" {
            team.finance = 0;
            team.transfer_budget = 0;
        }
    }
}

/// Leaves the named borrower as the only club with the wage room a loan needs. A loan does not
/// check a transfer budget, so `only_buyer_funded` alone leaves an incidental club in the market;
/// this keeps a scenario that asks what one borrower does from being won by another.
fn only_buyer_can_borrow(game: &mut Game, buyer: &str) {
    for team in &mut game.teams {
        if team.id != buyer && team.id != "team-1" {
            team.wage_budget = 0;
        }
    }
}

fn rate(player: &mut Player, rating: u8) {
    player.ovr = rating;
    player.potential = rating;
    player.attributes = PlayerAttributes {
        pace: rating,
        stamina: rating,
        strength: rating,
        agility: rating,
        passing: rating,
        shooting: rating,
        tackling: rating,
        dribbling: rating,
        defending: rating,
        positioning: rating,
        vision: rating,
        decisions: rating,
        composure: rating,
        aggression: rating,
        teamwork: rating,
        leadership: rating,
        handling: rating,
        reflexes: rating,
        aerial: rating,
    };
}

/// A. A big English club goes abroad for a clearly better player than anyone it could find at home.
#[test]
fn a_big_english_club_signs_a_clear_upgrade_in_france() {
    let mut game = world(
        vec![
            club("eng-big", "England", 900, 90_000_000),
            club("eng-seller", "England", 700, 0),
            club("fra-seller", "France", 800, 0),
        ],
        vec![
            forward("home-option", "eng-seller", 66, 6_000_000),
            forward("french-star", "fra-seller", 80, 12_000_000),
        ],
    );
    only_buyer_funded(&mut game, "eng-big");
    set_days_left(&mut game, 20);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "french-star").as_deref(), Some("eng-big"));
    // The French club then spent its fee at home: the border cost it nothing it could not afford,
    // and the English club did not also buy the lesser player.
    assert_ne!(owner(&game, "home-option").as_deref(), Some("eng-big"));
}

/// B. A small club keeps to its own market when the foreign option is only a little better, and
/// goes abroad when its own market has nothing and the foreign option is a great deal better:
/// geography weighs, it does not decide.
#[test]
fn a_small_french_club_prefers_a_local_solution_unless_the_foreign_one_is_far_better() {
    let build = |local_ovr: u8, foreign_ovr: u8| {
        let mut game = world(
            vec![
                club("fra-small", "France", 450, 30_000_000),
                club("fra-seller", "France", 450, 0),
                club("eng-seller", "England", 450, 0),
            ],
            vec![
                forward("local", "fra-seller", local_ovr, 2_000_000),
                forward("abroad", "eng-seller", foreign_ovr, 2_000_000),
            ],
        );
        only_buyer_funded(&mut game, "fra-small");
        set_days_left(&mut game, 1);
        evaluate_transfer_market(&mut game);
        game
    };
    let close = build(68, 70);
    assert_eq!(owner(&close, "local").as_deref(), Some("fra-small"));
    assert_eq!(owner(&close, "abroad").as_deref(), Some("eng-seller"));
    let far_better = build(61, 88);
    assert_eq!(owner(&far_better, "abroad").as_deref(), Some("fra-small"));
}

/// C. The first team player a club cannot replace costs more at J-2 than in mid-window.
#[test]
fn an_indispensable_player_is_dearer_two_days_before_the_close() {
    let make = |days: i64| {
        let mut star = make_player("star");
        star.team_id = Some("team-2".to_string());
        star.market_value = 4_000_000;
        star.ovr = 80;
        let mut game =
            make_game_with_player(star, vec!["star".to_string()], 100_000_000, 100_000_000);
        game.teams[0].wage_budget = 500_000_000;
        set_days_left(&mut game, days);
        game
    };
    // 1.45 x value: enough in mid-window (1.4), short of J-2 (1.4975).
    let fee = 1.45 * 4_000_000.0;
    let mut mid = make(20);
    let accepted = make_transfer_bid(&mut mid, "star", fee as u64).expect("bid");
    assert_eq!(accepted.decision, TransferNegotiationDecision::Accepted);
    let mut late = make(2);
    let countered = make_transfer_bid(&mut late, "star", fee as u64).expect("bid");
    assert_eq!(
        countered.decision,
        TransferNegotiationDecision::CounterOffer
    );
    assert!(countered.suggested_fee.unwrap() > fee as u64);
}

/// The fee of the one AI-to-AI transfer a sweep completed, if any.
fn completed_fee(game: &Game) -> Option<u64> {
    game.league
        .as_ref()
        .and_then(|league| league.transfer_log.first())
        .map(|entry| entry.fee)
}

fn surplus_world(days: i64, contract_end: &str) -> Game {
    let mut spare = forward("spare", "seller", 68, 4_000_000);
    spare.transfer_listed = false;
    spare.stage_contract_end(Some(contract_end.to_string()));
    spare.stage_wage(10_000);
    let mut game = world(
        vec![
            club("buyer", "England", 600, 90_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![spare],
    );
    // The seller already has three better forwards: he is beyond its plan, so surplus.
    for index in 0..5 {
        let mut better = forward(&format!("better-{index}"), "seller", 75, 0);
        better.transfer_listed = false;
        better.stage_wage(0);
        game.players.push(better);
    }
    only_buyer_funded(&mut game, "buyer");
    set_days_left(&mut game, days);
    game
}

/// D. A surplus player is offered for less at J-2 than in mid-window.
#[test]
fn a_surplus_player_is_cheaper_at_j_minus_2() {
    let mut mid = surplus_world(20, "2028-06-30");
    evaluate_transfer_market(&mut mid);
    let mut late = surplus_world(2, "2028-06-30");
    evaluate_transfer_market(&mut late);
    let (mid_fee, late_fee) = (completed_fee(&mid), completed_fee(&late));
    assert!(
        mid_fee.is_some() && late_fee.is_some(),
        "{mid_fee:?} {late_fee:?}"
    );
    assert!(
        late_fee.unwrap() + 400_000 < mid_fee.unwrap(),
        "{mid_fee:?} -> {late_fee:?}"
    );
}

/// E. A contract about to end is priced in: he goes for much less than one with years to run.
#[test]
fn a_player_in_the_last_weeks_of_his_contract_is_cheap_at_j_minus_2() {
    let mut long = surplus_world(2, "2028-06-30");
    evaluate_transfer_market(&mut long);
    let mut ending = surplus_world(2, "2026-08-25");
    evaluate_transfer_market(&mut ending);
    let (long_fee, ending_fee) = (
        completed_fee(&long).unwrap(),
        completed_fee(&ending).unwrap(),
    );
    assert!(
        ending_fee * 100 <= long_fee * 80,
        "{long_fee} vs {ending_fee}"
    );
}

/// F. A rich club with nobody to improve on buys nobody.
#[test]
fn a_club_with_no_need_forces_no_purchase() {
    let mut game = world(
        vec![
            club("rich", "England", 900, 900_000_000),
            club("seller", "England", 700, 0),
        ],
        vec![forward("average", "seller", 58, 1_000_000)],
    );
    // The rich club's forwards are all better than the one on offer.
    for player in &mut game.players {
        if player.team_id.as_deref() == Some("rich") {
            player.ovr = 75;
        }
    }
    only_buyer_funded(&mut game, "rich");
    set_days_left(&mut game, 0);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "average").as_deref(), Some("seller"));
    assert!(game.league.as_ref().unwrap().transfer_log.is_empty());
}

/// G. A club with a vacant position looks for a player for it, even on the last day.
#[test]
fn a_club_with_a_vacant_position_signs_for_it() {
    let mut keeper = make_player("keeper");
    keeper.position = Position::Goalkeeper;
    keeper.team_id = Some("seller".to_string());
    keeper.transfer_listed = true;
    keeper.ovr = 62;
    keeper.market_value = 1_000_000;
    keeper.date_of_birth = "1998-01-01".to_string();
    let mut game = world(
        vec![
            club("needy", "England", 600, 50_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![keeper],
    );
    // The needy club has lost its goalkeepers.
    game.players.retain(|player| {
        !(player.team_id.as_deref() == Some("needy") && player.position == Position::Goalkeeper)
    });
    only_buyer_funded(&mut game, "needy");
    set_days_left(&mut game, 0);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "keeper").as_deref(), Some("needy"));
}

/// Given a club with planned depth in every broad group, two elite goalkeepers and a weak attack
/// When the live AI market sweeps during the early window
/// Then Squad Intelligence's attacking need activates the club and it signs the available striker.
#[test]
fn a_weak_attack_enters_the_market_even_when_legacy_depth_is_complete() {
    let mut game = world(
        vec![
            // On 2026-08-01 this id is outside the early-window discretionary-shopping draw.
            // The club must therefore enter because of its shared squad need, not by chance.
            club("buyer", "England", 600, 50_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![forward("needed-striker", "seller", 82, 2_000_000)],
    );
    for player in &mut game.players {
        if player.team_id.as_deref() == Some("buyer") {
            match player.position.to_group_position() {
                Position::Goalkeeper => {
                    rate(player, if player.id.ends_with("-0") { 85 } else { 83 })
                }
                Position::Forward => rate(player, 45),
                _ => rate(player, 72),
            }
        } else if player.id == "needed-striker" {
            rate(player, 82);
        }
    }
    only_buyer_funded(&mut game, "buyer");
    set_days_left(&mut game, 60);

    let evaluated_before = market_buyer_evaluations();
    evaluate_transfer_market(&mut game);
    assert!(
        market_buyer_evaluations() > evaluated_before,
        "the critical attacking need activates the buyer in the early window"
    );

    // The next ordinary market day completes the same live pipeline: target discovery, ranking,
    // consent, financial checks and execution.
    set_days_left(&mut game, 40);
    evaluate_transfer_market(&mut game);

    assert_eq!(owner(&game, "needed-striker").as_deref(), Some("buyer"));
}

/// Given a club with weak centre-backs and eight defenders including a long-term injury
/// When an affordable centre-back is available through the production market
/// Then the club signs the player who improves its starting defence.
#[test]
fn an_injured_defender_does_not_block_a_needed_centre_back() {
    let mut target = forward("needed-centre-back", "seller", 80, 2_000_000);
    target.position = Position::CenterBack;
    target.natural_position = Position::CenterBack;
    rate(&mut target, 80);
    let mut game = world(
        vec![
            club("buyer", "England", 600, 50_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![target],
    );
    let team = game
        .teams
        .iter_mut()
        .find(|team| team.id == "buyer")
        .unwrap();
    team.formation = "4-4-2".to_string();
    let positions = [
        (Position::LeftBack, 72),
        (Position::LeftBack, 66),
        (Position::RightBack, 72),
        (Position::RightBack, 66),
        (Position::CenterBack, 58),
    ];
    for (player, (position, rating)) in game
        .players
        .iter_mut()
        .filter(|player| {
            player.team_id.as_deref() == Some("buyer") && player.position == Position::Defender
        })
        .zip(positions)
    {
        player.position = position.clone();
        player.natural_position = position;
        rate(player, rating);
    }
    for (id, rating, injured) in [
        ("centre-cover", 55, false),
        ("centre-reserve", 52, false),
        ("injured-defender", 70, true),
    ] {
        let mut player = forward(id, "buyer", rating, 0);
        player.transfer_listed = false;
        player.position = Position::CenterBack;
        player.natural_position = Position::CenterBack;
        rate(&mut player, rating);
        if injured {
            player.injury = Some(domain::player::Injury {
                name: "long injury".to_string(),
                days_remaining: 300,
            });
        }
        game.players.push(player);
    }
    only_buyer_funded(&mut game, "buyer");
    set_days_left(&mut game, 0);
    let mut without_injured_defender = game.clone();
    without_injured_defender
        .players
        .retain(|player| player.id != "injured-defender");
    evaluate_transfer_market(&mut without_injured_defender);
    assert_eq!(
        owner(&without_injured_defender, "needed-centre-back").as_deref(),
        Some("buyer"),
        "the same squad without the injured player must be able to buy the target"
    );
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "needed-centre-back").as_deref(), Some("buyer"));
}

fn full_back_market_world() -> Game {
    let mut target = forward("needed-left-back", "seller", 78, 2_000_000);
    target.position = Position::LeftBack;
    target.natural_position = Position::LeftBack;
    rate(&mut target, 78);
    let mut game = world(
        vec![
            club("buyer", "England", 600, 50_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![target],
    );
    game.teams
        .iter_mut()
        .find(|team| team.id == "buyer")
        .unwrap()
        .formation = "4-4-2".to_string();
    for player in game
        .players
        .iter_mut()
        .filter(|player| player.team_id.as_deref() == Some("buyer"))
    {
        if player.position == Position::Defender {
            player.position = Position::CenterBack;
            player.natural_position = Position::CenterBack;
        }
        rate(player, 80);
    }
    only_buyer_funded(&mut game, "buyer");
    set_days_left(&mut game, 0);
    game
}

/// Given five capable centre-backs but no natural full-back in a 4-4-2
/// When an affordable left-back is available in the production market
/// Then the buyer addresses the tactical gap despite its broad defensive depth.
#[test]
fn a_full_back_fills_a_tactical_gap_despite_centre_back_depth() {
    let mut game = full_back_market_world();

    evaluate_transfer_market(&mut game);

    assert_eq!(owner(&game, "needed-left-back").as_deref(), Some("buyer"));
}

/// Given eight centre-backs and a weak left-back slot
/// When a specialist is offered in the production market
/// Then broad defensive surplus does not veto the tactical improvement.
#[test]
fn a_full_back_can_improve_a_team_with_eight_centre_backs() {
    let mut game = full_back_market_world();
    for id in ["centre-six", "centre-seven", "centre-eight"] {
        let mut player = forward(id, "buyer", 80, 0);
        player.transfer_listed = false;
        player.position = Position::CenterBack;
        player.natural_position = Position::CenterBack;
        rate(&mut player, 80);
        game.players.push(player);
    }
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "needed-left-back").as_deref(), Some("buyer"));
}

/// Given a weak left-back slot and a suitable player trained to play there
/// When the player is offered in the production market
/// Then his alternate position can address the tactical need.
#[test]
fn an_alternate_position_can_address_a_starting_gap() {
    let mut game = full_back_market_world();
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == "needed-left-back")
        .unwrap();
    player.natural_position = Position::RightBack;
    player.alternate_positions.push(Position::LeftBack);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "needed-left-back").as_deref(), Some("buyer"));
}

/// Given a weak left-back slot, a covered right-back slot and a target whose only full-back
/// natural position is right-back
/// When the production market weighs him for his alternate left-back position
/// Then the alternate position alone can connect him to the starting need.
#[test]
fn an_alternate_position_addresses_a_starter_need_when_the_natural_slot_is_covered() {
    let mut game = full_back_market_world();
    // The right-back slot is covered, so his natural position is not a need. Only the alternate
    // left-back position can address the weak slot the side fields.
    let mut right_back = forward("strong-right-back", "buyer", 85, 0);
    right_back.transfer_listed = false;
    right_back.position = Position::RightBack;
    right_back.natural_position = Position::RightBack;
    rate(&mut right_back, 85);
    game.players.push(right_back);
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == "needed-left-back")
        .unwrap();
    player.natural_position = Position::RightBack;
    player.alternate_positions.push(Position::LeftBack);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "needed-left-back").as_deref(), Some("buyer"));
}

/// Given a weak left-back slot and a suitable player listed for loan
/// When the production market considers the loan
/// Then broad centre-back depth does not veto the tactical improvement.
#[test]
fn a_loan_can_address_a_starting_gap_despite_broad_depth() {
    let mut game = full_back_market_world();
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == "needed-left-back")
        .unwrap();
    player.transfer_listed = false;
    player.loan_listed = true;
    player.date_of_birth = "2006-01-01".to_string();
    player.potential = 80;
    player.stage_contract_end(Some("2028-06-30".to_string()));
    only_buyer_can_borrow(&mut game, "buyer");
    evaluate_transfer_market(&mut game);
    let player = game
        .players
        .iter()
        .find(|p| p.id == "needed-left-back")
        .unwrap();
    assert_eq!(
        player
            .active_loan
            .as_ref()
            .map(|loan| loan.loan_team_id.as_str()),
        Some("buyer")
    );
}

/// H. A club already over its wage budget buys nobody, however good the player.
#[test]
fn a_club_over_its_budget_refuses_a_purchase() {
    let mut game = world(
        vec![
            club("stretched", "England", 800, 50_000_000),
            club("seller", "England", 800, 0),
        ],
        vec![forward("bargain", "seller", 85, 2_000_000)],
    );
    let stretched = game.teams.iter_mut().find(|t| t.id == "stretched").unwrap();
    stretched.wage_budget = 1_000;
    let mut payroll = forward("payroll", "stretched", 60, 0);
    payroll.transfer_listed = false;
    payroll.stage_wage(100_000);
    game.players.push(payroll);
    only_buyer_funded(&mut game, "stretched");
    set_days_left(&mut game, 0);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "bargain").as_deref(), Some("seller"));
}

/// I. A young player who is not playing goes out on loan; another club borrows him; the wage is
/// shared; he comes back at the end; and the loan survives a save and reload.
#[test]
fn a_young_bench_player_goes_on_loan_and_comes_back() {
    let mut prospect = forward("prospect", "seller", 55, 500_000);
    prospect.transfer_listed = false;
    prospect.loan_listed = true;
    prospect.potential = 75;
    prospect.date_of_birth = "2006-01-01".to_string();
    prospect.stage_wage(20_000);
    prospect.stage_contract_end(Some("2028-06-30".to_string()));
    let mut game = world(
        vec![
            club("borrower", "England", 400, 20_000_000),
            club("seller", "England", 500, 0),
        ],
        vec![prospect],
    );
    // The borrower's forwards are weaker than the prospect.
    for player in &mut game.players {
        if player.team_id.as_deref() == Some("borrower") {
            player.ovr = 50;
        }
    }
    only_buyer_funded(&mut game, "borrower");
    set_days_left(&mut game, 20);
    evaluate_transfer_market(&mut game);

    let on_loan = game.players.iter().find(|p| p.id == "prospect").unwrap();
    let loan = on_loan
        .active_loan
        .as_ref()
        .expect("the prospect was borrowed");
    assert_eq!(loan.loan_team_id, "borrower");
    assert_eq!(loan.parent_team_id, "seller");
    assert!((50..=100).contains(&loan.wage_contribution_pct));

    // Save and reload: the loan is the same.
    let saved = serde_json::to_string(&game).expect("save");
    let mut reloaded: Game = serde_json::from_str(&saved).expect("reload");
    let after = reloaded
        .players
        .iter()
        .find(|p| p.id == "prospect")
        .unwrap();
    assert_eq!(after.active_loan, on_loan.active_loan);

    let end = chrono::NaiveDate::parse_from_str(&loan.end_date, "%Y-%m-%d").unwrap();
    reloaded.clock.current_date = Utc.from_utc_datetime(
        &(end + chrono::Duration::days(1))
            .and_hms_opt(12, 0, 0)
            .unwrap(),
    );
    process_loan_returns(&mut reloaded);
    let back = reloaded
        .players
        .iter()
        .find(|p| p.id == "prospect")
        .unwrap();
    assert!(back.active_loan.is_none());
    assert_eq!(back.team_id.as_deref(), Some("seller"));
}

/// I (refusal). The owner of a first-team player refuses a loan on cheap terms.
#[test]
fn the_owner_of_a_starter_refuses_a_cheap_loan() {
    let mut starter = make_player("starter");
    starter.team_id = Some("team-2".to_string());
    starter.loan_listed = true;
    starter.ovr = 75;
    starter.potential = 75;
    starter.date_of_birth = "1998-01-01".to_string();
    starter.stage_wage(10_000);
    let mut game =
        make_game_with_player(starter, vec!["starter".to_string()], 50_000_000, 50_000_000);
    game.teams[0].wage_budget = 500_000_000;
    set_days_left(&mut game, 20);
    let outcome = make_loan_offer(&mut game, "starter", "2027-01-01", 20, None).expect("offer");
    assert_ne!(outcome.decision, LoanOfferDecision::Accepted);
    assert!(
        game.players
            .iter()
            .find(|p| p.id == "starter")
            .unwrap()
            .active_loan
            .is_none()
    );
}

/// J. An international transfer survives a save and reload, with both clubs' money consistent.
#[test]
fn an_international_transfer_survives_a_save_and_reload() {
    let mut game = world(
        vec![
            club("eng-big", "England", 900, 90_000_000),
            club("fra-seller", "France", 800, 0),
        ],
        vec![forward("french-star", "fra-seller", 80, 12_000_000)],
    );
    only_buyer_funded(&mut game, "eng-big");
    set_days_left(&mut game, 5);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "french-star").as_deref(), Some("eng-big"));
    let fee = completed_fee(&game).expect("a fee was logged");
    let saved = serde_json::to_string(&game).expect("save");
    let reloaded: Game = serde_json::from_str(&saved).expect("reload");
    assert_eq!(owner(&reloaded, "french-star").as_deref(), Some("eng-big"));
    let cash = |g: &Game, id: &str| g.teams.iter().find(|t| t.id == id).unwrap().finance;
    assert_eq!(cash(&reloaded, "eng-big"), 90_000_000 - fee as i64);
    assert_eq!(cash(&reloaded, "fra-seller"), fee as i64);
}

/// K. With the window closed the AI market does nothing, and a bid is only scheduled.
#[test]
fn nothing_is_signed_with_the_window_closed() {
    let mut game = world(
        vec![
            club("buyer", "England", 600, 50_000_000),
            club("seller", "England", 600, 0),
        ],
        vec![forward("target", "seller", 80, 2_000_000)],
    );
    only_buyer_funded(&mut game, "buyer");
    game.season_context.transfer_window.status = TransferWindowStatus::Closed;
    game.season_context.transfer_window.days_remaining = None;
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "target").as_deref(), Some("seller"));
    assert!(game.league.as_ref().unwrap().transfer_log.is_empty());
}

/// L. Two clubs want the same player on the same day: one transfer, one new owner.
#[test]
fn two_competing_clubs_produce_a_single_transfer() {
    let mut game = world(
        vec![
            club("buyer-a", "England", 700, 50_000_000),
            club("buyer-b", "England", 700, 50_000_000),
            club("seller", "England", 700, 0),
        ],
        vec![forward("prize", "seller", 80, 3_000_000)],
    );
    set_days_left(&mut game, 1);
    evaluate_transfer_market(&mut game);
    let log = &game.league.as_ref().unwrap().transfer_log;
    assert_eq!(log.len(), 1);
    let new_owner = owner(&game, "prize").unwrap();
    assert!(new_owner == "buyer-a" || new_owner == "buyer-b");
    assert_eq!(log[0].to_team_id, new_owner);
    // A second sweep the same day cannot move him again to a third club.
    evaluate_transfer_market(&mut game);
    assert!(game.league.as_ref().unwrap().transfer_log.len() <= 2);
}

/// M12-A. A small club does not cross a border for a cheap player who barely improves its bench,
/// but a home player of the same level is fine.
#[test]
fn a_small_club_does_not_go_abroad_for_a_cheap_marginal_filler() {
    let build = |seller_country: &str| {
        let mut game = world(
            vec![
                club("fra-small", "France", 450, 30_000_000),
                club("seller", seller_country, 450, 0),
            ],
            vec![forward("filler", "seller", 62, 150_000)],
        );
        only_buyer_funded(&mut game, "fra-small");
        set_days_left(&mut game, 1);
        evaluate_transfer_market(&mut game);
        game
    };
    // Cheap, +2 on its first cover, and abroad: not worth the trip.
    assert_eq!(
        owner(&build("England"), "filler").as_deref(),
        Some("seller")
    );
    // The same player at home is signed.
    assert_eq!(
        owner(&build("France"), "filler").as_deref(),
        Some("fra-small")
    );
}

/// A goalkeeper of `ovr` for sale at `club`, rated on the same attributes as the depth players the
/// world gives every squad.
fn keeper(id: &str, club: &str, ovr: u8, value: u64) -> Player {
    let mut player = Player::new(
        id.to_string(),
        format!("{id}. Test"),
        format!("{id} Test"),
        "1999-01-01".to_string(),
        "England".to_string(),
        Position::Goalkeeper,
        default_attrs(),
    );
    player.team_id = Some(club.to_string());
    player.ovr = ovr;
    player.potential = ovr;
    player.market_value = value;
    player.transfer_listed = true;
    player.stage_wage(10_000);
    player
}

/// A funded buyer with `kept` senior keepers (85, then 82) and a keeper for sale elsewhere. The
/// buyer's keepers and the target are the same age, so the only difference between them is rating.
fn keeper_world(kept: usize, target_ovr: u8) -> Game {
    let mut game = world(
        vec![
            club("eng-buyer", "England", 900, 90_000_000),
            club("eng-seller", "England", 700, 0),
        ],
        vec![keeper("keeper-target", "eng-seller", target_ovr, 2_000_000)],
    );
    only_buyer_funded(&mut game, "eng-buyer");
    let mut buyer_keepers: Vec<String> = game
        .players
        .iter()
        .filter(|player| {
            player.team_id.as_deref() == Some("eng-buyer")
                && player.natural_position == Position::Goalkeeper
        })
        .map(|player| player.id.clone())
        .collect();
    buyer_keepers.sort();
    let ratings = [85_u8, 82, 79];
    for (rank, id) in buyer_keepers.iter().enumerate() {
        if rank >= kept {
            continue;
        }
        if let Some(player) = game.players.iter_mut().find(|player| &player.id == id) {
            player.ovr = ratings[rank];
            player.potential = ratings[rank];
            player.date_of_birth = "1999-01-01".to_string();
        }
    }
    let surplus: Vec<String> = buyer_keepers.into_iter().skip(kept).collect();
    game.players.retain(|player| !surplus.contains(&player.id));
    set_days_left(&mut game, 20);
    game
}

/// Given a club carrying two senior goalkeepers, when a third keeper who would displace neither is
/// offered, then it does not pay for him: a third keeper who never plays is not a signing, it is a
/// fee for a spectator.
#[test]
fn a_club_with_two_goalkeepers_does_not_pay_for_a_third_who_would_not_play() {
    let mut game = keeper_world(2, 60);
    evaluate_transfer_market(&mut game);
    assert_ne!(
        owner(&game, "keeper-target").as_deref(),
        Some("eng-buyer"),
        "a keeper rated below both seniors must not be bought as a third"
    );
}

/// Given the same club, when the keeper for sale is better than its number one, then it signs him:
/// a genuine first-team upgrade is still a signing.
#[test]
fn a_club_with_two_goalkeepers_still_buys_a_better_number_one() {
    let mut game = keeper_world(2, 88);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "keeper-target").as_deref(), Some("eng-buyer"));
}

/// Given a club with a single senior keeper, when a modest keeper is offered, then it signs him: a
/// club genuinely short of bodies still fills the hole.
#[test]
fn a_club_with_one_goalkeeper_still_signs_a_second() {
    let mut game = keeper_world(1, 60);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "keeper-target").as_deref(), Some("eng-buyer"));
}

/// A young keeper at `club`: `ovr` today, `potential` ahead, for sale at `value`.
fn young_keeper(id: &str, club: &str, ovr: u8, potential: u8, value: u64) -> Player {
    let mut player = keeper(id, club, ovr, value);
    player.potential = potential;
    player.date_of_birth = "2006-01-01".to_string();
    player
}

/// A funded buyer whose number one is 36 and will retire, with two young keepers behind him, and a
/// young keeper for sale elsewhere who would be the number one a season from now. `cash` is all the
/// buyer has. The seller also has a better prospect, so the target is cover to them, not their
/// future.
fn successor_world(cash: i64, value: u64) -> Game {
    let mut game = world(
        vec![
            club("eng-buyer", "England", 900, cash),
            club("eng-seller", "England", 700, 0),
        ],
        vec![young_keeper("keeper-target", "eng-seller", 60, 88, value)],
    );
    let mut rival = young_keeper("keeper-rival", "eng-seller", 62, 94, 50_000_000);
    rival.transfer_listed = false;
    game.players.push(rival);
    only_buyer_funded(&mut game, "eng-buyer");
    let mut buyer_keepers: Vec<String> = game
        .players
        .iter()
        .filter(|player| {
            player.team_id.as_deref() == Some("eng-buyer")
                && player.natural_position == Position::Goalkeeper
        })
        .map(|player| player.id.clone())
        .collect();
    buyer_keepers.sort();
    let profiles = [
        // The number one: 36 now, so the club plans without him in a season.
        (85_u8, "1990-01-01"),
        // Two young keepers who stay: the club is at the floor a season on, not below it.
        (60_u8, "2001-01-01"),
        (55_u8, "2002-01-01"),
    ];
    for (id, (ovr, dob)) in buyer_keepers.iter().zip(profiles) {
        if let Some(player) = game.players.iter_mut().find(|player| &player.id == id) {
            player.ovr = ovr;
            player.potential = ovr;
            player.date_of_birth = dob.to_string();
        }
    }
    // Deadline day: the seller will do business, so the only question left is the buyer's.
    set_days_left(&mut game, 0);
    game
}

/// Given a club at the floor a season on with little cash, when the young keeper who would be its
/// number one next season is offered for more than half of that cash, then it does not buy him: a
/// club that can already field a side must still find a future-only signing worth its fee.
#[test]
fn a_constrained_club_at_the_floor_does_not_pay_for_a_successor_who_cannot_play_yet() {
    let mut game = successor_world(2_200_000, 1_050_000);
    evaluate_transfer_market(&mut game);
    assert_ne!(
        owner(&game, "keeper-target").as_deref(),
        Some("eng-buyer"),
        "a bench keeper who only covers in a later season must be worth his fee"
    );
}

/// Given the same club with money to spare, when the same young keeper is offered on the same terms,
/// then it signs him: a genuine successor who is worth the money is still a succession signing.
#[test]
fn a_club_at_the_floor_still_signs_a_successor_who_is_worth_the_fee() {
    let mut game = successor_world(20_000_000, 1_050_000);
    evaluate_transfer_market(&mut game);
    assert_eq!(owner(&game, "keeper-target").as_deref(), Some("eng-buyer"));
}
