//! P4.7: how AI clubs' formations behave over a career. Measurement only (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test p47_formations -- --ignored --nocapture`
//!
//! Every day the formation of every AI club is read. Reports, per season: distinct formations and
//! the largest share; changes per club; clubs that changed more than twice; flip-flops (a club back
//! in the formation it left within 60 days); how many changes came within a month of a new
//! manager; the squad model's score for the formation played against the best formation on offer
//! (suitability, in rating points of the mean of the eleven slots); and, pooled, points a game
//! beyond what strength predicts for each formation (a formation that wins beyond its strength
//! would be one the engine rewards for the shape itself).

use std::collections::{BTreeMap, HashMap};

use domain::league::CompetitionType;
use ofm_core::ai_formation::read_all_clubs;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

#[derive(Default)]
struct SeasonRow {
    changes: u32,
    changers: HashMap<String, u32>,
    flips: u32,
    after_manager_change: u32,
    deficit: Vec<f64>,
    shares: BTreeMap<String, u32>,
}

#[derive(Default)]
struct Log {
    last: HashMap<String, String>,
    history: HashMap<String, Vec<(u32, String)>>,
    manager_of: HashMap<String, (String, u32)>,
    day_count: u32,
    seasons: Vec<SeasonRow>,
    current: SeasonRow,
    resid_rows: Vec<(String, f64, f64, usize)>,
    league_counter: usize,
}

impl SeasonObserver for Log {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        self.day_count += 1;
        let user = game.manager.team_id.as_deref();
        for team in &game.teams {
            if Some(team.id.as_str()) == user {
                continue;
            }
            if let Some(manager) = team.manager_id.as_deref() {
                let entry = self
                    .manager_of
                    .entry(team.id.clone())
                    .or_insert((manager.to_string(), self.day_count));
                if entry.0 != manager {
                    *entry = (manager.to_string(), self.day_count);
                }
            }
            let previous = self.last.insert(team.id.clone(), team.formation.clone());
            if let Some(previous) = previous
                && previous != team.formation
            {
                self.current.changes += 1;
                *self.current.changers.entry(team.id.clone()).or_default() += 1;
                let history = self.history.entry(team.id.clone()).or_default();
                if history
                    .iter()
                    .rev()
                    .take(2)
                    .any(|(day, f)| self.day_count - day <= 60 && *f == team.formation)
                {
                    self.current.flips += 1;
                }
                history.push((self.day_count, previous));
                if self
                    .manager_of
                    .get(&team.id)
                    .is_some_and(|(_, since)| self.day_count - since <= 35)
                {
                    self.current.after_manager_change += 1;
                }
            }
        }
    }

    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let readings = read_all_clubs(game);
        for reading in &readings {
            let own = reading
                .scores
                .iter()
                .find(|s| s.formation == reading.current);
            let best = reading
                .scores
                .iter()
                .map(|s| s.squad)
                .fold(f64::MIN, f64::max);
            if let Some(own) = own {
                self.current.deficit.push(best - own.squad);
            }
            *self
                .current
                .shares
                .entry(reading.current.clone())
                .or_default() += 1;
        }
        let by_club: HashMap<&str, Vec<u8>> = {
            let mut m: HashMap<&str, Vec<u8>> = HashMap::new();
            for p in &game.players {
                if let Some(t) = p.team_id.as_deref() {
                    m.entry(t).or_default().push(p.ovr);
                }
            }
            m
        };
        for league in &game.competitions {
            if league.kind != CompetitionType::League || league.standings.len() < 8 {
                continue;
            }
            self.league_counter += 1;
            for entry in &league.standings {
                let Some(reading) = readings.iter().find(|r| r.team_id == entry.team_id) else {
                    continue;
                };
                let mut ovr = by_club
                    .get(entry.team_id.as_str())
                    .cloned()
                    .unwrap_or_default();
                ovr.sort_unstable_by(|a, b| b.cmp(a));
                ovr.truncate(14);
                if ovr.is_empty() || entry.played == 0 {
                    continue;
                }
                let strength = ovr.iter().map(|o| f64::from(*o)).sum::<f64>() / ovr.len() as f64;
                self.resid_rows.push((
                    reading.current.clone(),
                    f64::from(entry.points) / f64::from(entry.played),
                    strength,
                    self.league_counter,
                ));
            }
        }
        self.seasons.push(std::mem::take(&mut self.current));
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_formations() {
    let seasons = env::first("P47_SEASONS", "10") as u32;
    let mut pooled: Vec<SeasonRow> = Vec::new();
    let mut resid: Vec<(String, f64, f64, usize)> = Vec::new();
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let mut log = Log::default();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        let offset = resid.iter().map(|r| r.3).max().unwrap_or(0);
        resid.extend(
            log.resid_rows
                .iter()
                .map(|r| (r.0.clone(), r.1, r.2, r.3 + offset)),
        );
        eprintln!("seed {seed}");
        for (index, row) in log.seasons.iter().enumerate() {
            let clubs: u32 = row.shares.values().sum();
            let top = row.shares.values().max().copied().unwrap_or(0);
            let over_two = row.changers.values().filter(|c| **c > 2).count();
            let mean_deficit = row.deficit.iter().sum::<f64>() / row.deficit.len().max(1) as f64;
            eprintln!(
                "  season {:>2} | formations {} (top {:.0}%) | changes {} ({:.2}/club) | >2 changes {over_two} | flips {} | within a month of a new manager {} | deficit vs best {mean_deficit:.2} | {:?}",
                index + 1,
                row.shares.len(),
                100.0 * f64::from(top) / f64::from(clubs.max(1)),
                row.changes,
                f64::from(row.changes) / f64::from(clubs.max(1)),
                row.flips,
                row.after_manager_change,
                row.shares
            );
        }
        pooled.extend(log.seasons);
    }
    let club_seasons: u32 = pooled.iter().map(|r| r.shares.values().sum::<u32>()).sum();
    let changes: u32 = pooled.iter().map(|r| r.changes).sum();
    let flips: u32 = pooled.iter().map(|r| r.flips).sum();
    let monthly: u32 = pooled.iter().map(|r| r.after_manager_change).sum();
    eprintln!(
        "pooled: club-seasons {club_seasons} | changes per club-season {:.3} | flip-flops {flips} ({:.1}% of changes) | changes within a month of a new manager {monthly} ({:.0}%)",
        f64::from(changes) / f64::from(club_seasons.max(1)),
        100.0 * f64::from(flips) / f64::from(changes.max(1)),
        100.0 * f64::from(monthly) / f64::from(changes.max(1))
    );
    // Points per game beyond strength, by formation, within league.
    let mut sums: HashMap<usize, (f64, f64, f64)> = HashMap::new();
    for (_, ppg, strength, league) in &resid {
        let e = sums.entry(*league).or_default();
        e.0 += ppg;
        e.1 += strength;
        e.2 += 1.0;
    }
    let (mut sxx, mut sxy) = (0.0, 0.0);
    let dm: Vec<(String, f64, f64)> = resid
        .iter()
        .map(|(f, p, s, l)| {
            let (sp, ss, n) = sums[l];
            (f.clone(), p - sp / n, s - ss / n)
        })
        .collect();
    for (_, p, s) in &dm {
        sxx += s * s;
        sxy += s * p;
    }
    let slope = sxy / sxx;
    let sd = (dm
        .iter()
        .map(|(_, p, s)| (p - slope * s).powi(2))
        .sum::<f64>()
        / dm.len() as f64)
        .sqrt();
    let mut by_formation: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (f, p, s) in &dm {
        let e = by_formation.entry(f.as_str()).or_default();
        e.0 += 1.0;
        e.1 += p - slope * s;
    }
    eprintln!(
        "points per game beyond strength, by formation (mean, standard error, club-seasons):"
    );
    for (f, (n, r)) in by_formation {
        eprintln!("  {f:<8} {:+.3} (±{:.3}) n {n}", r / n, sd / n.sqrt());
    }
}
