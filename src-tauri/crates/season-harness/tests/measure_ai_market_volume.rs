//! How much the AI transfer market does in a season, and why (ALPHA6.1 section 4).
//!
//! Measurement only, `#[ignore]`d, release mode, the FM24 world named by `APFM_WORLD_JSON`:
//!
//! `APFM_WORLD_JSON=… cargo test --release -p season-harness --test measure_ai_market_volume -- --ignored --nocapture`
//!
//! `APFM_SEEDS` (default `1,2,3`) and `APFM_SEASONS` (default `2`) choose how much is played. For
//! every season of every seed it prints one `VOLUME` line (what the market did, by kind of move,
//! between AI clubs only) and one `FUNNEL` line (where the buyers' turns ended): the questions are
//! the ones the brief names: who shopped, who found a target, who passed the tests, who could pay,
//! whom the seller or the player refused, and what completed.

use std::collections::{BTreeMap, HashMap, HashSet};

use domain::player::{PlayerMovementEntry, PlayerMovementKind};
use ofm_core::game::Game;
use ofm_core::transfers::{
    FUNNEL_NAMES, FUNNEL_STAGES, market_buyer_evaluations, market_execution_failures,
    market_funnel, market_participation, reset_market_funnel,
};
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const LENS: &str = "fm-c-501";

#[derive(Default, Clone)]
struct Season {
    permanent: u32,
    permanent_in_scope_buyer: u32,
    permanent_cross_border: u32,
    permanent_fees: Vec<u64>,
    loans_started: u32,
    free_agent_signings: u32,
    released: u32,
    window_open_days: u32,
    /// AI clubs at or over the 30-place senior list, summed over the days the window was open.
    full_club_days: u32,
    full_clubs_ever: HashSet<String>,
    /// Seniors registered at AI clubs on the first window day, as a histogram.
    opening_seniors: BTreeMap<usize, usize>,
}

struct Volume {
    user: String,
    seed: u64,
    ledger_len: HashMap<String, usize>,
    country: HashMap<String, String>,
    in_scope: HashSet<String>,
    current: Season,
    funnel_base: [u64; FUNNEL_STAGES],
    participation_base: [u64; ofm_core::transfers::PARTICIPATION_COUNTERS],
    evaluations_base: u64,
}

impl Volume {
    fn new(game: &Game, seed: u64) -> Self {
        Self {
            user: game.manager.team_id.clone().unwrap_or_default(),
            seed,
            ledger_len: game
                .players
                .iter()
                .map(|p| (p.id.clone(), p.movement_history.len()))
                .collect(),
            country: game
                .teams
                .iter()
                .map(|t| (t.id.clone(), t.country.clone()))
                .collect(),
            in_scope: game.active_team_ids().unwrap_or_default(),
            current: Season::default(),
            funnel_base: market_funnel(),
            participation_base: market_participation(),
            evaluations_base: market_buyer_evaluations(),
        }
    }

    fn involves_user(&self, entry: &PlayerMovementEntry) -> bool {
        entry.from_team_id.as_deref() == Some(self.user.as_str())
            || entry.to_team_id.as_deref() == Some(self.user.as_str())
    }
}

fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}

impl SeasonObserver for Volume {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        if game.season_context.transfer_window.days_remaining.is_some() {
            self.current.window_open_days += 1;
            let mut seniors: HashMap<&str, usize> = HashMap::new();
            for player in &game.players {
                if player.squad_role == domain::player::SquadRole::Senior && !player.retired {
                    if let Some(club) = player.team_id.as_deref() {
                        *seniors.entry(club).or_default() += 1;
                    }
                }
            }
            let mut full = 0;
            for (club, count) in &seniors {
                if *club == self.user {
                    continue;
                }
                if *count >= 30 {
                    full += 1;
                    self.current.full_clubs_ever.insert((*club).to_string());
                }
                if self.current.window_open_days == 1 {
                    *self.current.opening_seniors.entry(*count).or_default() += 1;
                }
            }
            self.current.full_club_days += full;
        }
        for player in &game.players {
            let seen = self.ledger_len.get(&player.id).copied().unwrap_or(0);
            for entry in player.movement_history.iter().skip(seen) {
                if self.involves_user(entry) {
                    continue;
                }
                match entry.kind {
                    PlayerMovementKind::PermanentTransfer => {
                        self.current.permanent += 1;
                        if let Some(fee) = entry.fee {
                            self.current.permanent_fees.push(fee);
                        }
                        let (from, to) = (
                            entry.from_team_id.as_deref().unwrap_or(""),
                            entry.to_team_id.as_deref().unwrap_or(""),
                        );
                        if self.in_scope.contains(to) {
                            self.current.permanent_in_scope_buyer += 1;
                        }
                        if self.country.get(from) != self.country.get(to) {
                            self.current.permanent_cross_border += 1;
                        }
                    }
                    PlayerMovementKind::LoanStart => self.current.loans_started += 1,
                    PlayerMovementKind::FreeAgentSigning => self.current.free_agent_signings += 1,
                    PlayerMovementKind::Released => self.current.released += 1,
                    PlayerMovementKind::LoanReturn
                    | PlayerMovementKind::LoanToBuy
                    | PlayerMovementKind::Renewal
                    | PlayerMovementKind::Retired
                    | PlayerMovementKind::InitialContract => {}
                }
            }
            self.ledger_len
                .insert(player.id.clone(), player.movement_history.len());
        }
    }

    fn on_before_rollover(&mut self, _game: &Game, season: u32) {
        let mut season_row = std::mem::take(&mut self.current);
        println!(
            "CAPACITY seed {} season {season}: AI clubs at the 30-place senior list: {:.1} a day on average over {} window days, {} different clubs; seniors per club on the first window day: {:?}",
            self.seed,
            f64::from(season_row.full_club_days) / f64::from(season_row.window_open_days.max(1)),
            season_row.window_open_days,
            season_row.full_clubs_ever.len(),
            season_row.opening_seniors,
        );
        let fee_median = median(&mut season_row.permanent_fees);
        println!(
            "VOLUME seed {} season {season}: permanent {} (cross-border {}, bought by clubs in the player's scope {}, median fee {fee_median}), loans started {}, free-agent signings {}, releases {}, window days {}",
            self.seed,
            season_row.permanent,
            season_row.permanent_cross_border,
            season_row.permanent_in_scope_buyer,
            season_row.loans_started,
            season_row.free_agent_signings,
            season_row.released,
            season_row.window_open_days,
        );
        let funnel = market_funnel();
        let delta: BTreeMap<&str, u64> = FUNNEL_NAMES
            .iter()
            .enumerate()
            .filter(|(_, name)| name.starts_with("ai_") || **name == "buyers_shopping")
            .map(|(i, name)| (*name, funnel[i] - self.funnel_base[i]))
            .collect();
        let participation = market_participation();
        let part: Vec<u64> = participation
            .iter()
            .zip(self.participation_base)
            .map(|(now, before)| now - before)
            .collect();
        println!(
            "FUNNEL seed {} season {season}: buyer_turns {} participation[considered,pos_need,squad_need,loan_need,shopped,review_day,closing_need,whole_market]={part:?} {delta:?}",
            self.seed,
            market_buyer_evaluations() - self.evaluations_base,
        );
        println!(
            "REFUSED AT EXECUTION seed {} season {season}: {:?}",
            self.seed,
            market_execution_failures()
        );
        reset_market_funnel();
        self.funnel_base = market_funnel();
        self.participation_base = participation;
        self.evaluations_base = market_buyer_evaluations();
    }
}

/// EXPERIMENT, not a proposal: every AI club above `limit` seniors is cut to it before the first
/// day by registering its weakest seniors on the reserve list. It isolates one cause: with
/// `APFM_TRIM_AI_LISTS=29` the season is played in a world where no AI club starts at the limit.
fn trim_ai_lists(game: &mut Game, user: &str, limit: usize) -> usize {
    let mut by_club: HashMap<String, Vec<(u8, String)>> = HashMap::new();
    for player in &game.players {
        if player.squad_role == domain::player::SquadRole::Senior
            && !player.retired
            && player.active_loan.is_none()
        {
            if let Some(club) = player.team_id.as_deref().filter(|club| *club != user) {
                by_club
                    .entry(club.to_string())
                    .or_default()
                    .push((player.ovr, player.id.clone()));
            }
        }
    }
    let mut demoted = HashSet::new();
    for seniors in by_club.values_mut() {
        seniors.sort_by(|a, b| b.cmp(a));
        for (_, id) in seniors.iter().skip(limit) {
            demoted.insert(id.clone());
        }
    }
    for player in &mut game.players {
        if demoted.contains(&player.id) {
            player.squad_role = domain::player::SquadRole::Youth;
        }
    }
    demoted.len()
}

#[test]
#[ignore = "measurement: prints the AI market's volume and funnel per season"]
fn ai_market_volume_and_funnel_per_season() {
    let Some(path) = world_path_from_env() else {
        println!("SKIPPED: APFM_WORLD_JSON is not set");
        return;
    };
    let seeds: Vec<u64> = std::env::var("APFM_SEEDS")
        .unwrap_or_else(|_| "1,2,3".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let seasons: u32 = std::env::var("APFM_SEASONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    for seed in seeds {
        let mut game = build_fm24_career(&path, LENS, seed).expect("the career builds");
        if let Some(limit) = std::env::var("APFM_TRIM_AI_LISTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            let user = game.manager.team_id.clone().unwrap_or_default();
            let demoted = trim_ai_lists(&mut game, &user, limit);
            println!(
                "EXPERIMENT trimmed AI senior lists to {limit}: {demoted} seniors moved to the reserve list"
            );
        }
        let mut volume = Volume::new(&game, seed);
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut volume).expect("seasons play");
    }
}
