//! P4.7: why does the prospect cohort shrink and why do prospects not play? Measurement only
//! (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test p47_youth_causes -- --ignored --nocapture`
//!
//! The cohort is every AI-club player aged 21 or under with at least six points of room to grow
//! (the same one `career_study` counts). At each season's end the previous cohort is closed, with
//! what became of each member, and a new one is opened. Reported per season, over all seeds:
//! the cohort's size, how it left (aged out, matured to under six room, released, retired, sold to
//! the player's club, or stayed), how it was filled (players new to the world, or known players
//! entering), and for the members still at an AI club their minutes by squad role and loan status,
//! and whether each was good enough to be in the club's rotation (as good as its first_cover-best
//! senior) or in its first eleven (within two points of its eleventh-best).

use std::collections::{HashMap, HashSet};

use domain::player::SquadRole;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

#[derive(Default, Clone)]
struct Season {
    opened: u32,
    stayed: u32,
    aged_out: u32,
    matured: u32,
    released: u32,
    retired: u32,
    to_user: u32,
    vanished: u32,
    new_to_world: u32,
    known_entrants: u32,
    // Minutes, for members still at an AI club at the close.
    academy: Bucket,
    senior: Bucket,
    on_loan: Bucket,
    // Readiness by role: [senior, academy].
    rotation_ready: [Bucket; 2],
    xi_ready: [Bucket; 2],
    not_ready: [Bucket; 2],
}

#[derive(Default, Clone)]
struct Bucket {
    n: u32,
    minutes: f64,
    under_450: u32,
}

impl Bucket {
    fn add(&mut self, minutes: u32) {
        self.n += 1;
        self.minutes += f64::from(minutes);
        self.under_450 += u32::from(minutes < 450);
    }
    fn show(&self) -> String {
        if self.n == 0 {
            return "-".into();
        }
        format!(
            "{} ({:.0} min, {:.0}% <450)",
            self.n,
            self.minutes / f64::from(self.n),
            100.0 * f64::from(self.under_450) / f64::from(self.n)
        )
    }
}

#[derive(Default)]
struct Log {
    cohort: HashSet<String>,
    seen: HashSet<String>,
    seasons: Vec<Season>,
    started: bool,
}

fn group_of(player: &domain::player::Player) -> usize {
    use domain::player::Position;
    match player.position.to_group_position() {
        Position::Goalkeeper => 0,
        Position::Defender => 1,
        Position::Midfielder => 2,
        _ => 3,
    }
}

fn is_prospect(player: &domain::player::Player, today: chrono::NaiveDate) -> bool {
    let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
    age <= 21 && i32::from(player.potential) >= i32::from(player.ovr) + 6
}

impl Log {
    fn close_and_open(&mut self, game: &Game) {
        let today = game.clock.current_date.date_naive();
        let user = game.manager.team_id.as_deref();
        let mut season = Season::default();
        // Cutoffs per club, from its seniors.
        // Seniors' ratings by club and position group (keeper, defence, midfield, attack).
        let mut by_club: HashMap<(&str, usize), Vec<u8>> = HashMap::new();
        for player in &game.players {
            if let (Some(team), SquadRole::Senior) = (player.team_id.as_deref(), &player.squad_role)
            {
                by_club
                    .entry((team, group_of(player)))
                    .or_default()
                    .push(player.ovr);
            }
        }
        for ovrs in by_club.values_mut() {
            ovrs.sort_unstable_by(|a, b| b.cmp(a));
        }
        if self.started {
            let by_id: HashMap<&str, &domain::player::Player> =
                game.players.iter().map(|p| (p.id.as_str(), p)).collect();
            for id in &self.cohort {
                let Some(player) = by_id.get(id.as_str()) else {
                    season.vanished += 1;
                    continue;
                };
                if player.retired {
                    season.retired += 1;
                    continue;
                }
                let Some(team) = player.team_id.as_deref() else {
                    season.released += 1;
                    continue;
                };
                if Some(team) == user {
                    season.to_user += 1;
                    continue;
                }
                let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
                if age >= 22 {
                    season.aged_out += 1;
                } else if !is_prospect(player, today) {
                    season.matured += 1;
                } else {
                    season.stayed += 1;
                }
                let minutes = player.stats.minutes_played;
                if player.squad_role == SquadRole::Youth {
                    season.academy.add(minutes);
                } else {
                    season.senior.add(minutes);
                }
                if player.active_loan.is_some() {
                    season.on_loan.add(minutes);
                }
                // Ready is read inside his own position group: as good as the club's last
                // starter there (within two points), or as good as its first cover.
                let group = group_of(player);
                let squad = by_club.get(&(team, group)).map_or(&[][..], Vec::as_slice);
                let starters = [1usize, 4, 4, 2][group];
                let last_starter = squad.get(starters - 1).copied().unwrap_or(0);
                let first_cover = squad.get(starters).copied().unwrap_or(0);
                let role = usize::from(player.squad_role == SquadRole::Youth);
                if i32::from(player.ovr) >= i32::from(last_starter) - 2 {
                    if role == 1 && std::env::var("P47_DEBUG").is_ok() {
                        eprintln!(
                            "  ready academy player: {} {:?} age {} ovr {}/{} injured {} | club {} group seniors {:?}",
                            player.id,
                            player.position,
                            age,
                            player.ovr,
                            player.potential,
                            player.injury.is_some(),
                            team,
                            &squad[..squad.len().min(6)],
                        );
                    }
                    season.xi_ready[role].add(minutes);
                } else if player.ovr >= first_cover {
                    season.rotation_ready[role].add(minutes);
                } else {
                    season.not_ready[role].add(minutes);
                }
            }
        }
        let mut next = HashSet::new();
        for player in &game.players {
            let Some(team) = player.team_id.as_deref() else {
                continue;
            };
            if Some(team) == user || player.retired || !is_prospect(player, today) {
                continue;
            }
            if !self.cohort.contains(&player.id) {
                if self.seen.contains(&player.id) {
                    season.known_entrants += 1;
                } else {
                    season.new_to_world += 1;
                }
            }
            next.insert(player.id.clone());
        }
        season.opened = next.len() as u32;
        self.seen.extend(game.players.iter().map(|p| p.id.clone()));
        self.cohort = next;
        if self.started {
            self.seasons.push(season);
        }
        self.started = true;
    }
}

impl SeasonObserver for Log {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        self.close_and_open(game);
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_youth_causes() {
    let seasons = env::first("P47_SEASONS", "10") as usize;
    let mut total: Vec<Season> = vec![Season::default(); seasons];
    let mut opening = 0u32;
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let mut log = Log::default();
        log.close_and_open(&game);
        opening += log.cohort.len() as u32;
        run_seasons(&mut game, RunOptions::seasons(seasons as u32), &mut log).expect("plays");
        for (index, s) in log.seasons.iter().enumerate().take(seasons) {
            let t = &mut total[index];
            t.opened += s.opened;
            t.stayed += s.stayed;
            t.aged_out += s.aged_out;
            t.matured += s.matured;
            t.released += s.released;
            t.retired += s.retired;
            t.to_user += s.to_user;
            t.vanished += s.vanished;
            t.new_to_world += s.new_to_world;
            t.known_entrants += s.known_entrants;
            let merge = |a: &mut Bucket, b: &Bucket| {
                a.n += b.n;
                a.minutes += b.minutes;
                a.under_450 += b.under_450;
            };
            merge(&mut t.academy, &s.academy);
            merge(&mut t.senior, &s.senior);
            merge(&mut t.on_loan, &s.on_loan);
            for role in 0..2 {
                merge(&mut t.rotation_ready[role], &s.rotation_ready[role]);
                merge(&mut t.xi_ready[role], &s.xi_ready[role]);
                merge(&mut t.not_ready[role], &s.not_ready[role]);
            }
        }
    }
    eprintln!("opening cohort (all seeds): {opening}");
    eprintln!(
        "season | cohort at end | previous cohort: stayed / aged out / matured / released / retired / sold to user / vanished | filled by: new to world / known players"
    );
    for (index, s) in total.iter().enumerate() {
        eprintln!(
            "{:>3} | {:>5} | {} / {} / {} / {} / {} / {} / {} | {} / {}",
            index + 1,
            s.opened,
            s.stayed,
            s.aged_out,
            s.matured,
            s.released,
            s.retired,
            s.to_user,
            s.vanished,
            s.new_to_world,
            s.known_entrants
        );
    }
    eprintln!(
        "season | first-eleven-ready: senior | academy || rotation-ready: senior | academy || not ready: senior | academy"
    );
    for (index, s) in total.iter().enumerate() {
        eprintln!(
            "{:>3} | {} | {} || {} | {} || {} | {}",
            index + 1,
            s.xi_ready[0].show(),
            s.xi_ready[1].show(),
            s.rotation_ready[0].show(),
            s.rotation_ready[1].show(),
            s.not_ready[0].show(),
            s.not_ready[1].show()
        );
    }
}
