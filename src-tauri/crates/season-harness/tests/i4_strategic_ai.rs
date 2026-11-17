//! I4A: do AI clubs plan and recruit coherently across a career? Measurement only (`#[ignore]`d).
//!
//! ```text
//! I4_SEEDS=1,2,3 I4_SEASONS=10 \
//!   cargo test --release -p season-harness --test i4_strategic_ai -- --ignored --nocapture
//! ```
//!
//! The brief's questions are read from the studies that already answer them, rather than from a
//! second copy of the instrumentation:
//!
//! * **recruitment by need** — a completed AI signing into a group below the floor plus the
//!   planning margin, read from the deal log's `buyer_group_depth`;
//! * **duplicate signings** — a signing into a group already at the depth the club plans to carry;
//! * **squad improvement** — the mean rating of the fielded eleven, in the slots the formation puts
//!   them in (`squad_study`'s proxy, not the model the decisions are made from);
//! * **age, size, contracts, wages, fees, debt and strength** — one row a season from the squad,
//!   career and finance reports.
//!
//! The first test below is the study. It decides nothing and asserts nothing: a long career's
//! output is evidence for a reader, not a pass/fail on a distribution.

use std::time::Instant;

use ofm_core::game::Game;
use season_harness::career_study::{CareerLog, report as career_report};
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;
use season_harness::finance_study::{FinanceLog, report as finance_report};
use season_harness::squad_study::{JOINER_KINDS, SquadLog, report as squad_report, summary};

/// The floor plus planning margin each group is planned to clear, `[GK, DEF, MID, FWD]`.
/// `squad_floor::MIN_PLAYERS_PER_GROUP` plus `PLANNING_MARGIN_PER_GROUP`; copied here because a
/// study reads the game through public API only.
const GROUP_NEED: [usize; 4] = [3, 5, 5, 3];

/// The depth a club plans to carry in each group (`transfers`' `DESIRED_DEPTH`, crate-private).
/// A signing into a group already this deep is depth the club did not need.
const PLANNED_DEPTH: [usize; 4] = [3, 7, 7, 5];

/// A club above this many seniors has registered more than the senior list allows.
const SENIOR_LIMIT: usize = 30;

/// Seconds each season took, read at the rollover.
struct Pace {
    started: Instant,
    seasons: Vec<f64>,
}

impl SeasonObserver for Pace {
    fn on_before_rollover(&mut self, _game: &Game, _season: u32) {
        self.seasons.push(self.started.elapsed().as_secs_f64());
        self.started = Instant::now();
    }
}

/// Names every club above the senior limit at a rollover, with how each senior arrived. The table
/// says *that* a club broke the limit; this says *who* and *through which route*, which is what a
/// reader needs to tell a leak from a counting rule.
struct LimitWatch;

impl SeasonObserver for LimitWatch {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        use domain::player::{PlayerMovementKind, SquadRole};

        for team in &game.teams {
            let seniors: Vec<&domain::player::Player> = game
                .players
                .iter()
                .filter(|player| !player.retired)
                .filter(|player| player.team_id.as_deref() == Some(team.id.as_str()))
                .filter(|player| player.squad_role == SquadRole::Senior)
                .collect();
            if seniors.len() <= SENIOR_LIMIT {
                continue;
            }
            eprintln!(
                "!! season {season}: {} ({}) holds {} seniors",
                team.name,
                team.id,
                seniors.len()
            );
            for player in seniors {
                let last = player.movement_history.last();
                eprintln!(
                    "   {} {:?} ovr {} age {} loan {:?} last {:?} {:?}->{:?} on {} trail {}",
                    player.id,
                    player.natural_position,
                    player.ovr,
                    domain::age::age_on_or_unknown(
                        game.clock.current_date.date_naive(),
                        &player.date_of_birth
                    ),
                    player
                        .active_loan
                        .as_ref()
                        .map(|loan| loan.parent_team_id.clone()),
                    last.map(|entry| entry.kind.clone()),
                    last.and_then(|entry| entry.from_team_id.clone()),
                    last.and_then(|entry| entry.to_team_id.clone()),
                    last.map(|entry| entry.date.clone()).unwrap_or_default(),
                    player
                        .movement_history
                        .iter()
                        .rev()
                        .take(3)
                        .map(|entry| match &entry.kind {
                            PlayerMovementKind::LoanStart => "LoanStart".to_string(),
                            PlayerMovementKind::LoanReturn => "LoanReturn".to_string(),
                            other => format!("{other:?}"),
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        }
    }
}

fn mean<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let (sum, count) = values
        .into_iter()
        .fold((0.0, 0_usize), |(sum, count), value| {
            (sum + value, count + 1)
        });
    sum / count.max(1) as f64
}

/// The I4A table: one row a season of squads, recruitment and the books, then the same for every
/// completed signing in the run.
fn i4_report(log: &SquadLog) -> String {
    let mut out = String::new();
    out.push_str(
        "season | clubs | seniors max | clubs >30 | XI fit | weak slots/club | thin groups/club | \
         joiners/club | free/fee/loan/other | fees M/club | deals need/full/other | deal age | \
         <=21/>=32 % | deal gain\n",
    );
    let (mut total_deals, mut total_need, mut total_full) = (0_usize, 0_usize, 0_usize);
    let (mut total_age, mut total_young, mut total_old, mut total_gain) =
        (0.0, 0_usize, 0_usize, 0.0);
    for (index, year) in log.years.iter().enumerate() {
        let clubs = year.len().max(1);
        let deals = log
            .deals
            .get(index)
            .map_or(&[][..], |deals| deals.as_slice());
        let (mut need, mut full) = (0_usize, 0_usize);
        for deal in deals {
            if deal.buyer_group_depth >= PLANNED_DEPTH[deal.position_group] {
                full += 1;
            } else if deal.buyer_group_depth < GROUP_NEED[deal.position_group] {
                need += 1;
            }
        }
        let aged = deals.len().max(1) as f64;
        let young = deals.iter().filter(|deal| deal.age <= 21).count();
        let old = deals.iter().filter(|deal| deal.age >= 32).count();
        let age = mean(deals.iter().map(|deal| f64::from(deal.age)));
        let gain = mean(deals.iter().map(|deal| f64::from(deal.gain)));
        let mut by_kind = [0_usize; JOINER_KINDS];
        for club in year {
            for (kind, count) in by_kind.iter_mut().enumerate() {
                *count += club.joined_by_kind[kind];
            }
        }
        let joined: usize = by_kind.iter().sum();
        out.push_str(&format!(
            "{:>3} | {:>3} | {:>2} | {:>2} | {:>5.1} | {:>4.1} | {:>4.1} | {:>4.1} | {}/{}/{}/{} | \
             {:>5.2} | {:>2}/{:>2}/{:>2} | {:>4.1} | {:>3.0}/{:>3.0} | {:>+5.2}\n",
            index + 1,
            clubs,
            year.iter().map(|club| club.seniors).max().unwrap_or(0),
            year.iter()
                .filter(|club| club.seniors > SENIOR_LIMIT)
                .count(),
            mean(year.iter().map(|club| club.xi_fit)),
            mean(year.iter().map(|club| club.weak_slots as f64)),
            mean(year.iter().map(|club| club.thin_groups as f64)),
            joined as f64 / clubs as f64,
            by_kind[0],
            by_kind[1],
            by_kind[2],
            by_kind[3],
            mean(year.iter().map(|club| club.fees_paid as f64)) / 1e6,
            need,
            full,
            deals.len().saturating_sub(need + full),
            age,
            100.0 * young as f64 / aged,
            100.0 * old as f64 / aged,
            gain,
        ));
        total_deals += deals.len();
        total_need += need;
        total_full += full;
        total_age += deals.iter().map(|deal| f64::from(deal.age)).sum::<f64>();
        total_young += young;
        total_old += old;
        total_gain += deals.iter().map(|deal| f64::from(deal.gain)).sum::<f64>();
    }
    let n = total_deals.max(1) as f64;
    out.push_str(&format!(
        "all | deals {total_deals} | need {:.0}% | full {:.0}% | mean age {:.1} | <=21/>=32 {:.0}/{:.0}% | \
         mean gain {:+.2}\n",
        100.0 * total_need as f64 / n,
        100.0 * total_full as f64 / n,
        total_age / n,
        100.0 * total_young as f64 / n,
        100.0 * total_old as f64 / n,
        total_gain / n,
    ));
    out
}

#[test]
#[ignore = "long-career study"]
fn i4_strategic_ai() {
    let seasons = env::first("I4_SEASONS", "10") as u32;
    for seed in env::list("I4_SEEDS", "1,2,3") {
        let mut game = env::world("I4_WORLD", seed)
            .build()
            .expect("the world builds");
        let user = game.manager.team_id.clone();
        eprintln!(
            "== seed {seed}: {} clubs, {} players, {seasons} seasons",
            game.teams.len(),
            game.players.len()
        );
        let mut squads = SquadLog::starting_from(&game);
        let mut career = CareerLog::starting_from(&game);
        let mut finance = FinanceLog::starting_from(&game);
        let mut pace = Pace {
            started: Instant::now(),
            seasons: Vec::new(),
        };
        ofm_core::transfers::enable_deal_log();
        let began = Instant::now();
        let mut limit = LimitWatch;
        let mut watched = (
            &mut squads,
            (&mut career, (&mut finance, (&mut pace, &mut limit))),
        );
        let outcome = run_seasons(&mut game, RunOptions::seasons(seasons), &mut watched);
        let stalled = outcome.as_ref().err().map(|error| error.to_string());
        eprintln!(
            "== seed {seed}: {} seasons in {:.1}s ({})",
            pace.seasons.len(),
            began.elapsed().as_secs_f64(),
            stalled.as_deref().unwrap_or("completed")
        );
        eprintln!(
            "seconds per season: {}",
            pace.seasons
                .iter()
                .map(|season| format!("{season:.1}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        eprintln!("{}", i4_report(&squads));
        let wanted: Vec<usize> = [1usize, 2, 3, 5, 10]
            .into_iter()
            .filter(|season| *season <= pace.seasons.len())
            .collect();
        eprintln!("{}", squad_report(&squads, &wanted));
        eprintln!("{}", summary(&squads));
        eprintln!("{}", career_report(&career));
        let finance_wanted: Vec<usize> = [0usize, 1, 2, 5, 10]
            .into_iter()
            .filter(|season| *season <= pace.seasons.len())
            .collect();
        eprintln!(
            "{}",
            finance_report(&finance, user.as_deref(), &finance_wanted)
        );
        if let Some(reason) = stalled {
            eprintln!("!! seed {seed} stopped early: {reason}");
        }
    }
}
