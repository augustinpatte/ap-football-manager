//! D6 measurement probe: quantify the shipped generated world's data before and
//! after the career-start identity upgrade, and audit the competition graph a
//! career builds. Not a regression test — run explicitly:
//!
//! ```text
//! cargo test -p ofm_core --test d6_world_measure -- --ignored --nocapture
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;
use domain::manager::Manager;
use domain::player::{Player, Position, SquadRole};
use domain::stats::StatsState;
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::generator::{
    DefinitionSources, WorldGenConfig, default_opening_year, generate_world_data_seeded_with,
    repair_opening_youth_academies,
};
use ofm_core::player_identity::upgrade_game_player_identities;
use ofm_core::player_rating::natural_ovr;
use ofm_core::world::start_date_for_year;
use ofm_core::world_integrity::{IntegrityLimits, audit_competitions, audit_players};

const SEED: u64 = 20260820;

fn build_world() -> Game {
    let sources = DefinitionSources::embedded_only();
    let mut config = WorldGenConfig::standard_from(&sources);
    let opening_year = i32::try_from(default_opening_year()).expect("year fits i32");
    config.opening_year = Some(u32::try_from(opening_year).expect("year fits u32"));
    let world = generate_world_data_seeded_with(SEED, &config, &sources);
    let start = start_date_for_year(opening_year).expect("valid start date");
    let manager = Manager::new(
        "d6-auditor".to_string(),
        "D6".to_string(),
        "Auditor".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    let mut game = Game::new(
        GameClock::new(start),
        manager,
        world.teams,
        world.players,
        world.staff,
        vec![],
    );
    game.seed = SEED;
    game.available_staff_market_last_activity_date = Some(start.format("%Y-%m-%d").to_string());
    repair_opening_youth_academies(&mut game);
    game
}

fn drift(players: &[Player]) -> (usize, usize, usize, f64) {
    let mut over_half = 0;
    let mut over_one = 0;
    let mut over_two = 0;
    let mut max: f64 = 0.0;
    for player in players {
        let d = (natural_ovr(player) - f64::from(player.ovr)).abs();
        if d > 0.5 {
            over_half += 1;
        }
        if d > 1.0 {
            over_one += 1;
        }
        if d > 2.0 {
            over_two += 1;
        }
        if d > max {
            max = d;
        }
    }
    (over_half, over_one, over_two, max)
}

fn position_counts(players: &[Player]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for player in players {
        *counts.entry(format!("{:?}", player.position)).or_default() += 1;
    }
    counts
}

fn natural_counts(players: &[Player]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for player in players {
        *counts
            .entry(format!("{:?}", player.natural_position))
            .or_default() += 1;
    }
    counts
}

fn age_band(age: i32) -> &'static str {
    match age {
        ..=15 => "<16",
        16..=18 => "16-18",
        19..=23 => "19-23",
        24..=29 => "24-29",
        30..=34 => "30-34",
        35..=39 => "35-39",
        _ => "40+",
    }
}

fn report_players(label: &str, teams: &[domain::team::Team], players: &[Player], on: NaiveDate) {
    println!("\n===== {label} =====");
    println!("players: {}", players.len());

    let free_agents = players.iter().filter(|p| p.team_id.is_none()).count();
    println!("free agents: {free_agents}");

    let mut per_club: HashMap<&str, usize> = HashMap::new();
    for player in players {
        if let Some(team_id) = player.team_id.as_deref() {
            *per_club.entry(team_id).or_default() += 1;
        }
    }
    let sizes: Vec<usize> = teams
        .iter()
        .map(|t| per_club.get(t.id.as_str()).copied().unwrap_or(0))
        .collect();
    let total_rows: usize = sizes.iter().sum();
    let min = sizes.iter().copied().min().unwrap_or(0);
    let max = sizes.iter().copied().max().unwrap_or(0);
    let mean = total_rows as f64 / sizes.len().max(1) as f64;
    let zero = sizes.iter().filter(|n| **n == 0).count();
    let under_18 = teams
        .iter()
        .filter(|t| {
            let seniors = players
                .iter()
                .filter(|p| {
                    p.team_id.as_deref() == Some(t.id.as_str()) && p.squad_role == SquadRole::Senior
                })
                .count();
            seniors < 18
        })
        .count();
    println!(
        "clubs: {} | roster rows {total_rows} | size min {min} max {max} mean {mean:.2} | empty {zero} | clubs <18 seniors {under_18}",
        teams.len()
    );

    let positions = position_counts(players);
    println!("bucket positions: {positions:?}");
    let naturals = natural_counts(players);
    println!("natural positions: {naturals:?}");

    // Keepers per club, by the coarse bucket the engine groups on.
    let mut gks: HashMap<&str, usize> = HashMap::new();
    for player in players {
        if player.position == Position::Goalkeeper
            && let Some(team_id) = player.team_id.as_deref()
        {
            *gks.entry(team_id).or_default() += 1;
        }
    }
    let no_gk = teams
        .iter()
        .filter(|t| gks.get(t.id.as_str()).copied().unwrap_or(0) == 0)
        .count();
    let one_gk = teams
        .iter()
        .filter(|t| gks.get(t.id.as_str()).copied().unwrap_or(0) == 1)
        .count();
    let three_plus_gk = teams
        .iter()
        .filter(|t| gks.get(t.id.as_str()).copied().unwrap_or(0) >= 3)
        .count();
    println!("clubs with 0 keepers {no_gk}, 1 keeper {one_gk}, 3+ keepers {three_plus_gk}");

    let mut bands: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut implausible = 0;
    for player in players {
        match domain::age::age_on(on, &player.date_of_birth) {
            Some(age) => {
                *bands.entry(age_band(age)).or_default() += 1;
                if !(14..=45).contains(&age) {
                    implausible += 1;
                }
            }
            None => implausible += 1,
        }
    }
    println!("age bands: {bands:?} | implausible {implausible}");

    let opening = on.format("%Y-%m-%d").to_string();
    let mut missing_end = 0;
    let mut missing_start = 0;
    let mut expired = 0;
    let mut zero_wage = 0;
    let mut after_opening_start = 0;
    for player in players {
        if player.team_id.is_none() {
            continue;
        }
        match player.contract_end() {
            None => missing_end += 1,
            Some(end) => {
                if end < opening.as_str() {
                    expired += 1;
                }
            }
        }
        if player.contract_start().is_none() {
            missing_start += 1;
        } else if let Some(start) = player.contract_start()
            && start > opening.as_str()
        {
            after_opening_start += 1;
        }
        if player.wage() == 0 {
            zero_wage += 1;
        }
    }
    let contracted = players.iter().filter(|p| p.team_id.is_some()).count();
    println!(
        "contracted {contracted} | missing start {missing_start} | start after opening {after_opening_start} | missing end {missing_end} | expired {expired} | zero wage {zero_wage}"
    );

    let limits = IntegrityLimits::new(on);
    let report = audit_players(teams, players, &limits);
    println!("audit findings: {:?}", report.counts());
    let (h, o, t, max_drift) = drift(players);
    println!(
        "ovr drift >0.5 {h} ({:.1}%), >1.0 {o} ({:.1}%), >2.0 {t}, max {max_drift:.2}",
        h as f64 * 100.0 / players.len() as f64,
        o as f64 * 100.0 / players.len() as f64,
    );
}

fn report_competitions(game: &Game) {
    println!("\n===== competitions after begin_career =====");
    println!("competitions: {}", game.competitions.len());
    let team_ids: HashSet<&str> = game.teams.iter().map(|t| t.id.as_str()).collect();
    let mut total_fixtures = 0;
    let mut unknown_participants = 0;
    let mut empty_participants = 0;
    let mut competitions_without_fixtures = 0;
    let mut participants: HashSet<&str> = HashSet::new();
    let mut unknown_by_competition: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for competition in &game.competitions {
        total_fixtures += competition.fixtures.len();
        if competition.participant_ids.is_empty() {
            empty_participants += 1;
        }
        if competition.fixtures.is_empty() {
            competitions_without_fixtures += 1;
        }
        for id in &competition.participant_ids {
            participants.insert(id.as_str());
            if !team_ids.contains(id.as_str()) {
                unknown_participants += 1;
                unknown_by_competition
                    .entry(competition.name.clone())
                    .or_default()
                    .push(id.clone());
            }
        }
    }
    println!("unknown participants by competition: {unknown_by_competition:?}");
    let teams_in_no_competition = team_ids
        .iter()
        .filter(|id| !participants.contains(**id))
        .count();
    println!(
        "fixtures {total_fixtures} | empty participant lists {empty_participants} | competitions without fixtures {competitions_without_fixtures}"
    );
    println!(
        "unknown participant ids {unknown_participants} | teams in no competition {teams_in_no_competition}"
    );

    let mut fixture_unknown = 0;
    let mut fixture_self = 0;
    let mut fixture_duplicates = 0;
    for competition in &game.competitions {
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for fixture in &competition.fixtures {
            for id in [&fixture.home_team_id, &fixture.away_team_id] {
                if !team_ids.contains(id.as_str()) {
                    fixture_unknown += 1;
                }
            }
            if fixture.home_team_id == fixture.away_team_id {
                fixture_self += 1;
            }
            let pair = (fixture.home_team_id.as_str(), fixture.away_team_id.as_str());
            if !seen.insert(pair) {
                fixture_duplicates += 1;
            }
        }
    }
    println!(
        "fixture unknown clubs {fixture_unknown} | self fixtures {fixture_self} | duplicate home/away pairs {fixture_duplicates}"
    );

    let mut fixture_outside_participants = 0;
    let mut duplicate_fixture_ids = 0;
    let mut fixture_ids: HashSet<&str> = HashSet::new();
    let mut bad_fixture_dates = 0;
    for competition in &game.competitions {
        let participants: HashSet<&str> = competition
            .participant_ids
            .iter()
            .map(String::as_str)
            .collect();
        for fixture in &competition.fixtures {
            if !fixture_ids.insert(fixture.id.as_str()) {
                duplicate_fixture_ids += 1;
            }
            for id in [&fixture.home_team_id, &fixture.away_team_id] {
                if !participants.contains(id.as_str()) {
                    fixture_outside_participants += 1;
                }
            }
            if NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").is_err() {
                bad_fixture_dates += 1;
            }
        }
    }
    println!(
        "fixture teams outside participant list {fixture_outside_participants} | duplicate fixture ids {duplicate_fixture_ids} | unparsable fixture dates {bad_fixture_dates}"
    );

    let mut known: HashSet<&str> = team_ids.iter().copied().collect();
    known.extend(game.national_teams.iter().map(|team| team.id.as_str()));
    let competition_report = audit_competitions(&game.competitions, &known);
    println!(
        "competition audit findings: {:?}",
        competition_report.counts()
    );

    let mut per_competition: Vec<(String, usize, usize)> = game
        .competitions
        .iter()
        .map(|c| (c.name.clone(), c.participant_ids.len(), c.fixtures.len()))
        .collect();
    per_competition.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, participants, fixtures) in per_competition.iter().take(12) {
        println!("  {name}: {participants} clubs, {fixtures} fixtures");
    }
}

#[test]
#[ignore = "measurement probe, run explicitly with --ignored --nocapture"]
fn measure_generated_world() {
    let mut game = build_world();
    let on = game.clock.current_date.date_naive();

    report_players(
        "generated world (before identification)",
        &game.teams,
        &game.players,
        on,
    );

    let before: HashMap<String, (u8, u8, u32)> = game
        .players
        .iter()
        .map(|player| {
            (
                player.id.clone(),
                (
                    player.ovr,
                    player.potential,
                    domain::age::playing_age(on, &player.date_of_birth),
                ),
            )
        })
        .collect();

    let changed = upgrade_game_player_identities(&mut game);
    println!("\nidentity upgrade changed players: {changed}");
    report_players(
        "generated world (after identification)",
        &game.teams,
        &game.players,
        on,
    );

    let mut potential_raised_to_ovr_young = 0;
    let mut potential_raised_above_old = 0;
    let mut headroom_reduced = 0;
    let mut headroom_reduced_young = 0;
    let mut max_raise = 0_u8;
    for player in &game.players {
        let Some((old_ovr, old_potential, age)) = before.get(&player.id) else {
            continue;
        };
        if player.potential > *old_potential {
            potential_raised_above_old += 1;
            max_raise = max_raise.max(player.potential - old_potential);
            if *age <= 23 {
                potential_raised_to_ovr_young += 1;
            }
        }
        let old_headroom = old_potential.saturating_sub(*old_ovr);
        let new_headroom = player.potential.saturating_sub(player.ovr);
        if new_headroom < old_headroom {
            headroom_reduced += 1;
            if *age <= 23 {
                headroom_reduced_young += 1;
            }
        }
    }
    println!(
        "ceilings raised to the corrected ovr (the stored ceiling was below it): {potential_raised_above_old} | of those aged <=23: {potential_raised_to_ovr_young} | max raise {max_raise} | growth room reduced {headroom_reduced} (aged <=23: {headroom_reduced_young})"
    );

    let club = game.teams[0].id.clone();
    match begin_career(
        &mut game,
        &club,
        CareerScope::default(),
        StatsState::default(),
    ) {
        Ok(_) => {
            report_players("after begin_career", &game.teams, &game.players, on);
            report_competitions(&game);
        }
        Err(error) => println!("begin_career failed: {error}"),
    }
}
