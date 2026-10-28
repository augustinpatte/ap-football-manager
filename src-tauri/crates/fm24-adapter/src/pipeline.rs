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
use crate::free_agents::{self, FreeAgentPlan, PoolStats};
use crate::ids;
use crate::journal::Journal;
use crate::placement::{self, Kind, Placement};
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
    /// Where he plays and for whom (see `placement`).
    pub placement: Placement,
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
    pub free_agent_plans: Vec<FreeAgentPlan>,
    pub free_agent_stats: PoolStats,
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
    /// Imported academy players promoted to the senior squad to reach OFM's floor instead of
    /// generating a filler (`squad.promote_academy_before_generating`).
    pub promoted_academy_players: Vec<String>,
    /// Players imported as loans (`team_id` = where he plays, `active_loan` = owner and terms).
    pub loans_applied: Vec<String>,
    pub total_players_in_source: (i64, i64),
    pub club_country: BTreeMap<i64, String>,
    pub short_codes: BTreeMap<i64, String>,
}

pub const EXPECTED_CLUBS: usize = 240;
pub const EXPECTED_COMPETITIONS: usize = 12;
/// The national cups created beside the leagues.
pub const EXPECTED_CUPS: usize = 6;

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
    let start_day = chrono::NaiveDate::parse_from_str(&cfg.start.date, "%Y-%m-%d")
        .map_err(|e| format!("start.date: {e}"))?;
    let affiliates = placement::affiliated_sides(&players, &unique_clubs);
    for row in &players {
        let Some(placement) = placement::place(
            row,
            &unique_clubs,
            &affiliates,
            start_day,
            ofm_core::roster::YOUTH_ACADEMY_MAX_AGE,
        ) else {
            journal.error(
                "player.not_placed",
                ids::player(row.person_id),
                "neither his contract club nor his registered club is in the perimeter",
            );
            continue;
        };
        // The package builder takes the club the player is built for: his owner while he is on a
        // loan (the loan is applied after the build), otherwise the club he plays for.
        let built_for = match placement.kind {
            Kind::LoanInsideWorld | Kind::LoanToOutsideClub | Kind::AffiliateLoan => {
                placement.owner_club.unwrap_or(row.club_id)
            }
            Kind::Own | Kind::LoanedInFromOutside | Kind::AffiliateAcademy => {
                placement.playing_club
            }
        };
        let club = club_by_id[&built_for];
        let country = &club_country[&built_for];
        if let Some(plan) = plan_player(row, placement, club, country, cfg, &refs, &mut journal) {
            plans.push(plan);
        }
    }

    // --- free agents: a bounded pool of the save's unattached players ----------------------------
    let (free_agent_plans, free_agent_stats) = if cfg.free_agents.enabled {
        let unattached = source.unattached_players()?;
        let pool = free_agents::select(&unattached, &players, cfg, &refs);
        (pool.plans, pool.stats)
    } else {
        (Vec::new(), PoolStats::default())
    };

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

    // --- reserve sides and the rules that depend on them ---------------------------------------
    let reserve_pairs = crate::rules::reserve_pairs(&players, &unique_clubs);
    for (side, parent) in &reserve_pairs {
        journal.info(
            "reserve.side",
            &ids::club(*side),
            format!(
                "{} is read as the reserve side of {} (players of the parent registered with it)",
                clubs
                    .iter()
                    .find(|c| c.id == *side)
                    .map_or("?", |c| c.name.as_str()),
                clubs
                    .iter()
                    .find(|c| c.id == *parent)
                    .map_or("?", |c| c.name.as_str()),
            ),
        );
    }
    let reputation: BTreeMap<i64, i64> = clubs.iter().map(|c| (c.id, c.reputation)).collect();
    let rule_context = competitions::RuleContext {
        authored: cfg.competitions.authored_rules,
        reserve_pairs: &reserve_pairs,
        reputation: &reputation,
    };

    // --- package on disk, then OFM's own loader and validator ----------------------------------
    write_package(
        package_dir,
        cfg,
        &teams,
        &plans,
        &free_agent_plans,
        &staff_plans,
        &competitions,
        &members,
        &rule_context,
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

    enforce_squad_roles(&mut world, &plans, &mut journal);
    let loans_applied = apply_loans(&mut world, &plans, cfg, &mut journal);
    let (removed, promoted_academy_players) = if cfg.squad.trim_generated_to_floor {
        trim_generated_players(
            &mut world,
            cfg.squad.promote_academy_before_generating,
            &mut journal,
        )
    } else {
        (0, Vec::new())
    };
    if cfg.squad.calibrate_generated_to_division {
        calibrate_generated_players(&mut world, &clubs, &mut journal);
    }
    apply_free_agent_wages(&mut world, &free_agent_plans);
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
        &free_agent_plans,
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
        free_agent_plans,
        free_agent_stats,
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
        promoted_academy_players,
        loans_applied,
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

/// What the save says a player is on the pitch, converted to OFM: shared by club players and free
/// agents so the two cannot be converted differently.
pub(crate) struct Converted {
    pub natural_code: &'static str,
    pub position: Position,
    pub attributes: domain::player::PlayerAttributes,
    pub ovr: u8,
    pub potential: u8,
    pub foot: domain::player::Footedness,
    pub weak_foot: Option<u8>,
    pub alternates: Vec<Position>,
}

pub(crate) fn convert_player(
    row: &PlayerRow,
    cfg: &Config,
    journal: &mut Journal,
) -> Option<Converted> {
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
    let Some((natural_code, position, attributes, ovr)) = best else {
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
    Some(Converted {
        natural_code,
        position,
        attributes,
        ovr,
        potential,
        foot,
        weak_foot: (!is_keeper).then_some(weak_foot),
        alternates,
    })
}

fn plan_player(
    row: &PlayerRow,
    placement: Placement,
    club: &ClubRow,
    club_country: &str,
    cfg: &Config,
    refs: &Refs,
    journal: &mut Journal,
) -> Option<PlayerPlan> {
    let id = ids::player(row.person_id);
    let Converted {
        natural_code,
        position,
        attributes: converted,
        ovr,
        potential,
        foot,
        weak_foot,
        alternates,
    } = convert_player(row, cfg, journal)?;
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
        youth: placement.role == SquadRole::Youth,
        contract_start: row.start_date.clone(),
        contract_end: Some(contract_end),
        contract_length: None,
        wage: Some(u32::try_from(cfg.to_eur(row.wage_gbp_week).max(0)).unwrap_or(u32::MAX)),
        value,
        weak_foot,
        alternate_positions: alternates,
        condition: None,
        morale: None,
        career_history: Vec::new(),
    };
    Some(PlayerPlan {
        person_id: row.person_id,
        def,
        placement,
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
    free_agent_plans: &[FreeAgentPlan],
    staff_plans: &[(ofm_core::generator::StaffDef, i64)],
    competition_rows: &[crate::source::CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    rule_context: &competitions::RuleContext<'_>,
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

    let competition_defs =
        competitions::build(competition_rows, members, refs, journal, rule_context)?;
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
    if !free_agent_plans.is_empty() {
        let defs: Vec<&PlayerDef> = free_agent_plans.iter().map(|p| &p.def).collect();
        write_items(
            &dir.join("players").join("free_agents.json"),
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
/// first; the rest of the generated players are removed. Imported players are never touched,
/// except that, with `promote_academy`, a club short of the floor first promotes players of its own
/// academy side (imported, never on loan), best rated first, so that a real player is preferred to a
/// generated one. Returns the number of generated players removed and the promoted players.
fn trim_generated_players(
    world: &mut WorldData,
    promote_academy: bool,
    journal: &mut Journal,
) -> (usize, Vec<String>) {
    use ofm_core::squad_floor::{MIN_PLAYERS_PER_GROUP, MIN_SENIOR_PLAYERS};
    let before = world.players.len();
    let team_ids: Vec<String> = world.teams.iter().map(|t| t.id.clone()).collect();
    let mut keep_generated: BTreeSet<String> = BTreeSet::new();
    let mut promoted: Vec<String> = Vec::new();
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
        if promote_academy {
            let mut academy: Vec<&domain::player::Player> = members
                .iter()
                .copied()
                .filter(|p| {
                    is_imported(&p.id)
                        && p.squad_role == SquadRole::Youth
                        && p.active_loan.is_none()
                })
                .collect();
            academy.sort_by(|a, b| b.ovr.cmp(&a.ovr).then(a.id.cmp(&b.id)));
            let mut here: Vec<String> = Vec::new();
            for (index, (group, floor)) in MIN_PLAYERS_PER_GROUP.iter().enumerate() {
                let mut need = floor.saturating_sub(counts.get(&index).copied().unwrap_or(0));
                for p in academy.iter().filter(|p| &group_of(p) == group) {
                    if need == 0 {
                        break;
                    }
                    if here.contains(&p.id) {
                        continue;
                    }
                    here.push(p.id.clone());
                    *counts.entry(index).or_default() += 1;
                    need -= 1;
                }
            }
            let mut extra = MIN_SENIOR_PLAYERS.saturating_sub(counts.values().sum());
            for p in academy.iter() {
                if extra == 0 {
                    break;
                }
                if !here.contains(&p.id) {
                    here.push(p.id.clone());
                    *counts.entry(group_index(&group_of(p))).or_default() += 1;
                    extra -= 1;
                }
            }
            if !here.is_empty() {
                journal.warn(
                    "squad.academy_promoted",
                    team_id,
                    format!(
                        "{} academy player(s) of the save's reserve/youth sides promoted to the senior squad to reach OFM's floor: {}",
                        here.len(),
                        here.join(" ")
                    ),
                );
                promoted.extend(here);
            }
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
    let promoted_set: BTreeSet<&str> = promoted.iter().map(String::as_str).collect();
    for p in world
        .players
        .iter_mut()
        .filter(|p| promoted_set.contains(p.id.as_str()))
    {
        p.squad_role = SquadRole::Senior;
    }
    promoted.sort();
    (before - world.players.len(), promoted)
}

/// OFM's own builder seeds each club's academy with its three youngest outfield players aged 21 or
/// less, whatever team the save registers them with. An imported player's role comes from the save
/// (the team he is registered with, then the academy's age limit): it replaces the builder's pick.
fn enforce_squad_roles(world: &mut WorldData, plans: &[PlayerPlan], journal: &mut Journal) {
    let role_of: BTreeMap<&str, SquadRole> = plans
        .iter()
        .map(|p| (p.def.id.as_str(), p.placement.role))
        .collect();
    let mut changed = 0;
    for player in world.players.iter_mut() {
        if let Some(role) = role_of.get(player.id.as_str())
            && player.squad_role != *role
        {
            player.squad_role = *role;
            changed += 1;
        }
    }
    journal.warn(
        "squad.roles_from_registration",
        "world",
        format!("{changed} imported players had their squad role replaced by the one the save implies (OFM's builder had seeded the academy itself)"),
    );
}

/// A player the save registers with a club other than his owner's plays for that club on loan:
/// `team_id` becomes the club he plays for and `active_loan` records his owner. The save carries no
/// loan terms, so the end (the season's, capped by his contract) and the wage share come from
/// `[loans]` in `adapter.toml`. A borrower the world does not contain keeps only its name.
/// Returns the ids of the players placed on loan.
fn apply_loans(
    world: &mut WorldData,
    plans: &[PlayerPlan],
    cfg: &Config,
    journal: &mut Journal,
) -> Vec<String> {
    if !cfg.loans.enabled {
        return Vec::new();
    }
    let index: BTreeMap<String, usize> = world
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i))
        .collect();
    let mut applied = Vec::new();
    for plan in plans.iter().filter(|p| p.placement.is_loan()) {
        let Some(&i) = index.get(&plan.def.id) else {
            continue;
        };
        let owner = ids::club(plan.placement.owner_club.unwrap_or_default());
        let borrower = ids::club(plan.placement.playing_club);
        let player = &mut world.players[i];
        // The loan ends with the season, or with his contract if that comes first and is later than
        // the start; a contract already over at the start keeps the configured end.
        let end = match player.contract_end() {
            Some(contract_end)
                if contract_end > cfg.start.date.as_str()
                    && contract_end < cfg.loans.end_date.as_str() =>
            {
                contract_end.to_string()
            }
            _ => cfg.loans.end_date.clone(),
        };
        let outside = matches!(
            plan.placement.kind,
            Kind::LoanToOutsideClub | Kind::AffiliateLoan
        );
        player.team_id = Some(borrower.clone());
        player.jersey_number = None;
        player.active_loan = Some(domain::player::ActiveLoan {
            parent_team_id: owner.clone(),
            loan_team_id: borrower,
            loan_team_name: outside
                .then(|| plan.placement.borrower_name.clone())
                .flatten(),
            start_date: cfg.start.date.clone(),
            end_date: end,
            wage_contribution_pct: cfg.loans.wage_contribution_pct,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        });
        let id = plan.def.id.clone();
        if let Some(team) = world.teams.iter_mut().find(|t| t.id == owner) {
            team.remove_player_references(&id);
        }
        applied.push(id);
    }
    journal.warn(
        "loans.applied",
        "world",
        format!(
            "{} players imported as loans (registered with a club other than their owner's); terms are configured, not read from the save",
            applied.len()
        ),
    );
    applied
}

/// The level and pay of OFM's generated fillers, brought down to the division they play in.
///
/// OFM generates fillers on its own scale: about 66 rated and 11 000 a week, in a club whose real
/// players are about 56 rated and paid 700 to 2 500. A National side with ten of them is the best
/// team of its division by ten points (it was promoted twice in a row, to the top flight), and a
/// club whose staff and fillers cost twenty times its wage budget is sacked in nine weeks.
/// A filler above the median rating of the imported senior players of its division is scaled
/// down to that median (all attributes by one factor, found against OFM's own rating), and its
/// wage is capped at the division's median wage; its market value follows the imported players of
/// the division at that rating. A filler already at or under the median is left as OFM made it.
/// Imported players are never touched.
fn calibrate_generated_players(world: &mut WorldData, clubs: &[ClubRow], journal: &mut Journal) {
    let division_of: BTreeMap<String, i64> = clubs
        .iter()
        .map(|c| (ids::club(c.id), c.competition_id))
        .collect();
    let mut references: BTreeMap<i64, Vec<(u8, u32, u64)>> = BTreeMap::new();
    for p in world
        .players
        .iter()
        .filter(|p| is_imported(&p.id) && p.squad_role == SquadRole::Senior)
    {
        if let Some(division) = p.team_id.as_ref().and_then(|t| division_of.get(t)) {
            references
                .entry(*division)
                .or_default()
                .push((p.ovr, p.wage(), p.market_value));
        }
    }
    let median = |mut values: Vec<u64>| -> u64 {
        values.sort_unstable();
        values.get(values.len() / 2).copied().unwrap_or(0)
    };
    let mut changed: BTreeMap<String, usize> = BTreeMap::new();
    for p in world.players.iter_mut().filter(|p| !is_imported(&p.id)) {
        let Some(team) = p.team_id.clone() else {
            continue;
        };
        let Some(reference) = division_of.get(&team).and_then(|d| references.get(d)) else {
            continue;
        };
        if reference.len() < 5 {
            continue;
        }
        let target_ovr = median(reference.iter().map(|r| u64::from(r.0)).collect()) as u8;
        let target_wage = median(reference.iter().map(|r| u64::from(r.1)).collect()) as u32;
        let mut touched = false;
        if p.ovr > target_ovr {
            let position = p.position.clone();
            let original = p.attributes.clone();
            let (mut low, mut high) = (0.2_f64, 1.0_f64);
            for _ in 0..24 {
                let mid = (low + high) / 2.0;
                if f64::from(attributes::ovr(
                    &scale_attributes(&original, mid),
                    &position,
                )) <= f64::from(target_ovr)
                {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            p.attributes = scale_attributes(&original, low);
            let headroom = p.potential.saturating_sub(p.ovr);
            p.ovr = attributes::ovr(&p.attributes, &position);
            p.potential = p.ovr.saturating_add(headroom).min(99);
            let near: Vec<u64> = reference
                .iter()
                .filter(|r| r.0.abs_diff(p.ovr) <= 3)
                .map(|r| r.2)
                .collect();
            let all: Vec<u64> = reference.iter().map(|r| r.2).collect();
            p.market_value = median(if near.is_empty() { all } else { near });
            touched = true;
        }
        if p.wage() > target_wage {
            p.stage_wage(target_wage);
            touched = true;
        }
        if touched {
            *changed.entry(team).or_default() += 1;
        }
    }
    for (team, n) in changed {
        journal.warn(
            "squad.generated_calibrated",
            &team,
            format!("{n} generated player(s) brought down to the level and pay of the division's imported players"),
        );
    }
}

/// Every attribute multiplied by `factor`, kept in 1..=99.
fn scale_attributes(
    a: &domain::player::PlayerAttributes,
    factor: f64,
) -> domain::player::PlayerAttributes {
    let f = |v: u8| ((f64::from(v) * factor).round() as i64).clamp(1, 99) as u8;
    domain::player::PlayerAttributes {
        pace: f(a.pace),
        stamina: f(a.stamina),
        strength: f(a.strength),
        agility: f(a.agility),
        passing: f(a.passing),
        shooting: f(a.shooting),
        tackling: f(a.tackling),
        dribbling: f(a.dribbling),
        defending: f(a.defending),
        positioning: f(a.positioning),
        vision: f(a.vision),
        decisions: f(a.decisions),
        composure: f(a.composure),
        aggression: f(a.aggression),
        teamwork: f(a.teamwork),
        leadership: f(a.leadership),
        handling: f(a.handling),
        reflexes: f(a.reflexes),
        aerial: f(a.aerial),
    }
}

fn group_index(group: &Position) -> usize {
    ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
        .iter()
        .position(|(g, _)| g == group)
        .unwrap_or(0)
}

/// `StaffDef` carries no wage or contract end: set them on the built staff.
/// A free agent's asked wage is staged on him after the world is built: the package builder
/// unattaches an authored player with no club and clears his pay, because a generated free agent
/// asks whatever OFM derives from his value. These ones ask what the save's clubs pay players of
/// their ability and age. They are on no club's bill (`calc_wages` counts a wage only for the club
/// that holds the player); the figure is what a signing club starts negotiating from.
fn apply_free_agent_wages(world: &mut WorldData, plans: &[FreeAgentPlan]) {
    let asked: BTreeMap<String, u32> = plans
        .iter()
        .filter_map(|p| p.asked_wage.map(|w| (p.def.id.clone(), w)))
        .collect();
    for player in world.players.iter_mut().filter(|p| p.team_id.is_none()) {
        if let Some(wage) = asked.get(&player.id) {
            player.stage_wage(*wage);
        }
    }
}

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
    let mut commitments: BTreeMap<String, i64> = BTreeMap::new();
    for p in &world.players {
        if let Some(team) = &p.team_id {
            *commitments.entry(team.clone()).or_default() += i64::from(p.wage());
        }
    }
    for s in &world.staff {
        if let Some(team) = &s.team_id {
            *commitments.entry(team.clone()).or_default() += i64::from(s.wage);
        }
    }
    let mut records = Vec::new();
    for club in clubs {
        let id = ids::club(club.id);
        if let Some(team) = world.teams.iter_mut().find(|t| t.id == id) {
            let bill = bills.get(&id).copied().unwrap_or(0);
            let committed = commitments.get(&id).copied().unwrap_or(0);
            records.extend(finance::restore(team, club, bill, committed, cfg, journal));
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

/// What the mapping says about where a player plays when it is not simply his contract club.
fn placement_note(plan: Option<&PlayerPlan>) -> String {
    let Some(plan) = plan else {
        return String::new();
    };
    let p = &plan.placement;
    match p.kind {
        Kind::Own => format!("{:?}", p.role).to_lowercase(),
        _ => format!(
            "{} ; joue à {} ; propriétaire {} ; {:?}",
            p.kind.label(),
            ids::club(p.playing_club),
            p.owner_club.map(ids::club).unwrap_or_default(),
            p.role
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn mapping_rows(
    players: &[PlayerRow],
    staff_rows: &[StaffRow],
    clubs: &[ClubRow],
    competitions: &[crate::source::CompetitionRow],
    plans: &[PlayerPlan],
    free_agent_plans: &[FreeAgentPlan],
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
            ofm_club_id: ids::club(
                plans
                    .iter()
                    .find(|plan| plan.person_id == p.person_id)
                    .map_or(p.club_id, |plan| plan.placement.playing_club),
            ),
            note: placement_note(plans.iter().find(|plan| plan.person_id == p.person_id)),
        });
    }
    for fa in free_agent_plans {
        rows.push(MappingRow {
            ofm_id: ids::player(fa.person_id),
            entity: "player",
            world_db_id: fa.person_id,
            fm_uid: fa.fm_uid,
            fm_uid_status: fa.fm_uid_status.clone(),
            name: fa.name.clone(),
            ofm_club_id: String::new(),
            note: "agent libre FM24".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{Player, PlayerAttributes};

    fn attributes(level: u8) -> PlayerAttributes {
        scale_attributes(
            &PlayerAttributes {
                pace: 100,
                stamina: 100,
                strength: 100,
                agility: 100,
                passing: 100,
                shooting: 100,
                tackling: 100,
                dribbling: 100,
                defending: 100,
                positioning: 100,
                vision: 100,
                decisions: 100,
                composure: 100,
                aggression: 100,
                teamwork: 100,
                leadership: 100,
                handling: 100,
                reflexes: 100,
                aerial: 100,
            },
            f64::from(level) / 100.0,
        )
    }

    fn player(id: &str, team: &str, level: u8, wage: u32) -> Player {
        let mut p = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "2000-01-01".to_string(),
            "FR".to_string(),
            Position::CentralMidfielder,
            attributes(level),
        );
        p.team_id = Some(team.to_string());
        p.squad_role = SquadRole::Senior;
        p.ovr = attributes::ovr(&p.attributes, &p.position);
        p.potential = p.ovr + 4;
        p.stage_wage(wage);
        p.market_value = 100_000;
        p
    }

    fn club(id: i64, division: i64) -> ClubRow {
        ClubRow {
            id,
            fm_uid: None,
            name: format!("club {id}"),
            short_name: "C".into(),
            nation: None,
            reputation: 100,
            competition_id: division,
            colours: None,
            finance: None,
        }
    }

    /// Given a division whose imported players are rated about 55 and paid about 1 000 a week, and a
    /// club holding two OFM fillers rated about 75 and paid 12 000, plus one at 50 and paid 500,
    /// When the fillers are calibrated,
    /// Then the strong ones come down to the division's median rating and wage, the weak one is
    ///      left alone, imported players are untouched, and another division's reference is not used.
    #[test]
    fn fillers_above_their_division_are_brought_down_to_its_median() {
        let mut world = WorldData::default();
        for i in 0..7 {
            world
                .players
                .push(player(&format!("fm-p-{i}"), "fm-c-1", 55, 1_000));
        }
        world
            .players
            .push(player("filler-strong-a", "fm-c-1", 75, 12_000));
        world
            .players
            .push(player("filler-strong-b", "fm-c-1", 75, 12_000));
        world.players.push(player("filler-weak", "fm-c-1", 50, 500));
        for i in 0..7 {
            world
                .players
                .push(player(&format!("fm-p-top-{i}"), "fm-c-2", 80, 50_000));
        }
        let before_weak = world
            .players
            .iter()
            .find(|p| p.id == "filler-weak")
            .unwrap()
            .clone();
        let snapshot = |w: &WorldData| -> Vec<(String, u8, u32, u64)> {
            w.players
                .iter()
                .filter(|p| is_imported(&p.id))
                .map(|p| (p.id.clone(), p.ovr, p.wage(), p.market_value))
                .collect()
        };
        let imported_before = snapshot(&world);

        calibrate_generated_players(
            &mut world,
            &[club(1, 10), club(2, 20)],
            &mut Journal::default(),
        );

        let median = world.players.iter().find(|p| p.id == "fm-p-0").unwrap().ovr;
        for id in ["filler-strong-a", "filler-strong-b"] {
            let p = world.players.iter().find(|p| p.id == id).unwrap();
            assert!(
                p.ovr <= median && p.ovr + 2 >= median,
                "{id} rated {} against a median of {median}",
                p.ovr
            );
            assert_eq!(p.wage(), 1_000);
            assert!(p.potential >= p.ovr);
        }
        let weak = world
            .players
            .iter()
            .find(|p| p.id == "filler-weak")
            .unwrap();
        assert_eq!(
            (weak.ovr, weak.wage()),
            (before_weak.ovr, before_weak.wage())
        );
        assert_eq!(imported_before, snapshot(&world));
    }
}
