//! P4 Phase C: do managers play their clubs differently? Measurement only (`#[ignore]`d), written
//! against public API only so it runs unchanged on the pre-P4 tree for the before figures.
//!
//! `P4_SEEDS=1,2,3 cargo test --release -p season-harness --test p4_manager_diversity -- --ignored
//! --nocapture` (from `src-tauri/`).
//!
//! At days 30, 100 and 200 of the first season: how many AI clubs have a manager, how many distinct
//! tactical plans clubs of one play style are running, and how often a plan departs from the style's
//! own blueprint (results and the squad's legs already move it; so does the manager).

use std::collections::{HashMap, HashSet};

use ofm_core::ai_tactics::blueprint_for;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

#[derive(Default)]
struct Log {
    lines: Vec<String>,
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        if season != 1 || ![30, 100, 200].contains(&day) {
            return;
        }
        let user = game.manager.team_id.as_deref();
        let clubs: Vec<_> = game
            .teams
            .iter()
            .filter(|team| Some(team.id.as_str()) != user)
            .collect();
        let with_manager = clubs.iter().filter(|t| t.manager_id.is_some()).count();
        let off_blueprint = clubs
            .iter()
            .filter(|t| t.tactics_phase != blueprint_for(&t.play_style))
            .count();
        let mut by_style: HashMap<String, HashSet<String>> = HashMap::new();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for team in &clubs {
            let style = format!("{:?}", team.play_style);
            by_style
                .entry(style.clone())
                .or_default()
                .insert(format!("{:?}", team.tactics_phase));
            *counts.entry(style).or_default() += 1;
        }
        let mut styles: Vec<_> = by_style.into_iter().collect();
        styles.sort_by(|a, b| a.0.cmp(&b.0));
        let distinct: Vec<String> = styles
            .iter()
            .map(|(style, plans)| format!("{style} {}/{}", plans.len(), counts[style]))
            .collect();
        let all_plans: HashSet<String> = clubs
            .iter()
            .map(|t| format!("{:?}", t.tactics_phase))
            .collect();
        self.lines.push(format!(
            "season 1 day {day:>3}: {}/{} AI clubs have a manager | plans differing from the \
             style's blueprint {:.1}% | distinct plans in the world {} | distinct plans / clubs \
             by style: {}",
            with_manager,
            clubs.len(),
            100.0 * off_blueprint as f64 / clubs.len() as f64,
            all_plans.len(),
            distinct.join(", ")
        ));
    }
}

#[test]
#[ignore = "measurement study"]
fn p4_manager_diversity() {
    let seeds = env::list("P4_SEEDS", "1");
    for seed in seeds {
        let mut game = env::world("P4_WORLD", seed)
            .build()
            .expect("the world builds");
        let mut log = Log::default();
        run_seasons(&mut game, RunOptions::seasons(1), &mut log).expect("a season plays");
        eprintln!("== seed {seed}");
        for line in &log.lines {
            eprintln!("{line}");
        }
    }
}
