//! The external world's career projection.
//!
//! D7B1 made the source registry a read-only snapshot: the same 81 762 source people, with the
//! same 2026 attributes, for every career. That is not a world. This module is the deterministic
//! rule that turns the snapshot into a career-specific market, without simulating 44 032 clubs.
//!
//! The decision (see `docs/APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md`) is **architecture A with a
//! materialised per-career cache**: every projected value is a pure function of
//!
//! * the source record (frozen, read-only),
//! * the career's own seed (`Game::seed`, random per career in `world::build`),
//! * the career's season (derived from its clock, never wall-clock time),
//! * the absolute season being replayed.
//!
//! Nothing here mutates the source and nothing here stores one career's history in the shared
//! registry. A save that was promoted into the active world is removed from the market by the
//! registry's identity exclusion, exactly as D7B1 already does. Because the projection replays the
//! same seasons in the same order, a save, a reload, a rebuild of the cache and a second search all
//! see the same player: retirement is monotone, contracts never flip at random, and two careers
//! with different seeds diverge.
//!
//! The development curve is the game's own [`crate::development::develop_one_season`], so an
//! external player ages by the same rules as an active one (physical decline before technical,
//! potential as a ceiling, variable growth). The only external-specific rules here are contract
//! evolution, retirement timing and a lightweight club decision model.

use chrono::{Datelike, NaiveDate};
use domain::player::{Player, Position};
use domain::team::Team;
use rand::{Rng, RngExt};

use crate::development::develop_one_season;
use crate::external_registry::{ExternalClub, ExternalPlayer};
use crate::player_rating::{natural_ovr, refresh_player_derived};
use crate::seed::rng_for_seed;

/// The largest number of new professional players one season may add to the market.
///
/// The intake is **not** a fixed number any more: it replaces the players who actually retired in
/// the season (see [`newgen_intake_for`]). That keeps the market populated without inflating it,
/// because births track deaths instead of adding a permanently growing layer of generated players.
/// The cap is a safety bound for a corrupt source or a pathological retirement season; a real
/// 61k-player market retires a few thousand a season, well below it.
pub const MAX_NEWGEN_INTAKE_PER_SEASON: usize = 6_000;

/// Elite prospects per thousand newgens. The generator's ordinary ceilings make an 85+ player
/// vanishingly rare; this small deterministic supplement keeps elite talent emerging without
/// turning the market into a superclub. At a 2 300 intake this is about 19 prospects a season.
pub const ELITE_NEWGENS_PER_THOUSAND: usize = 8;

/// No season with an intake may add more than this many elite prospects, so a very large intake
/// cannot flood the top of the market.
pub const ELITE_NEWGEN_MAX_PER_SEASON: usize = 24;

/// The projection is only ever asked for a plausible career span. A corrupt clock cannot make the
/// cache builder replay ten thousand seasons.
pub const MAX_CAREER_SEASONS: i32 = 80;

/// How many newgens a season generates for the number of external players who retired at its end.
///
/// One-for-one replacement is the sustainable rule: the external market is a population, not a
/// pipeline, so a season that loses 2 400 players needs about 2 400 new ones. The old fixed 900
/// intake was below the source's retirement flow and shrank the searchable market from ~61k to
/// ~14k over twenty seasons. The cap is the only bound; a season with nothing retiring adds nobody.
pub fn newgen_intake_for(retirements: usize) -> usize {
    retirements.min(MAX_NEWGEN_INTAKE_PER_SEASON)
}

/// How many of this season's newgens are elite prospects. Proportional to the intake, bounded, and
/// zero when nothing is generated.
pub fn elite_newgens_for(intake: usize) -> usize {
    if intake == 0 {
        return 0;
    }
    intake
        .saturating_mul(ELITE_NEWGENS_PER_THOUSAND)
        .div_ceil(1_000)
        .clamp(1, ELITE_NEWGEN_MAX_PER_SEASON)
}

/// A career's external-world identity. The seed is `Game::seed`; the start season is the season the
/// career's clock started in. The pair is saved with the game, so a reload projects the same world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalCareerKey {
    pub seed: u64,
    pub start_season: i32,
}

impl ExternalCareerKey {
    pub fn new(seed: u64, start_season: i32) -> Self {
        Self { seed, start_season }
    }
}

/// An external club the market may move a player to. Deliberately not a `Team`: no squad, no
/// fixtures, no finances. `reputation` is on the source's 0..=1000 scale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalClubRef {
    pub id: String,
    pub name: String,
    pub nation_code: String,
    pub reputation: i64,
}

/// One projected source player, or one generated newgen, at a given season.
#[derive(Debug, Clone)]
pub struct ProjectedExternalPlayer {
    pub player: ExternalPlayer,
    pub retired: bool,
    pub newgen: bool,
    pub season: i32,
}

/// The season a date belongs to. A season runs July to June, matching the rest of the game.
pub fn projection_season(today: NaiveDate, start_season: i32) -> i32 {
    let candidate = if today.month() >= 7 {
        today.year()
    } else {
        today.year() - 1
    };
    candidate.max(start_season)
}

/// How many completed seasons have been replayed by `season`.
pub fn completed_seasons(key: ExternalCareerKey, season: i32) -> i32 {
    (season - key.start_season).clamp(0, MAX_CAREER_SEASONS)
}

fn season_start(season: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(season, 7, 1).unwrap_or(NaiveDate::MAX)
}

fn season_end(season: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(season, 6, 30).unwrap_or(NaiveDate::MAX)
}

fn age_on(date: NaiveDate, birth_date: &str) -> i32 {
    domain::age::age_on(date, birth_date).unwrap_or(30)
}

fn parse_end(end: Option<&str>) -> Option<NaiveDate> {
    end.and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

/// The source record as the game's `Player`, without any contract ledger: this is only the body
/// the development curve reads.
fn body_of(player: &ExternalPlayer, on: NaiveDate) -> Player {
    let mut body = Player::new(
        player.id.clone(),
        player.match_name.clone(),
        player.full_name.clone(),
        player.date_of_birth.clone(),
        player.nationality.clone(),
        player.position.clone(),
        player.attributes.clone(),
    );
    body.natural_position = player.natural_position.clone();
    body.alternate_positions = player.alternate_positions.clone();
    body.footedness = player.footedness;
    body.weak_foot = player.weak_foot;
    body.potential = player.potential;
    body.ovr = natural_ovr(&body).round().clamp(1.0, 99.0) as u8;
    refresh_player_derived(&mut body, on);
    body
}

/// Market value at a season: a projection of current ability, ceiling and age, so a player cannot
/// inflate by being valued once at 20 and then ageing without a revaluation.
pub fn projected_market_value(ovr: u8, potential: u8, age: i32) -> u64 {
    if ovr == 0 {
        return 0;
    }
    let ability = f64::from(ovr);
    let ceiling = f64::from(potential.max(ovr));
    let youth = if age <= 21 {
        1.6
    } else if age <= 25 {
        1.3
    } else if age <= 29 {
        1.0
    } else if age <= 32 {
        0.6
    } else {
        0.25
    };
    let growth = 1.0 + (ceiling - ability).max(0.0) / 100.0;
    let value = (ability / 50.0).powi(3) * 2_000_000.0 * youth * growth;
    value.max(0.0) as u64
}

/// The weekly wage a club of this level pays a player of this ability and age. Kept in one place so
/// a renewal, a generated contract and the preview screen cannot disagree.
pub fn projected_wage(ovr: u8, age: i32, club_reputation: i64) -> u32 {
    if ovr == 0 {
        return 0;
    }
    let value = projected_market_value(ovr, ovr, age) as f64;
    let youth = if age <= 21 {
        0.65
    } else if age <= 24 {
        0.85
    } else if age <= 29 {
        1.0
    } else {
        1.05
    };
    let level = 0.75 + (club_reputation.clamp(0, 1000) as f64) / 2000.0;
    (value / 420.0 * youth * level)
        .round()
        .clamp(500.0, 400_000.0) as u32
}

/// The ability a club of this reputation is built around, used to decide whether a player is
/// important to it.
pub fn expected_ovr_for_reputation(reputation: i64) -> i32 {
    38 + (reputation.clamp(0, 1000) * 50 / 1000) as i32
}

fn club_target_reputation(ovr: u8) -> i64 {
    (i64::from(ovr) - 38) * 20
}

/// Choose a club whose level suits the player. `clubs` is sorted by reputation; the window is
/// found by binary search so a 44 000-club source stays O(log n) per decision.
fn pick_club<'a>(
    clubs: &'a [ExternalClubRef],
    target: i64,
    rng: &mut impl Rng,
) -> Option<&'a ExternalClubRef> {
    if clubs.is_empty() {
        return None;
    }
    let low = clubs.partition_point(|club| club.reputation < target - 250);
    let high = clubs.partition_point(|club| club.reputation <= target + 250);
    if high > low {
        return clubs.get(low + rng.random_range(0..high - low));
    }
    let nearest = clubs
        .partition_point(|club| club.reputation < target)
        .min(clubs.len() - 1);
    clubs.get(nearest)
}

/// A deterministic result for one club's decision on one offer. It is a pure function of its input,
/// so repeating a preview cannot re-roll it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalOfferDecision {
    /// The club accepts the offer as it stands.
    Accepted { fee: u64 },
    /// The club wants this fee instead. The one-click preview shows it as the price.
    Countered { fee: u64 },
    /// The offer is too far below the club's floor. A future offer screen may raise it.
    Rejected { minimum: u64 },
}

/// Everything a club's decision depends on. Kept as a plain value so the rule can be tested without
/// a `Game` and reused by the future P5 transfer negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOfferContext {
    pub seed: u64,
    pub season: i32,
    pub asking_price: u64,
    pub free_agent: bool,
    pub player_ovr: u8,
    pub player_potential: u8,
    pub player_age: i32,
    pub contract_years_left: u8,
    pub seller_reputation: i64,
    pub buyer_reputation: i64,
    /// Stable identity for the deterministic club jitter.
    pub player_id: String,
}

impl ExternalOfferContext {
    /// The club's own minimum, before the buyer's pull. Exposed so the preview and the tests read
    /// the same number.
    pub fn minimum_fee(&self) -> u64 {
        if self.free_agent || self.asking_price == 0 {
            return 0;
        }
        let importance =
            i32::from(self.player_ovr) - expected_ovr_for_reputation(self.seller_reputation);
        let importance = f64::from(importance.clamp(-20, 35));
        let youth = if self.player_age <= 23 {
            0.10
        } else if self.player_age <= 27 {
            0.05
        } else if self.player_age >= 32 {
            -0.08
        } else {
            0.0
        };
        let contract = f64::from(self.contract_years_left.min(5)) * 0.025;
        let buyer_pull =
            ((self.buyer_reputation - self.seller_reputation) as f64 / 2500.0).clamp(-0.08, 0.08);
        // A club's own valuation jitter is stable for the player and season, not per call.
        let mut jitter_rng = rng_for_seed(
            self.seed,
            &format!("external/club-valuation/{}", self.player_id),
            &self.season.to_string(),
        );
        let jitter: f64 = jitter_rng.random_range(-0.05..=0.05);
        let ratio =
            (0.90 + importance * 0.012 + youth + contract + buyer_pull + jitter).clamp(0.80, 1.30);
        let fee = (self.asking_price as f64 * ratio).round() as u64;
        fee.max(1_000)
    }

    pub fn decide(&self, offer: u64) -> ExternalOfferDecision {
        if self.free_agent || self.asking_price == 0 {
            return ExternalOfferDecision::Accepted { fee: 0 };
        }
        let minimum = self.minimum_fee();
        if offer.saturating_mul(4) < minimum.saturating_mul(3) {
            ExternalOfferDecision::Rejected { minimum }
        } else if offer < minimum {
            ExternalOfferDecision::Countered { fee: minimum }
        } else {
            ExternalOfferDecision::Accepted { fee: offer }
        }
    }
}

/// The chance, in percent, that an external player retires at the end of a season. Mirrors the
/// active game's age curve but uses no match appearances (the external world invents none) and
/// raises the chance for a free agent with no club and a low-level player with no market.
fn retirement_chance(age: i32, contracted: bool, ovr: u8) -> u32 {
    if age < 32 {
        return 0;
    }
    let mut chance = match age {
        32 => 6,
        33 => 14,
        34 => 26,
        35 => 44,
        36 => 62,
        37 => 80,
        _ => 100,
    };
    if !contracted {
        chance += 18;
    }
    if ovr < 55 {
        chance += 12;
    }
    if ovr < 45 {
        chance += 10;
    }
    chance.min(100)
}

fn roll_in_percent(seed: u64, tag: &str, season: i32, id: &str) -> u32 {
    let mut rng = rng_for_seed(seed, tag, &format!("{season}/{id}"));
    rng.random_range(0..100)
}

fn renewal_chance(ovr: u8, age: i32, club_reputation: i64) -> u32 {
    let importance = i32::from(ovr) - expected_ovr_for_reputation(club_reputation);
    let base = if importance >= 6 {
        62
    } else if importance >= 0 {
        48
    } else {
        32
    };
    let age_penalty = if age >= 33 {
        25
    } else if age >= 31 {
        12
    } else {
        0
    };
    (base - age_penalty).clamp(8, 72) as u32
}

fn transfer_chance(ovr: u8, age: i32) -> u32 {
    if age >= 33 || ovr < 45 {
        return 0;
    }
    if age <= 25 {
        12
    } else if age <= 29 {
        8
    } else {
        4
    }
}

fn signing_chance(ovr: u8, age: i32) -> u32 {
    if age >= 35 || ovr < 40 {
        0
    } else if age <= 28 {
        55
    } else if age <= 31 {
        38
    } else {
        18
    }
}

fn evolve_contract(
    player: &ExternalPlayer,
    body: &Player,
    club: Option<ExternalClub>,
    key: ExternalCareerKey,
    season: i32,
    clubs: &[ExternalClubRef],
) -> Option<ExternalClub> {
    let end = season_end(season);
    let age = age_on(end, &player.date_of_birth);
    let mut rng = rng_for_seed(
        key.seed,
        &format!("external/contract/{}", player.id),
        &season.to_string(),
    );

    match club {
        None => {
            if rng.random_range(0..100) < signing_chance(body.ovr, age) {
                let target = club_target_reputation(body.ovr);
                pick_club(clubs, target, &mut rng).map(|choice| {
                    let years = rng.random_range(1..=3);
                    ExternalClub {
                        id: choice.id.clone(),
                        name: choice.name.clone(),
                        contract_end: Some(
                            NaiveDate::from_ymd_opt(season + 1 + years, 6, 30)
                                .unwrap_or(end)
                                .format("%Y-%m-%d")
                                .to_string(),
                        ),
                        weekly_wage: projected_wage(body.ovr, age, choice.reputation),
                        reputation: choice.reputation,
                    }
                })
            } else {
                None
            }
        }
        Some(mut current) => {
            let expiring = parse_end(current.contract_end.as_deref())
                .is_none_or(|end| end <= season_end(season));
            if expiring {
                if rng.random_range(0..100) < renewal_chance(body.ovr, age, current.reputation) {
                    let years = rng.random_range(1..=4);
                    current.contract_end = Some(
                        NaiveDate::from_ymd_opt(season + 1 + years, 6, 30)
                            .unwrap_or(end)
                            .format("%Y-%m-%d")
                            .to_string(),
                    );
                    current.weekly_wage = projected_wage(body.ovr, age, current.reputation);
                    return Some(current);
                }
                return None;
            }
            if rng.random_range(0..100) < transfer_chance(body.ovr, age) {
                let target = club_target_reputation(body.ovr);
                if let Some(choice) = pick_club(clubs, target, &mut rng)
                    && choice.id != current.id
                {
                    let years = rng.random_range(2..=4);
                    return Some(ExternalClub {
                        id: choice.id.clone(),
                        name: choice.name.clone(),
                        contract_end: Some(
                            NaiveDate::from_ymd_opt(season + 1 + years, 6, 30)
                                .unwrap_or(end)
                                .format("%Y-%m-%d")
                                .to_string(),
                        ),
                        weekly_wage: projected_wage(body.ovr, age, choice.reputation),
                        reputation: choice.reputation,
                    });
                }
            }
            Some(current)
        }
    }
}

/// Project one source player from `from_season` to `season`. The replay always starts at the source
/// snapshot and walks forward, so the same absolute seasons are evaluated in the same order.
pub fn project_from(
    player: &ExternalPlayer,
    key: ExternalCareerKey,
    from_season: i32,
    season: i32,
    clubs: &[ExternalClubRef],
    newgen: bool,
) -> ProjectedExternalPlayer {
    let from = from_season.clamp(key.start_season, season);
    let to = season.clamp(from, from.saturating_add(MAX_CAREER_SEASONS));
    let mut body = body_of(player, season_start(from));
    let mut current = player.club.clone();
    let mut retired = false;

    for replay in (from + 1)..=to {
        let end = season_end(replay);
        let age = age_on(end, &player.date_of_birth);
        let mut stream = rng_for_seed(
            key.seed,
            &format!("external/development/{}", player.id),
            &format!("{replay}-05-31"),
        );
        develop_one_season(&mut body, age, key.seed, &mut stream);
        refresh_player_derived(&mut body, end);

        if roll_in_percent(key.seed, "external/retirement", replay, &player.id)
            < retirement_chance(age, current.is_some(), body.ovr)
        {
            retired = true;
            break;
        }
        current = evolve_contract(player, &body, current, key, replay, clubs);
    }

    let on = season_start(to);
    let age = age_on(on, &player.date_of_birth);
    let market_value = projected_market_value(body.ovr, body.potential, age);
    let mut projected = player.clone();
    projected.attributes = body.attributes;
    projected.ovr = body.ovr;
    projected.potential = body.potential.max(body.ovr);
    projected.market_value = market_value;
    projected.club = current.map(|club| ExternalClub {
        weekly_wage: projected_wage(body.ovr, age, club.reputation),
        ..club
    });
    ProjectedExternalPlayer {
        player: projected,
        retired,
        newgen,
        season: to,
    }
}

/// Project a source player at `season` from the career's snapshot.
pub fn project_player(
    player: &ExternalPlayer,
    key: ExternalCareerKey,
    season: i32,
    clubs: &[ExternalClubRef],
) -> ProjectedExternalPlayer {
    project_from(player, key, key.start_season, season, clubs, false)
}

const NEWGEN_POSITIONS: [Position; 10] = [
    Position::Goalkeeper,
    Position::CenterBack,
    Position::RightBack,
    Position::LeftBack,
    Position::DefensiveMidfielder,
    Position::CentralMidfielder,
    Position::AttackingMidfielder,
    Position::RightWinger,
    Position::LeftWinger,
    Position::Striker,
];

fn fallback_clubs() -> Vec<ExternalClubRef> {
    [
        ("fm-c-ext-1", "External Athletic", "GB", 620),
        ("fm-c-ext-2", "External Sporting", "ES", 560),
        ("fm-c-ext-3", "External Calcio", "IT", 500),
        ("fm-c-ext-4", "External Verein", "DE", 440),
        ("fm-c-ext-5", "External Football", "FR", 380),
    ]
    .into_iter()
    .map(|(id, name, nation_code, reputation)| ExternalClubRef {
        id: id.to_string(),
        name: name.to_string(),
        nation_code: nation_code.to_string(),
        reputation,
    })
    .collect()
}

/// One season's aggregate intake. Generated from the seed and the absolute season alone, so a
/// rebuild of the cache for the same season cannot invent a different class of youngsters.
///
/// The size is the season's retirement count (see [`newgen_intake_for`]). The first
/// [`elite_newgens_for`] slots of the class are the season's elite prospects; every other slot is
/// an ordinary youth. Slot order is fixed, so the same seed and season always names the same
/// player, whichever rebuild produced the cohort.
pub fn newgen_cohort(
    key: ExternalCareerKey,
    season: i32,
    clubs: &[ExternalClubRef],
    intake: usize,
) -> Vec<ExternalPlayer> {
    let fallback;
    let pool = if clubs.is_empty() {
        fallback = fallback_clubs();
        fallback.as_slice()
    } else {
        clubs
    };
    let elite = elite_newgens_for(intake);
    (0..intake)
        .filter_map(|index| newgen_one(key, season, index, pool, index < elite))
        .collect()
}

/// The potential an elite prospect may reach: the top of the market, drawn narrowly so the very
/// best players keep being replaced without a superclub forming. Ordinary youth keep the shared
/// generator's ceiling.
fn elite_potential(rng: &mut impl Rng) -> u8 {
    86 + rng.random_range(0..10) as u8
}

fn newgen_one(
    key: ExternalCareerKey,
    season: i32,
    index: usize,
    clubs: &[ExternalClubRef],
    elite: bool,
) -> Option<ExternalPlayer> {
    let mut rng = rng_for_seed(
        key.seed,
        &format!("external/newgen/{season}/{index}"),
        "generate",
    );
    let position = NEWGEN_POSITIONS
        [(index + rng.random_range(0..NEWGEN_POSITIONS.len())) % NEWGEN_POSITIONS.len()]
    .clone();
    let club = pick_club(
        clubs,
        club_target_reputation(55 + (index % 25) as u8),
        &mut rng,
    )?;
    let team = Team::new(
        format!("external-newgen-{season}-{index}"),
        club.name.clone(),
        "EXT".to_string(),
        club.nation_code.clone(),
        "Unknown".to_string(),
        "Unknown".to_string(),
        1_000,
    );
    let mut player = crate::generator::generate_youth_academy_recruit_with_nationality(
        &team,
        Some(&position),
        Some(&club.nation_code),
        season.max(0) as u32,
        &mut rng,
    );
    player.id = format!("fm-n-{season}-{index:04}");
    player.team_id = None;
    if elite {
        player.potential = player.potential.max(elite_potential(&mut rng));
    }
    let age = age_on(season_start(season), &player.date_of_birth);
    let attached = rng.random_range(0..100) < 70;
    let contract = if attached {
        let years = rng.random_range(1..=3);
        Some(ExternalClub {
            id: club.id.clone(),
            name: club.name.clone(),
            contract_end: Some(
                NaiveDate::from_ymd_opt(season + years, 6, 30)
                    .unwrap_or(season_end(season))
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
            weekly_wage: projected_wage(player.ovr, age, club.reputation),
            reputation: club.reputation,
        })
    } else {
        None
    };
    Some(ExternalPlayer {
        id: player.id,
        match_name: player.match_name,
        full_name: player.full_name,
        date_of_birth: player.date_of_birth,
        nationality: player.nationality,
        position: player.natural_position.clone(),
        natural_position: player.natural_position,
        alternate_positions: player.alternate_positions,
        footedness: player.footedness,
        weak_foot: player.weak_foot,
        attributes: player.attributes,
        ovr: player.ovr,
        potential: player.potential,
        market_value: projected_market_value(player.ovr, player.potential, age),
        club: contract,
    })
}

/// The whole market at `season`: surviving source players plus generated newgens. Used by the
/// long-career validation, so it applies exactly the rule the cache builder applies — one-for-one
/// replacement sized by the retirements of the season just replayed.
pub fn projected_market(
    sources: &[ExternalPlayer],
    key: ExternalCareerKey,
    season: i32,
    clubs: &[ExternalClubRef],
) -> Vec<ProjectedExternalPlayer> {
    let mut market: Vec<(ExternalPlayer, bool)> = sources
        .iter()
        .cloned()
        .map(|player| (player, false))
        .collect();
    let start = key.start_season;
    for replay in (start + 1)..=season {
        let mut retired = 0;
        market.retain_mut(|(external, newgen)| {
            let projected = project_from(external, key, replay - 1, replay, clubs, *newgen);
            if projected.retired {
                retired += 1;
                return false;
            }
            *external = projected.player;
            true
        });
        for newgen in newgen_cohort(key, replay, clubs, newgen_intake_for(retired)) {
            market.push((newgen, true));
        }
    }
    market
        .into_iter()
        .map(|(player, newgen)| ProjectedExternalPlayer {
            player,
            retired: false,
            newgen,
            season,
        })
        .collect()
}

/// The annual external-world maintenance the turn loop can call, without touching the match engine:
/// it only advances the cache's season. The cache builder is idempotent, so this is safe to call
/// more than once for the same season.
pub fn cache_is_current(
    meta_seed: Option<u64>,
    meta_season: Option<i32>,
    key: ExternalCareerKey,
    season: i32,
) -> bool {
    meta_seed == Some(key.seed) && meta_season == Some(season)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{Footedness, PlayerAttributes};

    fn attrs(ovr: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: ovr,
            stamina: ovr,
            strength: ovr,
            agility: ovr,
            passing: ovr,
            shooting: ovr,
            tackling: ovr,
            dribbling: ovr,
            defending: ovr,
            positioning: ovr,
            vision: ovr,
            decisions: ovr,
            composure: ovr,
            aggression: ovr,
            teamwork: ovr,
            leadership: ovr,
            handling: 20,
            reflexes: 20,
            aerial: ovr,
        }
    }

    fn player(id: &str, dob: &str, ovr: u8, potential: u8) -> ExternalPlayer {
        ExternalPlayer {
            id: id.to_string(),
            match_name: id.to_string(),
            full_name: format!("Player {id}"),
            date_of_birth: dob.to_string(),
            nationality: "GB".to_string(),
            position: Position::CentralMidfielder,
            natural_position: Position::CentralMidfielder,
            alternate_positions: vec![],
            footedness: Footedness::Right,
            weak_foot: 3,
            attributes: attrs(ovr),
            ovr,
            potential,
            market_value: 1_000_000,
            club: Some(ExternalClub {
                id: "fm-c-1".to_string(),
                name: "External FC".to_string(),
                contract_end: Some("2028-06-30".to_string()),
                weekly_wage: 8_000,
                reputation: 600,
            }),
        }
    }

    fn clubs() -> Vec<ExternalClubRef> {
        let mut clubs: Vec<ExternalClubRef> = (0..40)
            .map(|n| ExternalClubRef {
                id: format!("fm-c-{n}"),
                name: format!("Club {n}"),
                nation_code: "GB".to_string(),
                reputation: 100 + n * 22,
            })
            .collect();
        clubs.sort_by_key(|club| club.reputation);
        clubs
    }

    /// Given a source player and the same career key, when he is projected twice for the same
    /// season, then every field is identical: a repeated preview cannot re-roll him.
    #[test]
    fn a_projection_is_identical_when_repeated() {
        let key = ExternalCareerKey::new(7, 2026);
        let one = project_player(&player("fm-p-1", "2000-01-01", 65, 80), key, 2030, &clubs());
        let two = project_player(&player("fm-p-1", "2000-01-01", 65, 80), key, 2030, &clubs());
        assert_eq!(one.player.ovr, two.player.ovr);
        assert_eq!(one.player.potential, two.player.potential);
        assert_eq!(one.player.market_value, two.player.market_value);
        assert_eq!(one.player.club, two.player.club);
        assert_eq!(one.retired, two.retired);
    }

    /// Given two careers with different seeds, when the same source player is projected, then at
    /// least one career's history differs: the shared registry is not one career's save.
    #[test]
    fn two_careers_with_different_seeds_diverge() {
        let source = player("fm-p-1", "2000-01-01", 65, 80);
        let a = project_player(&source, ExternalCareerKey::new(1, 2026), 2032, &clubs());
        let b = project_player(&source, ExternalCareerKey::new(2, 2026), 2032, &clubs());
        assert!(
            a.player.ovr != b.player.ovr
                || a.player.club != b.player.club
                || a.retired != b.retired,
            "two seeds projected the same career"
        );
    }

    /// Given a young player with room to grow, when he is projected several seasons forward, then
    /// he improves, never passes his ceiling, and remains above his starting ability.
    #[test]
    fn a_young_player_develops_within_his_ceiling() {
        let source = player("fm-p-2", "2010-01-01", 52, 88);
        let projected = project_player(&source, ExternalCareerKey::new(11, 2026), 2033, &clubs());
        assert!(projected.player.ovr > source.ovr);
        assert!(projected.player.ovr <= projected.player.potential);
        assert!(projected.player.potential >= source.potential.saturating_sub(4));
    }

    /// Given an old player, when he is projected forward, then he declines and eventually retires:
    /// a source player cannot be frozen at 38.
    #[test]
    fn an_old_player_declines_and_retires() {
        let source = player("fm-p-3", "1990-01-01", 72, 72);
        let early = project_player(&source, ExternalCareerKey::new(3, 2026), 2029, &clubs());
        let late = project_player(&source, ExternalCareerKey::new(3, 2026), 2034, &clubs());
        assert!(late.retired || late.player.ovr <= early.player.ovr);
        assert!(project_player(&source, ExternalCareerKey::new(3, 2026), 2040, &clubs()).retired);
    }

    /// Given a retirement at one season, when the same player is projected further, then he never
    /// comes back: retirement is monotone across a rebuild.
    #[test]
    fn retirement_never_reverses() {
        let key = ExternalCareerKey::new(5, 2026);
        let source = player("fm-p-4", "1989-01-01", 70, 70);
        let mut saw_retired = false;
        for season in 2026..=2040 {
            let projected = project_player(&source, key, season, &clubs());
            if projected.retired {
                saw_retired = true;
            } else {
                assert!(!saw_retired, "a retired player reappeared at {season}");
            }
        }
        assert!(saw_retired);
    }

    /// Given a lowball offer, when the club decides, then it rejects; at or above its minimum it
    /// counters, and at the asking price it accepts. None of it is random.
    #[test]
    fn club_decisions_are_ordered_and_deterministic() {
        let context = ExternalOfferContext {
            seed: 9,
            season: 2030,
            asking_price: 1_000_000,
            free_agent: false,
            player_ovr: 75,
            player_potential: 82,
            player_age: 24,
            contract_years_left: 3,
            seller_reputation: 620,
            buyer_reputation: 700,
            player_id: "fm-p-5".to_string(),
        };
        let minimum = context.minimum_fee();
        assert!((800_000..=1_300_000).contains(&minimum));
        assert!(matches!(
            context.decide(minimum / 2),
            ExternalOfferDecision::Rejected { .. }
        ));
        assert!(matches!(
            context.decide(minimum - 1),
            ExternalOfferDecision::Countered { fee } if fee == minimum
        ));
        assert_eq!(
            context.decide(minimum),
            ExternalOfferDecision::Accepted { fee: minimum }
        );
        assert_eq!(context.decide(minimum), context.decide(minimum));
        // The price a one-click move offers is the club's own request, so it always accepts it.
        let requested = context.asking_price.max(minimum);
        assert_eq!(
            context.decide(requested),
            ExternalOfferDecision::Accepted { fee: requested }
        );
        // An important player is expensive, not impossible: the counter is never above 1.3x.
        assert!(minimum <= 1_300_000);
    }

    /// Given a free agent, when an offer is decided, then no fee is due whichever way the club's
    /// valuation would have gone.
    #[test]
    fn a_free_agent_is_always_accepted_for_nothing() {
        let context = ExternalOfferContext {
            asking_price: 0,
            free_agent: true,
            player_id: "fm-p-6".to_string(),
            ..offer_context()
        };
        assert_eq!(
            context.decide(0),
            ExternalOfferDecision::Accepted { fee: 0 }
        );
        assert_eq!(
            context.decide(5_000_000),
            ExternalOfferDecision::Accepted { fee: 0 }
        );
    }

    /// Given a newgen class, when it is generated twice, then the identities are unique and the
    /// same seed re-generates the same class; the elite slots reach the top of the market and only
    /// a bounded share of the class is elite.
    #[test]
    fn newgens_are_unique_and_repeatable() {
        let key = ExternalCareerKey::new(21, 2026);
        let intake = 400;
        let first = newgen_cohort(key, 2027, &clubs(), intake);
        let second = newgen_cohort(key, 2027, &clubs(), intake);
        assert_eq!(first.len(), intake);
        assert_eq!(
            first.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            second.iter().map(|p| p.id.clone()).collect::<Vec<_>>()
        );
        let mut ids: Vec<&String> = first.iter().map(|p| &p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), first.len());
        assert!(first.iter().all(|p| p.id.starts_with("fm-n-2027-")));
        assert!(first.iter().all(|p| p.potential >= p.ovr));

        let elite_count = elite_newgens_for(intake);
        assert!((1..=ELITE_NEWGEN_MAX_PER_SEASON).contains(&elite_count));
        let elite = first.iter().filter(|p| p.potential >= 86).count();
        assert_eq!(elite, elite_count, "elite slots did not reach 86");
        assert!(elite * 100 / first.len() < 10, "elite share is not small");
    }

    /// Given a season's retirement count, when the intake is computed, then it replaces deaths
    /// one-for-one and is bounded; a season with no deaths adds nobody.
    #[test]
    fn intake_tracks_retirements_one_for_one() {
        assert_eq!(newgen_intake_for(0), 0);
        assert_eq!(newgen_intake_for(2_340), 2_340);
        assert_eq!(
            newgen_intake_for(MAX_NEWGEN_INTAKE_PER_SEASON + 5_000),
            MAX_NEWGEN_INTAKE_PER_SEASON
        );
        assert_eq!(elite_newgens_for(0), 0);
        assert_eq!(elite_newgens_for(2_000), 16);
        assert_eq!(
            elite_newgens_for(MAX_NEWGEN_INTAKE_PER_SEASON),
            ELITE_NEWGEN_MAX_PER_SEASON
        );
    }

    /// Given a career thirty seasons old, when the market is projected, then it is populated by
    /// newgens, not emptied by the source cohort's retirement; and no player exceeds his ceiling.
    #[test]
    fn thirty_seasons_keep_a_populated_market() {
        let key = ExternalCareerKey::new(31, 2026);
        let sources: Vec<ExternalPlayer> = (0..400)
            .map(|n| {
                let age = 16 + (n % 22);
                let dob = format!("{}-01-01", 2026 - age);
                player(
                    &format!("fm-p-{n}"),
                    &dob,
                    45 + (n % 35) as u8,
                    65 + (n % 30) as u8,
                )
            })
            .collect();
        let market = projected_market(&sources, key, 2056, &clubs());
        assert!(
            market.len() >= sources.len() * 9 / 10,
            "market collapsed: {} players",
            market.len()
        );
        assert!(
            market.len() <= sources.len() * 11 / 10,
            "market inflated: {} players",
            market.len()
        );
        assert!(market.iter().any(|projected| projected.newgen));
        assert!(market.iter().all(|projected| !projected.retired));
        assert!(
            market
                .iter()
                .all(|projected| projected.player.ovr <= projected.player.potential)
        );
        let sources_left = market.iter().filter(|projected| !projected.newgen).count();
        assert!(sources_left < sources.len(), "no source player retired");
        // Elite regeneration: some generated prospect must carry a top-of-market ceiling, or the
        // top of the world decays to the source cohort's last veterans.
        assert!(
            market
                .iter()
                .any(|projected| projected.newgen && projected.player.potential >= 86),
            "no elite prospect was generated"
        );
        let elite = market
            .iter()
            .filter(|projected| projected.player.potential >= 86)
            .count();
        assert!(
            elite * 100 / market.len() < 15,
            "elite prospects are not rare: {elite} of {}",
            market.len()
        );
    }

    fn source_player(n: usize) -> ExternalPlayer {
        let age = 16 + (n % 22);
        let ovr = 42 + (n % 43) as u8;
        let potential = (u16::from(ovr) + (n % 25) as u16).min(96) as u8;
        let position = match n % 4 {
            0 => Position::Goalkeeper,
            1 => Position::CenterBack,
            2 => Position::CentralMidfielder,
            _ => Position::Striker,
        };
        let mut player = player(
            &format!("fm-p-{n}"),
            &format!("{}-01-01", 2026 - age),
            ovr,
            potential,
        );
        player.position = position.clone();
        player.natural_position = position;
        player.club = if n.is_multiple_of(5) {
            None
        } else {
            let club = n % 40;
            Some(ExternalClub {
                id: format!("fm-c-{club}"),
                name: format!("Club {club}"),
                contract_end: Some(format!("{}-06-30", 2026 + (n % 4) as i32)),
                weekly_wage: 5_000 + (n % 30) as u32 * 500,
                reputation: 100 + (club as i64) * 22,
            })
        };
        player
    }

    fn advance_market(
        market: &mut Vec<ExternalPlayer>,
        key: ExternalCareerKey,
        season: i32,
        clubs: &[ExternalClubRef],
    ) {
        let mut retirements = 0usize;
        market.retain_mut(|external| {
            let projected = project_from(
                external,
                key,
                season - 1,
                season,
                clubs,
                external.id.starts_with("fm-n-"),
            );
            if projected.retired {
                retirements += 1;
                return false;
            }
            *external = projected.player;
            true
        });
        for newgen in newgen_cohort(key, season, clubs, newgen_intake_for(retirements)) {
            let projected = project_from(&newgen, key, season, season, clubs, true);
            if !projected.retired {
                market.push(projected.player);
            }
        }
    }

    fn market_fingerprint(market: &[ExternalPlayer]) -> String {
        let mut rows: Vec<String> = market
            .iter()
            .map(|player| {
                format!(
                    "{}:{}:{}:{}",
                    player.id,
                    player.ovr,
                    player.club.as_ref().map(|c| c.id.as_str()).unwrap_or("-"),
                    player.market_value
                )
            })
            .collect();
        rows.sort();
        rows.join("|")
    }

    /// Given a synthetic source, when a five-season market is advanced twice, then the same seed
    /// produces the same world and two seeds produce different ones.
    #[test]
    fn five_seasons_are_repeatable_and_seed_specific() {
        let sources: Vec<ExternalPlayer> = (0..120).map(source_player).collect();
        let key = ExternalCareerKey::new(41, 2026);
        let mut one = sources.clone();
        let mut two = sources.clone();
        for season in 2027..=2031 {
            advance_market(&mut one, key, season, &clubs());
            advance_market(&mut two, key, season, &clubs());
        }
        assert_eq!(market_fingerprint(&one), market_fingerprint(&two));

        let mut other_seed = sources;
        let other = ExternalCareerKey::new(42, 2026);
        for season in 2027..=2031 {
            advance_market(&mut other_seed, other, season, &clubs());
        }
        assert_ne!(market_fingerprint(&one), market_fingerprint(&other_seed));
    }

    /// Thirty seasons of a synthetic worldwide market, for three seeds. Run explicitly:
    /// `cargo test -p ofm_core --lib external_world::tests::thirty_season_markets -- --ignored --nocapture`.
    /// The assertions are the long-career invariants the report reads: no collapse, no duplicate
    /// identity, no ability above a ceiling, no universal free agency and no quality inflation.
    #[test]
    #[ignore = "30-season multi-seed market benchmark; run explicitly for the D7B2 report"]
    fn thirty_season_markets_stay_populated_and_credible() {
        let start = 2026;
        let sources: Vec<ExternalPlayer> = (0..1_200).map(source_player).collect();
        for seed in [101_u64, 202, 303] {
            let key = ExternalCareerKey::new(seed, start);
            let mut market = sources.clone();
            println!("seed={seed} season={start} pop={}", market.len());
            for season in (start + 1)..=(start + 30) {
                advance_market(&mut market, key, season, &clubs());
                if (season - start) % 5 == 0 {
                    let on = NaiveDate::from_ymd_opt(season, 7, 1).unwrap();
                    let ages: Vec<i32> = market
                        .iter()
                        .filter_map(|player| player.age_on(on))
                        .collect();
                    let mean_age =
                        ages.iter().map(|age| f64::from(*age)).sum::<f64>() / ages.len() as f64;
                    let free = market
                        .iter()
                        .filter(|player| player.is_free_agent(on))
                        .count();
                    let mean_ovr = market
                        .iter()
                        .map(|player| f64::from(player.ovr))
                        .sum::<f64>()
                        / market.len() as f64;
                    let high = market.iter().filter(|player| player.ovr >= 85).count();
                    let young = ages.iter().filter(|age| **age <= 23).count();
                    println!(
                        "seed={seed} season={season} pop={} newgens={} avg_age={mean_age:.1} \
                         young={:.1}% free={:.1}% avg_ovr={mean_ovr:.1} ovr85+={:.1}%",
                        market.len(),
                        market.iter().filter(|p| p.id.starts_with("fm-n-")).count(),
                        young as f64 * 100.0 / market.len() as f64,
                        free as f64 * 100.0 / market.len() as f64,
                        high as f64 * 100.0 / market.len() as f64,
                    );
                }
            }
            let mut ids: Vec<&String> = market.iter().map(|player| &player.id).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(
                ids.len(),
                market.len(),
                "seed {seed} duplicated an identity"
            );
            assert!(market.len() > sources.len() / 2, "seed {seed} collapsed");
            assert!(
                market.iter().all(|player| player.ovr <= player.potential),
                "seed {seed} exceeded a ceiling"
            );
            assert!(market.iter().any(|player| player.id.starts_with("fm-n-")));
            let on = NaiveDate::from_ymd_opt(start + 30, 7, 1).unwrap();
            let free = market
                .iter()
                .filter(|player| player.is_free_agent(on))
                .count();
            let free_share = free as f64 / market.len() as f64;
            assert!(
                (0.02..0.75).contains(&free_share),
                "seed {seed} free-agent share {free_share}"
            );
            let high = market.iter().filter(|player| player.ovr >= 85).count();
            assert!(
                (high as f64 / market.len() as f64) < 0.20,
                "seed {seed} inflated quality"
            );
        }
    }

    fn offer_context() -> ExternalOfferContext {
        ExternalOfferContext {
            seed: 1,
            season: 2030,
            asking_price: 1_000_000,
            free_agent: false,
            player_ovr: 70,
            player_potential: 80,
            player_age: 26,
            contract_years_left: 2,
            seller_reputation: 600,
            buyer_reputation: 600,
            player_id: "fm-p-x".to_string(),
        }
    }
}
