//! P4 Phase E: do loans have a sporting purpose? Measurement only (`#[ignore]`d), public API only
//! so it runs unchanged on the pre-P4 tree.
//!
//! `P4_SEEDS=1,2,3 P4_SEASONS=3 cargo test --release -p season-harness --test p4_loans -- --ignored
//! --nocapture` (from `src-tauri/`).
//!
//! Every loan of AI-managed players is followed day by day. For each loan that has ended by the end
//! of the run: how long it lasted, the minutes the player played for the borrower, and how his
//! rating moved. A loan that lasts at least two months and gives its player fewer than 180 minutes
//! (two matches' worth) is called idle: the player went to a club that did not play him.

use std::collections::HashMap;

use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

#[derive(Clone)]
struct Tracked {
    age: i32,
    ovr_start: u8,
    potential: u8,
    last_minutes: u32,
    minutes: u32,
    days: u32,
    ovr_now: u8,
}

struct Ended {
    age: i32,
    potential_gap: i32,
    minutes: u32,
    days: u32,
    ovr_change: i32,
}

#[derive(Default)]
struct Log {
    open: HashMap<String, Tracked>,
    ended: Vec<Ended>,
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let user = game.manager.team_id.as_deref();
        let today = game.clock.current_date.date_naive();
        let mut seen: Vec<&str> = Vec::new();
        for player in &game.players {
            let Some(loan) = &player.active_loan else {
                continue;
            };
            if Some(loan.loan_team_id.as_str()) == user
                || Some(loan.parent_team_id.as_str()) == user
            {
                continue;
            }
            seen.push(player.id.as_str());
            let minutes = player.stats.minutes_played;
            let entry = self
                .open
                .entry(player.id.clone())
                .or_insert_with(|| Tracked {
                    age: domain::age::age_on_or_unknown(today, &player.date_of_birth),
                    ovr_start: player.ovr,
                    potential: player.potential,
                    last_minutes: minutes,
                    minutes: 0,
                    days: 0,
                    ovr_now: player.ovr,
                });
            // The counter restarts each season, so a drop is a new season's first minutes.
            entry.minutes += if minutes >= entry.last_minutes {
                minutes - entry.last_minutes
            } else {
                minutes
            };
            entry.last_minutes = minutes;
            entry.days += 1;
            entry.ovr_now = player.ovr;
        }
        let finished: Vec<String> = self
            .open
            .keys()
            .filter(|id| !seen.contains(&id.as_str()))
            .cloned()
            .collect();
        for id in finished {
            if let Some(t) = self.open.remove(&id) {
                self.ended.push(Ended {
                    age: t.age,
                    potential_gap: i32::from(t.potential) - i32::from(t.ovr_start),
                    minutes: t.minutes,
                    days: t.days,
                    ovr_change: i32::from(t.ovr_now) - i32::from(t.ovr_start),
                });
            }
        }
    }
}

fn describe(label: &str, loans: &[&Ended]) -> String {
    if loans.is_empty() {
        return format!("  {label:<22} none");
    }
    let n = loans.len() as f64;
    let long: Vec<&&Ended> = loans.iter().filter(|l| l.days >= 60).collect();
    let idle = long.iter().filter(|l| l.minutes < 180).count();
    let per_day = loans.iter().map(|l| f64::from(l.minutes)).sum::<f64>()
        / loans.iter().map(|l| f64::from(l.days.max(1))).sum::<f64>();
    format!(
        "  {label:<22} loans {:>4} | mean days {:>5.0} | minutes per loan day {:>5.1} | idle loans \
         {:>5.1}% of {} lasting 2+ months | mean rating change {:+.2}",
        loans.len(),
        loans.iter().map(|l| f64::from(l.days)).sum::<f64>() / n,
        per_day,
        100.0 * idle as f64 / long.len().max(1) as f64,
        long.len(),
        loans.iter().map(|l| f64::from(l.ovr_change)).sum::<f64>() / n,
    )
}

#[test]
#[ignore = "measurement study"]
fn p4_loan_outcomes() {
    let list = |name: &str, default: &str| -> Vec<u64> {
        std::env::var(name)
            .unwrap_or_else(|_| default.to_string())
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    };
    let seasons = list("P4_SEASONS", "3")[0] as u32;
    let mut all: Vec<Ended> = Vec::new();
    for seed in list("P4_SEEDS", "1") {
        let mut game = WorldSpec::gate(seed).build().expect("the world builds");
        let mut log = Log::default();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        all.extend(log.ended);
    }
    let pick =
        |f: &dyn Fn(&Ended) -> bool| -> Vec<&Ended> { all.iter().filter(|l| f(l)).collect() };
    eprintln!("loan outcomes, {} loans ended:", all.len());
    eprintln!("{}", describe("all", &pick(&|_| true)));
    eprintln!("{}", describe("age 21 or under", &pick(&|l| l.age <= 21)));
    eprintln!(
        "{}",
        describe("age 22 to 24", &pick(&|l| (22..=24).contains(&l.age)))
    );
    eprintln!("{}", describe("age 25 or over", &pick(&|l| l.age >= 25)));
    eprintln!(
        "{}",
        describe("room to grow (gap>=6)", &pick(&|l| l.potential_gap >= 6))
    );
}
