//! Mission 4, phase 6: what a day, a season, a save and the state sent to the screen cost.
//!
//! `apfm_perf <club id> <out dir>` writes `<out dir>/state.json` (the state the screen receives
//! after a day) and prints one JSON line of measurements.
use std::time::Instant;

use domain::stats::StatsState;
use ofm_core::end_of_season;
use ofm_core::turn;
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use serde_json::json;

fn main() {
    let club = std::env::args().nth(1).expect("club id");
    let out = std::path::PathBuf::from(std::env::args().nth(2).expect("out dir"));
    std::fs::create_dir_all(&out).unwrap();
    let n_seasons: u32 = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let t = Instant::now();
    let mut game = if club == "native" {
        season_harness::worlds::WorldSpec::shipped(7)
            .build()
            .expect("native world")
    } else {
        let path = world_path_from_env().expect("APFM_WORLD_JSON");
        build_fm24_career(&path, &club, 7).expect("career")
    };
    let build = t.elapsed().as_secs_f64();
    // APFM_PERF_EVERYTHING=1: every competition is played by the match engine, not only the user's
    if std::env::var_os("APFM_PERF_EVERYTHING").is_some() {
        game.active_region_ids.clear();
        game.active_competition_ids.clear();
    }

    let mut day_ms = Vec::new();
    let mut seasons = Vec::new();
    let mut growth = Vec::new();
    for season in 1..=n_seasons {
        let t_season = Instant::now();
        let mut days = 0;
        while !end_of_season::is_season_complete(&game) {
            if days > 500 {
                eprintln!(
                    "season {season} did not complete in 500 days (date {}, manager club {:?})",
                    game.clock.current_date, game.manager.team_id
                );
                break;
            }
            // hold the manager's post, as the season loop does: the question here is cost, not tenure
            game.manager.satisfaction = game.manager.satisfaction.max(60);
            game.manager.warning_stage = 0;
            let t = Instant::now();
            turn::process_day(&mut game);
            day_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            days += 1;
        }
        let t = Instant::now();
        end_of_season::advance_to_next_season(&mut game).expect("rollover");
        let rollover = t.elapsed().as_secs_f64();
        let attached = game.players.iter().filter(|p| p.team_id.is_some()).count();
        growth.push(json!({"after_season": season, "players": game.players.len(), "attached": attached, "free_agents": game.players.len() - attached, "retired_archive": game.retired_players.len(), "staff": game.staff.len(), "messages": game.messages.len(), "news": game.news.len()}));
        let played_this_season: usize = game
            .competitions
            .iter()
            .map(|c| {
                c.fixtures
                    .iter()
                    .filter(|f| f.status == domain::league::FixtureStatus::Completed)
                    .count()
            })
            .sum();
        seasons.push(json!({"season": season, "fixtures_completed_in_world": played_this_season, "days": days, "seconds_including_rollover": t_season.elapsed().as_secs_f64(), "rollover_seconds": rollover}));
    }
    let mut sorted = day_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];

    // the state a day hands to the screen
    let t = Instant::now();
    let bytes = serde_json::to_vec(&game).unwrap();
    let to_json = t.elapsed().as_secs_f64();
    std::fs::write(out.join("state.json"), &bytes).unwrap();
    let t = Instant::now();
    let cloned = game.clone();
    let clone_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let back: ofm_core::game::Game = serde_json::from_slice(&bytes).unwrap();
    let from_json = t.elapsed().as_secs_f64();
    // size of each top-level field of the state
    let value = serde_json::to_value(&game).unwrap();
    let mut fields: Vec<(String, usize)> = value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string().len()))
        .collect();
    fields.sort_by_key(|f| std::cmp::Reverse(f.1));
    let mut player_fields: std::collections::BTreeMap<String, usize> = Default::default();
    for p in value["players"].as_array().unwrap() {
        for (k, v) in p.as_object().unwrap() {
            *player_fields.entry(k.clone()).or_default() += v.to_string().len();
        }
    }
    let mut pf: Vec<(String, usize)> = player_fields.into_iter().collect();
    pf.sort_by_key(|f| std::cmp::Reverse(f.1));

    // save and load by the production save manager
    let dir = std::env::temp_dir().join(format!("apfm-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut sm = db::save_manager::SaveManager::init(&dir).unwrap();
    let t = Instant::now();
    let id = sm
        .create_save_with_stats(&game, &StatsState::default(), "perf")
        .unwrap();
    let save_s = t.elapsed().as_secs_f64();
    let size: u64 = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.metadata().unwrap().len())
        .sum();
    let mut sm2 = db::save_manager::SaveManager::init(&dir).unwrap();
    sm2.load_saves().unwrap();
    let t = Instant::now();
    let _loaded = sm2.load_game(&id).unwrap();
    let load_s = t.elapsed().as_secs_f64();
    let _ = std::fs::remove_dir_all(&dir);
    drop((cloned, back));

    println!(
        "{}",
        json!({
            "build_career_seconds": build,
            "days": day_ms.len(),
            "day_ms": {"mean": day_ms.iter().sum::<f64>() / day_ms.len() as f64, "p50": q(0.5), "p95": q(0.95), "p99": q(0.99), "max": q(1.0)},
            "seasons": seasons, "world_growth": growth,
            "state_json_mb": bytes.len() as f64 / 1e6, "serialize_state_seconds": to_json, "deserialize_state_seconds": from_json, "clone_game_seconds": clone_s,
            "state_fields_mb": fields.iter().take(8).map(|(k, v)| json!([k, (*v as f64 / 1e6 * 100.0).round() / 100.0])).collect::<Vec<_>>(),
            "player_fields_mb": pf.iter().take(10).map(|(k, v)| json!([k, (*v as f64 / 1e6 * 100.0).round() / 100.0])).collect::<Vec<_>>(),
            "players": game.players.len(),
            "save_seconds": save_s, "save_mb": size as f64 / 1e6, "load_seconds": load_s,
        })
    );
}
