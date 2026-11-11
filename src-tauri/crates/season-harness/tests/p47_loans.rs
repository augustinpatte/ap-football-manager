//! P4.7: why do loans go idle later in a career, and does the borrower's own model see it coming?
//! Measurement only (`#[ignore]`d), public API only.
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test p47_loans -- --ignored --nocapture`
//!
//! Every loan is followed from the day it starts to the day it ends. At the start: the loanee's age,
//! rating and room to grow, whether he is a "loan for minutes" (22 or under, six points of room),
//! and where he ranks in the borrower's squad in his position group. During it: days injured,
//! whether the borrower changed formation, and which players joined the borrower in his group. At
//! the end: his minutes. An idle loan is 60 or more days with under 180 minutes. Idle loans are
//! then attributed: to the loanee being injured for a quarter of the loan or more, to a better player
//! joining the borrower in his group, to the borrower changing formation, to his having been behind
//! the borrower's starters in his group at the start (not a starter on arrival), or to none of these.
//! Reported by the season the loan started in, in bands.

use std::collections::{HashMap, HashSet};

use domain::player::Player;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

const IDLE_AFTER_DAYS: i64 = 60;
const IDLE_MINUTES: u32 = 180;

struct Track {
    season: u32,
    start_day: u32,
    for_minutes: bool,
    borrower: String,
    formation_at_start: String,
    group: usize,
    group_ids_at_start: HashSet<String>,
    ovr: u8,
    rank_in_group: usize,
    starters_in_group: usize,
    injured_days: u32,
    last_minutes: u32,
    last_day: u32,
    last_formation: String,
    overtaken: bool,
}

#[derive(Default, Clone)]
struct Band {
    loans: u32,
    long: u32,
    idle: u32,
    minutes_per_day: f64,
    days: f64,
    for_minutes_long: u32,
    for_minutes_idle: u32,
    not_starter_at_start_long: u32,
    not_starter_at_start_idle: u32,
    // Idle attribution (a loan can have several causes; counted in each).
    idle_injured: u32,
    idle_overtaken: u32,
    idle_formation: u32,
    idle_behind_at_start: u32,
    idle_none: u32,
    // Long loans and idle ones by [for minutes][behind the starters on arrival].
    cell_long: [[u32; 2]; 2],
    cell_idle: [[u32; 2]; 2],
}

#[derive(Default)]
struct Log {
    season: u32,
    day: u32,
    active: HashMap<String, Track>,
    bands: [Band; 4],
}

fn group_of(player: &Player) -> usize {
    use domain::player::Position;
    match player.position.to_group_position() {
        Position::Goalkeeper => 0,
        Position::Defender => 1,
        Position::Midfielder => 2,
        _ => 3,
    }
}

fn band_of(season: u32) -> usize {
    match season {
        0..=2 => 0,
        3..=5 => 1,
        6..=8 => 2,
        _ => 3,
    }
}

impl Log {
    fn finish(&mut self, track: Track) {
        let days = i64::from(track.last_day) - i64::from(track.start_day);
        let band = &mut self.bands[band_of(track.season)];
        band.loans += 1;
        band.minutes_per_day += f64::from(track.last_minutes);
        band.days += days.max(1) as f64;
        if days < IDLE_AFTER_DAYS {
            return;
        }
        band.long += 1;
        band.cell_long[usize::from(track.for_minutes)]
            [usize::from(track.rank_in_group > track.starters_in_group)] += 1;
        if track.last_minutes < IDLE_MINUTES {
            band.cell_idle[usize::from(track.for_minutes)]
                [usize::from(track.rank_in_group > track.starters_in_group)] += 1;
        }
        band.for_minutes_long += u32::from(track.for_minutes);
        let behind = track.rank_in_group > track.starters_in_group;
        band.not_starter_at_start_long += u32::from(behind);
        if track.last_minutes >= IDLE_MINUTES {
            return;
        }
        band.idle += 1;
        band.for_minutes_idle += u32::from(track.for_minutes);
        band.not_starter_at_start_idle += u32::from(behind);
        let injured = i64::from(track.injured_days) * 4 >= days;
        let overtaken = track.overtaken;
        let formation = track.last_formation != track.formation_at_start;
        band.idle_injured += u32::from(injured);
        band.idle_overtaken += u32::from(overtaken);
        band.idle_formation += u32::from(formation);
        band.idle_behind_at_start += u32::from(behind);
        if !(injured || overtaken || formation || behind) {
            band.idle_none += 1;
        }
    }
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        self.season = season;
        self.day = day;
        let by_id: HashMap<&str, &Player> =
            game.players.iter().map(|p| (p.id.as_str(), p)).collect();
        let today = game.clock.current_date.date_naive();
        let mut seen = HashSet::new();
        for player in &game.players {
            let Some(loan) = &player.active_loan else {
                continue;
            };
            seen.insert(player.id.clone());
            let borrower = loan.loan_team_id.clone();
            let formation = game
                .teams
                .iter()
                .find(|t| t.id == borrower)
                .map(|t| t.formation.clone())
                .unwrap_or_default();
            if let Some(track) = self.active.get_mut(&player.id) {
                track.last_minutes = player
                    .stats
                    .minutes_played
                    .saturating_sub(loan.loan_start_minutes);
                track.last_day = day + (season - track.season) * 450;
                track.last_formation = formation;
                track.injured_days += u32::from(player.injury.is_some());
                continue;
            }
            // A new loan.
            let group = group_of(player);
            let mut mates: Vec<&Player> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(borrower.as_str()))
                .filter(|p| p.id != player.id)
                .filter(|p| p.squad_role == domain::player::SquadRole::Senior)
                .filter(|p| group_of(p) == group)
                .collect();
            mates.sort_by_key(|m| std::cmp::Reverse(m.ovr));
            let rank = 1 + mates.iter().filter(|m| m.ovr > player.ovr).count();
            let starters = [1usize, 4, 4, 2][group];
            let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
            self.active.insert(
                player.id.clone(),
                Track {
                    season,
                    start_day: day,
                    for_minutes: age <= 22
                        && i32::from(player.potential) >= i32::from(player.ovr) + 6,
                    borrower: borrower.clone(),
                    formation_at_start: formation.clone(),
                    group,
                    group_ids_at_start: mates.iter().map(|m| m.id.clone()).collect(),
                    ovr: player.ovr,
                    rank_in_group: rank,
                    starters_in_group: starters,
                    injured_days: 0,
                    last_minutes: 0,
                    last_day: day,
                    last_formation: formation,
                    overtaken: false,
                },
            );
        }
        // Who joined the borrower in the loanee's group since the loan began.
        for track in self.active.values_mut() {
            if track.overtaken {
                continue;
            }
            track.overtaken = by_id.values().any(|p| {
                p.team_id.as_deref() == Some(track.borrower.as_str())
                    && p.squad_role == domain::player::SquadRole::Senior
                    && group_of(p) == track.group
                    && p.ovr >= track.ovr
                    && p.active_loan.is_none()
                    && !track.group_ids_at_start.contains(&p.id)
            });
        }
        let ended: Vec<String> = self
            .active
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in ended {
            if let Some(track) = self.active.remove(&id) {
                self.finish(track);
            }
        }
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_loans() {
    let seasons = env::first("P47_SEASONS", "10") as u32;
    let mut total: [Band; 4] = Default::default();
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let mut log = Log::default();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("plays");
        for (t, b) in total.iter_mut().zip(&log.bands) {
            t.loans += b.loans;
            t.long += b.long;
            t.idle += b.idle;
            t.minutes_per_day += b.minutes_per_day;
            t.days += b.days;
            t.for_minutes_long += b.for_minutes_long;
            t.for_minutes_idle += b.for_minutes_idle;
            t.not_starter_at_start_long += b.not_starter_at_start_long;
            t.not_starter_at_start_idle += b.not_starter_at_start_idle;
            t.idle_injured += b.idle_injured;
            t.idle_overtaken += b.idle_overtaken;
            t.idle_formation += b.idle_formation;
            t.idle_behind_at_start += b.idle_behind_at_start;
            t.idle_none += b.idle_none;
            for i in 0..2 {
                for j in 0..2 {
                    t.cell_long[i][j] += b.cell_long[i][j];
                    t.cell_idle[i][j] += b.cell_idle[i][j];
                }
            }
        }
    }
    eprintln!(
        "loan start season | loans | 60+ days | idle (% of 60+) | for-minutes: 60+ / idle | not a starter on arrival: 60+ / idle | minutes per loan day | idle causes (injured / better player joined / formation changed / behind starters at start / none)"
    );
    for (index, b) in total.iter().enumerate() {
        let label = ["1-3", "4-6", "7-9", "10"][index];
        eprintln!(
            "{label:>4} | {} | {} | {} ({:.0}%) | {} / {} | {} / {} | {:.2} | {} / {} / {} / {} / {}",
            b.loans,
            b.long,
            b.idle,
            100.0 * f64::from(b.idle) / f64::from(b.long.max(1)),
            b.for_minutes_long,
            b.for_minutes_idle,
            b.not_starter_at_start_long,
            b.not_starter_at_start_idle,
            b.minutes_per_day / b.days.max(1.0),
            b.idle_injured,
            b.idle_overtaken,
            b.idle_formation,
            b.idle_behind_at_start,
            b.idle_none,
        );
    }
    eprintln!("idle rate by purpose and role on arrival (60+ day loans: idle / loans):");
    for (index, b) in total.iter().enumerate() {
        let label = ["1-3", "4-6", "7-9", "10"][index];
        let cell = |i: usize, j: usize| format!("{}/{}", b.cell_idle[i][j], b.cell_long[i][j]);
        eprintln!(
            "{label:>4} | for minutes, starter on arrival {} | for minutes, behind starters {} | others, starter on arrival {} | others, behind starters {}",
            cell(1, 0),
            cell(1, 1),
            cell(0, 0),
            cell(0, 1)
        );
    }
}
