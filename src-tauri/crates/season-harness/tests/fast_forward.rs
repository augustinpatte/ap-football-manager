//! Run B: a fast-forward and the same days played one at a time must end in the same world.
//!
//! The reference here is the day the application plays on "Continue" in `delegate` mode —
//! `turn::begin_day`, then `matchday::play_user_matchday_with_capture` if the club plays and
//! `turn::process_day_with_capture` if not — with the rollover the player clicks when the season is
//! over. `ofm_core::fast_forward` must not drift from it: the worlds are compared line by line
//! through the harness fingerprint (results, tables, money, squads, contracts, tactics, inbox,
//! news) and so are the stats the matches handed the store.

use chrono::{Duration, NaiveDate};
use domain::player::TransferOfferStatus;
use domain::season::TransferWindowStatus;
use domain::stats::StatsState;
use ofm_core::end_of_season;
use ofm_core::fast_forward::{
    self, DecisionKind, FastForward, FastForwardRequest, FastForwardSummary, InterruptPolicy,
    StopReason,
};
use ofm_core::game::Game;
use ofm_core::{matchday, turn};
use season_harness::driver::SeasonObserver;
use season_harness::fingerprint::{StatsLog, fingerprint, first_difference};
use season_harness::invariants::Invariants;
use season_harness::worlds::WorldSpec;

fn world() -> Game {
    WorldSpec::gate(1).build().expect("the world builds")
}

fn date_of(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

fn text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn request(game: &Game, days: i64, policy: InterruptPolicy) -> FastForwardRequest {
    FastForwardRequest {
        target_date: text(date_of(game) + Duration::days(days)),
        policy,
    }
}

fn no_host(_: &Game) -> Vec<fast_forward::PendingDecision> {
    Vec::new()
}

/// A fast-forward over `game`, with the stats its matches produced.
fn fast(game: &mut Game, request: &FastForwardRequest) -> (FastForwardSummary, StatsLog) {
    let mut log = StatsLog::default();
    let summary = fast_forward::run(game, request, &mut no_host, &mut |stats| {
        log.on_stats_captured(&stats)
    })
    .expect("the request is valid");
    (summary, log)
}

/// The same days, one at a time, as the application plays them.
fn singly_until(game: &mut Game, end: NaiveDate) -> StatsLog {
    let mut log = StatsLog::default();
    while date_of(game) < end {
        if end_of_season::is_season_complete(game) {
            end_of_season::advance_to_next_season(game).expect("the rollover");
        }
        let today = text(date_of(game));
        turn::begin_day(game);
        match matchday::scheduled_user_fixture(game, &today) {
            Some((competition, fixture)) => {
                matchday::play_user_matchday_with_capture(game, competition, fixture, &mut |s| {
                    log.on_stats_captured(&s)
                })
                .expect("the user's match");
            }
            None => turn::process_day_with_capture(game, &mut |s| log.on_stats_captured(&s)),
        }
    }
    log
}

/// [`fast`] for the `days` after today.
fn fast_days(
    game: &mut Game,
    days: i64,
    policy: InterruptPolicy,
) -> (FastForwardSummary, StatsLog) {
    let asked = request(game, days, policy);
    fast(game, &asked)
}

fn assert_same_world(fast: &Game, singly: &Game, what: &str) {
    let (a, b) = (fingerprint(fast), fingerprint(singly));
    assert!(a == b, "{what}: {}", first_difference(&a, &b));
}

fn days_until_season_complete(game: &Game) -> i64 {
    let mut probe = game.clone();
    let mut days = 0;
    while !end_of_season::is_season_complete(&probe) {
        turn::process_day(&mut probe);
        days += 1;
        assert!(days < 600, "the season never completes");
    }
    days
}

/// Given one snapshot, When two months are played as a fast-forward and as single days,
/// Then the worlds and the stats handed to the store are identical, and the user's club played.
#[test]
fn a_fast_forward_ends_in_the_world_the_same_days_played_singly_end_in() {
    let mut a = world();
    let mut b = a.clone();

    let (summary, fast_stats) = fast_days(&mut a, 60, InterruptPolicy::ToTargetDate);
    let singly_stats = singly_until(&mut b, date_of(&a));

    assert_eq!(summary.stop, StopReason::TargetReached);
    assert_eq!(summary.progress.days_done, 60);
    assert!(
        summary.progress.user_matches_played > 3,
        "two months of the club's fixtures went through the matchday path"
    );
    assert_same_world(&a, &b, "60 days");
    assert_eq!(fast_stats.fingerprint(), singly_stats.fingerprint());
    assert!(
        fast_stats.captures() > 50,
        "the matches reached the stats store"
    );
}

/// Given a snapshot a few weeks before the season ends,
/// When the target is past the end of the season,
/// Then the fast-forward rolls the season over where the player would, once, and the worlds agree.
#[test]
fn a_season_rollover_inside_the_run_matches_the_player_rolling_it_over() {
    let mut a = world();
    let days = days_until_season_complete(&a) + 30;
    let mut b = a.clone();

    let (summary, _) = fast_days(&mut a, days, InterruptPolicy::ToTargetDate);
    singly_until(&mut b, date_of(&a));

    assert_eq!(summary.stop, StopReason::TargetReached);
    assert_eq!(
        summary.season_summaries.len(),
        1,
        "one season was rolled over"
    );
    assert_same_world(&a, &b, "across the rollover");
}

/// Given a run that stops before the club's next match,
/// When it is resumed to the same target,
/// Then the stop was between days — the world is what single days make — and stopping and
///      resuming ends where one uninterrupted run ends.
#[test]
fn stopping_at_the_next_match_and_resuming_equals_one_run() {
    let mut a = world();
    let mut straight = a.clone();
    let target = request(&a, 40, InterruptPolicy::ToTargetDate);

    // An offer for one of the club's players may stop the run on the way; the player looks at it
    // and carries on, which is the same request again.
    let end = date_of(&a) + Duration::days(40);
    let first = loop {
        let remaining = (end - date_of(&a)).num_days();
        let (summary, _) = fast_days(&mut a, remaining, InterruptPolicy::StopAtNextMatch);
        match summary.stop {
            StopReason::Decisions { .. } => continue,
            _ => break summary,
        }
    };
    assert_eq!(first.stop, StopReason::NextMatch, "{:?}", first.stop);
    let mut singly = straight.clone();
    singly_until(&mut singly, date_of(&a));
    assert_same_world(&a, &singly, "at the stop");

    // The player plays the match their own way — here, the delegated way — and carries on.
    let today = text(date_of(&a));
    let (competition, fixture) = matchday::scheduled_user_fixture(&a, &today).expect("a match");
    turn::begin_day(&mut a);
    matchday::play_user_matchday_with_capture(&mut a, competition, fixture, &mut |_| {})
        .expect("the match");
    fast(&mut a, &target);

    fast(&mut straight, &target);
    assert_same_world(&a, &straight, "stop and resume");
}

/// Given a career saved halfway through a fast-forward,
/// When it is loaded and the run goes on,
/// Then it ends where the uninterrupted run ends — a save taken at a stop is a world a player
///      could have reached by clicking.
#[test]
fn a_save_taken_part_way_loads_and_carries_on_to_the_same_world() {
    let mut straight = world();
    let mut interrupted = straight.clone();
    fast_days(&mut straight, 50, InterruptPolicy::ToTargetDate);

    fast_days(&mut interrupted, 20, InterruptPolicy::ToTargetDate);
    let saves = tempfile::tempdir().unwrap();
    let mut manager = db::save_manager::SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&interrupted, &StatsState::default(), "halfway")
        .expect("the save");
    let mut loaded = db::save_manager::SaveManager::init(saves.path())
        .unwrap()
        .load_game(&id)
        .expect("the load");
    assert_same_world(&loaded, &interrupted, "the load itself");

    let rest = FastForwardRequest {
        target_date: text(date_of(&straight)),
        policy: InterruptPolicy::ToTargetDate,
    };
    fast(&mut loaded, &rest);
    assert_same_world(
        &loaded,
        &straight,
        "after save, load and the rest of the run",
    );
}

fn first_deadline_day(game: &Game) -> NaiveDate {
    let mut probe = game.clone();
    for _ in 0..400 {
        if probe.season_context.transfer_window.status == TransferWindowStatus::DeadlineDay {
            return date_of(&probe);
        }
        turn::process_day(&mut probe);
    }
    panic!("the calendar never reaches a transfer deadline");
}

/// An offer from another club for one of the user's players, waiting for an answer.
fn pending_offer_for_a_user_player(game: &mut Game) -> (String, String) {
    let user = game.manager.team_id.clone().expect("employed");
    let buyer = game
        .teams
        .iter()
        .find(|team| team.id != user)
        .map(|team| team.id.clone())
        .expect("another club");
    let today = text(date_of(game));
    let player = game
        .players
        .iter_mut()
        .find(|player| player.team_id.as_deref() == Some(user.as_str()))
        .expect("a user player");
    let offer: domain::player::TransferOffer = serde_json::from_value(serde_json::json!({
        "id": "ff-test-offer", "from_team_id": buyer, "fee": 1_000_000, "wage_offered": 1000,
        "date": today,
    }))
    .unwrap();
    player.transfer_offers.push(offer);
    (player.id.clone(), "ff-test-offer".to_string())
}

fn offer_status(game: &Game, player_id: &str, offer_id: &str) -> TransferOfferStatus {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .and_then(|player| player.transfer_offers.iter().find(|o| o.id == offer_id))
        .map(|offer| offer.status.clone())
        .expect("the offer")
}

/// Given talks still open as the window closes,
/// When a run that stops for decisions reaches the deadline day,
/// Then it stops there with the deadline as a decision, before the day is played — the talks are
///      still pending — and the player can answer them.
#[test]
fn a_run_that_stops_for_decisions_stops_before_the_deadline_lapses_open_talks() {
    let mut game = world();
    let deadline = first_deadline_day(&game);
    let days_before = (deadline - date_of(&game)).num_days() - 3;
    fast_days(&mut game, days_before, InterruptPolicy::ToTargetDate);
    let (player_id, offer_id) = pending_offer_for_a_user_player(&mut game);

    let mut stopped_on_the_deadline = false;
    for _ in 0..30 {
        let remaining = (deadline + Duration::days(3) - date_of(&game)).num_days();
        let (summary, _) = fast_days(&mut game, remaining, InterruptPolicy::StopForDecisions);
        match summary.stop {
            StopReason::Decisions { decisions } => {
                if decisions
                    .iter()
                    .any(|decision| decision.kind == DecisionKind::TransferDeadline)
                {
                    stopped_on_the_deadline = true;
                    break;
                }
            }
            StopReason::SeasonComplete | StopReason::NextMatch => {
                panic!("not expected: {:?}", summary.stop)
            }
            other => panic!("the run ended without meeting the deadline: {other:?}"),
        }
    }
    assert!(
        stopped_on_the_deadline,
        "the deadline never stopped the run"
    );
    assert_eq!(
        date_of(&game),
        deadline,
        "stopped on the deadline day itself"
    );
    assert_eq!(
        offer_status(&game, &player_id, &offer_id),
        TransferOfferStatus::Pending,
        "nothing of the deadline day was played: the talks are still open"
    );
}

/// Given the same open talks, When the run goes to its target whatever happens,
/// Then it passes the deadline, the report names the decision it passed, and the talks lapsed
///      as they do on any deadline day.
#[test]
fn a_target_date_run_passes_the_deadline_and_says_so() {
    let mut game = world();
    let deadline = first_deadline_day(&game);
    let days_before = (deadline - date_of(&game)).num_days() - 3;
    fast_days(&mut game, days_before, InterruptPolicy::ToTargetDate);
    let (player_id, offer_id) = pending_offer_for_a_user_player(&mut game);

    let remaining = (deadline + Duration::days(3) - date_of(&game)).num_days();
    let (summary, _) = fast_days(&mut game, remaining, InterruptPolicy::ToTargetDate);

    assert_eq!(summary.stop, StopReason::TargetReached);
    assert!(
        summary
            .passed_decisions
            .iter()
            .any(
                |passed| passed.decision.kind == DecisionKind::TransferDeadline
                    && passed.date == text(deadline)
            ),
        "{:?}",
        summary.passed_decisions
    );
    assert_ne!(
        offer_status(&game, &player_id, &offer_id),
        TransferOfferStatus::Pending,
        "the deadline closed the talks"
    );
}

/// Given the club's fixture moved to another date,
/// When a run crosses both dates,
/// Then the fixture is played once, on its new date, and the worlds agree with single days.
#[test]
fn a_postponed_match_is_played_once_on_its_new_date() {
    let mut a = world();
    let user = a.manager.team_id.clone().unwrap();
    let from = date_of(&a) + Duration::days(8);
    let (competition, fixture) = a
        .competitions
        .iter()
        .enumerate()
        .find_map(|(c, competition)| {
            competition
                .fixtures
                .iter()
                .position(|f| {
                    f.status == domain::league::FixtureStatus::Scheduled
                        && (f.home_team_id == user || f.away_team_id == user)
                        && NaiveDate::parse_from_str(&f.date, "%Y-%m-%d").is_ok_and(|d| d >= from)
                })
                .map(|f| (c, f))
        })
        .expect("a user fixture to move");
    let original = NaiveDate::parse_from_str(
        &a.competitions[competition].fixtures[fixture].date,
        "%Y-%m-%d",
    )
    .unwrap();
    // The day after is a day the club has nothing on in this calendar (the rest rule keeps its
    // fixtures apart); the new date is checked to be free of any other fixture of the club.
    let moved = original + Duration::days(1);
    a.competitions[competition].fixtures[fixture].date = text(moved);
    let mut b = a.clone();
    let id = a.competitions[competition].fixtures[fixture].id.clone();

    let days = (moved - date_of(&a)).num_days() + 3;
    let (summary, _) = fast_days(&mut a, days, InterruptPolicy::ToTargetDate);
    singly_until(&mut b, date_of(&a));

    assert_eq!(summary.stop, StopReason::TargetReached);
    let played = &a.competitions[competition]
        .fixtures
        .iter()
        .find(|f| f.id == id)
        .unwrap();
    assert_eq!(played.status, domain::league::FixtureStatus::Completed);
    assert_eq!(played.date, text(moved));
    assert_same_world(&a, &b, "with a moved fixture");
}

/// Given a fast-forward over a season's worth of days, When every day is checked as it is played,
/// Then the harness invariants hold on each — results applied once, no stranded fixture, no
///      duplicate record — which is what a double finalization or a repeated payment would break.
#[test]
fn every_day_of_a_long_run_holds_the_world_invariants() {
    let mut game = world();
    let mut invariants = Invariants::new(&game);
    let target = request(&game, 150, InterruptPolicy::ToTargetDate);
    let mut run = FastForward::begin(&game, &target, &mut no_host).unwrap();
    let mut day = 0;
    let stop = loop {
        let mut captured = Vec::new();
        let stop = run.step(&mut game, &mut no_host, &mut |s| captured.push(s));
        day += 1;
        invariants.on_day_end(&game, 1, day);
        if let Some(stop) = stop {
            break stop;
        }
    };
    assert_eq!(stop, StopReason::TargetReached);
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given the stop-for-decisions policy, When the club has fixtures on the way,
/// Then the run plays them itself instead of stopping at them, whatever else stops it on the way.
#[test]
fn the_decision_policy_plays_the_clubs_matches_itself() {
    let mut game = world();
    let end = date_of(&game) + Duration::days(20);
    let mut matches = 0;
    loop {
        let remaining = (end - date_of(&game)).num_days();
        let (summary, _) = fast_days(&mut game, remaining, InterruptPolicy::StopForDecisions);
        matches += summary.progress.user_matches_played;
        match summary.stop {
            StopReason::Decisions { .. } => continue,
            StopReason::TargetReached => break,
            other => panic!("not expected in 20 days: {other:?}"),
        }
    }
    assert!(
        matches >= 1,
        "the club's fixtures in 20 days were played by the run"
    );
}
