//! The free-agent market a player opens must be made of players a club could really sign.
//!
//! Run F1, anomaly 1. Career test with Schalke 04: free agents "Adam Parker", "Jack Johnson",
//! "Igor Ribeiro" and others appeared rated 79-82, valued in the millions, with names that do
//! not match their nationality. They were not part of the world: `world_cup::prepare_national_squads`
//! made them up to fill thin national squads and left them in `game.players` as ordinary free
//! agents.

use chrono::{TimeZone, Utc};
use domain::manager::Manager;
use domain::player::Player;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::generator::{self, definitions::DefinitionSources};

fn world_game(seed: u64) -> Game {
    let (teams, players, staff) =
        generator::generate_world_seeded(seed, &DefinitionSources::embedded_only());
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr".to_string(),
        "A".to_string(),
        "B".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire(teams[0].id.clone());
    Game::new(clock, manager, teams, players, staff, vec![])
}

fn free_agents(game: &Game) -> Vec<&Player> {
    game.players
        .iter()
        .filter(|player| player.team_id.is_none() && !player.retired)
        .collect()
}

/// Several seeds: the defects are about the shape of a distribution, so one seed can miss them.
const SEEDS: [u64; 4] = [20260820, 1, 7, 9001];

fn club_ovrs(game: &Game) -> Vec<u8> {
    let mut ovrs: Vec<u8> = game
        .players
        .iter()
        .filter(|player| player.team_id.is_some())
        .map(|player| player.ovr)
        .collect();
    ovrs.sort_unstable();
    ovrs
}

fn at(sorted: &[u8], percent: usize) -> u8 {
    sorted[(sorted.len() - 1) * percent / 100]
}

/// Given worlds generated from several seeds
/// When the opening free agents are compared with the players the clubs hold
/// Then no free agent is better than three quarters of the clubs' players (the ceiling the
/// generator draws under), none is a star, and the unemployed are not better than the employed.
#[test]
fn the_opening_free_agents_are_not_better_than_the_players_clubs_hold() {
    for seed in SEEDS {
        let game = world_game(seed);
        let clubs = club_ovrs(&game);
        let mut agents: Vec<u8> = free_agents(&game).iter().map(|p| p.ovr).collect();
        agents.sort_unstable();
        assert!(
            !agents.is_empty(),
            "seed {seed}: a world opens with free agents"
        );

        assert!(
            *agents.last().unwrap() <= at(&clubs, 75),
            "seed {seed}: best free agent {} is above the clubs' 75th percentile {}",
            agents.last().unwrap(),
            at(&clubs, 75)
        );
        assert!(
            at(&agents, 50) <= at(&clubs, 50),
            "seed {seed}: median free agent {} above median club player {}",
            at(&agents, 50),
            at(&clubs, 50)
        );
        let stars = agents.iter().filter(|ovr| **ovr >= at(&clubs, 90)).count();
        assert_eq!(
            stars, 0,
            "seed {seed}: {stars} free agents rated like the clubs' best"
        );
    }
}

/// Given a World Cup scheduled in a generated world, whose thin national squads are completed
/// with made-up players
/// When the free-agent market is read before and after
/// Then the made-up players exist (the squads are full) but none of them is on the market.
#[test]
fn national_squad_fillers_never_reach_the_free_agent_market() {
    let mut game = world_game(20260820);
    let market_before: Vec<String> = game
        .players
        .iter()
        .filter(|player| player.is_on_free_market())
        .map(|player| player.id.clone())
        .collect();
    let players_before = game.players.len();

    let kickoff = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    ofm_core::world_cup::schedule_world_cup(&mut game, kickoff, &ofm_core::world_cup::FORMAT_16);

    let fillers: Vec<&Player> = game
        .players
        .iter()
        .filter(|player| player.is_national_pool_filler())
        .collect();
    assert!(
        game.players.len() > players_before && !fillers.is_empty(),
        "the squads were completed with made-up players"
    );
    assert!(fillers.iter().all(|p| p.team_id.is_none()));
    let market_after: Vec<String> = game
        .players
        .iter()
        .filter(|player| player.is_on_free_market())
        .map(|player| player.id.clone())
        .collect();
    assert_eq!(
        market_before, market_after,
        "the market is exactly what it was"
    );
}

/// Given a save written before made-up players carried a marker (a uuid id and the generator's
/// shirt number), next to a genuine free agent who was released by his club
/// When the free-agent list is read
/// Then the first is out of it and the second is in it.
#[test]
fn an_older_save_keeps_its_released_players_and_hides_its_fillers() {
    let mut game = world_game(1);
    let template = free_agents(&game)[0].clone();

    let mut legacy_filler = template.clone();
    legacy_filler.id = "6f9619ff-8b86-d011-b42d-00c04fc964ff".to_string();
    legacy_filler.jersey_number = Some(9);
    legacy_filler.ovr = 80;

    let mut released = template.clone();
    released.id = "released-by-a-club".to_string();
    released.jersey_number = Some(9);
    released
        .record_movement(domain::player::PlayerMovementEntry::new(
            "2026-06-30",
            domain::player::PlayerMovementKind::Released,
        ))
        .expect("a release is recordable");

    game.players.push(legacy_filler.clone());
    game.players.push(released.clone());
    let listed: Vec<&str> = game
        .players
        .iter()
        .filter(|player| player.is_on_free_market())
        .map(|player| player.id.as_str())
        .collect();

    assert!(!listed.contains(&legacy_filler.id.as_str()));
    assert!(listed.contains(&released.id.as_str()));
}
