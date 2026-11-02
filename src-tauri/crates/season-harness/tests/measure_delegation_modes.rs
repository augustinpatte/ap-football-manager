//! Run F2, phase 5: what does each delegation mode make the staff actually do?
//!
//! The same club, world and seed are played for a stretch of days under Manual, Approval and Full,
//! and what the assistant did (the history), what he asked (the proposals), how often the eleven
//! changed and how many decisions were left waiting in the manager's inbox are counted. Run with
//! `-- --nocapture` to read the table; the assertions are the properties the modes promise.

use domain::delegation::{DecisionOutcome, DelegationMode};
use domain::staff::StaffPost;
use ofm_core::game::Game;
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::worlds::WorldSpec;
use std::collections::BTreeMap;

const DAYS: u32 = 120;

struct Quiet;
impl SeasonObserver for Quiet {}

#[derive(Debug, Default)]
struct Measured {
    executed: BTreeMap<String, usize>,
    left_to_manager: usize,
    proposals_open: usize,
    xi_changes: usize,
    unresolved_decisions: usize,
    /// What the waiting decisions are, by inbox category.
    waiting_by_category: BTreeMap<String, usize>,
}

fn user_team(game: &Game) -> String {
    game.manager.team_id.clone().expect("a club")
}

fn with_assistant(mut game: Game) -> Game {
    let team = user_team(&game);
    let has = profile::team_staff(&game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    if !has {
        let reputation = game
            .teams
            .iter()
            .find(|t| t.id == team)
            .map_or(0, |t| t.reputation);
        let (id, wage) = game
            .staff
            .iter()
            .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
            .max_by_key(|s| (s.overall(), s.id.clone()))
            .map(|s| (s.id.clone(), market::asking_wage(s, reputation)))
            .expect("a free assistant exists");
        market::hire(&mut game, &team, &id, wage, 3).expect("hired");
    }
    game
}

fn play(mode: DelegationMode) -> Measured {
    let mut game = with_assistant(WorldSpec::gate(1).build().expect("world builds"));
    game.delegation.apply_mode(mode);
    // The ceilings a manager who "trusts the assistant" would plausibly set.
    game.delegation.limits.max_transfer_fee = 5_000_000;
    game.delegation.limits.max_player_wage = 40_000;
    let team = user_team(&game);
    let options = RunOptions::seasons(1);
    let mut measured = Measured::default();
    let mut previous_xi = game
        .teams
        .iter()
        .find(|t| t.id == team)
        .map(|t| t.starting_xi_ids.clone());
    for day in 1..=DAYS {
        play_day(&mut game, options, 1, day, &mut Quiet).expect("the day is played");
        let xi = game
            .teams
            .iter()
            .find(|t| t.id == team)
            .map(|t| t.starting_xi_ids.clone());
        if xi != previous_xi {
            measured.xi_changes += 1;
        }
        previous_xi = xi;
    }
    for entry in &game.delegation.log {
        match entry.outcome {
            DecisionOutcome::Executed => {
                *measured
                    .executed
                    .entry(format!("{:?}", entry.duty))
                    .or_default() += 1;
            }
            DecisionOutcome::LeftToManager => measured.left_to_manager += 1,
            DecisionOutcome::Approved | DecisionOutcome::Declined => {}
        }
    }
    measured.proposals_open = game.delegation.proposals.len();
    for message in game
        .messages
        .iter()
        .filter(|m| m.actions.iter().any(|a| !a.resolved))
    {
        measured.unresolved_decisions += 1;
        *measured
            .waiting_by_category
            .entry(format!("{:?}", message.category))
            .or_default() += 1;
    }
    measured
}

/// Given the same club, world and seed
/// When 120 days are played under Manual, Approval and Full
/// Then Manual's assistant does nothing and proposes nothing, and Full acts at least as much as
///      Approval. What this stretch does NOT show is Approval asking where Full acts: over 120 days
///      from the opening neither mode reached a spending decision, so the two behaved identically
///      (documented in docs/APFM_RUN_F2_GAMEPLAY.md as the main finding of the delegation audit).
#[test]
fn manual_does_nothing_and_full_acts_at_least_as_much_as_approval() {
    let manual = play(DelegationMode::Manual);
    let approval = play(DelegationMode::Approval);
    let full = play(DelegationMode::Full);

    println!(
        "\nmode      executed(by duty)                 left  open-proposals  xi-changes  open-decisions"
    );
    for (name, m) in [
        ("Manual", &manual),
        ("Approval", &approval),
        ("Full", &full),
    ] {
        println!(
            "{name:<9} {:<33?} {:>4}  {:>14}  {:>10}  {:>14}",
            m.executed, m.left_to_manager, m.proposals_open, m.xi_changes, m.unresolved_decisions
        );
    }
    println!(
        "waiting under Full, by inbox category: {:?}",
        full.waiting_by_category
    );
    println!(
        "waiting under Manual, by inbox category: {:?}",
        manual.waiting_by_category
    );

    assert!(manual.executed.is_empty(), "Manual: the assistant acted");
    assert_eq!(manual.proposals_open, 0, "Manual: the assistant proposed");
    let total = |m: &Measured| m.executed.values().sum::<usize>();
    assert!(total(&full) > 0, "Full did nothing in {DAYS} days");
    assert!(
        total(&full) >= total(&approval),
        "Full acts at least as much as Approval"
    );
}
