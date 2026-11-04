//! Run F5.0: what a career costs on disk as matches accumulate.
//!
//! Measures real files (database, journal, WAL, index, anything else in the saves directory), not
//! payload sizes, at 1 / 100 / 380 matches, after a season, and after several seasons, with every
//! competition active. Save and load times are measured at the same points.
//!
//! `F50_SEASONS=10 cargo test --release -p season-harness --test f50_storage -- --ignored --nocapture`

use std::path::Path;
use std::time::Instant;

use domain::stats::StatsState;
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::{Scope, WorldSpec};

/// What a clone of `value` adds to the resident set, in MB: the memory the value occupies,
/// measured from the operating system (an allocator that reuses freed pages can only understate it).
fn heap_mb<T: Clone>(value: &T) -> f64 {
    let before = rss_mb();
    let copy = value.clone();
    let after = rss_mb();
    drop(copy);
    after.saturating_sub(before) as f64
}

fn dir_bytes(dir: &Path) -> (u64, Vec<(String, u64)>) {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).expect("dir").flatten() {
        if let Ok(meta) = entry.metadata()
            && meta.is_file()
        {
            files.push((entry.file_name().to_string_lossy().to_string(), meta.len()));
        }
    }
    (files.iter().map(|(_, n)| n).sum(), files)
}

/// This process's resident set, in MB, as the operating system reports it.
fn rss_mb() -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map_or(0, |kb| kb / 1024)
}

fn engine_matches(game: &Game) -> u64 {
    game.competitions
        .iter()
        .flat_map(|c| &c.fixtures)
        .filter(|f| f.result.as_ref().is_some_and(|r| r.report.is_some()))
        .count() as u64
}

/// Bytes of the file by what they hold: the history of matches (their lines, ids, facts, archived
/// editions and the competitions' own fixtures) against everything else a career keeps.
fn match_history_bytes(db_path: &Path) -> (i64, i64) {
    let conn = rusqlite::Connection::open(db_path).expect("open");
    let mut stmt = conn
        .prepare("SELECT name, SUM(pgsize) FROM dbstat GROUP BY name")
        .expect("dbstat");
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("rows")
        .flatten()
        .collect();
    let is_history = |name: &str| {
        name.contains("match_lines")
            || name.contains("match_stat")
            || name.contains("stat_ids")
            || name.contains("edition_archive")
            || name == "competitions"
            || name == "fixtures"
    };
    let history = rows
        .iter()
        .filter(|(n, _)| is_history(n))
        .map(|(_, b)| b)
        .sum();
    let other = rows
        .iter()
        .filter(|(n, _)| !is_history(n))
        .map(|(_, b)| b)
        .sum();
    (history, other)
}

struct Bench {
    last_history: i64,
    last_other: i64,
    started: Instant,
    last_marker: Instant,
    dir: tempfile::TempDir,
    manager: db::save_manager::SaveManager,
    save_id: Option<String>,
    stats: StatsState,
    thresholds: Vec<u64>,
    last_total: u64,
    last_matches: u64,
    rows: Vec<String>,
    seasons_done: u32,
    report_seasons: Vec<u32>,
}

impl Bench {
    fn measure(&mut self, game: &Game, label: &str) {
        let matches = engine_matches(game);
        let save_started = Instant::now();
        let id = match &self.save_id {
            Some(id) => {
                self.manager
                    .save_game_with_stats(game, &self.stats, id)
                    .expect("save");
                id.clone()
            }
            None => self
                .manager
                .create_save_with_stats(game, &self.stats, "f50")
                .expect("create"),
        };
        let save_ms = save_started.elapsed().as_millis();
        self.save_id = Some(id.clone());
        let (total, files) = dir_bytes(self.dir.path());
        let db_bytes = files
            .iter()
            .filter(|(n, _)| n.ends_with(".db"))
            .map(|(_, s)| *s)
            .sum::<u64>();
        let dm = matches.saturating_sub(self.last_matches);
        let per = if dm > 0 {
            (total as i64 - self.last_total as i64) as f64 / dm as f64
        } else {
            f64::NAN
        };
        let load_started = Instant::now();
        let mut fresh = db::save_manager::SaveManager::init(self.dir.path()).expect("init");
        let loaded = fresh.load_game(&id).expect("load");
        let loaded_stats = fresh.load_stats_state(&id).expect("stats");
        let load_ms = load_started.elapsed().as_millis();
        assert_eq!(
            loaded_stats.player_matches.len(),
            self.stats.player_matches.len()
        );
        assert_eq!(engine_matches(&loaded), matches);
        let path = self.manager.save_db_path(&id).expect("path");
        let (history, other) = match_history_bytes(&path);
        let simulated = self.last_marker.elapsed().as_millis();
        self.rows.push(format!(
            "{label:<14} fixtures={matches:>6} total={total:>11} history={history:>10} (+{:>9}) other={other:>10} (+{:>9}) files={:?} save_ms={save_ms:>5} load_ms={load_ms:>5} sim_ms_since_last={simulated:>6} rss_mb={} edition_archive={} stat_rows={}/{}",
            history - self.last_history,
            other - self.last_other,
            files.iter().map(|(n, s)| (n.rsplit('.').next().unwrap_or("").to_string(), *s)).collect::<Vec<_>>(),
            rss_mb(),
            game.edition_archive.len(),
            self.stats.player_matches.len(),
            self.stats.team_matches.len(),
        ));
        let _ = (per, db_bytes, self.started);
        self.last_history = history;
        self.last_other = other;
        self.last_marker = Instant::now();
        self.last_total = total;
        self.last_matches = matches;
    }
}

impl SeasonObserver for Bench {
    fn on_stats_captured(&mut self, stats: &StatsState) {
        self.stats.append(stats.clone());
    }

    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let matches = engine_matches(game);
        if let Some(&next) = self.thresholds.first()
            && matches >= next
        {
            self.thresholds.remove(0);
            self.measure(game, &format!("~{next} matches"));
        }
    }

    fn on_season_end(&mut self, game: &Game, _s: &EndOfSeasonSummary, season: u32) {
        self.seasons_done = season;
        if self.report_seasons.contains(&season) {
            self.measure(game, &format!("season {season}"));
        }
    }
}

#[test]
#[ignore = "measurement; run in release with --ignored --nocapture"]
fn f50_storage_growth() {
    let seasons: u32 = std::env::var("F50_SEASONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let dir = tempfile::tempdir().expect("tmp");
    let manager = db::save_manager::SaveManager::init(dir.path()).expect("saves");
    let mut game = WorldSpec::gate(21)
        .with_scope(Scope::Everything)
        .build()
        .expect("world");
    let mut report_seasons = vec![1, 2, 3, 5, 10, 15, 20, 25, 30];
    report_seasons.retain(|s| *s <= seasons);
    let mut bench = Bench {
        dir,
        manager,
        save_id: None,
        stats: StatsState::default(),
        thresholds: vec![1, 100, 380],
        last_total: 0,
        last_matches: 0,
        rows: Vec::new(),
        seasons_done: 0,
        report_seasons,
        last_history: 0,
        last_other: 0,
        started: Instant::now(),
        last_marker: Instant::now(),
    };
    bench.measure(&game, "baseline");
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut bench).expect("run");
    for row in &bench.rows {
        println!("{row}");
    }
    println!(
        "heap (a clone's size): whole game {:.0} MB | edition archive {:.0} MB | match stats {:.0} MB ({} player records)",
        heap_mb(&game),
        heap_mb(&game.edition_archive),
        heap_mb(&bench.stats),
        bench.stats.player_matches.len()
    );
    // Where the bytes are, in the final file.
    let id = bench.save_id.clone().expect("saved");
    let path = bench.manager.save_db_path(&id).expect("path");
    let conn = rusqlite::Connection::open(&path).expect("open");
    let mut stmt = conn
        .prepare("SELECT name, SUM(pgsize) FROM dbstat GROUP BY name ORDER BY 2 DESC LIMIT 14")
        .expect("dbstat");
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .expect("rows");
    println!("-- bytes by table/index (dbstat)");
    for row in rows.flatten() {
        println!("{:<48} {:>12}", row.0, row.1);
    }
}

/// Does deflate earn its place on the edition archive's JSON? Measured on the final file's rows.
#[test]
#[ignore = "measurement; run in release with --ignored --nocapture"]
fn f50_archive_compression_worth() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tmp");
    let manager = db::save_manager::SaveManager::init(dir.path()).expect("saves");
    let mut game = WorldSpec::gate(21)
        .with_scope(Scope::Everything)
        .build()
        .expect("world");
    let mut bench = Bench {
        dir,
        manager,
        save_id: None,
        stats: StatsState::default(),
        thresholds: vec![],
        last_total: 0,
        last_matches: 0,
        rows: Vec::new(),
        seasons_done: 0,
        report_seasons: vec![],
        last_history: 0,
        last_other: 0,
        started: Instant::now(),
        last_marker: Instant::now(),
    };
    run_seasons(&mut game, RunOptions::seasons(2), &mut bench).expect("run");
    bench.measure(&game, "final");
    let id = bench.save_id.clone().expect("saved");
    let path = bench.manager.save_db_path(&id).expect("path");
    let conn = rusqlite::Connection::open(&path).expect("open");
    let mut stmt = conn
        .prepare("SELECT record_json FROM competition_edition_archive")
        .expect("select");
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("rows")
        .flatten()
        .collect();
    let raw: usize = rows.iter().map(String::len).sum();
    for level in [1u32, 6, 9] {
        let mut total = 0;
        for json in &rows {
            let mut enc =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
            enc.write_all(json.as_bytes()).expect("write");
            total += enc.finish().expect("finish").len();
        }
        println!(
            "archive: {} editions, raw {raw} B, deflate level {level}: {total} B ({:.1}%)",
            rows.len(),
            100.0 * total as f64 / raw as f64
        );
    }
}

/// What one M12 replay costs in a save, raw and deflated.
#[test]
#[ignore = "measurement; run in release with --ignored --nocapture"]
fn f50_replay_size() {
    use std::io::Write;
    let game = WorldSpec::gate(21)
        .with_scope(Scope::Everything)
        .build()
        .expect("world");
    let team = game.manager.team_id.clone().expect("club");
    let (competition, index, fixture) = game
        .competitions
        .iter()
        .find_map(|c| {
            c.fixtures
                .iter()
                .position(|f| f.home_team_id == team || f.away_team_id == team)
                .map(|i| (c.id.clone(), i, c.fixtures[i].id.clone()))
        })
        .expect("a fixture of the club");
    let mut game = game;
    // The fixture is read through the legacy slot, as the live session does.
    let slot = game
        .competitions
        .iter()
        .position(|c| c.id == competition)
        .expect("slot");
    game.league = Some(game.competitions[slot].clone());
    let prepared = ofm_core::spatial_runner::build_selected_fixture_replay(
        &game,
        index,
        &competition,
        &fixture,
    )
    .expect("replay");
    let raw = prepared.replay_json.len();
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    enc.write_all(prepared.replay_json.as_bytes())
        .expect("write");
    let z = enc.finish().expect("finish").len();
    println!(
        "replay: raw {raw} B, deflate-6 {z} B ({:.1}%)",
        100.0 * z as f64 / raw as f64
    );
}
