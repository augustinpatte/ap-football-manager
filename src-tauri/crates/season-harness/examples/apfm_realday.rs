//! Mission 11: what a day costs on the path the application takes, split by stage.
//!
//! `apfm_realday <club id> <seasons> <out.json> [--seed N]`
//!
//! `apfm_perf` times `turn::process_day` alone. The Tauri command `advance_time_with_mode` also
//! builds the response: the day's results, the cup summaries, a whole-game clone and, behind the
//! IPC, the JSON of that clone. This example times each of those next to the day itself, season by
//! season, with a fixed seed, so a slowdown can be placed on one stage. Compile time is not part of
//! any figure. The world file comes from `APFM_WORLD_JSON`.
use std::time::Instant;

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use ofm_core::advance_results::collect_advance_results;
use ofm_core::end_of_season;
use ofm_core::turn::build_cup_round_summaries;
use season_harness::driver::{DayPath, RunOptions, SeasonObserver, play_day, roll_season_over};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use serde_json::json;

struct Quiet;
impl SeasonObserver for Quiet {}

#[derive(Default)]
struct Stage {
    ms: Vec<f64>,
}

impl Stage {
    fn push(&mut self, started: Instant) {
        self.ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    fn summary(&self) -> serde_json::Value {
        if self.ms.is_empty() {
            return json!(null);
        }
        let mut sorted = self.ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
        let round = |v: f64| (v * 100.0).round() / 100.0;
        json!({
            "total_s": round(self.ms.iter().sum::<f64>() / 1000.0),
            "mean_ms": round(self.ms.iter().sum::<f64>() / self.ms.len() as f64),
            "p50_ms": round(q(0.5)),
            "p95_ms": round(q(0.95)),
            "max_ms": round(q(1.0)),
        })
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let club = args.next().expect("club id");
    let seasons: u32 = args.next().expect("seasons").parse().expect("number");
    let out = args.next().expect("output path");
    let rest: Vec<String> = args.collect();
    let seed: u64 = rest
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| rest.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);

    let path = world_path_from_env().expect("APFM_WORLD_JSON is not set");
    let mut game = build_fm24_career(&path, &club, seed).expect("career builds");
    let mut options = RunOptions::seasons(seasons);
    options.day_path = DayPath::UserMatchLive;

    let dir = std::env::temp_dir().join(format!("apfm-realday-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut save_manager = SaveManager::init(&dir).expect("save manager");

    let mut report = Vec::new();
    for season in 1..=seasons {
        let (mut day, mut results, mut cups, mut clone, mut serialize) = (
            Stage::default(),
            Stage::default(),
            Stage::default(),
            Stage::default(),
            Stage::default(),
        );
        let mut payload_bytes = 0usize;
        let mut day_no = 0u32;
        let t_season = Instant::now();
        while !end_of_season::is_season_complete(&game) {
            day_no += 1;
            if day_no > 500 {
                eprintln!("season {season} did not complete in 500 days");
                break;
            }
            let today = game.clock.current_date.format("%Y-%m-%d").to_string();
            let t = Instant::now();
            play_day(&mut game, options, season, day_no, &mut Quiet).expect("day");
            day.push(t);
            let t = Instant::now();
            let found = collect_advance_results(&game, &today);
            results.push(t);
            let t = Instant::now();
            let _ = build_cup_round_summaries(&game, &today);
            cups.push(t);
            let t = Instant::now();
            let copy = game.clone();
            clone.push(t);
            let t = Instant::now();
            let bytes = serde_json::to_vec(&copy).expect("serialize");
            serialize.push(t);
            payload_bytes = bytes.len();
            drop((found, copy, bytes));
        }
        let season_seconds = t_season.elapsed().as_secs_f64();

        let t = Instant::now();
        roll_season_over(&mut game, options, season, &mut Quiet).expect("rollover");
        let rollover_s = t.elapsed().as_secs_f64();

        let t = Instant::now();
        let id = save_manager
            .create_save_with_stats(&game, &StatsState::default(), "realday")
            .expect("save");
        let save_s = t.elapsed().as_secs_f64();
        let size: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.metadata().unwrap().len())
            .sum();
        let t = Instant::now();
        let mut reader = SaveManager::init(&dir).expect("reader");
        reader.load_saves().expect("list");
        let _ = reader.load_game(&id).expect("reload");
        let load_s = t.elapsed().as_secs_f64();
        save_manager.delete_save(&id).ok();

        let fixtures: usize = game.competitions.iter().map(|c| c.fixtures.len()).sum();
        let scheduled_now = game
            .competitions
            .iter()
            .filter(|c| {
                c.fixtures
                    .iter()
                    .any(|f| f.status == domain::league::FixtureStatus::Scheduled)
            })
            .count();
        report.push(json!({
            "season": season,
            "days": day_no,
            "season_seconds_without_rollover": season_seconds,
            "rollover_s": rollover_s,
            "stage_day": day.summary(),
            "stage_results": results.summary(),
            "stage_cup_summaries": cups.summary(),
            "stage_clone": clone.summary(),
            "stage_serialize": serialize.summary(),
            "ipc_payload_mb": payload_bytes as f64 / 1e6,
            "save_s": save_s,
            "save_mb": size as f64 / 1e6,
            "reload_s": load_s,
            "players": game.players.len(),
            "teams": game.teams.len(),
            "competitions": game.competitions.len(),
            "competitions_with_scheduled_fixtures": scheduled_now,
            "active_competition_ids": game.active_competition_ids.len(),
            "fixtures_total": fixtures,
            "messages": game.messages.len(),
            "news": game.news.len(),
            "staff": game.staff.len(),
            "retired_archive": game.retired_players.len(),
        }));
        eprintln!("season {season} done in {season_seconds:.1}s");
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
}
