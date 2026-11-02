//! Run B: what advancing the career costs, day by day and in one fast-forward.
//!
//! `apfm_ff_bench <seed> <gate|shipped> [rounds] [n|season]` prints one JSON line per window (7, 30, 90 days
//! and a complete season), for the same starting snapshot:
//!
//! * `fast_forward_ms`: `ofm_core::fast_forward::run`, the code the application drives;
//! * `daily_ms`: the same days played one at a time the way the screen does it (one day, then the
//!   whole game cloned for the screen and serialized, as the IPC layer must);
//! * `ui_state_ms`: the part of `daily_ms` that is the screen's copy, which a fast-forward pays once.
//!
//! The run is deterministic for a seed, so the two end in the same world; the benchmark asserts it.
use std::time::Instant;

use chrono::{Duration, NaiveDate};
use ofm_core::end_of_season;
use ofm_core::fast_forward::{FastForward, FastForwardRequest, InterruptPolicy, StopReason};
use ofm_core::game::Game;
use ofm_core::{matchday, turn};
use season_harness::fingerprint::fingerprint;
use season_harness::worlds::WorldSpec;
use serde_json::json;

/// The harness's tenure guard: a long run would otherwise get the manager sacked, and a fast-forward
/// rightly ends on a firing while this benchmark wants to time whole seasons. Applied before every
/// day on both paths, so they stay comparable.
fn keep_employed(game: &mut Game) {
    game.manager.satisfaction = game.manager.satisfaction.max(60);
    game.manager.warning_stage = 0;
}

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

/// The reference: the application's `delegate` day, then the screen's copy of the state.
fn daily(game: &mut Game, days: i64, with_screen_copy: bool) -> (f64, f64) {
    let target = today(game) + Duration::days(days);
    let mut ui = 0.0;
    let started = Instant::now();
    while today(game) < target {
        keep_employed(game);
        let day = game.clock.current_date.format("%Y-%m-%d").to_string();
        if end_of_season::is_season_complete(game) {
            end_of_season::advance_to_next_season(game).expect("rollover");
        }
        turn::begin_day(game);
        match matchday::scheduled_user_fixture(game, &day) {
            Some((competition, fixture)) => {
                matchday::play_user_matchday_with_capture(game, competition, fixture, &mut |_| {})
                    .expect("user match");
            }
            None => turn::process_day(game),
        }
        if with_screen_copy {
            let t = Instant::now();
            let bytes = serde_json::to_vec(&game.ui_clone()).expect("serialize");
            std::hint::black_box(bytes.len());
            ui += t.elapsed().as_secs_f64() * 1000.0;
        }
    }
    (started.elapsed().as_secs_f64() * 1000.0, ui)
}

fn fast_forwarded(game: &mut Game, days: i64) -> (f64, usize) {
    let target = today(game) + Duration::days(days);
    let request = FastForwardRequest {
        target_date: target.format("%Y-%m-%d").to_string(),
        policy: InterruptPolicy::ToTargetDate,
    };
    let started = Instant::now();
    let mut run = FastForward::begin(game, &request, &mut |_| Vec::new()).expect("valid request");
    let (mut guard, mut sink) = (|_: &Game| Vec::new(), |_| {});
    let stop = loop {
        // Not on the step that only reads "target reached": the daily loop has no such step.
        if today(game) < target {
            keep_employed(game);
        }
        if let Some(stop) = run.step(game, &mut guard, &mut sink) {
            break stop;
        }
    };
    assert_eq!(stop, StopReason::TargetReached);
    let summary = run.finish(game, stop);
    // The one copy the screen gets at the end.
    let bytes = serde_json::to_vec(&game.ui_clone()).expect("serialize");
    std::hint::black_box(bytes.len());
    (
        started.elapsed().as_secs_f64() * 1000.0,
        summary.season_summaries.len(),
    )
}

fn days_until_season_complete(game: &Game) -> i64 {
    let mut probe = game.clone();
    let mut days = 0;
    while !end_of_season::is_season_complete(&probe) && days < 600 {
        keep_employed(&mut probe);
        turn::process_day(&mut probe);
        days += 1;
    }
    days
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let world = std::env::args().nth(2).unwrap_or_else(|| "gate".into());
    let rounds: u32 = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let only: Option<String> = std::env::args().nth(4);
    let t = Instant::now();
    let spec = if world == "shipped" {
        WorldSpec::shipped(seed)
    } else {
        WorldSpec::gate(seed)
    };
    let base = spec.build().expect("world");
    let build = t.elapsed().as_secs_f64();
    let season_days = days_until_season_complete(&base) + 1;
    for days in [7, 30, 90, season_days] {
        let label = if days == season_days { "season" } else { "n" };
        if only.as_deref().is_some_and(|only| only != label) {
            continue;
        }
        for round in 0..rounds {
            let mut a = base.clone();
            let (ff_ms, rollovers) = fast_forwarded(&mut a, days);
            let mut b = base.clone();
            let (daily_ms, ui_ms) = daily(&mut b, days, true);
            let (fa, fb) = (fingerprint(&a), fingerprint(&b));
            assert!(
                fa == fb,
                "{days} days: fast-forward and daily ended in different worlds: {}",
                season_harness::fingerprint::first_difference(&fa, &fb)
            );
            println!(
                "{}",
                json!({
                    "world": world, "seed": seed, "window": label, "days": days, "round": round,
                    "build_world_s": build,
                    "fast_forward_ms": ff_ms,
                    "ff_ms_per_day": ff_ms / days as f64,
                    "daily_ms": daily_ms,
                    "daily_ms_per_day": daily_ms / days as f64,
                    "ui_state_ms": ui_ms,
                    "rollovers": rollovers,
                    "players": a.players.len(),
                })
            );
        }
    }
}
