//! Run F5.1: is the football world still healthy after many seasons?
//!
//! Measurement only. One [`WorldHealth`] is a snapshot of the whole world on one day; a
//! [`WorldHealthLog`] takes one when the run opens and one after every rollover. Nothing here
//! judges a number: the study that prints them (`tests/f51_world_health.rs`) and the
//! acceptance tests that read them do.
//!
//! The questions it answers, each from the real game state and none from a model of it:
//! does the elite tail survive generational replacement; does the player population hold its
//! size and age; do squads stay viable (size and position cover); do the clubs the world
//! starts with as its strongest stay near the top without being pinned there; are the clubs'
//! finances sustainable; how much money the market moves.

use std::collections::{BTreeMap, HashMap, HashSet};

use domain::age::age_on_or_unknown;
use domain::player::{Player, PlayerMovementKind, Position, SquadRole};
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::finances::calc_wages;
use ofm_core::game::Game;

use crate::driver::SeasonObserver;

/// A club's strength is the mean rating of its best this-many seniors: deep enough to include
/// the rotation, shallow enough that a large squad of reserves does not dilute it.
pub const STRENGTH_DEPTH: usize = 14;

/// A senior squad below this is not viable (a side of eleven plus bench).
pub const VIABLE_SQUAD: usize = 16;

/// A "world-class" rating floor used for the elite counts. The tiers are reported as counts at
/// several floors so the choice of one is not load-bearing.
pub const ELITE_FLOORS: [u8; 4] = [90, 85, 80, 75];

/// Age ceilings of the "young" cohort the promising-talent counts are taken over.
pub const YOUTH_MAX_AGE: i32 = 21;

#[derive(Debug, Clone, Default)]
pub struct Quantiles {
    pub min: f64,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug, Clone, Default)]
pub struct WorldHealth {
    /// Seasons completed when the snapshot was taken (0 = the opening world).
    pub seasons_done: u32,
    pub players_active: usize,
    pub players_at_clubs: usize,
    pub free_agents: usize,
    pub retired_total: usize,
    pub retired_since_last: usize,
    /// Players who appeared since the previous snapshot (youth intake and any other source),
    /// and how good their ceilings are; players who left `game.players` since.
    pub arrivals: usize,
    pub arrivals_potential_mean: f64,
    pub arrivals_potential_p90: f64,
    pub arrivals_potential_80_plus: usize,
    pub departures: usize,
    pub mean_retirement_age: f64,
    /// Mean age at retirement of players whose final rating was 70 or more.
    pub mean_retirement_age_good: f64,
    pub mean_age_at_clubs: f64,
    /// Share of club players by age band: <21, 21-25, 26-29, 30-33, 34+.
    pub age_bands: [f64; 5],
    // Elite tail, over every active player.
    pub top10_mean: f64,
    pub top50_mean: f64,
    pub top100_mean: f64,
    pub best: u8,
    pub p99: f64,
    pub elite_counts: [usize; 4],
    // Talent pipeline.
    pub promising_young: usize,
    pub young_potential_p90: f64,
    pub young_potential_mean: f64,
    pub potential_p99_all: f64,
    pub best_potential_under_21: u8,
    pub peak_age_of_top100: f64,
    // Squad viability.
    pub squad_size: Quantiles,
    pub clubs_below_viable: usize,
    pub clubs_without_two_keepers: usize,
    pub clubs_thin_in_a_line: usize,
    pub clubs: usize,
    // Per-country median rating of club seniors.
    pub median_by_country: BTreeMap<String, f64>,
    // Club strength cohorts, as a mean within-country percentile (0 weakest, 1 strongest).
    pub founding_big_cohort_percentile: f64,
    pub founding_small_cohort_percentile: f64,
    pub founding_big_cohort_in_top_quarter: f64,
    pub founding_small_cohort_in_top_quarter: f64,
    pub strength_spread_p90_p10: f64,
    /// Pearson correlation across clubs (within the whole world) of reputation with squad
    /// strength, with cash, and with the weekly wage bill. Structural advantage shows here.
    pub corr_reputation_strength: f64,
    pub corr_reputation_cash: f64,
    pub corr_reputation_wages: f64,
    pub strength_mean: f64,
    // Finance.
    pub cash: Quantiles,
    pub clubs_in_debt: usize,
    pub clubs_deep_debt: usize,
    pub weekly_wage_bill: Quantiles,
    pub wage_over_budget_clubs: usize,
    pub reputation: Quantiles,
    // Market, cumulative since the world opened.
    pub paid_transfers_total: u64,
    pub free_signings_total: u64,
    pub releases_total: u64,
    pub fees_paid_total: u64,
    pub renewals_total: u64,
}

pub fn quantiles(values: &[f64]) -> Quantiles {
    if values.is_empty() {
        return Quantiles::default();
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    Quantiles {
        min: v[0],
        p10: at(0.10),
        p50: at(0.50),
        p90: at(0.90),
        max: v[v.len() - 1],
        mean: v.iter().sum::<f64>() / v.len() as f64,
    }
}

/// Pearson correlation of two equal-length series; 0 when either has no spread.
pub fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let (ma, mb) = (mean_of(&a[..n]), mean_of(&b[..n]));
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    if va <= 0.0 || vb <= 0.0 {
        0.0
    } else {
        cov / (va.sqrt() * vb.sqrt())
    }
}

fn mean_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn is_national_pool(player: &Player) -> bool {
    player
        .id
        .starts_with(domain::player::NATIONAL_POOL_ID_PREFIX)
}

fn is_keeper(position: &Position) -> bool {
    *position == Position::Goalkeeper
}

/// The four lines a squad needs cover in. A "thin" line is one with fewer natural players than
/// a formation can field in it plus one reserve.
fn line_of(position: &Position) -> usize {
    match position {
        Position::Goalkeeper => 0,
        Position::Defender
        | Position::RightBack
        | Position::CenterBack
        | Position::LeftBack
        | Position::RightWingBack
        | Position::LeftWingBack => 1,
        Position::Forward | Position::RightWinger | Position::LeftWinger | Position::Striker => 3,
        _ => 2,
    }
}

const MIN_BY_LINE: [usize; 4] = [2, 5, 5, 3];

fn strength_of(seniors: &[&Player]) -> f64 {
    let mut ovr: Vec<f64> = seniors.iter().map(|p| f64::from(p.ovr)).collect();
    ovr.sort_by(|a, b| b.total_cmp(a));
    ovr.truncate(STRENGTH_DEPTH);
    mean_of(&ovr)
}

/// Each club's within-country strength percentile, 0 for the weakest and 1 for the strongest.
fn strength_percentiles(strength: &HashMap<String, f64>, game: &Game) -> HashMap<String, f64> {
    let mut by_country: HashMap<&str, Vec<(&str, f64)>> = HashMap::new();
    for team in &game.teams {
        if let Some(s) = strength.get(&team.id) {
            by_country
                .entry(team.country.as_str())
                .or_default()
                .push((team.id.as_str(), *s));
        }
    }
    let mut out = HashMap::new();
    for (_, mut clubs) in by_country {
        clubs.sort_by(|a, b| a.1.total_cmp(&b.1));
        let n = clubs.len().max(2) - 1;
        for (rank, (id, _)) in clubs.iter().enumerate() {
            out.insert((*id).to_string(), rank as f64 / n as f64);
        }
    }
    out
}

/// Which clubs the world opens with as its big and its small clubs: by reputation, within the
/// club's own country, top and bottom quarter.
#[derive(Debug, Clone, Default)]
pub struct Cohorts {
    pub big: HashSet<String>,
    pub small: HashSet<String>,
}

pub fn founding_cohorts(game: &Game) -> Cohorts {
    let mut by_country: HashMap<&str, Vec<(&str, u32)>> = HashMap::new();
    for team in &game.teams {
        by_country
            .entry(team.country.as_str())
            .or_default()
            .push((team.id.as_str(), team.reputation));
    }
    let mut cohorts = Cohorts::default();
    for (_, mut clubs) in by_country {
        clubs.sort_by_key(|c| std::cmp::Reverse(c.1));
        let quarter = (clubs.len() / 4).max(1);
        for (id, _) in clubs.iter().take(quarter) {
            cohorts.big.insert((*id).to_string());
        }
        for (id, _) in clubs.iter().rev().take(quarter) {
            cohorts.small.insert((*id).to_string());
        }
    }
    cohorts
}

#[derive(Default)]
struct MarketTotals {
    paid: u64,
    free: u64,
    released: u64,
    fees: u64,
    renewals: u64,
}

fn market_totals(game: &Game) -> MarketTotals {
    let mut totals = MarketTotals::default();
    let live: HashSet<&str> = game.players.iter().map(|p| p.id.as_str()).collect();
    let ledgers = game
        .players
        .iter()
        .map(|p| p.movement_history.iter().collect::<Vec<_>>())
        .chain(
            game.retired_players
                .iter()
                .filter(|p| !live.contains(p.id.as_str()))
                .map(|p| p.movements.iter().collect::<Vec<_>>()),
        );
    for ledger in ledgers {
        for entry in ledger {
            match entry.kind {
                PlayerMovementKind::PermanentTransfer => {
                    totals.paid += 1;
                    totals.fees += entry.fee.unwrap_or(0);
                }
                PlayerMovementKind::FreeAgentSigning => totals.free += 1,
                PlayerMovementKind::Released => totals.released += 1,
                PlayerMovementKind::Renewal => totals.renewals += 1,
                _ => {}
            }
        }
    }
    totals
}

/// The snapshot of `game` today. `previous_retired` is the retired count at the last snapshot.
pub fn world_health(
    game: &Game,
    cohorts: &Cohorts,
    seasons_done: u32,
    previous_retired: usize,
) -> WorldHealth {
    let today = game.clock.current_date.date_naive();
    // National-pool fillers (generated for a World Cup bench, owned by no club) are not part of
    // the football economy and would swing the population and the tail every fourth season.
    let active: Vec<&Player> = game
        .players
        .iter()
        .filter(|p| !p.retired && !is_national_pool(p))
        .collect();
    let at_clubs: Vec<&Player> = active
        .iter()
        .copied()
        .filter(|p| p.team_id.is_some())
        .collect();
    let age_of = |p: &Player| age_on_or_unknown(today, &p.date_of_birth);

    let mut by_ovr: Vec<&Player> = active.clone();
    by_ovr.sort_by_key(|p| std::cmp::Reverse(p.ovr));
    let ovr_f = |ps: &[&Player]| ps.iter().map(|p| f64::from(p.ovr)).collect::<Vec<_>>();
    let top = |n: usize| mean_of(&ovr_f(&by_ovr[..n.min(by_ovr.len())]));
    let all_ovr = {
        let mut v = ovr_f(&active);
        v.sort_by(f64::total_cmp);
        v
    };
    let p99 = all_ovr
        .get(((all_ovr.len().max(1) - 1) as f64 * 0.99) as usize)
        .copied()
        .unwrap_or(0.0);
    let elite_counts = ELITE_FLOORS.map(|floor| active.iter().filter(|p| p.ovr >= floor).count());

    let young: Vec<&Player> = active
        .iter()
        .copied()
        .filter(|p| age_of(p) <= YOUTH_MAX_AGE)
        .collect();
    let young_potential: Vec<f64> = young.iter().map(|p| f64::from(p.potential)).collect();
    let mut all_potential: Vec<f64> = active.iter().map(|p| f64::from(p.potential)).collect();
    all_potential.sort_by(f64::total_cmp);

    let mut bands = [0usize; 5];
    for p in &at_clubs {
        let band = match age_of(p) {
            i32::MIN..=20 => 0,
            21..=25 => 1,
            26..=29 => 2,
            30..=33 => 3,
            _ => 4,
        };
        bands[band] += 1;
    }
    let band_total = at_clubs.len().max(1) as f64;

    // Per-club squads.
    let mut squads: HashMap<&str, Vec<&Player>> = HashMap::new();
    for p in &at_clubs {
        if p.squad_role == SquadRole::Senior {
            squads
                .entry(p.team_id.as_deref().unwrap_or(""))
                .or_default()
                .push(p);
        }
    }
    let mut strength: HashMap<String, f64> = HashMap::new();
    let mut sizes = Vec::new();
    let (mut below, mut no_keepers, mut thin) = (0usize, 0usize, 0usize);
    let mut by_country: HashMap<&str, Vec<f64>> = HashMap::new();
    let country_of: HashMap<&str, &str> = game
        .teams
        .iter()
        .map(|t| (t.id.as_str(), t.country.as_str()))
        .collect();
    for team in &game.teams {
        let squad = squads.get(team.id.as_str()).cloned().unwrap_or_default();
        sizes.push(squad.len() as f64);
        below += usize::from(squad.len() < VIABLE_SQUAD);
        let mut lines = [0usize; 4];
        for p in &squad {
            lines[line_of(&p.natural_position)] += 1;
        }
        no_keepers += usize::from(
            squad
                .iter()
                .filter(|p| is_keeper(&p.natural_position))
                .count()
                < 2,
        );
        thin += usize::from(
            lines
                .iter()
                .zip(MIN_BY_LINE)
                .any(|(have, need)| *have < need),
        );
        strength.insert(team.id.clone(), strength_of(&squad));
        for p in &squad {
            by_country
                .entry(country_of.get(team.id.as_str()).copied().unwrap_or(""))
                .or_default()
                .push(f64::from(p.ovr));
        }
    }
    let median_by_country = by_country
        .into_iter()
        .map(|(c, v)| (c.to_string(), quantiles(&v).p50))
        .collect();

    let percentile = strength_percentiles(&strength, game);
    let cohort_stats = |ids: &HashSet<String>| {
        let ps: Vec<f64> = ids
            .iter()
            .filter_map(|id| percentile.get(id).copied())
            .collect();
        let top_quarter = ps.iter().filter(|p| **p >= 0.75).count();
        (mean_of(&ps), top_quarter as f64 / ps.len().max(1) as f64)
    };
    let (big_pct, big_top) = cohort_stats(&cohorts.big);
    let (small_pct, small_top) = cohort_stats(&cohorts.small);
    let strengths: Vec<f64> = strength.values().copied().collect();
    let sq = quantiles(&strengths);

    let cash: Vec<f64> = game.teams.iter().map(|t| t.finance as f64).collect();
    let wages: Vec<f64> = game
        .teams
        .iter()
        .map(|t| calc_wages(game, &t.id) as f64)
        .collect();
    let over_budget = game
        .teams
        .iter()
        .zip(&wages)
        .filter(|(t, w)| t.wage_budget > 0 && **w > t.wage_budget as f64 * 1.1)
        .count();
    let reputation: Vec<f64> = game.teams.iter().map(|t| f64::from(t.reputation)).collect();

    let retired: Vec<f64> = game
        .retired_players
        .iter()
        .filter_map(|r| {
            let on = chrono::NaiveDate::parse_from_str(r.retired_on.get(..10)?, "%Y-%m-%d").ok()?;
            Some(f64::from(age_on_or_unknown(on, &r.date_of_birth)))
        })
        .collect();
    let retired_good: Vec<f64> = game
        .retired_players
        .iter()
        .filter(|r| r.ovr >= 70)
        .filter_map(|r| {
            let on = chrono::NaiveDate::parse_from_str(r.retired_on.get(..10)?, "%Y-%m-%d").ok()?;
            Some(f64::from(age_on_or_unknown(on, &r.date_of_birth)))
        })
        .collect();
    let market = market_totals(game);
    let top100_ages: Vec<f64> = by_ovr
        .iter()
        .take(100)
        .map(|p| f64::from(age_of(p)))
        .collect();

    WorldHealth {
        arrivals: 0,
        arrivals_potential_mean: 0.0,
        arrivals_potential_p90: 0.0,
        arrivals_potential_80_plus: 0,
        departures: 0,
        seasons_done,
        players_active: active.len(),
        players_at_clubs: at_clubs.len(),
        free_agents: active.len() - at_clubs.len(),
        retired_total: game.retired_players.len(),
        retired_since_last: game.retired_players.len().saturating_sub(previous_retired),
        mean_retirement_age: mean_of(&retired),
        mean_retirement_age_good: mean_of(&retired_good),
        mean_age_at_clubs: mean_of(
            &at_clubs
                .iter()
                .map(|p| f64::from(age_of(p)))
                .collect::<Vec<_>>(),
        ),
        age_bands: bands.map(|b| b as f64 / band_total),
        top10_mean: top(10),
        top50_mean: top(50),
        top100_mean: top(100),
        best: by_ovr.first().map_or(0, |p| p.ovr),
        p99,
        elite_counts,
        promising_young: young
            .iter()
            .filter(|p| p.potential >= 80 && p.ovr >= 60)
            .count(),
        young_potential_p90: quantiles(&young_potential).p90,
        young_potential_mean: mean_of(&young_potential),
        potential_p99_all: all_potential
            .get(((all_potential.len().max(1) - 1) as f64 * 0.99) as usize)
            .copied()
            .unwrap_or(0.0),
        best_potential_under_21: young.iter().map(|p| p.potential).max().unwrap_or(0),
        peak_age_of_top100: mean_of(&top100_ages),
        squad_size: quantiles(&sizes),
        clubs_below_viable: below,
        clubs_without_two_keepers: no_keepers,
        clubs_thin_in_a_line: thin,
        clubs: game.teams.len(),
        median_by_country,
        founding_big_cohort_percentile: big_pct,
        founding_small_cohort_percentile: small_pct,
        founding_big_cohort_in_top_quarter: big_top,
        founding_small_cohort_in_top_quarter: small_top,
        strength_spread_p90_p10: sq.p90 - sq.p10,
        corr_reputation_strength: pearson(
            &game
                .teams
                .iter()
                .map(|t| f64::from(t.reputation))
                .collect::<Vec<_>>(),
            &game
                .teams
                .iter()
                .map(|t| strength.get(&t.id).copied().unwrap_or(0.0))
                .collect::<Vec<_>>(),
        ),
        corr_reputation_cash: pearson(&reputation, &cash),
        corr_reputation_wages: pearson(&reputation, &wages),
        strength_mean: sq.mean,
        cash: quantiles(&cash),
        clubs_in_debt: game.teams.iter().filter(|t| t.finance < 0).count(),
        clubs_deep_debt: game
            .teams
            .iter()
            .filter(|t| t.finance < -50_000_000)
            .count(),
        weekly_wage_bill: quantiles(&wages),
        wage_over_budget_clubs: over_budget,
        reputation: quantiles(&reputation),
        paid_transfers_total: market.paid,
        free_signings_total: market.free,
        releases_total: market.released,
        fees_paid_total: market.fees,
        renewals_total: market.renewals,
    }
}

/// A snapshot when the run opens and one after every rollover.
pub struct WorldHealthLog {
    cohorts: Cohorts,
    known_ids: HashSet<String>,
    pub snapshots: Vec<WorldHealth>,
}

impl WorldHealthLog {
    pub fn starting_from(game: &Game) -> Self {
        let cohorts = founding_cohorts(game);
        let first = world_health(game, &cohorts, 0, game.retired_players.len());
        Self {
            cohorts,
            known_ids: game
                .players
                .iter()
                .filter(|p| !is_national_pool(p))
                .map(|p| p.id.clone())
                .collect(),
            snapshots: vec![first],
        }
    }

    pub fn last(&self) -> &WorldHealth {
        self.snapshots
            .last()
            .expect("a log always has its opening snapshot")
    }
}

impl SeasonObserver for WorldHealthLog {
    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        let _ = season;
        let done = u32::try_from(self.snapshots.len()).unwrap_or(u32::MAX);
        let previous = self.last().retired_total;
        let mut snapshot = world_health(game, &self.cohorts, done, previous);
        let now: HashSet<String> = game
            .players
            .iter()
            .filter(|p| !is_national_pool(p))
            .map(|p| p.id.clone())
            .collect();
        let arrivals: Vec<f64> = game
            .players
            .iter()
            .filter(|p| !is_national_pool(p) && !self.known_ids.contains(&p.id))
            .map(|p| f64::from(p.potential))
            .collect();
        let q = quantiles(&arrivals);
        snapshot.arrivals = arrivals.len();
        snapshot.arrivals_potential_mean = q.mean;
        snapshot.arrivals_potential_p90 = q.p90;
        snapshot.arrivals_potential_80_plus = arrivals.iter().filter(|p| **p >= 80.0).count();
        snapshot.departures = self.known_ids.difference(&now).count();
        self.known_ids = now;
        self.snapshots.push(snapshot);
    }
}

/// One line of a table: the columns the study prints and the acceptance tests read.
pub fn headline(h: &WorldHealth) -> String {
    format!(
        "s{:>2} | pop {:>5} fa {:>4} age {:>4.1} | top10 {:>5.1} top50 {:>5.1} top100 {:>5.1} best {:>2} p99 {:>4.1} | >=90 {:>3} >=85 {:>3} >=80 {:>4} >=75 {:>4} | young80+ {:>3} potP90 {:>4.1} | sq {:>4.1}[{:>2.0}-{:>2.0}] <{VIABLE_SQUAD} {:>2} 1gk {:>2} thin {:>2} | bigPct {:>4.2} smallPct {:>4.2} bigTop4th {:>4.2} r(rep,str) {:>5.2} r(rep,cash) {:>5.2} | debt {:>2}/{} cashP50 {:>6.1}M | ret {:>4}",
        h.seasons_done,
        h.players_active,
        h.free_agents,
        h.mean_age_at_clubs,
        h.top10_mean,
        h.top50_mean,
        h.top100_mean,
        h.best,
        h.p99,
        h.elite_counts[0],
        h.elite_counts[1],
        h.elite_counts[2],
        h.elite_counts[3],
        h.promising_young,
        h.young_potential_p90,
        h.squad_size.mean,
        h.squad_size.min,
        h.squad_size.max,
        h.clubs_below_viable,
        h.clubs_without_two_keepers,
        h.clubs_thin_in_a_line,
        h.founding_big_cohort_percentile,
        h.founding_small_cohort_percentile,
        h.founding_big_cohort_in_top_quarter,
        h.corr_reputation_strength,
        h.corr_reputation_cash,
        h.clubs_in_debt,
        h.clubs,
        h.cash.p50 / 1e6,
        h.retired_since_last,
    )
}
