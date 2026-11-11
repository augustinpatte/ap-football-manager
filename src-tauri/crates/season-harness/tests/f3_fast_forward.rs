//! Run F3: advancing 30 days in one operation gives the same world as advancing them one at a time,
//! with everything F3 added in play: a fully delegated club (training, tactics, the eleven, offers),
//! a manager who takes a decision of his own part-way, dormant leagues that credit minutes, and
//! development that reads them.
//!
//! The reference is the day the application plays on "Continue" (see `fast_forward.rs`); the
//! comparison is the harness fingerprint plus the parts of the state F3 introduced that the
//! fingerprint does not read: the delegation state and history, and each player's season record.

use chrono::{Duration, NaiveDate};
use domain::delegation::{DelegationMode, Duty};
use domain::staff::StaffPost;
use domain::team::{TrainingFocus, TrainingIntensity};
use ofm_core::fast_forward::{self, FastForwardRequest, InterruptPolicy, StopReason};
use ofm_core::game::Game;
use ofm_core::staff_system::{market, profile};
use ofm_core::{end_of_season, matchday, turn};
use season_harness::fingerprint::{fingerprint, first_difference};
use season_harness::worlds::WorldSpec;

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

fn text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The gate world with an assistant in post and everything delegated.
fn delegated_world() -> Game {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let team = game.manager.team_id.clone().expect("a club");
    let has = profile::team_staff(&game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    if !has {
        let id = game
            .staff
            .iter()
            .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
            .max_by_key(|s| (s.overall(), s.id.clone()))
            .map(|s| s.id.clone())
            .expect("a free assistant");
        market::hire(&mut game, &team, &id, 3_000, 3).expect("hired");
    }
    game.delegation.apply_mode(DelegationMode::Full);
    game
}

/// The same days, one at a time, as the application plays them.
fn singly_until(game: &mut Game, end: NaiveDate) {
    while today(game) < end {
        if end_of_season::is_season_complete(game) {
            end_of_season::advance_to_next_season(game).expect("the rollover");
        }
        let day = text(today(game));
        turn::begin_day(game);
        match matchday::scheduled_user_fixture(game, &day) {
            Some((competition, fixture)) => {
                matchday::play_user_matchday_with_capture(game, competition, fixture, &mut |_| {})
                    .expect("the user's match");
            }
            None => turn::process_day_with_capture(game, &mut |_| {}),
        }
    }
}

fn fast_forward_until(game: &mut Game, end: NaiveDate) {
    let request = FastForwardRequest {
        target_date: text(end),
        policy: InterruptPolicy::ToTargetDate,
    };
    let summary = fast_forward::run(game, &request, &mut |_: &Game| Vec::new(), &mut |_| {})
        .expect("the request is valid");
    assert_eq!(summary.stop, StopReason::TargetReached);
}

/// What the fingerprint does not read and F3 introduced.
fn f3_state(game: &Game) -> String {
    let delegation = serde_json::to_string(&game.delegation).expect("serializes");
    let records: Vec<String> = game
        .players
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}:{}:{}",
                p.id, p.stats.appearances, p.stats.minutes_played, p.stats.injured_days, p.ovr
            )
        })
        .collect();
    format!("{delegation}\n{}", records.join("\n"))
}

fn assert_same(fast: &Game, singly: &Game, what: &str) {
    let (a, b) = (fingerprint(fast), fingerprint(singly));
    assert!(a == b, "{what}: {}", first_difference(&a, &b));
    assert!(
        f3_state(fast) == f3_state(singly),
        "{what}: F3 state differs"
    );
}

/// Given a fully delegated club and dormant leagues in the world
/// When 30 days are advanced in one operation and, from the same snapshot, one at a time
/// Then the worlds are identical, the assistant acted, and the dormant leagues credited minutes.
#[test]
fn thirty_days_at_once_equal_thirty_days_one_by_one_with_everything_delegated() {
    let mut a = delegated_world();
    let mut b = a.clone();
    let end = today(&a) + Duration::days(30);

    fast_forward_until(&mut a, end);
    singly_until(&mut b, end);

    assert_same(&a, &b, "30 delegated days");
    assert!(
        !a.delegation.log.is_empty(),
        "the assistant acted during the run"
    );
    assert!(
        a.players.iter().any(|p| p.stats.minutes_played > 0),
        "matches left a playing record"
    );
}

/// Given a delegated club whose manager changes the training himself after a few days
/// When the rest of 30 days is advanced at once and one by one
/// Then both worlds agree, and the assistant is recorded as having stood back. (The week's
/// stand-back is over by the end of 27 days; that the manager's setting stands during it is a
/// unit test of the duty.)
#[test]
fn a_manager_override_inside_the_run_is_the_same_at_once_and_one_by_one() {
    let mut a = delegated_world();
    let team = a.manager.team_id.clone().expect("a club");
    let after_three = today(&a) + Duration::days(3);
    singly_until(&mut a, after_three);
    let mut b = a.clone();
    for game in [&mut a, &mut b] {
        let club = game.teams.iter_mut().find(|t| t.id == team).expect("club");
        // Not what the assistant had set (Defending at high intensity in this world).
        club.training_focus = TrainingFocus::Attacking;
        club.training_intensity = TrainingIntensity::Low;
    }
    let end = today(&a) + Duration::days(27);

    fast_forward_until(&mut a, end);
    singly_until(&mut b, end);

    assert_same(&a, &b, "an override inside the run");
    assert!(
        a.delegation.log.iter().any(|e| e.duty == Duty::Training
            && e.outcome == domain::delegation::DecisionOutcome::LeftToManager),
        "the assistant stood back from the training the manager set"
    );
}

/// Given a delegated game saved part-way through a run
/// When it is loaded and carried on, against the original carried on
/// Then both end in the same world: the assistant's memory is part of the save.
#[test]
fn a_save_taken_in_the_middle_of_a_delegated_run_carries_on_identically() {
    let mut a = delegated_world();
    let after_ten = today(&a) + Duration::days(10);
    singly_until(&mut a, after_ten);
    let mut reloaded: Game = serde_json::from_str(&serde_json::to_string(&a).expect("serializes"))
        .expect("the game reads back");
    let end = today(&a) + Duration::days(20);

    singly_until(&mut a, end);
    singly_until(&mut reloaded, end);

    assert_same(&a, &reloaded, "after a reload");
}
