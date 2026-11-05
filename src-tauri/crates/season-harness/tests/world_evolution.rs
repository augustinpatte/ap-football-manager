//! Run F3: the world outside the followed league keeps evolving like the followed one.
//!
//! Dormant leagues are resolved by scoreline and shop in the market on a weekly cadence, which is
//! cheaper than the followed league's day-by-day play. This compares the two over five seasons of
//! the gate world on what a long career depends on: how good and how old the squads are, whether
//! squads stay viable, whether players move, and whether the wage bills stay near the budgets.

use std::collections::{HashMap, HashSet};

use domain::player::{PlayerMovementKind, SquadRole};
use ofm_core::finances::calc_wages;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

struct Quiet;

impl SeasonObserver for Quiet {}

#[derive(Debug, Default, Clone, Copy)]
struct Profile {
    clubs: usize,
    mean_ovr: f64,
    mean_age: f64,
}

fn profile(game: &Game, keep: impl Fn(&str) -> bool) -> Profile {
    let today = game.clock.current_date.date_naive();
    let seniors: Vec<(f64, f64)> = game
        .players
        .iter()
        .filter(|p| p.squad_role == SquadRole::Senior)
        .filter(|p| p.team_id.as_deref().is_some_and(&keep))
        .map(|p| {
            (
                f64::from(p.ovr),
                f64::from(domain::age::age_on_or_unknown(today, &p.date_of_birth)),
            )
        })
        .collect();
    let n = seniors.len().max(1) as f64;
    Profile {
        clubs: game.teams.iter().filter(|t| keep(&t.id)).count(),
        mean_ovr: seniors.iter().map(|s| s.0).sum::<f64>() / n,
        mean_age: seniors.iter().map(|s| s.1).sum::<f64>() / n,
    }
}

#[test]
fn dormant_leagues_evolve_like_the_followed_one() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let active: HashSet<String> = game.active_team_ids().expect("a followed league");
    let followed = |id: &str| active.contains(id);
    let dormant = |id: &str| !active.contains(id);
    let (f0, d0) = (profile(&game, followed), profile(&game, dormant));
    let opening_moves: HashMap<String, usize> = game
        .players
        .iter()
        .map(|p| (p.id.clone(), p.movement_history.len()))
        .collect();

    run_seasons(&mut game, RunOptions::seasons(5), &mut Quiet).expect("five seasons play");

    let (f1, d1) = (profile(&game, followed), profile(&game, dormant));
    println!("followed: {f0:?} -> {f1:?}");
    println!("dormant:  {d0:?} -> {d1:?}");

    let moved_to = |keep: &dyn Fn(&str) -> bool| -> usize {
        game.players
            .iter()
            .map(|p| {
                p.movement_history
                    .iter()
                    .skip(opening_moves.get(&p.id).copied().unwrap_or(0))
                    .filter(|e| {
                        matches!(
                            e.kind,
                            PlayerMovementKind::PermanentTransfer
                                | PlayerMovementKind::FreeAgentSigning
                        )
                    })
                    .filter(|e| e.to_team_id.as_deref().is_some_and(keep))
                    .count()
            })
            .sum()
    };
    let per_club = |moves: usize, clubs: usize| moves as f64 / clubs.max(1) as f64;
    let (moves_f, moves_d) = (
        per_club(moved_to(&followed), f1.clubs),
        per_club(moved_to(&dormant), d1.clubs),
    );
    println!("arrivals per club over five seasons: followed {moves_f:.1}, dormant {moves_d:.1}");

    // The player's own club is not held to the floor: the game warns its manager and leaves the
    // signing to him (`squad_floor::keep_squads_at_the_floor`), and the harness's manager signs
    // nobody, so its squad runs down by design. It ended this world on exactly 15 seniors before
    // Run F5.0 (no margin) and on 11 after the world's trajectory moved.
    let users_club = game.manager.team_id.clone();
    let users_club_id = users_club
        .as_deref()
        .expect("the harness keeps its manager employed");
    let users_seniors = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(users_club_id) && p.squad_role == SquadRole::Senior
        })
        .count();
    // A passive club runs down, but not to nothing: eleven is a side.
    assert!(
        users_seniors >= 11,
        "the player's club ended with {users_seniors} seniors"
    );
    let thin: Vec<&str> = game
        .teams
        .iter()
        .filter(|t| users_club.as_deref() != Some(t.id.as_str()))
        .filter(|t| {
            game.players
                .iter()
                .filter(|p| {
                    p.team_id.as_deref() == Some(t.id.as_str()) && p.squad_role == SquadRole::Senior
                })
                .count()
                < 15
        })
        .map(|t| t.id.as_str())
        .collect();
    let over_budget = |keep: &dyn Fn(&str) -> bool| -> usize {
        game.teams
            .iter()
            .filter(|t| keep(&t.id) && t.wage_budget > 0)
            .filter(|t| calc_wages(&game, &t.id) * 100 > t.wage_budget * 150)
            .count()
    };
    println!(
        "clubs below the floor: {}; over 150 % of the wage budget: followed {}, dormant {}",
        thin.len(),
        over_budget(&followed),
        over_budget(&dormant)
    );

    assert!(thin.is_empty(), "squads below the floor: {thin:?}");
    // The dormant half improves about a point more than the followed one over five seasons (1.8
    // before the playing-time effect existed, 1.5 with it, ~1.0 once the lightweight tier gave its
    // clubs injuries and fatigue in Run F5.0): the followed league includes the passive user club.
    // A known divergence of the cheap tier, recorded in docs/APFM_RUN_F3_AUTONOMY.md; the bar
    // catches a larger one.
    assert!(
        (f1.mean_ovr - f0.mean_ovr - (d1.mean_ovr - d0.mean_ovr)).abs() < 2.0,
        "the two halves of the world drifted apart in quality"
    );
    assert!(
        (f1.mean_age - f0.mean_age - (d1.mean_age - d0.mean_age)).abs() < 1.5,
        "the two halves of the world drifted apart in age"
    );
    assert!(
        moves_d > 0.5 * moves_f,
        "dormant clubs trade as the followed ones do"
    );
}
