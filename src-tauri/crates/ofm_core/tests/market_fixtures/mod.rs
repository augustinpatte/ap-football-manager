//! Shared fixtures for the market integration tests: a user club, a bigger and a smaller rival,
//! one player, and squads deep enough that a departure never trips the squad floor.

use chrono::{TimeZone, Utc};
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position, TransferOffer, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;

pub const PLAYER_ID: &str = "prospect-1";
pub const USER_REPUTATION: u32 = 100;

pub fn attrs() -> PlayerAttributes {
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

pub fn player(date_of_birth: &str) -> Player {
    let mut player = Player::new(
        PLAYER_ID.to_string(),
        "P. Prospect".to_string(),
        "Pat Prospect".to_string(),
        date_of_birth.to_string(),
        "England".to_string(),
        Position::Forward,
        attrs(),
    );
    player.team_id = None;
    player.ovr = 60;
    player.potential = 75;
    player.market_value = 1_000_000;
    player.morale = 70;
    player
}

pub fn team(id: &str, reputation: u32) -> Team {
    let mut team = Team::new(
        id.to_string(),
        format!("{id} FC"),
        id.chars().take(3).collect(),
        "England".to_string(),
        "London".to_string(),
        format!("{id} Ground"),
        30_000,
    );
    team.reputation = reputation;
    team.finance = 20_000_000;
    team.transfer_budget = 10_000_000;
    team.wage_budget = 5_000_000;
    team
}

pub fn game_with(player: Player) -> Game {
    let mut manager = Manager::new(
        "manager-1".to_string(),
        "Jane".to_string(),
        "Doe".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team-1".to_string());
    let mut user = team("team-1", USER_REPUTATION);
    user.manager_id = Some("manager-1".to_string());
    let mut game = Game::new(
        GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()),
        manager,
        vec![user, team("big-club", 400), team("small-club", 90)],
        vec![player],
        vec![],
        vec![],
    );
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    give_squad_depth(&mut game);
    game
}

/// Sixteen inert seniors per club so one departure never trips the squad floor.
pub fn give_squad_depth(game: &mut Game) {
    let team_ids: Vec<String> = game.teams.iter().map(|team| team.id.clone()).collect();
    for team_id in team_ids {
        for ((group, _), count) in ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
            .into_iter()
            .zip([3, 5, 5, 3])
        {
            for index in 0..count {
                let id = format!("depth-{team_id}-{group:?}-{index}");
                let mut depth = Player::new(
                    id.clone(),
                    id.clone(),
                    id,
                    "1996-01-01".to_string(),
                    "England".to_string(),
                    group.clone(),
                    attrs(),
                );
                depth.team_id = Some(team_id.clone());
                depth.stage_contract_end(Some("2031-06-30".to_string()));
                depth.stage_wage(0);
                depth.market_value = 0;
                game.players.push(depth);
            }
        }
    }
}

pub fn set_date(game: &mut Game, year: i32, month: u32, day: u32) {
    game.clock.current_date = Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap();
}

pub fn prospect(game: &Game) -> &Player {
    game.players.iter().find(|p| p.id == PLAYER_ID).unwrap()
}

/// A pending offer on the fixture player, dated `date`, from club `from`.
pub fn pending_offer(game: &mut Game, id: &str, from: &str, fee: u64, date: &str) {
    game.players
        .iter_mut()
        .find(|p| p.id == PLAYER_ID)
        .unwrap()
        .transfer_offers
        .push(TransferOffer {
            id: id.to_string(),
            from_team_id: from.to_string(),
            fee,
            wage_offered: 0,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            status: TransferOfferStatus::Pending,
            date: date.to_string(),
            registration_date: None,
            closed_on: None,
            registration_failure_reason: None,
            terms: Default::default(),
        });
}
