//! P4 Phase E/G: what happens to prospects, loaned or not? Measurement only (`#[ignore]`d), public
//! API only so it runs unchanged on the pre-P4 tree.
//!
//! `P4_SEEDS=1,2,3 P4_SEASONS=3 cargo test --release -p season-harness --test p4_youth -- --ignored
//! --nocapture` (from `src-tauri/`).
//!
//! The cohort is every rostered AI-club player aged 21 or under with at least six points of room to
//! grow at the end of a season (the opening counts as the first). A year later, for those still in
//! the world: how far their rating moved, how many minutes they played that season, and how that
//! differs by whether they were out on loan at the season's end. The loan study counts loans; this
//! counts players, so a policy that lends fewer prospects cannot hide a loss in a per-loan average.

use std::collections::HashMap;

use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

struct Snap {
    ovr: u8,
}

#[derive(Default)]
struct Totals {
    n: f64,
    growth: f64,
    minutes: f64,
    played_under_450: f64,
    gap_closed: f64,
    gap_open: f64,
}

#[derive(Default)]
struct Log {
    cohort: HashMap<String, Snap>,
    all: Totals,
    on_loan: Totals,
    at_parent: Totals,
}

fn add(t: &mut Totals, growth: f64, minutes: f64, gap: f64) {
    t.n += 1.0;
    t.growth += growth;
    t.minutes += minutes;
    if minutes < 450.0 {
        t.played_under_450 += 1.0;
    }
    t.gap_open += gap;
    t.gap_closed += growth;
}

impl Log {
    fn read(&mut self, game: &Game, today: chrono::NaiveDate) {
        let user = game.manager.team_id.as_deref();
        // Close the previous cohort.
        for player in &game.players {
            let Some(before) = self.cohort.get(&player.id) else {
                continue;
            };
            let growth = f64::from(player.ovr) - f64::from(before.ovr);
            let minutes = f64::from(player.stats.minutes_played);
            let gap = 0.0;
            add(&mut self.all, growth, minutes, gap);
            if player.active_loan.is_some() {
                add(&mut self.on_loan, growth, minutes, gap);
            } else {
                add(&mut self.at_parent, growth, minutes, gap);
            }
        }
        // Open the next.
        self.cohort.clear();
        for player in &game.players {
            let Some(team) = player.team_id.as_deref() else {
                continue;
            };
            if Some(team) == user || player.retired {
                continue;
            }
            let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
            if age <= 21 && i32::from(player.potential) >= i32::from(player.ovr) + 6 {
                self.cohort
                    .insert(player.id.clone(), Snap { ovr: player.ovr });
            }
        }
    }
}

impl SeasonObserver for Log {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let today = game.clock.current_date.date_naive();
        self.read(game, today);
    }
}

fn line(label: &str, t: &Totals) -> String {
    if t.n == 0.0 {
        return format!("  {label:<22} none");
    }
    format!(
        "  {label:<22} player-seasons {:>5} | rating growth {:+.2} | minutes {:>6.0} | under 450 minutes {:>5.1}%",
        t.n,
        t.growth / t.n,
        t.minutes / t.n,
        100.0 * t.played_under_450 / t.n
    )
}

#[test]
#[ignore = "measurement study"]
fn p4_prospect_cohort() {
    let seasons = env::first("P4_SEASONS", "3") as u32;
    let mut total = Log::default();
    for seed in env::list("P4_SEEDS", "1") {
        let mut game = env::world("P4_WORLD", seed)
            .build()
            .expect("the world builds");
        let mut log = Log::default();
        let opening = game.clock.current_date.date_naive();
        log.read(&game, opening);
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        for (from, to) in [
            (&log.all, &mut total.all),
            (&log.on_loan, &mut total.on_loan),
            (&log.at_parent, &mut total.at_parent),
        ] {
            to.n += from.n;
            to.growth += from.growth;
            to.minutes += from.minutes;
            to.played_under_450 += from.played_under_450;
        }
    }
    eprintln!("prospects (21 or under, 6+ to grow) followed a season at a time:");
    eprintln!("{}", line("all prospects", &total.all));
    eprintln!("{}", line("on loan at season end", &total.on_loan));
    eprintln!("{}", line("with their own club", &total.at_parent));
}
