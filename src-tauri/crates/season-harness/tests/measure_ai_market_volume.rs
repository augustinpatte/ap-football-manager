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
    market_funnel, market_participation, market_sweep_nanos, reset_market_funnel,
    reset_room_making_report, room_making_report, set_room_making_enabled,
};
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use season_harness::worlds::WorldSpec;

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
    /// AI-club-days with more than 30 registered seniors: the registration limit broken.
    over_limit_club_days: u32,
    /// Times an AI club went from 30 registered seniors or fewer to more than 30 between two days:
    /// the limit broken by something that happened, as against a world that opened over it.
    crossed_the_limit: u32,
    /// Times an AI club that met the squad floor (15 seniors, 2/4/4/2 by group) yesterday did not
    /// meet it today.
    crossed_the_floor: u32,
    /// Players with two permanent transfers on one day (a duplicate movement).
    duplicate_movements: u32,
    /// Mean rating of the best 18 seniors of the AI clubs, on the first window day.
    top18_at_window_open: Option<f64>,
}

/// Whether each AI club meets the squad floor today (`squad_floor`), read in one pass over the world.
fn floor_met_by_club(game: &Game, user: &str) -> HashMap<String, bool> {
    let mut by_club: HashMap<&str, ([usize; 4], usize)> = HashMap::new();
    for player in &game.players {
        if player.squad_role == domain::player::SquadRole::Senior
            && !player.retired
            && let Some(club) = player.team_id.as_deref().filter(|club| *club != user)
        {
            let entry = by_club.entry(club).or_default();
            entry.1 += 1;
            let group = match player.position.to_group_position() {
                domain::player::Position::Goalkeeper => 0,
                domain::player::Position::Defender => 1,
                domain::player::Position::Midfielder => 2,
                _ => 3,
            };
            entry.0[group] += 1;
        }
    }
    by_club
        .into_iter()
        .map(|(club, (groups, total))| {
            let met = total >= ofm_core::squad_floor::MIN_SENIOR_PLAYERS
                && groups
                    .iter()
                    .zip(ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP)
                    .all(|(held, (_, floor))| *held >= floor);
            (club.to_string(), met)
        })
        .collect()
}

/// Mean rating of every AI club's best 18 seniors, the quality of the squads the market works on.
fn mean_top18(game: &Game, user: &str) -> f64 {
    let mut by_club: HashMap<&str, Vec<u8>> = HashMap::new();
    for player in &game.players {
        if player.squad_role == domain::player::SquadRole::Senior
            && !player.retired
            && let Some(club) = player.team_id.as_deref().filter(|club| *club != user)
        {
            by_club.entry(club).or_default().push(player.ovr);
        }
    }
    let (mut sum, mut n) = (0_u64, 0_u64);
    for ratings in by_club.values_mut() {
        ratings.sort_unstable_by(|a, b| b.cmp(a));
        for ovr in ratings.iter().take(18) {
            sum += u64::from(*ovr);
            n += 1;
        }
    }
    sum as f64 / n.max(1) as f64
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
    sweep_nanos_base: u64,
    /// The harness's own rules for any run (referential integrity, finances in range, ...).
    invariants: Invariants,
    seniors_yesterday: HashMap<String, usize>,
    floor_met_yesterday: HashMap<String, bool>,
    senior_ids_yesterday: HashMap<String, HashSet<String>>,
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
            sweep_nanos_base: market_sweep_nanos(),
            invariants: Invariants::new(game),
            seniors_yesterday: HashMap::new(),
            floor_met_yesterday: HashMap::new(),
            senior_ids_yesterday: HashMap::new(),
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
    fn on_season_end(
        &mut self,
        game: &Game,
        summary: &ofm_core::end_of_season::EndOfSeasonSummary,
        season: u32,
    ) {
        self.invariants.on_season_end(game, summary, season);
    }

    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        self.invariants.on_day_end(game, season, day);
        let mut seniors_today: HashMap<String, usize> = HashMap::new();
        for player in &game.players {
            if player.squad_role == domain::player::SquadRole::Senior
                && !player.retired
                && let Some(club) = player.team_id.as_deref().filter(|club| *club != self.user)
            {
                *seniors_today.entry(club.to_string()).or_default() += 1;
            }
        }
        for (club, count) in &seniors_today {
            if *count > 30
                && self
                    .seniors_yesterday
                    .get(club)
                    .is_some_and(|before| *before <= 30)
            {
                self.current.crossed_the_limit += 1;
                if std::env::var("APFM_SHOW_CROSSINGS").is_ok() {
                    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
                    let moves: Vec<String> = game
                        .players
                        .iter()
                        .filter(|p| {
                            p.movement_history.last().is_some_and(|m| {
                                m.date == today
                                    && (m.to_team_id.as_deref() == Some(club.as_str())
                                        || m.from_team_id.as_deref() == Some(club.as_str()))
                            })
                        })
                        .map(|p| {
                            format!(
                                "{} {:?}",
                                p.id,
                                p.movement_history.last().map(|m| (
                                    &m.kind,
                                    &m.from_team_id,
                                    &m.to_team_id
                                ))
                            )
                        })
                        .collect();
                    let newcomers: Vec<String> = game
                        .players
                        .iter()
                        .filter(|p| {
                            p.team_id.as_deref() == Some(club.as_str())
                                && p.squad_role == domain::player::SquadRole::Senior
                                && !p.retired
                                && !self
                                    .senior_ids_yesterday
                                    .get(club)
                                    .is_some_and(|ids| ids.contains(&p.id))
                        })
                        .map(|p| {
                            format!(
                                "{} dob {} loan {:?} last-move {:?}",
                                p.id,
                                p.date_of_birth,
                                p.active_loan
                                    .as_ref()
                                    .map(|l| (&l.parent_team_id, &l.loan_team_id)),
                                p.movement_history.last().map(|m| (&m.date, &m.kind))
                            )
                        })
                        .collect();
                    println!(
                        "CROSSING {club} on {today}: {count} seniors; today's moves {moves:?}; newly senior {newcomers:?}"
                    );
                }
            }
        }
        if std::env::var("APFM_SHOW_CROSSINGS").is_ok() {
            self.senior_ids_yesterday.clear();
            for player in &game.players {
                if player.squad_role == domain::player::SquadRole::Senior
                    && !player.retired
                    && let Some(club) = player.team_id.as_deref()
                {
                    self.senior_ids_yesterday
                        .entry(club.to_string())
                        .or_default()
                        .insert(player.id.clone());
                }
            }
        }
        let floor_today = floor_met_by_club(game, &self.user);
        for (club, met) in &floor_today {
            if !met && self.floor_met_yesterday.get(club).copied().unwrap_or(true) {
                self.current.crossed_the_floor += 1;
            }
        }
        self.floor_met_yesterday = floor_today;
        self.seniors_yesterday = seniors_today;
        if game.season_context.transfer_window.days_remaining.is_some() {
            self.current.window_open_days += 1;
            let mut seniors: HashMap<&str, usize> = HashMap::new();
            for player in &game.players {
                if player.squad_role == domain::player::SquadRole::Senior
                    && !player.retired
                    && let Some(club) = player.team_id.as_deref()
                {
                    *seniors.entry(club).or_default() += 1;
                }
            }
            let mut full = 0;
            if self.current.window_open_days == 1 {
                self.current.top18_at_window_open = Some(mean_top18(game, &self.user));
            }
            for (club, count) in &seniors {
                if *club == self.user {
                    continue;
                }
                if *count > 30 {
                    self.current.over_limit_club_days += 1;
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
            let new_permanent = player
                .movement_history
                .iter()
                .skip(seen)
                .filter(|entry| entry.kind == PlayerMovementKind::PermanentTransfer)
                .count();
            if new_permanent > 1 {
                self.current.duplicate_movements += 1;
            }
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

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        self.invariants.on_before_rollover(game, season);
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
        let sweep_seconds = (market_sweep_nanos() - self.sweep_nanos_base) as f64 / 1e9;
        println!(
            "QUALITY_AND_LIMITS seed {} season {season}: AI top-18 mean rating {:.3} at window open, {:.3} before rollover; club-days over the 30-senior limit {} (of which new crossings from 30 or fewer: {}); new floor breaches {}; duplicate same-day permanent transfers {}; market sweep time {sweep_seconds:.1}s over {} window days",
            self.seed,
            season_row.top18_at_window_open.unwrap_or(0.0),
            mean_top18(game, &self.user),
            season_row.over_limit_club_days,
            season_row.crossed_the_limit,
            season_row.crossed_the_floor,
            season_row.duplicate_movements,
            season_row.window_open_days,
        );
        // Money: every club's cash equals what its journal says, and the transfer fees posted
        // across the world net to nothing (a fee is one atomic pair of posts).
        let mut journal_sum: HashMap<&str, i64> = HashMap::new();
        let mut fee_posts_net = 0_i64;
        for post in game.cash_journal.iter() {
            *journal_sum.entry(post.club_id.as_str()).or_default() += post.amount;
            if matches!(
                post.kind,
                domain::finance::CashKind::TransferFeeIn
                    | domain::finance::CashKind::TransferFeeOut
            ) {
                fee_posts_net += post.amount;
            }
        }
        let cash_mismatches = game
            .teams
            .iter()
            .filter(|team| journal_sum.get(team.id.as_str()).copied().unwrap_or(0) != team.finance)
            .count();
        println!(
            "MONEY seed {} season {season}: clubs whose cash differs from their journal {cash_mismatches}; transfer-fee posts net across the world {fee_posts_net}; AI clubs in debt {}; mean AI-club cash {:.0}",
            self.seed,
            game.teams
                .iter()
                .filter(|team| team.id != self.user && team.finance < 0)
                .count(),
            game.teams
                .iter()
                .filter(|team| team.id != self.user)
                .map(|team| team.finance as f64)
                .sum::<f64>()
                / game.teams.len().max(1) as f64,
        );
        let room = room_making_report();
        println!(
            "ROOM seed {} season {season}: clubs that found a target but lacked space {}, clubs that attempted to free space {}, clubs that freed a place {}, clubs refused {} {:?}",
            self.seed,
            room.clubs_found_target_lacked_space,
            room.clubs_attempted,
            room.clubs_freed_a_place,
            room.clubs_refused,
            room.counters,
        );
        let completed: Vec<_> = room
            .swaps
            .iter()
            .filter(|swap| swap.signing_completed)
            .collect();
        let n = completed.len().max(1) as f64;
        let mean = |f: &dyn Fn(&ofm_core::transfers::RoomSwap) -> f64| {
            completed.iter().map(|swap| f(swap)).sum::<f64>() / n
        };
        println!(
            "ROOM_SWAPS seed {} season {season}: {} completed swaps ({} sold, {} reassigned); mean rating departed {:.1} signed {:.1}; mean age departed {:.1} signed {:.1}; mean weekly wage departed {:.0} signed (new contract) {:.0}; cash received {} paid {}",
            self.seed,
            completed.len(),
            completed
                .iter()
                .filter(|swap| swap.departure == "permanent_sale")
                .count(),
            completed
                .iter()
                .filter(|swap| swap.departure == "academy_reassignment")
                .count(),
            mean(&|swap| f64::from(swap.departed.ovr)),
            mean(&|swap| f64::from(swap.signed.ovr)),
            mean(&|swap| f64::from(swap.departed.age)),
            mean(&|swap| f64::from(swap.signed.age)),
            mean(&|swap| f64::from(swap.departed.weekly_wage)),
            mean(&|swap| f64::from(swap.signed.weekly_wage)),
            completed.iter().map(|swap| swap.fee_received).sum::<u64>(),
            completed.iter().map(|swap| swap.fee_paid).sum::<u64>(),
        );
        let reassigned: HashSet<&str> = room
            .swaps
            .iter()
            .filter(|swap| swap.departure == "academy_reassignment")
            .map(|swap| swap.departed.player_id.as_str())
            .collect();
        let promoted_back = game
            .players
            .iter()
            .filter(|player| reassigned.contains(player.id.as_str()))
            .filter(|player| player.squad_role == domain::player::SquadRole::Senior)
            .count();
        println!(
            "ROOM_REASSIGNED seed {} season {season}: {} players reassigned to the academy, {promoted_back} are seniors again before the rollover",
            self.seed,
            reassigned.len(),
        );
        let starts = completed
            .iter()
            .filter(|swap| swap.signed.would_start == Some(true))
            .count();
        println!(
            "ROOM_CASE seed {} season {season}: of {} completed swaps the signing would start for the club in its own model in {starts}; mean rating points added to the side {:.1}; sold players' mean fee as a share of value {:.2}; departed players' status: {:?}",
            self.seed,
            completed.len(),
            mean(&|swap| f64::from(swap.signed.added_points.unwrap_or(0))),
            {
                let sold: Vec<_> = completed
                    .iter()
                    .filter(|swap| swap.departure == "permanent_sale")
                    .collect();
                sold.iter()
                    .map(|swap| swap.fee_received as f64 / swap.departed.value.max(1) as f64)
                    .sum::<f64>()
                    / sold.len().max(1) as f64
            },
            completed
                .iter()
                .fold(BTreeMap::<&str, usize>::new(), |mut tally, swap| {
                    *tally.entry(swap.departed.status).or_default() += 1;
                    tally
                }),
        );
        for swap in room.swaps.iter().take(6) {
            println!("ROOM_EXAMPLE seed {} season {season}: {swap:?}", self.seed);
        }
        reset_room_making_report();
        reset_market_funnel();
        self.funnel_base = market_funnel();
        self.participation_base = participation;
        self.evaluations_base = market_buyer_evaluations();
        self.sweep_nanos_base = market_sweep_nanos();
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
            && let Some(club) = player.team_id.as_deref().filter(|club| *club != user)
        {
            by_club
                .entry(club.to_string())
                .or_default()
                .push((player.ovr, player.id.clone()));
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
    // `APFM_GENERATED=1` plays the shipped generated world (440 clubs) instead of the FM24 one.
    let generated = std::env::var("APFM_GENERATED").is_ok_and(|value| value == "1");
    let path = world_path_from_env();
    if path.is_none() && !generated {
        println!("SKIPPED: APFM_WORLD_JSON is not set");
        return;
    }
    let seeds: Vec<u64> = std::env::var("APFM_SEEDS")
        .unwrap_or_else(|_| "1,2,3".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let seasons: u32 = std::env::var("APFM_SEASONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    // `APFM_ROOM_MAKING=0` plays the market as it was before room-making (the ALPHA6.1 baseline).
    let room_making = std::env::var("APFM_ROOM_MAKING").map_or(true, |value| value != "0");
    set_room_making_enabled(room_making);
    println!("ROOM_MAKING enabled: {room_making}");
    for seed in seeds {
        reset_room_making_report();
        let mut game = match (&path, generated) {
            (_, true) | (None, _) => WorldSpec::shipped(seed).build().expect("the world builds"),
            (Some(path), false) => build_fm24_career(path, LENS, seed).expect("the career builds"),
        };
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
        println!(
            "INVARIANTS seed {seed}: {} violations{}",
            volume.invariants.violations().len(),
            if volume.invariants.is_clean() {
                String::new()
            } else {
                format!("\n{}", volume.invariants.report())
            }
        );
    }
}
