//! Mission 4: what the production time-advancement paths, the screen's copy of the world, a save and
//! a market sweep cost, measured on a generated world with a fixed seed.
//!
//! `#[ignore]`d: it is a benchmark, and only a release build means anything.
//!
//! ```text
//! PERF_WORLD=gate PERF_SEED=1 PERF_ROUNDS=3 cargo test --release --lib perf_baseline -- --ignored --nocapture
//! ```
//!
//! `PERF_WORLD` is `gate` (80 clubs) or `shipped` (440 clubs). One JSON line is printed per
//! measurement with the milliseconds of every round, so the spread is visible and the caller can take
//! a median. Every round starts from the same cloned snapshot, so rounds differ only by machine noise.
//! Compile time is not part of any figure. The sibling harness examples (`apfm_ff_bench`,
//! `apfm_season`) measure the daily/fast-forward windows and a full season on the same worlds.

use std::time::Instant;

use chrono::Duration;
use db::save_manager::SaveManager;
use domain::stats::StatsState;
use ofm_core::fast_forward::{FastForwardRequest, InterruptPolicy};
use ofm_core::game::Game;
use ofm_core::state::StateManager;
use season_harness::worlds::WorldSpec;
use serde_json::json;

use super::fast_forward::fast_forward_to_date_internal;
use super::time::{
    advance_time_internal, advance_to_next_event_internal, skip_to_match_day_internal,
};

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

/// A line the report is read from.
fn report(what: &str, world: &str, seed: u64, extra: serde_json::Value) {
    eprintln!(
        "PERF {}",
        json!({ "what": what, "world": world, "seed": seed, "data": extra })
    );
}

fn fresh(base: &Game) -> StateManager {
    let state = StateManager::new();
    state.set_game(base.clone());
    state
}

fn bytes_of<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value).expect("serializes").len()
}

/// Days played up to and including the next one on which the user has a match, by the production
/// `advance_time` path, so the clock sits on a quiet day again for the next measurement.
fn play_through_match_day(state: &StateManager) {
    let _ = advance_time_internal(state).expect("a day advances");
}

#[test]
#[ignore = "benchmark: release build only; see the module comment"]
fn perf_baseline() {
    let world: String = env_or("PERF_WORLD", "gate".to_string());
    let seed: u64 = env_or("PERF_SEED", 1);
    let rounds: usize = env_or("PERF_ROUNDS", 3);
    let spec = if world == "shipped" {
        WorldSpec::shipped(seed)
    } else {
        WorldSpec::gate(seed)
    };

    let started = Instant::now();
    let mut base = spec.build().expect("the world builds");
    // A career that has already lasted: `PERF_SEASONS` seasons are played first (not measured), so
    // the payload and the save show what history costs a long career.
    let seasons_first: u32 = env_or("PERF_SEASONS", 0);
    if seasons_first > 0 {
        struct Quiet;
        impl season_harness::driver::SeasonObserver for Quiet {}
        let played = Instant::now();
        season_harness::driver::run_seasons(
            &mut base,
            season_harness::driver::RunOptions::seasons(seasons_first),
            &mut Quiet,
        )
        .expect("the seasons play");
        report(
            "seasons_first",
            &world,
            seed,
            json!({ "seasons": seasons_first, "ms": ms(played) }),
        );
    }
    report(
        "world",
        &world,
        seed,
        json!({
            "build_ms": ms(started),
            "clubs": base.teams.len(),
            "players": base.players.len(),
            "competitions": base.competitions.len(),
            "date": base.clock.current_date.format("%Y-%m-%d").to_string(),
        }),
    );

    // 6. The world's clone and the screen's copy.
    let (mut full_clone, mut ui_clone, mut full_json, mut ui_json) =
        (vec![], vec![], vec![], vec![]);
    let (mut full_bytes, mut ui_bytes) = (0, 0);
    for _ in 0..rounds {
        let t = Instant::now();
        let full = base.clone();
        full_clone.push(ms(t));
        let t = Instant::now();
        let ui = base.ui_clone();
        ui_clone.push(ms(t));
        let t = Instant::now();
        full_bytes = bytes_of(&full);
        full_json.push(ms(t));
        let t = Instant::now();
        ui_bytes = bytes_of(&ui);
        ui_json.push(ms(t));
    }
    // What the screen's copy is made of: the serialized size of each top-level field, largest first.
    let ui_value = serde_json::to_value(base.ui_clone()).expect("serializes");
    let mut parts: Vec<(String, usize)> = ui_value
        .as_object()
        .expect("a game is an object")
        .iter()
        .map(|(key, value)| (key.clone(), bytes_of(value)))
        .collect();
    parts.sort_by_key(|part| std::cmp::Reverse(part.1));
    parts.truncate(10);
    report("ui_payload_by_field", &world, seed, json!(parts));
    if let Ok(path) = std::env::var("PERF_DUMP_UI") {
        std::fs::write(
            &path,
            serde_json::to_vec(&base.ui_clone()).expect("serializes"),
        )
        .expect("the payload is written");
    }
    report(
        "clone_and_payload",
        &world,
        seed,
        json!({
            "clone_full_ms": full_clone, "ui_clone_ms": ui_clone,
            "serialize_full_ms": full_json, "serialize_ui_ms": ui_json,
            "full_bytes": full_bytes, "ui_bytes": ui_bytes,
        }),
    );

    // 1. The daily advance, as the screen calls it (the day, then the copy for the screen).
    let days = 14;
    let mut per_day = vec![];
    for _ in 0..rounds {
        let state = fresh(&base);
        let t = Instant::now();
        for _ in 0..days {
            let _ = advance_time_internal(&state).expect("a day advances");
        }
        per_day.push(ms(t) / f64::from(days));
    }
    report(
        "advance_time_per_day_ms",
        &world,
        seed,
        json!({ "days": days, "rounds": per_day }),
    );

    // 2 and 3. "Continue to the next decision" and "skip to match day", and what each returns.
    for (name, run) in [
        (
            "advance_to_next_event",
            advance_to_next_event_internal
                as fn(&StateManager) -> Result<serde_json::Value, String>,
        ),
        ("skip_to_match_day", |state: &StateManager| {
            skip_to_match_day_internal(state, true)
        }),
    ] {
        let (mut times, mut skipped, mut payload, mut payload_ms) =
            (vec![], vec![], vec![], vec![]);
        for _ in 0..rounds {
            let state = fresh(&base);
            for _ in 0..4 {
                let t = Instant::now();
                let response = run(&state).expect("the skip runs");
                times.push(ms(t));
                skipped.push(response["days_skipped"].as_u64().unwrap_or(0));
                let t = Instant::now();
                payload.push(bytes_of(&response));
                payload_ms.push(ms(t));
                play_through_match_day(&state);
            }
        }
        report(
            name,
            &world,
            seed,
            json!({ "call_ms": times, "days_skipped": skipped, "response_bytes": payload, "serialize_ms": payload_ms }),
        );
    }

    // 4. A 30-day career segment: one fast-forward, and the same days one at a time.
    let target = (base.clock.current_date.date_naive() + Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    let (mut ff, mut ff_bytes, mut one_by_one) = (vec![], 0, vec![]);
    for _ in 0..rounds {
        let state = fresh(&base);
        let t = Instant::now();
        let response = fast_forward_to_date_internal(
            &state,
            &FastForwardRequest {
                target_date: target.clone(),
                policy: InterruptPolicy::ToTargetDate,
            },
            &mut |_| {},
        )
        .expect("the fast-forward runs");
        ff.push(ms(t));
        ff_bytes = bytes_of(&response);
        let state = fresh(&base);
        let t = Instant::now();
        for _ in 0..30 {
            let _ = advance_time_internal(&state).expect("a day advances");
        }
        one_by_one.push(ms(t));
    }
    report(
        "segment_30_days",
        &world,
        seed,
        json!({ "fast_forward_ms": ff, "fast_forward_response_bytes": ff_bytes, "day_by_day_ms": one_by_one }),
    );

    // 7. A save and a load, through a real SQLite file.
    let (mut save, mut load, mut size) = (vec![], vec![], 0);
    for round in 0..rounds {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut saves = SaveManager::init(dir.path()).expect("saves open");
        let t = Instant::now();
        let id = saves
            .create_save_with_stats(&base, &StatsState::default(), &format!("perf-{round}"))
            .expect("saves");
        save.push(ms(t));
        size = std::fs::read_dir(dir.path())
            .expect("readable")
            .flatten()
            .map(|entry| entry.metadata().map(|m| m.len()).unwrap_or(0))
            .sum::<u64>();
        let mut reader = SaveManager::init(dir.path()).expect("saves open");
        let t = Instant::now();
        let loaded = reader.load_game(&id).expect("loads");
        load.push(ms(t));
        assert_eq!(loaded.players.len(), base.players.len());
    }
    report(
        "save_and_load",
        &world,
        seed,
        json!({ "save_ms": save, "load_ms": load, "dir_bytes": size }),
    );

    // 8. The AI market sweep: one `evaluate_transfer_market` on a window-open day, on a clone.
    let mut warm = base.clone();
    let mut opened = None;
    for _ in 0..400 {
        if warm.season_context.transfer_window.status == domain::season::TransferWindowStatus::Open
        {
            opened = Some(warm.clock.current_date.format("%Y-%m-%d").to_string());
            break;
        }
        ofm_core::turn::process_day(&mut warm);
    }
    let mut sweeps = vec![];
    for _ in 0..rounds {
        let mut game = warm.clone();
        let t = Instant::now();
        ofm_core::transfers::evaluate_transfer_market(&mut game);
        sweeps.push(ms(t));
    }
    report(
        "market_sweep_ms",
        &world,
        seed,
        json!({ "window_open_on": opened, "rounds": sweeps }),
    );
}
