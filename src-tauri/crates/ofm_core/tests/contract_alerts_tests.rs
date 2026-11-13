//! Run F1, anomaly 5. An expiring contract is an alert the manager can deal with. Once dealt with
//! it stops interrupting the advance, until the contract reaches its next stage or is replaced.
//!
//! Before: `key_contract_risk` / `contract_wage_risk` were raised every day from a year out until
//! the contract was renewed or released; answering the player's message, opening renewal talks or
//! choosing "continue anyway" changed nothing, and "continue anyway" in the day-by-day advance was
//! asked again the next morning.

use chrono::{NaiveDate, TimeZone, Utc};
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position};
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::contracts::{
    RenewalOffer, acknowledge_user_team_contract_warnings, contract_needs_attention,
    propose_renewal, set_contract_exit_intent,
};
use ofm_core::game::Game;
use ofm_core::player_events;

const PLAYER: &str = "player-1";

fn attrs() -> PlayerAttributes {
    PlayerAttributes {
        pace: 60,
        stamina: 60,
        strength: 60,
        agility: 60,
        passing: 60,
        shooting: 60,
        tackling: 60,
        dribbling: 60,
        defending: 60,
        positioning: 60,
        vision: 60,
        decisions: 60,
        composure: 60,
        aggression: 60,
        teamwork: 60,
        leadership: 60,
        handling: 30,
        reflexes: 30,
        aerial: 60,
    }
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn set_today(game: &mut Game, date: NaiveDate) {
    game.clock.current_date = Utc
        .with_ymd_and_hms(
            date.year_ce().1 as i32,
            date.month0() + 1,
            date.day0() + 1,
            12,
            0,
            0,
        )
        .unwrap();
}

use chrono::Datelike;

/// The user's club with one player whose contract ends on 2027-06-30.
fn game_on(date: NaiveDate) -> Game {
    let mut manager = Manager::new(
        "manager-1".to_string(),
        "Jane".to_string(),
        "Doe".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team-1".to_string());
    let mut team = Team::new(
        "team-1".to_string(),
        "Alpha FC".to_string(),
        "ALP".to_string(),
        "England".to_string(),
        "London".to_string(),
        "Ground".to_string(),
        30_000,
    );
    team.manager_id = Some("manager-1".to_string());
    team.wage_budget = 500_000;
    let mut player = Player::new(
        PLAYER.to_string(),
        "J. Smith".to_string(),
        "John Smith".to_string(),
        "2000-01-01".to_string(),
        "England".to_string(),
        Position::Forward,
        attrs(),
    );
    player.team_id = Some("team-1".to_string());
    player.stage_contract_end(Some("2027-06-30".to_string()));
    player.stage_wage(12_000);
    player.market_value = 350_000;
    let mut game = Game::new(
        GameClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()),
        manager,
        vec![team],
        vec![player],
        vec![],
        vec![],
    );
    set_today(&mut game, date);
    game
}

fn needs_attention(game: &Game) -> bool {
    contract_needs_attention(
        game.players.iter().find(|p| p.id == PLAYER).unwrap(),
        game.clock.current_date.date_naive(),
    )
}

/// Given a contract with 14 months left, then 6 months, then 3 months, then 2 weeks
/// When the alert is evaluated at each point
/// Then it is silent a year out (the first message comes at six months) and raised for the last
/// three stages.
#[test]
fn a_contract_is_an_alert_only_inside_its_last_six_months() {
    let mut game = game_on(day(2026, 4, 30));
    assert!(!needs_attention(&game), "14 months left is not an alert");
    set_today(&mut game, day(2026, 8, 1));
    assert!(
        !needs_attention(&game),
        "11 months left (twelve-month stage) is not an alert"
    );
    set_today(&mut game, day(2027, 1, 10));
    assert!(needs_attention(&game), "six-month stage");
    set_today(&mut game, day(2027, 4, 15));
    assert!(needs_attention(&game), "three-month stage");
    set_today(&mut game, day(2027, 6, 15));
    assert!(needs_attention(&game), "final weeks");
}

/// Given an alert at the six-month stage that the manager has dealt with
/// When days pass inside that stage, then the contract reaches the next stage
/// Then it stays quiet for the rest of the stage and comes back once at the next.
#[test]
fn a_handled_alert_stays_quiet_for_its_stage_and_returns_at_the_next() {
    let mut game = game_on(day(2027, 1, 10));
    assert!(needs_attention(&game));

    assert_eq!(acknowledge_user_team_contract_warnings(&mut game), 1);

    for date in [day(2027, 1, 11), day(2027, 2, 15), day(2027, 3, 30)] {
        set_today(&mut game, date);
        assert!(!needs_attention(&game), "still quiet on {date}");
    }
    set_today(&mut game, day(2027, 4, 15));
    assert!(
        needs_attention(&game),
        "the three-month stage is a new warning"
    );
    assert_eq!(acknowledge_user_team_contract_warnings(&mut game), 1);
    set_today(&mut game, day(2027, 5, 10));
    assert!(!needs_attention(&game));
    set_today(&mut game, day(2027, 6, 10));
    assert!(
        needs_attention(&game),
        "the final weeks are a new warning again"
    );
}

/// Given an alert dealt with on the old contract
/// When the contract is renewed to a later end
/// Then the new contract's own warnings are not silenced by the old one.
#[test]
fn a_renewed_contract_warns_afresh() {
    let mut game = game_on(day(2027, 6, 10));
    assert!(needs_attention(&game));
    acknowledge_user_team_contract_warnings(&mut game);
    assert!(!needs_attention(&game));

    // Renewed for a year; seven months later the new contract is six months from its end.
    game.players[0].stage_contract_end(Some("2028-06-30".to_string()));
    set_today(&mut game, day(2028, 1, 10));

    assert!(
        needs_attention(&game),
        "a different contract is a different warning"
    );
}

/// Given an alert and a manager who answers the player's own concern message
/// When he picks any answer
/// Then the alert for that stage is dealt with.
#[test]
fn answering_the_players_concern_message_deals_with_the_alert() {
    for option in ["reassure", "noncommittal"] {
        let mut game = game_on(day(2027, 1, 10));
        player_events::generate_contract_concern_messages(&mut game, false);
        let message = game
            .messages
            .iter()
            .find(|m| m.id.starts_with("contract_concern_"))
            .expect("the six-month message was sent")
            .clone();
        let action = message.actions[0].id.clone();
        assert!(needs_attention(&game));

        player_events::apply_player_response(&mut game, &message.id, &action, option)
            .expect("the answer is applied");

        assert!(!needs_attention(&game), "answered with {option}");
    }
}

/// Given an alert
/// When the manager opens renewal talks, even ones that fail
/// Then the alert is dealt with; and giving up on the player (let expire) also ends it.
#[test]
fn opening_renewal_talks_or_letting_him_go_deals_with_the_alert() {
    let mut talks = game_on(day(2027, 1, 10));
    propose_renewal(
        &mut talks,
        PLAYER,
        RenewalOffer {
            weekly_wage: 5_000,
            contract_years: 2,
        },
    )
    .expect("the offer is processed");
    assert!(!needs_attention(&talks));

    let mut letting_go = game_on(day(2027, 1, 10));
    assert!(needs_attention(&letting_go));
    set_contract_exit_intent(&mut letting_go, PLAYER, None).unwrap();
    assert!(!needs_attention(&letting_go));
}
