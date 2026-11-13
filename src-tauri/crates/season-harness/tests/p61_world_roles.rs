//! P6.1: the tactical roles on real squads. How many players of a freshly built, warmed world hold
//! each role (a role the clubs never assign is irrelevant however it is wired), and what a league's
//! worth of fixtures looks like with the roles as the clubs assigned them, or with every role
//! cleared (`P61_NOROLES=1`).
//!
//! `P6_SEEDS=1,2,3 P61_REPS=4 cargo test --release -p season-harness --test p61_world_roles -- --ignored --nocapture`
//!
//! Every ordered pair of the biggest league's clubs plays once a repetition, each repetition
//! re-seeded, every player fit at kick-off. The numbers are per match, so a league of 20 gives 380
//! a repetition. Uncertainty is the spread of the per-world means (worlds are the independent
//! units).

use std::collections::BTreeMap;

use domain::league::CompetitionType;
use ofm_core::ai_formation::freeze_reviews_for_study;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::env;
use season_harness::match_study::{Football, SideResult, mean_se, play};

struct Quiet;
impl SeasonObserver for Quiet {}

/// A fit squad on both sides: the study plays one league's fixtures over and over.
fn refresh(game: &mut Game, clubs: [&str; 2]) {
    for player in game
        .players
        .iter_mut()
        .filter(|p| p.team_id.as_deref().is_some_and(|id| clubs.contains(&id)))
    {
        player.condition = 100;
        player.injury = None;
    }
}

/// The ids of the biggest league's clubs that the user does not manage.
fn league_clubs(game: &Game) -> Vec<String> {
    let user = game.manager.team_id.clone();
    game.competitions
        .iter()
        .filter(|c| c.kind == CompetitionType::League && c.participant_ids.len() >= 16)
        .max_by_key(|c| c.participant_ids.len())
        .expect("a league")
        .participant_ids
        .iter()
        .filter(|id| Some(*id) != user.as_ref())
        .cloned()
        .collect()
}

#[test]
#[ignore = "measurement study"]
fn p61_world_roles() {
    let reps = env::first("P61_REPS", "4");
    let clear_roles = std::env::var("P61_NOROLES").is_ok_and(|v| v == "1");
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    let mut squads_counted = 0usize;
    // One row per world: both sides of every match, home first.
    let mut per_world: Vec<(SideResult, SideResult, f64)> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=env::first("P6_WARMUP", "60") as u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let _frozen = freeze_reviews_for_study();
        let clubs = league_clubs(&game);
        // `P61_FORCE=4-1-4-1` puts every club of the league in that shape: the league's goal rate
        // when everyone plays it.
        if let Ok(shape) = std::env::var("P61_FORCE") {
            for club in &clubs {
                if let Some(team) = game.teams.iter_mut().find(|t| &t.id == club) {
                    team.formation = shape.clone();
                    team.starting_xi_ids.clear();
                }
            }
        }
        if clear_roles {
            for team in game.teams.iter_mut() {
                team.player_roles.clear();
            }
        }
        for club in &clubs {
            squads_counted += 1;
            if let Some(team) = game.teams.iter().find(|t| &t.id == club) {
                *shapes.entry(team.formation.clone()).or_default() += 1;
                for role in team.player_roles.values() {
                    *census.entry(format!("{role:?}")).or_default() += 1;
                }
            }
        }
        let (mut homes, mut aways, mut draws) = (Vec::new(), Vec::new(), 0.0);
        let mut played = 0.0;
        for rep in 0..reps {
            for home in &clubs {
                for away in clubs.iter().filter(|c| *c != home) {
                    refresh(&mut game, [home, away]);
                    game.seed = seed * 1_000_000 + rep * 10_000 + played as u64;
                    let (h, a) = play(&mut game, Football::FiveZone, home, away).expect("plays");
                    if h.points == 1.0 {
                        draws += 1.0;
                    }
                    homes.push(h);
                    aways.push(a);
                    played += 1.0;
                }
            }
        }
        per_world.push((
            SideResult::mean(&homes),
            SideResult::mean(&aways),
            draws / played,
        ));
    }

    eprintln!(
        "\n== roles on real squads: {} worlds, {} matches a world{} ==",
        per_world.len(),
        reps * 380,
        if clear_roles { ", ROLES CLEARED" } else { "" }
    );
    eprintln!(
        "formations of the {squads_counted} league clubs: {}",
        shapes
            .iter()
            .map(|(shape, n)| format!("{shape} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    eprintln!("role census over {squads_counted} league squads (players holding the role):");
    for (role, count) in &census {
        eprintln!(
            "  {role:<20}{:>6}  ({:.2} a squad)",
            count,
            *count as f64 / squads_counted as f64
        );
    }
    let column = |f: &dyn Fn(&(SideResult, SideResult, f64)) -> f64| -> (f64, f64) {
        mean_se(&per_world.iter().map(f).collect::<Vec<_>>())
    };
    let report = |name: &str, (m, se): (f64, f64)| eprintln!("  {name:<28}{m:>8.3}  (se {se:.3})");
    report("goals a match", column(&|w| w.0.goals_for + w.1.goals_for));
    report("shots a match", column(&|w| w.0.shots + w.1.shots));
    report(
        "on target a match",
        column(&|w| w.0.on_target + w.1.on_target),
    );
    report("draws", column(&|w| w.2));
    report("home points a match", column(&|w| w.0.points));
    report("home possession %", column(&|w| w.0.possession));
}
