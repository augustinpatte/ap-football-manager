//! I1 study: one career that really recruits from the external registry while P5's world plays on.
//!
//! The registry is built from the player's own FM database (`I1_WORLD_DB`, never in the
//! repository) with the production `build_registry`; the career is a generated P5 world played
//! through the production day and rollover; the manager shops in the external market before the
//! season and monthly for free agents, under the world's own money and wage rules (nothing is
//! topped up). What is measured is what the brief names: money, squads, identities, the market's
//! population, the elite tier, every refusal, and whether a reload plays on identically.
//!
//! `I1_SEED` (default 1), `I1_SEASONS` (default 5), `I1_WORLD` (`gate` | `shipped` | `lower` | `fm24`).
//! `fm24` plays the imported 240-club world: `APFM_WORLD_JSON` names the world file the importer
//! wrote and `I1_CLUB` the club to manage (default `fm-c-501`). Both studies return at once when
//! `I1_WORLD_DB` is not set.

use std::collections::{BTreeMap, HashSet};

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use fm24_adapter::registry::{build_registry, RegistryAvailability, RegistrySort};
use ofm_core::end_of_season;
use ofm_core::external_registry::{
    negotiate_external_transfer, recruit_external_player, ExternalNegotiationRequest,
    NEGOTIATION_AGREED,
};
use ofm_core::finances::calc_wages;
use season_harness::driver::{play_day, roll_season_over, RunOptions, SeasonObserver};
use season_harness::env;
use season_harness::fingerprint::fingerprint;
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

use super::*;

const MONTH: u32 = 30;

/// One look at the market: the best `attempts` of a kind by ability, stopping at `quota` signings.
/// A season's passes keep a study squad a squad and still reach a contracted player, whom the
/// best-ability list never lets in (the elite are the dearest).
#[derive(Clone, Copy)]
struct Pass {
    availability: RegistryAvailability,
    attempts: usize,
    quota: u32,
}

const PRESEASON: [Pass; 2] = [
    Pass {
        availability: RegistryAvailability::All,
        attempts: 12,
        quota: 2,
    },
    Pass {
        availability: RegistryAvailability::FreeAgent,
        attempts: 12,
        quota: 2,
    },
];
/// A contracted player can only be bought with the window open, so this pass waits for one.
const WINDOW: [Pass; 1] = [Pass {
    availability: RegistryAvailability::Contracted,
    attempts: 100,
    quota: 1,
}];
const MONTHLY: [Pass; 1] = [Pass {
    availability: RegistryAvailability::FreeAgent,
    attempts: 12,
    quota: 1,
}];

#[derive(Default)]
struct Tally {
    signed_free: u32,
    signed_contracted: u32,
    fees: u64,
    refused: BTreeMap<String, u32>,
    /// A refusal that changed cash, budget, roster or inbox: the invariant that must stay zero.
    refusals_that_mutated: u32,
    /// A signing whose cash or budget did not fall by exactly its fee.
    ledger_mismatches: u32,
}

struct Money {
    cash: i64,
    budget: i64,
    players: usize,
    messages: usize,
}

fn money_of(game: &Game, club: &str) -> Money {
    let team = game
        .teams
        .iter()
        .find(|team| team.id == club)
        .expect("the club");
    Money {
        cash: team.finance,
        budget: team.transfer_budget,
        players: game.players.len(),
        messages: game.messages.len(),
    }
}

/// The best `attempts` of a kind, by ability, as the screen would list them.
fn candidates(
    base: &Path,
    game: &Game,
    availability: RegistryAvailability,
    attempts: usize,
) -> Vec<ExternalPlayer> {
    let context = registry_context(game);
    let request = RegistrySearchRequest {
        availability,
        sort: RegistrySort::Ovr,
        // Best first. D7B3 made `descending` mean "the larger value first" for every sort, the
        // reading the screen's arrow has; before it the registry read `descending: true` as
        // ascending for Ovr and Value, and this study asked for `false` to compensate.
        descending: true,
        page_size: attempts as u32,
        ..RegistrySearchRequest::default()
    };
    let page = search_internal(base, &context, request).expect("the search runs");
    page.items
        .iter()
        .filter_map(|item| detail_internal(base, &context, &item.id).ok().flatten())
        .filter_map(|detail| external_from_detail(detail).ok())
        .collect()
}

/// One shopping round. `I1_SIGNINGS=0` is the control: the same career with nobody recruited.
fn shop(base: &Path, game: &mut Game, club: &str, passes: &[Pass], tally: &mut Tally) {
    if std::env::var("I1_SIGNINGS").as_deref() == Ok("0") {
        return;
    }
    for pass in passes {
        let mut signed = 0;
        for external in candidates(base, game, pass.availability, pass.attempts) {
            if signed >= pass.quota {
                break;
            }
            let before = money_of(game, club);
            let negotiation =
                negotiate_external_transfer(game, &external, ExternalNegotiationRequest::default())
                    .expect("a negotiation");
            match recruit_external_player(game, &external) {
                Ok(outcome) => {
                    assert_eq!(
                        negotiation.status, NEGOTIATION_AGREED,
                        "signed without agreement"
                    );
                    signed += 1;
                    let after = money_of(game, club);
                    let fee = i64::try_from(outcome.fee).expect("a fee fits");
                    if before.cash - after.cash != fee || before.budget - after.budget != fee {
                        tally.ledger_mismatches += 1;
                    }
                    tally.fees += outcome.fee;
                    if outcome.free_agent {
                        tally.signed_free += 1;
                    } else {
                        tally.signed_contracted += 1;
                    }
                }
                Err(key) => {
                    let after = money_of(game, club);
                    let changed = before.cash != after.cash
                        || before.budget != after.budget
                        || before.players != after.players
                        || before.messages != after.messages;
                    tally.refusals_that_mutated += u32::from(changed);
                    // The screen said so too: what the negotiation reports is what signing does,
                    // since one-click signing is a negotiation that signs only on agreement.
                    assert_ne!(
                        negotiation.status, NEGOTIATION_AGREED,
                        "refused {key} but the negotiation was agreed"
                    );
                    *tally.refused.entry(key).or_default() += 1;
                }
            }
        }
    }
}

struct Census {
    market: u32,
    free_agents: u32,
    contracted: u32,
    newgens: usize,
    elite_80: usize,
    elite_85: usize,
    best_ovr: u8,
}

/// The market as this career sees it today: its size, and the elite tier among the best hundred.
fn census(base: &Path, game: &Game) -> Census {
    let context = registry_context(game);
    let request = RegistrySearchRequest {
        sort: RegistrySort::Ovr,
        // Best first. D7B3 made `descending` mean "the larger value first" for every sort, the
        // reading the screen's arrow has; before it the registry read `descending: true` as
        // ascending for Ovr and Value, and this study asked for `false` to compensate.
        descending: true,
        page_size: 100,
        ..RegistrySearchRequest::default()
    };
    let page = search_internal(base, &context, request).expect("the search runs");
    let newgens = page
        .items
        .iter()
        .filter(|item| item.id.starts_with("fm-n-"))
        .count();
    Census {
        market: page.counts.all,
        free_agents: page.counts.free_agent,
        contracted: page.counts.contracted,
        newgens,
        elite_80: page.items.iter().filter(|item| item.ovr >= 80).count(),
        elite_85: page.items.iter().filter(|item| item.ovr >= 85).count(),
        best_ovr: page.items.first().map_or(0, |item| item.ovr),
    }
}

fn duplicate_identities(game: &Game) -> usize {
    let mut seen = HashSet::new();
    game.players
        .iter()
        .map(|player| player.id.as_str())
        .chain(game.retired_players.iter().map(|player| player.id.as_str()))
        .filter(|id| !seen.insert(*id))
        .count()
}

fn window_is_open(game: &Game) -> bool {
    use domain::season::TransferWindowStatus::{DeadlineDay, Open};
    matches!(
        game.season_context.transfer_window.status,
        Open | DeadlineDay
    )
}

fn seniors_of(game: &Game, club: &str) -> usize {
    ofm_core::contracts::squad_capacity(game, club).registered_seniors
}

/// The career the study plays: a generated world, or the imported 240-club world of the player's file.
fn world_for(seed: u64) -> Game {
    if std::env::var("I1_WORLD").as_deref() == Ok("fm24") {
        let path = world_path_from_env().expect("APFM_WORLD_JSON names the imported world");
        let club = std::env::var("I1_CLUB").unwrap_or_else(|_| "fm-c-501".to_string());
        return build_fm24_career(&path, &club, seed).expect("the imported world builds");
    }
    env::world("I1_WORLD", seed)
        .build()
        .expect("the world builds")
}

fn user_club(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

struct Quiet;

impl SeasonObserver for Quiet {}

#[test]
#[ignore = "study: needs I1_WORLD_DB (the player's own FM database) and minutes of runtime"]
fn i1_combined_career() {
    let Some(source) = std::env::var_os("I1_WORLD_DB").map(PathBuf::from) else {
        eprintln!("I1_WORLD_DB is not set; nothing to study");
        return;
    };
    let seed = env::first("I1_SEED", "1");
    let seasons = env::first("I1_SEASONS", "5") as u32;
    let dir = tempfile::tempdir().expect("a directory");
    let base = dir.path().join("registry.db");
    let report = build_registry(&source, &base).expect("the registry builds");
    eprintln!(
        "registry: {} stored, {} eligible, {} active-world",
        report.stored, report.eligible, report.active_world
    );

    let mut game = world_for(seed);
    let club = user_club(&game);
    let mut rules = Invariants::new(&game);
    let options = RunOptions::seasons(1);
    let mut tally = Tally::default();
    let started = std::time::Instant::now();
    println!(
        "season | market (free/contracted) | newgens/100 | best ovr, 80+/85+ of best 100 | signed free/contracted | \
         cash | budget | wages/wk | seniors | clubs in debt | duplicates"
    );
    for season in 1..=seasons {
        let market = census(&base, &game);
        let (free_before, contracted_before) = (tally.signed_free, tally.signed_contracted);
        shop(&base, &mut game, &club, &PRESEASON, &mut tally);
        let mut day = 0;
        let mut window_shopped = false;
        while !end_of_season::is_season_complete(&game) {
            let open = window_is_open(&game);
            if open && !window_shopped {
                shop(&base, &mut game, &club, &WINDOW, &mut tally);
            }
            window_shopped = open;
            day += 1;
            assert!(day < 500, "season {season} did not finish");
            play_day(&mut game, options, season, day, &mut rules).expect("a day plays");
            if day % MONTH == 0 {
                shop(&base, &mut game, &club, &MONTHLY, &mut tally);
            }
        }
        let team = game
            .teams
            .iter()
            .find(|team| team.id == club)
            .expect("the club");
        println!(
            "{season} | {} ({}/{}) | {} | {}, {}/{} | {}/{} | {} | {} | {} | {} | {} | {}",
            market.market,
            market.free_agents,
            market.contracted,
            market.newgens,
            market.best_ovr,
            market.elite_80,
            market.elite_85,
            tally.signed_free - free_before,
            tally.signed_contracted - contracted_before,
            team.finance,
            team.transfer_budget,
            calc_wages(&game, &club),
            seniors_of(&game, &club),
            game.teams.iter().filter(|team| team.finance < 0).count(),
            duplicate_identities(&game),
        );
        roll_season_over(&mut game, options, season, &mut rules).expect("the rollover");
    }

    println!(
        "recruited {} free agents and {} contracted players for {} in fees over {seasons} seasons in {:.0?}",
        tally.signed_free,
        tally.signed_contracted,
        tally.fees,
        started.elapsed()
    );
    println!("refusals by key: {:?}", tally.refused);
    println!(
        "refusals that mutated: {}; ledger mismatches: {}; duplicate identities: {}; harness invariants: {}",
        tally.refusals_that_mutated,
        tally.ledger_mismatches,
        duplicate_identities(&game),
        if rules.is_clean() { "clean".to_string() } else { rules.report() }
    );
    println!("{}", reload_twin(&mut game, &base, seasons, &mut rules));
    assert_eq!(
        tally.refusals_that_mutated, 0,
        "a refused signing changed the game"
    );
    assert_eq!(
        tally.ledger_mismatches, 0,
        "a signing did not cost exactly its fee"
    );
    assert_eq!(duplicate_identities(&game), 0, "an identity exists twice");
    assert!(rules.is_clean(), "{}", rules.report());
}

/// The kinds of line (the fingerprint's first word) on which two fingerprints disagree, with how
/// many lines, ignoring order. `top_up` lines are the squad-floor diagnostics journal, which a save
/// does not hold; on the imported world a reload also yields a few extra `news` articles 60 days on.
/// Both are true of P5's tip as well (`season-harness/tests/i1_fm24_reload.rs`), so a difference in
/// any other kind (a player, a contract, a table, money) is the one that would matter.
fn differing_kinds(left: &str, right: &str) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for line in left.lines().filter(|line| !line.starts_with("top_up ")) {
        counts.entry(line).or_default().0 += 1;
    }
    for line in right.lines().filter(|line| !line.starts_with("top_up ")) {
        counts.entry(line).or_default().1 += 1;
    }
    let mut kinds = BTreeMap::new();
    for (line, (a, b)) in counts {
        if a != b {
            let kind = line.split_whitespace().next().unwrap_or("").to_string();
            *kinds.entry(kind).or_default() += a.abs_diff(b);
        }
    }
    kinds
}

/// Save the career, load it back, play both sixty days on, and say whether they and the markets
/// they see agree: as listed, and with competitions in a fixed order. Reports, never asserts, so a
/// difference is measured rather than hidden behind a panic.
fn reload_twin(original: &mut Game, base: &Path, seasons: u32, rules: &mut Invariants) -> String {
    let dir = tempfile::tempdir().expect("a directory");
    let mut manager = SaveManager::init(dir.path()).expect("a save manager");
    let id = manager
        .create_save_with_stats(original, &StatsState::default(), "i1")
        .expect("the save");
    let mut twin = SaveManager::init(dir.path())
        .expect("a fresh manager")
        .load_game(&id)
        .expect("the load");
    let at_reload = (fingerprint(original), fingerprint(&twin));
    let options = RunOptions::seasons(1);
    for day in 1..=60 {
        play_day(original, options, seasons + 1, day, rules).expect("plays");
        play_day(&mut twin, options, seasons + 1, day, &mut Quiet).expect("plays");
    }
    let after = (fingerprint(original), fingerprint(&twin));
    let ids = |game: &Game| -> Vec<String> {
        candidates(base, game, RegistryAvailability::All, 50)
            .into_iter()
            .map(|player| player.id)
            .collect()
    };
    let kinds = |pair: &(String, String)| differing_kinds(&pair.0, &pair.1);
    format!(
        "reload twin after {seasons} seasons: lines that differ by kind (squad-floor journal excluded) \
         at reload {:?}, 60 days on {:?}; same market {}",
        kinds(&at_reload),
        kinds(&after),
        ids(original) == ids(&twin),
    )
}

/// How the registry's books and the game's roster relate in one world: what the exclusion set is
/// protecting, and what no screen could ever reach.
#[test]
#[ignore = "study: needs I1_WORLD_DB and, for the imported world, APFM_WORLD_JSON"]
fn i1_world_overlap() {
    let Some(source) = std::env::var_os("I1_WORLD_DB").map(PathBuf::from) else {
        eprintln!("I1_WORLD_DB is not set; nothing to study");
        return;
    };
    let dir = tempfile::tempdir().expect("a directory");
    let base = dir.path().join("registry.db");
    build_registry(&source, &base).expect("the registry builds");
    let game = world_for(env::first("I1_SEED", "1"));
    let context = registry_context(&game);
    let cache = career_registry_path(&base, context.seed, context.today).expect("a cache");
    let conn = fm24_adapter::registry::open_registry(&cache).expect("the cache opens");
    let status_of: BTreeMap<String, String> = conn
        .prepare("SELECT ofm_id, status FROM registry_player")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .and_then(|rows| rows.collect())
        })
        .expect("the rows read");
    let roster: HashSet<&str> = game
        .players
        .iter()
        .map(|player| player.id.as_str())
        .collect();
    let registry_ids: HashSet<&str> = status_of.keys().map(String::as_str).collect();
    let roster_source: Vec<&str> = roster
        .iter()
        .copied()
        .filter(|id| id.starts_with("fm-p-"))
        .collect();
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
    for id in &roster_source {
        let status = status_of
            .get(*id)
            .map_or("not in the registry", String::as_str);
        *by_status.entry(status).or_default() += 1;
    }
    let mut active_absent: BTreeMap<&str, usize> = BTreeMap::new();
    for (id, status) in &status_of {
        if status == "active_world" && !roster.contains(id.as_str()) {
            *active_absent
                .entry("active_world rows not on the roster")
                .or_default() += 1;
        }
    }
    let request = |excluded: Vec<String>| {
        let context = RegistryContext {
            excluded_ids: excluded,
            ..context.clone()
        };
        search_internal(&base, &context, RegistrySearchRequest::default())
            .expect("a search")
            .total
    };
    let unfiltered = request(vec![]);
    let filtered = request(context.excluded_ids.clone());
    let searchable_on_roster = unfiltered - filtered;
    println!(
        "game: {} clubs, {} players, {} with fm-p-* ids",
        game.teams.len(),
        game.players.len(),
        roster_source.len()
    );
    println!(
        "registry: {} rows; roster fm-p ids by registry status: {by_status:?}",
        status_of.len()
    );
    println!("{active_absent:?}");
    println!(
        "searchable players the roster already holds (what the exclusion hides): {searchable_on_roster};          market offered: {filtered} of {unfiltered}"
    );
    let collisions = game
        .players
        .iter()
        .filter(|p| registry_ids.contains(p.id.as_str()))
        .count();
    println!("roster ids that are registry ids: {collisions}");
}
