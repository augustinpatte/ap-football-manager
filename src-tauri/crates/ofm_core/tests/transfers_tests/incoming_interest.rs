//! REALISM3: who approaches the user's best players, scenario by scenario.
//!
//! The user's club is passive in these tests: nothing answers an offer, so every one is counted
//! where it arrives. Each plays the production sweep for a few days; nothing here is random.

use super::realism::{club, only_buyer_funded, set_days_left, world};
use super::*;

const STAR: &str = "star";

/// A player of the user's club with the given rating, age and value: healthy, unlisted, happy and
/// on a long contract, so no bargain signal applies to him.
fn user_player(id: &str, ovr: u8, born: &str, value: u64) -> Player {
    let mut player = make_player(id);
    player.team_id = Some("team-1".to_string());
    player.ovr = ovr;
    player.potential = ovr;
    player.market_value = value;
    player.transfer_listed = false;
    player.morale = 85;
    player.date_of_birth = born.to_string();
    player.stage_wage(20_000);
    player.stage_contract_end(Some("2031-06-30".to_string()));
    player
}

/// The clubs that opened an approach for `id` over `days` days of sweeps.
fn approaches(game: &mut Game, id: &str, days: u32) -> Vec<String> {
    for day in 0..days {
        set_days_left(game, i64::from(20 - day));
        evaluate_transfer_market(game);
        game.clock.current_date += chrono::Duration::days(1);
    }
    let player = game
        .players
        .iter()
        .find(|p| p.id == id)
        .expect("the player");
    let mut from: Vec<String> = player
        .transfer_offers
        .iter()
        .map(|offer| offer.from_team_id.clone())
        .collect();
    from.sort();
    from.dedup();
    from
}

fn game_with(buyers: Vec<Team>, players: Vec<Player>, funded: &str) -> Game {
    let mut game = world(buyers, players);
    only_buyer_funded(&mut game, funded);
    game
}

/// Given a user's star who is not listed, not near the end of his contract and not unhappy, and a
/// rich club of stature that is clearly better with him,
/// When the market sweeps for a few days,
/// Then that club approaches him: a bargain is not the only reason to bid.
#[test]
fn a_healthy_unlisted_star_is_approached_by_a_club_that_needs_and_affords_him() {
    let mut game = game_with(
        vec![club("rich", "England", 900, 90_000_000)],
        vec![user_player(STAR, 92, "1999-01-01", 12_000_000)],
        "rich",
    );

    assert_eq!(approaches(&mut game, STAR, 5), vec!["rich".to_string()]);
}

/// Given the same star but a buyer whose transfer budget is a fraction of his value,
/// When the market sweeps,
/// Then nobody approaches: being visible is not being affordable.
#[test]
fn visibility_does_not_make_a_star_affordable() {
    let mut game = game_with(
        vec![club("poor", "England", 900, 1_000_000)],
        vec![user_player(STAR, 92, "1999-01-01", 12_000_000)],
        "poor",
    );

    assert!(approaches(&mut game, STAR, 5).is_empty());
}

/// Given an ordinary player of the user's club and a rich club with a need,
/// When the market sweeps,
/// Then nobody approaches him: with no realistic buyer there is no interest to invent.
#[test]
fn an_ordinary_player_draws_no_invented_interest() {
    let mut game = game_with(
        vec![club("rich", "England", 900, 90_000_000)],
        vec![
            user_player(STAR, 92, "1999-01-01", 12_000_000),
            user_player("ordinary", 58, "1999-01-01", 600_000),
        ],
        "rich",
    );

    let approaches = approaches(&mut game, "ordinary", 5);

    assert!(approaches.is_empty(), "{approaches:?}");
}

/// Given a star at the end of his career,
/// When the market sweeps,
/// Then he is not approached: a buyer looks past a player who has no resale.
#[test]
fn a_star_at_the_end_of_his_career_is_not_approached() {
    let mut game = game_with(
        vec![club("rich", "England", 900, 90_000_000)],
        vec![user_player(STAR, 92, "1990-01-01", 12_000_000)],
        "rich",
    );

    assert!(approaches(&mut game, STAR, 5).is_empty());
}

/// Given three rich clubs that all want the star,
/// When the market sweeps for a day,
/// Then at most one new club approaches him that day: the inbox throttle still holds.
#[test]
fn the_inbox_throttle_still_limits_new_approaches_per_player_per_day() {
    let mut game = game_with(
        vec![
            club("rich-a", "England", 900, 90_000_000),
            club("rich-b", "England", 900, 90_000_000),
            club("rich-c", "England", 900, 90_000_000),
        ],
        vec![user_player(STAR, 92, "1999-01-01", 12_000_000)],
        "rich-a",
    );
    for team in &mut game.teams {
        if team.id.starts_with("rich") {
            team.finance = 90_000_000;
            team.transfer_budget = 90_000_000;
        }
    }

    let from = approaches(&mut game, STAR, 1);

    assert_eq!(from.len(), 1, "{from:?}");
}

/// Given a star and a rich club that already has four forwards better than him,
/// When the market sweeps,
/// Then it does not approach: the buyer must make a sporting case, as for any AI club's player.
#[test]
fn a_club_that_is_already_better_in_his_position_does_not_approach() {
    let mut own: Vec<Player> = (0..4)
        .map(|n| {
            let mut forward = user_player(&format!("own-{n}"), 97, "1999-01-01", 90_000_000);
            forward.team_id = Some("rich".to_string());
            forward
        })
        .collect();
    own.push(user_player(STAR, 92, "1999-01-01", 12_000_000));
    let mut game = game_with(vec![club("rich", "England", 900, 90_000_000)], own, "rich");

    assert!(approaches(&mut game, STAR, 5).is_empty());
}
