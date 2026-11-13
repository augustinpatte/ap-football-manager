//! Does a saved and reloaded career in the imported FM24 world equal the one that was saved?
//!
//! P5's reload twin (`p45_reload_twin`) plays generated worlds. The imported 240-club world is
//! the player's own file (`APFM_WORLD_JSON`), never in the repository, so this study returns at
//! once without it. It says what differs, as listed and with every fingerprint line sorted, so an
//! ordering difference is told apart from a content difference.
//!
//! `I1_SEASONS` (default 0) plays that many seasons before the save, `I1_DAYS` (default 0) that
//! many days on both copies after the reload; `I1_CLUB` is the club
//! (default `fm-c-501`).

use std::collections::BTreeMap;

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::env;
use season_harness::fingerprint::{fingerprint, first_difference};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

struct Quiet;

impl SeasonObserver for Quiet {}

/// Lines that appear a different number of times in the two fingerprints, with both counts.
fn differing_lines<'a>(left: &'a str, right: &'a str) -> Vec<(&'a str, usize, usize)> {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for line in left.lines() {
        counts.entry(line).or_default().0 += 1;
    }
    for line in right.lines() {
        counts.entry(line).or_default().1 += 1;
    }
    counts
        .into_iter()
        .filter(|(_, (a, b))| a != b)
        .map(|(line, (a, b))| (line, a, b))
        .collect()
}

#[test]
#[ignore = "study: needs APFM_WORLD_JSON (the player's own imported world)"]
fn the_imported_world_reloads_as_it_was_saved() {
    let Some(path) = world_path_from_env() else {
        eprintln!("APFM_WORLD_JSON is not set; nothing to study");
        return;
    };
    let club = std::env::var("I1_CLUB").unwrap_or_else(|_| "fm-c-501".to_string());
    let seasons = env::first("I1_SEASONS", "0") as u32;
    let mut original = build_fm24_career(&path, &club, 1).expect("the world builds");
    if seasons > 0 {
        run_seasons(&mut original, RunOptions::seasons(seasons), &mut Quiet).expect("plays");
    }
    let dir = tempfile::tempdir().expect("a directory");
    let mut manager = SaveManager::init(dir.path()).expect("a save manager");
    let id = manager
        .create_save_with_stats(&original, &StatsState::default(), "i1")
        .expect("the save");
    let mut twin = SaveManager::init(dir.path())
        .expect("a fresh manager")
        .load_game(&id)
        .expect("the load");

    report("at reload", seasons, &original, &twin);
    let days = env::first("I1_DAYS", "0") as u32;
    if days > 0 {
        let options = RunOptions::seasons(1);
        for day in 1..=days {
            play_day(&mut original, options, seasons + 1, day, &mut Quiet).expect("plays");
            play_day(&mut twin, options, seasons + 1, day, &mut Quiet).expect("plays");
        }
        report(&format!("{days} days on"), seasons, &original, &twin);
    }
}

fn report(when: &str, seasons: u32, original: &Game, twin: &Game) {
    let (left, right) = (fingerprint(original), fingerprint(twin));
    let differing = differing_lines(&left, &right);
    println!(
        "{when}, after {seasons} seasons: identical as listed {}; {} lines differ in content (sorted view); \
         first difference as listed: {}",
        left == right,
        differing.len(),
        first_difference(&left, &right)
    );
    for (line, in_original, in_twin) in differing.iter().take(12) {
        println!("  original x{in_original}, reloaded x{in_twin}: {line}");
    }
}
