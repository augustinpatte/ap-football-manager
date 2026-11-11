//! A generated career world's competition graph must be referentially sound: every club a
//! competition names exists, every fixture is between two of that competition's participants, no
//! fixture id is reused, and no club plays itself. National teams are legitimate participants of
//! the World Cup and must not be reported as strangers.

use std::collections::HashSet;

use domain::manager::Manager;
use domain::stats::StatsState;
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::generator::{
    DefinitionSources, WorldGenConfig, generate_world_data_seeded_with,
    repair_opening_youth_academies,
};
use ofm_core::world::start_date_for_year;
use ofm_core::world_integrity::audit_competitions;

const SEED: u64 = 20260820;
const OPENING_YEAR: i32 = 2026;

fn career_world() -> Game {
    let sources = DefinitionSources::embedded_only();
    let mut config = WorldGenConfig::standard_from(&sources);
    config.opening_year = Some(u32::try_from(OPENING_YEAR).expect("year fits u32"));
    let world = generate_world_data_seeded_with(SEED, &config, &sources);
    let start = start_date_for_year(OPENING_YEAR).expect("valid start date");
    let manager = Manager::new(
        "competition-auditor".into(),
        "Competition".into(),
        "Auditor".into(),
        "1980-01-01".into(),
        "England".into(),
    );
    let mut game = Game::new(
        GameClock::new(start),
        manager,
        world.teams,
        world.players,
        world.staff,
        vec![],
    );
    game.seed = SEED;
    game.available_staff_market_last_activity_date = Some(start.format("%Y-%m-%d").to_string());
    repair_opening_youth_academies(&mut game);
    game
}

/// Given the shipped generated world, when a career opens, then its competitions pass the
/// referential audit: no unknown participant, no unknown or repeated fixture, no self-match, and
/// every fixture is between two clubs the competition actually contains.
#[test]
fn a_generated_career_competition_graph_is_referentially_sound() {
    let mut game = career_world();
    let club = game.teams[0].id.clone();
    begin_career(
        &mut game,
        &club,
        CareerScope::default(),
        StatsState::default(),
    )
    .expect("the generated world opens a career");

    let mut known: HashSet<&str> = game.teams.iter().map(|team| team.id.as_str()).collect();
    known.extend(game.national_teams.iter().map(|team| team.id.as_str()));

    assert!(
        !game.competitions.is_empty(),
        "an empty competition list would pass the audit vacuously"
    );
    let report = audit_competitions(&game.competitions, &known);
    assert!(
        report.findings.is_empty(),
        "{:?}",
        report
            .findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}
