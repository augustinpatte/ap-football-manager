//! The import pipeline: world.db -> OFM package (on disk) -> OFM-built world -> overrides.
//!
//! ```text
//! Source (read-only) --convert--> package files --load_world_package + validate_package-->
//! build_world_from_package_seeded(seed) --> post-build overrides --> WorldData
//! ```
//!
//! The package is written to disk and read back with OFM's own loader, so the world is built from
//! exactly what `ofm-cli validate` sees. Nothing here uses a random source: the build seed is a
//! configured constant.

use crate::Result;
use crate::attributes;
use crate::competitions;
use crate::config::Config;
use crate::finance::{self, FinanceRecord};
use crate::ids;
use crate::journal::Journal;
use crate::positions;
use crate::refs::{Refs, ShortCodes};
use crate::source::{ClubRow, PlayerRow, Source, StaffRow};
use crate::staff;
use domain::manager::Manager;
use domain::player::{Position, SquadRole};
use ofm_core::generator::{
    DefinitionSources, PlayerDef, TeamColorsDef, TeamDef, WorldData, WorldDataKind, WorldMetaDef,
    build_world_from_package_seeded, load_world_package, validate_package,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What was decided for one imported player, kept so the checks can compare the built world
/// against it field by field.
#[derive(Debug, Clone)]
pub struct PlayerPlan {
    pub person_id: i64,
    pub def: PlayerDef,
    pub natural_code: &'static str,
    pub ovr_expected: u8,
    pub nationality_source: &'static str,
    pub contract_end_source: &'static str,
    pub value_source: &'static str,
}

#[derive(Debug, Clone)]
pub struct MappingRow {
    pub ofm_id: String,
    pub entity: &'static str,
    pub world_db_id: i64,
    pub fm_uid: Option<i64>,
    pub fm_uid_status: String,
    pub name: String,
    pub ofm_club_id: String,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct GeneratedPlayer {
    pub club_id: i64,
    pub id: String,
    pub position: Position,
    pub ovr: u8,
}

#[derive(Debug, Clone)]
pub struct GeneratedStaff {
    pub club_id: i64,
    pub id: String,
    pub role: String,
}

/// Everything a run produces and the checks and report need.
pub struct Run {
    pub cfg: Config,
    pub source_sha256_before: String,
    pub source_sha256_after: String,
    pub schema: crate::source::SchemaFacts,
    pub competitions: Vec<crate::source::CompetitionRow>,
    pub clubs: Vec<ClubRow>,
    pub players: Vec<PlayerRow>,
    pub staff_rows: Vec<StaffRow>,
    pub plans: Vec<PlayerPlan>,
    pub staff_plans: Vec<(ofm_core::generator::StaffDef, i64)>,
    pub managers: Vec<(String, i64)>,
    pub manager_clubs_missing: Vec<i64>,
    pub package_dir: PathBuf,
    pub package_errors: Vec<String>,
    pub world: WorldData,
    pub journal: Journal,
    pub mapping: Vec<MappingRow>,
    pub finance: Vec<FinanceRecord>,
    pub generated_players: Vec<GeneratedPlayer>,
    pub generated_staff: Vec<GeneratedStaff>,
    pub removed_generated_players: usize,
    pub total_players_in_source: (i64, i64),
    pub club_country: BTreeMap<i64, String>,
    pub short_codes: BTreeMap<i64, String>,
}

pub const EXPECTED_CLUBS: usize = 240;
pub const EXPECTED_COMPETITIONS: usize = 12;

fn hex_colour(rgb: &str) -> String {
    format!("#{}", rgb.to_lowercase())
}

/// Entry point. `package_dir` is created (or emptied of our own files) and receives the package.
pub fn run(world_db: &Path, package_dir: &Path, cfg: &Config) -> Result<Run> {
    let refs = Refs::embedded()?;
    let source_sha256_before = Source::sha256(world_db)?;
    let source = Source::open(world_db)?;
    let schema = source.schema_facts()?;
    let mut journal = Journal::default();

    // --- perimeter, by internal ids ------------------------------------------------------------
    let perimeter = source.perimeter(schema.start_season)?;
    let club_ids: Vec<i64> = perimeter.iter().map(|(_, club)| *club).collect();
    let unique_clubs: BTreeSet<i64> = club_ids.iter().copied().collect();
    if unique_clubs.len() != club_ids.len() {
        return Err("a club appears in two perimeter competitions".to_string());
    }
    let competition_ids: Vec<i64> = perimeter
        .iter()
        .map(|(comp, _)| *comp)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let competitions = source.competitions(&competition_ids)?;
    let mut members: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for (comp, club) in &perimeter {
        members.entry(*comp).or_default().push(*club);
    }
    let clubs = source.clubs(&perimeter)?;
    let players = source.players(&club_ids)?;
    let staff_rows = source.staff(&club_ids)?;
    let total_players_in_source = source.total_players()?;

    // --- clubs -> teams ------------------------------------------------------------------------
    let mut club_country: BTreeMap<i64, String> = BTreeMap::new();
    let mut short_codes_by_club: BTreeMap<i64, String> = BTreeMap::new();
    let mut shorts = ShortCodes::default();
    let mut teams: Vec<TeamDef> = Vec::new();
    for club in &clubs {
        let id = ids::club(club.id);
        let country = club
            .nation
            .as_deref()
            .and_then(|n| refs.nation_code(n))
            .ok_or_else(|| {
                format!(
                    "{id} ({}): club nation `{:?}` has no OFM code",
                    club.name, club.nation
                )
            })?;
        club_country.insert(club.id, country.clone());
        let city = match refs.city(club.id) {
            Some((checked_name, city)) if checked_name == club.name => city.to_string(),
            Some((checked_name, _)) => {
                journal.error(
                    "club.city_row_mismatch",
                    &id,
                    format!(
                        "club_city.csv names `{checked_name}` but world.db names `{}`",
                        club.name
                    ),
                );
                club.name.clone()
            }
            None => {
                journal.error(
                    "club.city_missing",
                    &id,
                    format!("no city for {}", club.name),
                );
                club.name.clone()
            }
        };
        let short = shorts.assign(&club.short_name, &club.name);
        short_codes_by_club.insert(club.id, short.clone());
        let (primary, secondary) = match &club.colours {
            Some(c) => (hex_colour(&c.primary), hex_colour(&c.secondary)),
            None => {
                journal.warn(
                    "club.colours_missing",
                    &id,
                    "no colours in world.db; white/black used",
                );
                ("#ffffff".to_string(), "#000000".to_string())
            }
        };
        let reputation = (club.reputation / 10).clamp(0, 1000) as u32;
        teams.push(TeamDef {
            id,
            name: club.name.clone(),
            short_name: short,
            city,
            country,
            colors: TeamColorsDef { primary, secondary },
            play_style: "Balanced".to_string(),
            stadium_name: String::new(),
            reputation_range: Some([reputation, reputation]),
            finance_range: club.finance.as_ref().map(|f| {
                let balance = cfg.to_eur(f.balance_gbp).max(0);
                [balance, balance]
            }),
            logo: None,
            kit_pattern: None,
            founded_year: None,
        });
    }

    // --- players -------------------------------------------------------------------------------
    let club_by_id: BTreeMap<i64, &ClubRow> = clubs.iter().map(|c| (c.id, c)).collect();
    let mut plans: Vec<PlayerPlan> = Vec::new();
    for row in &players {
        let club = club_by_id[&row.club_id];
        let country = &club_country[&row.club_id];
        if let Some(plan) = plan_player(row, club, country, cfg, &refs, &mut journal) {
            plans.push(plan);
        }
    }

    // --- staff and managers --------------------------------------------------------------------
    let country_of = |club_id: i64| club_country.get(&club_id).cloned();
    let staff_plans = staff::build(&staff_rows, &country_of, &refs, &mut journal);
    let (chosen_managers, manager_clubs_missing) = staff::pick_managers(&staff_rows, &club_ids);
    let mut managers: Vec<(String, i64)> = Vec::new();
    for row in &chosen_managers {
        managers.push((ids::staff(row.person_id), row.club_id));
    }
    for club in &manager_clubs_missing {
        journal.warn(
            "manager.club_without_head_coach",
            ids::club(*club),
            "no first-team head coach in world.db; OFM will appoint a generated manager",
        );
    }

    // --- package on disk, then OFM's own loader and validator ----------------------------------
    write_package(
        package_dir,
        cfg,
        &teams,
        &plans,
        &staff_plans,
        &competitions,
        &members,
        &refs,
        &mut journal,
    )?;
    let (package, load_errors) = load_world_package(package_dir);
    let mut package_errors: Vec<String> = load_errors.iter().map(describe_package_error).collect();
    package_errors.extend(
        validate_package(&package)
            .iter()
            .map(describe_package_error),
    );
    if !package_errors.is_empty() {
        let shown: Vec<_> = package_errors.iter().take(5).cloned().collect();
        return Err(format!(
            "OFM rejects the package ({} errors), first: {}",
            package_errors.len(),
            shown.join(" | ")
        ));
    }

    // --- OFM builds the world --------------------------------------------------------------------
    let mut world = build_world_from_package_seeded(
        cfg.world.seed,
        &package,
        Some(cfg.world.base_year as u32),
        &DefinitionSources::embedded_only(),
    )?;

    world.metadata.world_id = cfg.world.id.clone();
    world.metadata.kind = WorldDataKind::RosterBaseline;
    world.metadata.base_year = Some(cfg.world.base_year);
    world.metadata.snapshot_date = None;
    world.name = cfg.world.name.clone();
    world.description = cfg.world.description.clone();

    let removed = if cfg.squad.trim_generated_to_floor {
        trim_generated_players(&mut world)
    } else {
        0
    };
    apply_staff_contracts(&mut world, &staff_rows, cfg);
    apply_managers(&mut world, &chosen_managers, &refs, cfg, &mut journal);
    let finance_records = restore_finance(&mut world, &clubs, cfg, &mut journal);

    // --- bookkeeping -----------------------------------------------------------------------------
    let (generated_players, generated_staff) = list_generated(&world, &clubs);
    let mapping = mapping_rows(
        &players,
        &staff_rows,
        &clubs,
        &competitions,
        &plans,
        &staff_plans,
        &chosen_managers,
    );
    let source_sha256_after = Source::sha256(world_db)?;

    Ok(Run {
        cfg: cfg.clone(),
        source_sha256_before,
        source_sha256_after,
        schema,
        competitions,
        clubs,
        players,
        staff_rows,
        plans,
        staff_plans,
        managers,
        manager_clubs_missing,
        package_dir: package_dir.to_path_buf(),
        package_errors,
        world,
        journal,
        mapping,
        finance: finance_records,
        generated_players,
        generated_staff,
        removed_generated_players: removed,
        total_players_in_source,
        club_country,
        short_codes: short_codes_by_club,
    })
}

fn describe_package_error(error: &ofm_core::generator::package::PackageError) -> String {
    let params: Vec<String> = error
        .params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!("{} [{}] {}", error.code, error.file, params.join(","))
}

// ---------------------------------------------------------------------------------------------
// players
// ---------------------------------------------------------------------------------------------

fn plan_player(
    row: &PlayerRow,
    club: &ClubRow,
    club_country: &str,
    cfg: &Config,
    refs: &Refs,
    journal: &mut Journal,
) -> Option<PlayerPlan> {
    let id = ids::player(row.person_id);
    // Best FM note; among positions tied for it, the one OFM itself rates highest for this player.
    let mut best: Option<(&'static str, Position, domain::player::PlayerAttributes, u8)> = None;
    for (code, candidate) in
        positions::natural_candidates(&row.positions, cfg.positions.natural_within)
    {
        let attrs = attributes::convert(&row.attrs, &candidate);
        let rating = attributes::ovr(&attrs, &candidate);
        if best.as_ref().is_none_or(|(_, _, _, r)| rating > *r) {
            best = Some((code, candidate, attrs, rating));
        }
    }
    let Some((natural_code, position, converted, ovr)) = best else {
        journal.error(
            "player.excluded_no_position",
            &id,
            "no usable FM position note; player not imported",
        );
        return None;
    };
    // OFM keeps weak foot and alternate positions only for a specific outfield position; a goalkeeper
    // has neither (the engine re-derives them on every load), so none is written for one.
    let is_keeper = position == Position::Goalkeeper;
    let alternates = if is_keeper {
        Vec::new()
    } else {
        positions::alternates(&row.positions, natural_code, &cfg.positions)
    };
    let (foot, weak_foot) = positions::foot(row.foot_left, row.foot_right, &cfg.positions);

    let potential = attributes::potential(ovr, row.ca, row.pa, cfg.potential.ovr_points_per_ca);

    let (nationality, nationality_source) =
        match row.nation.as_deref().and_then(|n| refs.nation_code(n)) {
            Some(code) => (code, "save"),
            None => {
                journal.warn(
                "player.nation_fallback",
                &id,
                "no usable nationality in the save; the club's country is used (not an FM fact)",
            );
                (club_country.to_string(), "club (repli)")
            }
        };
    if row.birth_date.is_none() {
        journal.warn(
            "player.birth_missing",
            &id,
            "no birth date in the save; OFM draws an age",
        );
    }

    let (contract_end, contract_end_source) = match &row.end_date {
        Some(end)
            if row
                .start_date
                .as_deref()
                .is_none_or(|start| end.as_str() > start) =>
        {
            (end.clone(), "save")
        }
        Some(end) => {
            journal.warn(
                "player.contract_end_not_after_start",
                &id,
                format!("contract end {end} is not after its start; configured fallback end used"),
            );
            (cfg.contracts.missing_end.clone(), "générée")
        }
        None => (cfg.contracts.missing_end.clone(), "générée"),
    };

    let (value, value_source) = match (row.value_gbp, row.value_source.as_deref()) {
        (Some(v), Some("save")) => (Some(cfg.to_eur(v).max(0) as u64), "save"),
        (Some(v), _) => (Some(cfg.to_eur(v).max(0) as u64), "estimée (importeur)"),
        (None, _) => (None, "ofm-générée"),
    };

    let name = row
        .common_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_default();

    let def = PlayerDef {
        id: id.clone(),
        name,
        first_name: row.first_name.clone(),
        last_name: row.last_name.clone(),
        club: ids::club(club.id),
        nationality,
        position: position.clone(),
        date_of_birth: row.birth_date.clone(),
        age: None,
        overall: None,
        potential: Some(potential),
        attributes: Some(converted),
        photo: None,
        footedness: Some(positions::footedness_name(&foot).to_string()),
        youth: row.team_is_youth,
        contract_start: row.start_date.clone(),
        contract_end: Some(contract_end),
        contract_length: None,
        wage: Some(u32::try_from(cfg.to_eur(row.wage_gbp_week).max(0)).unwrap_or(u32::MAX)),
        value,
        weak_foot: (!is_keeper).then_some(weak_foot),
        alternate_positions: alternates,
        condition: None,
        morale: None,
        career_history: Vec::new(),
    };
    Some(PlayerPlan {
        person_id: row.person_id,
        def,
        natural_code,
        ovr_expected: ovr,
        nationality_source,
        contract_end_source,
        value_source,
    })
}

// ---------------------------------------------------------------------------------------------
// package files
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn write_package(
    dir: &Path,
    cfg: &Config,
    teams: &[TeamDef],
    plans: &[PlayerPlan],
    staff_plans: &[(ofm_core::generator::StaffDef, i64)],
    competition_rows: &[crate::source::CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    refs: &Refs,
    journal: &mut Journal,
) -> Result<()> {
    // The package folder is ours: start clean so a previous run cannot leak files into this one.
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("clean {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(dir.join("players")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("staff")).map_err(|e| e.to_string())?;

    let meta = WorldMetaDef {
        id: cfg.world.id.clone(),
        name: cfg.world.name.clone(),
        description: cfg.world.description.clone(),
        default_active_regions: vec!["europe".to_string()],
        default_active_competitions: Vec::new(),
        base_year: Some(cfg.world.base_year),
        version: cfg.world.version.clone(),
        author: cfg.world.author.clone(),
        format_version: 1,
        license: cfg.world.license.clone(),
        game_min_version: String::new(),
        package_type: "database".to_string(),
        logo: None,
        fallback_league: None,
    };
    let mut manifest = serde_json::to_value(&meta).map_err(|e| e.to_string())?;
    manifest["schema"] = json!("world");
    write_json(&dir.join("package.json"), &manifest)?;

    write_items(&dir.join("teams.json"), "team", teams)?;

    let competition_defs = competitions::build(competition_rows, members, refs, journal)?;
    write_items(
        &dir.join("competitions.json"),
        "competition",
        &competition_defs,
    )?;

    let mut by_club: BTreeMap<&str, Vec<&PlayerDef>> = BTreeMap::new();
    for plan in plans {
        by_club
            .entry(plan.def.club.as_str())
            .or_default()
            .push(&plan.def);
    }
    for (club, defs) in by_club {
        write_items(
            &dir.join("players").join(format!("{club}.json")),
            "player",
            &defs,
        )?;
    }
    let mut staff_by_club: BTreeMap<&str, Vec<&ofm_core::generator::StaffDef>> = BTreeMap::new();
    for (def, _) in staff_plans {
        staff_by_club
            .entry(def.club.as_str())
            .or_default()
            .push(def);
    }
    for (club, defs) in staff_by_club {
        write_items(
            &dir.join("staff").join(format!("{club}.json")),
            "staff",
            &defs,
        )?;
    }
    Ok(())
}

fn write_items<T: serde::Serialize>(path: &Path, schema: &str, items: &[T]) -> Result<()> {
    write_json(path, &json!({ "schema": schema, "items": items }))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// post-build overrides
// ---------------------------------------------------------------------------------------------

fn is_imported(id: &str) -> bool {
    id.starts_with("fm-p-") || id.starts_with("fm-s-")
}

/// OFM pads every club to 22 players with generated ones. Only what OFM's own squad floor needs is
/// kept (15 seniors, at least 2 G / 4 D / 4 M / 2 F counting the imported players), best rated
/// first; the rest of the generated players are removed. Imported players are never touched.
/// Returns the number of generated players removed.
fn trim_generated_players(world: &mut WorldData) -> usize {
    use ofm_core::squad_floor::{MIN_PLAYERS_PER_GROUP, MIN_SENIOR_PLAYERS};
    let before = world.players.len();
    let team_ids: Vec<String> = world.teams.iter().map(|t| t.id.clone()).collect();
    let mut keep_generated: BTreeSet<String> = BTreeSet::new();
    for team_id in &team_ids {
        let members: Vec<&domain::player::Player> = world
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team_id.as_str()))
            .collect();
        let group_of = |p: &domain::player::Player| p.position.to_group_position();
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for p in members
            .iter()
            .filter(|p| is_imported(&p.id) && p.squad_role == SquadRole::Senior)
        {
            *counts.entry(group_index(&group_of(p))).or_default() += 1;
        }
        let mut chosen: Vec<&domain::player::Player> = Vec::new();
        // Generated players only. OFM makes some of them academy players; one needed to reach the
        // floor is promoted to the senior squad (seniors are preferred, then best rated, then id).
        let mut pool: Vec<&domain::player::Player> = members
            .iter()
            .copied()
            .filter(|p| !is_imported(&p.id))
            .collect();
        pool.sort_by(|a, b| {
            (b.squad_role == SquadRole::Senior)
                .cmp(&(a.squad_role == SquadRole::Senior))
                .then(b.ovr.cmp(&a.ovr))
                .then(a.id.cmp(&b.id))
        });
        for (index, (group, floor)) in MIN_PLAYERS_PER_GROUP.iter().enumerate() {
            let have = counts.get(&index).copied().unwrap_or(0);
            let mut need = floor.saturating_sub(have);
            for p in pool.iter().filter(|p| &group_of(p) == group) {
                if need == 0 {
                    break;
                }
                chosen.push(p);
                *counts.entry(index).or_default() += 1;
                need -= 1;
            }
        }
        let have_total: usize = counts.values().sum();
        let mut extra = MIN_SENIOR_PLAYERS.saturating_sub(have_total);
        for p in pool.iter() {
            if extra == 0 {
                break;
            }
            if !chosen.iter().any(|c| c.id == p.id) {
                chosen.push(p);
                extra -= 1;
            }
        }
        keep_generated.extend(chosen.into_iter().map(|p| p.id.clone()));
    }
    world
        .players
        .retain(|p| is_imported(&p.id) || keep_generated.contains(&p.id));
    for p in world.players.iter_mut().filter(|p| !is_imported(&p.id)) {
        p.squad_role = SquadRole::Senior;
    }
    before - world.players.len()
}

fn group_index(group: &Position) -> usize {
    ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
        .iter()
        .position(|(g, _)| g == group)
        .unwrap_or(0)
}

/// `StaffDef` carries no wage or contract end: set them on the built staff.
fn apply_staff_contracts(world: &mut WorldData, rows: &[StaffRow], cfg: &Config) {
    let by_id: BTreeMap<String, &StaffRow> =
        rows.iter().map(|r| (ids::staff(r.person_id), r)).collect();
    for member in &mut world.staff {
        if let Some(row) = by_id.get(&member.id) {
            let (wage, end) = staff::contract(row, cfg);
            member.wage = wage;
            member.contract_end = end;
        }
    }
}

/// Real head coaches become the clubs' managers.
fn apply_managers(
    world: &mut WorldData,
    chosen: &[&StaffRow],
    refs: &Refs,
    cfg: &Config,
    journal: &mut Journal,
) {
    for row in chosen {
        let id = ids::staff(row.person_id);
        let team_id = ids::club(row.club_id);
        let Some(dob) = row.birth_date.clone() else {
            journal.warn(
                "manager.birth_missing",
                &id,
                "head coach has no birth date; OFM appoints a generated manager",
            );
            continue;
        };
        let club_country = world
            .teams
            .iter()
            .find(|t| t.id == team_id)
            .map(|t| t.country.clone())
            .unwrap_or_default();
        let nationality = match row.nation.as_deref().and_then(|n| refs.nation_code(n)) {
            Some(code) => code,
            None => {
                journal.warn(
                    "manager.nation_fallback",
                    &id,
                    "no usable nationality; the club's country is used",
                );
                club_country
            }
        };
        let mut manager = Manager::new(
            id.clone(),
            row.first_name.clone(),
            row.last_name.clone(),
            dob,
            nationality,
        );
        manager.reputation = staff::manager_reputation(row.rep_current);
        manager.hire(team_id.clone());
        // The save does not say when the tenure began: the entry opens on the world's configured
        // start date (the same convention OFM uses for the managers it invents).
        let team_name = world
            .teams
            .iter()
            .find(|t| t.id == team_id)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        manager
            .career_history
            .push(domain::manager::ManagerCareerEntry::open(
                team_id.clone(),
                team_name,
                cfg.start.date.clone(),
            ));
        if let Some(common) = row.common_name.as_ref().filter(|c| !c.trim().is_empty()) {
            manager.set_known_as(Some(common.clone()));
        }
        if let Some(team) = world.teams.iter_mut().find(|t| t.id == team_id) {
            team.manager_id = Some(id);
        }
        world.managers.push(manager);
    }
    world.managers.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Restores balance, transfer budget and wage budget where the source has a compatible value.
fn restore_finance(
    world: &mut WorldData,
    clubs: &[ClubRow],
    cfg: &Config,
    journal: &mut Journal,
) -> Vec<FinanceRecord> {
    let mut bills: BTreeMap<String, i64> = BTreeMap::new();
    for p in world.players.iter().filter(|p| is_imported(&p.id)) {
        if let Some(team) = &p.team_id {
            *bills.entry(team.clone()).or_default() += i64::from(p.wage());
        }
    }
    let mut records = Vec::new();
    for club in clubs {
        let id = ids::club(club.id);
        if let Some(team) = world.teams.iter_mut().find(|t| t.id == id) {
            let bill = bills.get(&id).copied().unwrap_or(0);
            records.extend(finance::restore(team, club, bill, cfg, journal));
        }
    }
    records
}

fn list_generated(
    world: &WorldData,
    clubs: &[ClubRow],
) -> (Vec<GeneratedPlayer>, Vec<GeneratedStaff>) {
    let club_of = |team_id: &str| ids::internal_key(team_id, "fm-c-").unwrap_or(0);
    let known: BTreeSet<i64> = clubs.iter().map(|c| c.id).collect();
    let mut players: Vec<GeneratedPlayer> = world
        .players
        .iter()
        .filter(|p| !is_imported(&p.id))
        .map(|p| GeneratedPlayer {
            club_id: p.team_id.as_deref().map(club_of).unwrap_or(0),
            id: p.id.clone(),
            position: p.position.clone(),
            ovr: p.ovr,
        })
        .collect();
    players.sort_by(|a, b| a.club_id.cmp(&b.club_id).then(a.id.cmp(&b.id)));
    let mut staff: Vec<GeneratedStaff> = world
        .staff
        .iter()
        .filter(|s| !is_imported(&s.id))
        .map(|s| GeneratedStaff {
            club_id: s.team_id.as_deref().map(club_of).unwrap_or(0),
            id: s.id.clone(),
            role: format!("{:?}", s.role),
        })
        .collect();
    staff.sort_by(|a, b| a.club_id.cmp(&b.club_id).then(a.id.cmp(&b.id)));
    debug_assert!(players.iter().all(|p| known.contains(&p.club_id)));
    (players, staff)
}

#[allow(clippy::too_many_arguments)]
fn mapping_rows(
    players: &[PlayerRow],
    staff_rows: &[StaffRow],
    clubs: &[ClubRow],
    competitions: &[crate::source::CompetitionRow],
    plans: &[PlayerPlan],
    staff_plans: &[(ofm_core::generator::StaffDef, i64)],
    managers: &[&StaffRow],
) -> Vec<MappingRow> {
    let mut rows = Vec::new();
    for c in competitions {
        rows.push(MappingRow {
            ofm_id: ids::competition(c.id),
            entity: "competition",
            world_db_id: c.id,
            fm_uid: c.fm_uid,
            fm_uid_status: String::new(),
            name: c.name.clone(),
            ofm_club_id: String::new(),
            note: format!(
                "{} ; niveau {}",
                if c.playable { "jouable" } else { "simulée" },
                c.level.unwrap_or(0)
            ),
        });
    }
    for c in clubs {
        rows.push(MappingRow {
            ofm_id: ids::club(c.id),
            entity: "club",
            world_db_id: c.id,
            fm_uid: c.fm_uid,
            fm_uid_status: String::new(),
            name: c.name.clone(),
            ofm_club_id: String::new(),
            note: String::new(),
        });
    }
    let planned: BTreeSet<i64> = plans.iter().map(|p| p.person_id).collect();
    for p in players.iter().filter(|p| planned.contains(&p.person_id)) {
        rows.push(MappingRow {
            ofm_id: ids::player(p.person_id),
            entity: "player",
            world_db_id: p.person_id,
            fm_uid: p.fm_uid,
            fm_uid_status: p.fm_uid_status.clone(),
            name: format!("{} {}", p.first_name, p.last_name)
                .trim()
                .to_string(),
            ofm_club_id: ids::club(p.club_id),
            note: String::new(),
        });
    }
    let staffed: BTreeSet<i64> = staff_plans.iter().map(|(_, id)| *id).collect();
    let managed: BTreeSet<i64> = managers.iter().map(|m| m.person_id).collect();
    for s in staff_rows {
        let (entity, note) = if managed.contains(&s.person_id) {
            ("manager", s.role_code.clone())
        } else if staffed.contains(&s.person_id) {
            ("staff", s.role_code.clone())
        } else {
            continue;
        };
        rows.push(MappingRow {
            ofm_id: ids::staff(s.person_id),
            entity,
            world_db_id: s.person_id,
            fm_uid: s.fm_uid,
            fm_uid_status: s.fm_uid_status.clone(),
            name: format!("{} {}", s.first_name, s.last_name)
                .trim()
                .to_string(),
            ofm_club_id: ids::club(s.club_id),
            note,
        });
    }
    rows.sort_by(|a, b| {
        a.entity
            .cmp(b.entity)
            .then(a.world_db_id.cmp(&b.world_db_id))
    });
    rows
}
