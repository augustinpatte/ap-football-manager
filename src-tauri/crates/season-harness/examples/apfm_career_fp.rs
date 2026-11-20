//! Mission 4, phase 3: a long career's world, season by season, as numbers two builds can compare.
//!
//! `apfm_career_fp <seed> <gate|shipped> <seasons>` plays the career through the production day and
//! rollover entry points with the harness invariants watching, and prints one JSON line per season:
//! the fingerprint hash of the world at the season's end (fixtures, tables, players, contracts,
//! money, staff, inbox: see `fingerprint`), what a club's books look like (clubs in debt, total cash,
//! mean wage bill), how many players there are and how old they are, and the seconds the season took.
//! A last line says whether the invariants held. Two builds that simulate the same football print
//! the same hashes and aggregates on every line; only the seconds differ.
use std::time::Instant;

use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fingerprint::fingerprint;
use season_harness::invariants::Invariants;
use season_harness::worlds::WorldSpec;
use serde_json::json;

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

struct Watch {
    invariants: Invariants,
    started: Instant,
    season_started: Instant,
}

impl SeasonObserver for Watch {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        self.invariants.on_day_end(game, season, day);
    }

    fn on_stats_captured(&mut self, stats: &domain::stats::StatsState) {
        self.invariants.on_stats_captured(stats);
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        self.invariants.on_before_rollover(game, season);
    }

    fn on_season_end(&mut self, game: &Game, summary: &EndOfSeasonSummary, season: u32) {
        self.invariants.on_season_end(game, summary, season);
        let print = fingerprint(game);
        let clubs = game.teams.len().max(1) as f64;
        let in_debt = game.teams.iter().filter(|team| team.finance < 0).count();
        let cash: i64 = game.teams.iter().map(|team| team.finance).sum();
        let today = game.clock.current_date.date_naive();
        let active: Vec<_> = game.players.iter().filter(|p| !p.retired).collect();
        let mean_age = active
            .iter()
            .map(|p| f64::from(domain::age::age_on_or_unknown(today, &p.date_of_birth)))
            .sum::<f64>()
            / active.len().max(1) as f64;
        let wage_bill: i64 = ofm_core::finances::wage_bills_by_club(game).values().sum();
        println!(
            "{}",
            json!({
                "season": season,
                "fingerprint": format!("{:016x}", fnv1a(&print)),
                "lines": print.lines().count(),
                "clubs_in_debt": in_debt,
                "total_cash": cash,
                "mean_wage_bill_per_club": wage_bill as f64 / clubs,
                "players": game.players.len(),
                "mean_age": (mean_age * 1000.0).round() / 1000.0,
                "season_seconds": self.season_started.elapsed().as_secs_f64(),
                "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            })
        );
        self.season_started = Instant::now();
    }
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let world = std::env::args().nth(2).unwrap_or_else(|| "gate".into());
    let seasons: u32 = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let spec = if world == "shipped" {
        WorldSpec::shipped(seed)
    } else {
        WorldSpec::gate(seed)
    };
    let mut game = spec.build().expect("the world builds");
    let mut watch = Watch {
        invariants: Invariants::new(&game),
        started: Instant::now(),
        season_started: Instant::now(),
    };
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut watch).expect("the seasons play");
    println!(
        "{}",
        json!({
            "world": world, "seed": seed, "seasons": seasons,
            "invariants_clean": watch.invariants.is_clean(),
            "invariants_report": watch.invariants.report(),
            "total_seconds": watch.started.elapsed().as_secs_f64(),
        })
    );
}
