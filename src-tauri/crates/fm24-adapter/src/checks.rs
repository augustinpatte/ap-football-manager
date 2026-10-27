//! Integrity checks of a finished run, read against the **source rows** (not against the plans
//! alone), so an error in the conversion cannot hide behind itself.
//!
//! A blocking check that fails makes the CLI refuse to write the world.

use crate::ids;
use crate::pipeline::Run;
use domain::player::SquadRole;
use ofm_core::nations::nation_by_code;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub blocking: bool,
    pub ok: bool,
    pub detail: String,
}

fn check(
    name: &'static str,
    blocking: bool,
    failures: Vec<String>,
    ok_detail: impl Into<String>,
) -> Check {
    let ok = failures.is_empty();
    let detail = if ok {
        ok_detail.into()
    } else {
        let shown: Vec<_> = failures.iter().take(5).cloned().collect();
        format!("{} failure(s), e.g. {}", failures.len(), shown.join(" ; "))
    };
    Check {
        name,
        blocking,
        ok,
        detail,
    }
}

pub fn run_checks(run: &Run) -> Vec<Check> {
    let mut checks = Vec::new();
    let world = &run.world;

    // --- source and perimeter ----------------------------------------------------------------
    checks.push(check(
        "source.untouched",
        true,
        if run.source_sha256_before == run.source_sha256_after {
            vec![]
        } else {
            vec![format!(
                "{} -> {}",
                run.source_sha256_before, run.source_sha256_after
            )]
        },
        format!(
            "world.db SHA-256 unchanged ({})",
            &run.source_sha256_before[..16]
        ),
    ));
    checks.push(check(
        "perimeter.clubs",
        true,
        if run.clubs.len() == crate::pipeline::EXPECTED_CLUBS
            && world.teams.len() == crate::pipeline::EXPECTED_CLUBS
        {
            vec![]
        } else {
            vec![format!(
                "source {} / world {} clubs, expected {}",
                run.clubs.len(),
                world.teams.len(),
                crate::pipeline::EXPECTED_CLUBS
            )]
        },
        "240 clubs in the source perimeter and in the world",
    ));
    let comp_failures = {
        let mut failures = Vec::new();
        if run.competitions.len() != crate::pipeline::EXPECTED_COMPETITIONS {
            failures.push(format!("{} competitions", run.competitions.len()));
        }
        let defs = world
            .competition_definitions
            .as_ref()
            .map(|d| d.competitions.len())
            .unwrap_or(0);
        if defs != run.competitions.len() {
            failures.push(format!("world holds {defs} competition definitions"));
        }
        failures
    };
    checks.push(check(
        "perimeter.competitions",
        true,
        comp_failures,
        "12 league definitions",
    ));

    // Club membership of every league equals the save's.
    let mut membership_failures = Vec::new();
    if let Some(defs) = &world.competition_definitions {
        let from_source: BTreeMap<String, BTreeSet<String>> =
            run.clubs.iter().fold(BTreeMap::new(), |mut acc, c| {
                acc.entry(ids::competition(c.competition_id))
                    .or_default()
                    .insert(ids::club(c.id));
                acc
            });
        for def in &defs.competitions {
            let value = serde_json::to_value(def).unwrap_or_default();
            let participants: BTreeSet<String> = value["participants"]["explicit"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if from_source.get(&def.id) != Some(&participants) {
                membership_failures.push(format!("{} membership differs from the save", def.id));
            }
        }
    }
    checks.push(check(
        "competition.membership",
        true,
        membership_failures,
        "league members equal the save's 2026/27 membership",
    ));

    // --- identifiers -------------------------------------------------------------------------
    let mut seen = BTreeSet::new();
    let mut duplicate_ids = Vec::new();
    for id in world
        .players
        .iter()
        .map(|p| &p.id)
        .chain(world.staff.iter().map(|s| &s.id))
        .chain(world.teams.iter().map(|t| &t.id))
        .chain(world.managers.iter().map(|m| &m.id))
    {
        if !seen.insert(id.clone()) {
            duplicate_ids.push(id.clone());
        }
    }
    checks.push(check(
        "ids.unique",
        true,
        duplicate_ids,
        format!("{} ids, all distinct", seen.len()),
    ));

    let free_agent_keys: BTreeSet<i64> = run.free_agent_plans.iter().map(|p| p.person_id).collect();
    let player_keys: BTreeSet<i64> = run
        .players
        .iter()
        .map(|p| p.person_id)
        .chain(free_agent_keys.iter().copied())
        .collect();
    let mut key_failures = Vec::new();
    for p in world.players.iter().filter(|p| p.id.starts_with("fm-p-")) {
        match ids::internal_key(&p.id, "fm-p-") {
            Some(key) if player_keys.contains(&key) => {}
            _ => key_failures.push(format!("{} is not an internal person key", p.id)),
        }
    }
    checks.push(check(
        "ids.internal_keys_only",
        true,
        key_failures,
        "every fm-p-* id is a world.db person.id",
    ));

    let shared: Vec<_> = run
        .players
        .iter()
        .filter(|p| p.fm_uid_status == "partagé")
        .collect();
    let shared_failures: Vec<String> = shared
        .iter()
        .filter(|p| {
            !world
                .players
                .iter()
                .any(|w| w.id == ids::player(p.person_id))
        })
        .map(|p| format!("{} (shared FM UID) missing", ids::player(p.person_id)))
        .collect();
    checks.push(check(
        "ids.shared_fm_uid_kept_distinct",
        true,
        shared_failures,
        format!(
            "{} players with a shared FM UID are all present as distinct players",
            shared.len()
        ),
    ));

    // --- no silent drop ------------------------------------------------------------------------
    let excluded_players = run.journal.count("player.excluded_no_position");
    let imported_in_world = world
        .players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-"))
        .filter(|p| {
            ids::internal_key(&p.id, "fm-p-").is_none_or(|key| !free_agent_keys.contains(&key))
        })
        .count();
    checks.push(check(
        "players.none_dropped_silently",
        true,
        if imported_in_world + excluded_players == run.players.len() {
            vec![]
        } else {
            vec![format!(
                "source {} = world {imported_in_world} + journaled exclusions {excluded_players}?",
                run.players.len()
            )]
        },
        format!("{} source players = {imported_in_world} imported + {excluded_players} excluded and journaled", run.players.len()),
    ));
    let (total, distinct) = run.total_players_in_source;
    checks.push(check(
        "source.players_distinct",
        false,
        if total == distinct {
            vec![]
        } else {
            vec![format!("{total} rows, {distinct} distinct person ids")]
        },
        format!("{total} source players, all distinct by person.id"),
    ));

    // --- players, field by field -----------------------------------------------------------------
    let world_players: BTreeMap<&str, &domain::player::Player> =
        world.players.iter().map(|p| (p.id.as_str(), p)).collect();
    let row_by_id: BTreeMap<i64, &crate::source::PlayerRow> =
        run.players.iter().map(|p| (p.person_id, p)).collect();
    let mut field_failures = Vec::new();
    let mut club_failures = Vec::new();
    let mut position_failures = Vec::new();
    let mut nation_failures = Vec::new();
    let mut contract_failures = Vec::new();
    let mut bound_failures = Vec::new();
    for plan in &run.plans {
        let id = &plan.def.id;
        let Some(player) = world_players.get(id.as_str()) else {
            field_failures.push(format!("{id} absent from the world"));
            continue;
        };
        let row = row_by_id[&plan.person_id];
        // Independent of the plan: straight from the source row.
        if let Some(dob) = &row.birth_date
            && &player.date_of_birth != dob
        {
            field_failures.push(format!("{id} dob {} != {dob}", player.date_of_birth));
        }
        if player.team_id.as_deref() != Some(ids::club(row.club_id).as_str()) {
            club_failures.push(format!(
                "{id} club {:?} != fm-c-{}",
                player.team_id, row.club_id
            ));
        }
        if player.position.is_legacy_bucket()
            && player.position != domain::player::Position::Goalkeeper
        {
            position_failures.push(format!("{id} has a general position {:?}", player.position));
        }
        if player.natural_position != plan.def.position {
            position_failures.push(format!(
                "{id} natural {:?} != planned {:?}",
                player.natural_position, plan.def.position
            ));
        }
        if player.position != plan.def.position {
            position_failures.push(format!(
                "{id} position {:?} != planned {:?}",
                player.position, plan.def.position
            ));
        }
        if player.attributes != plan.def.attributes.clone().unwrap() {
            field_failures.push(format!("{id} attributes differ from the conversion"));
        }
        if i64::from(player.wage()) != crate::config::Config::to_eur(&run.cfg, row.wage_gbp_week) {
            field_failures.push(format!(
                "{id} wage {} != {}",
                player.wage(),
                run.cfg.to_eur(row.wage_gbp_week)
            ));
        }
        if let Some(value) = row.value_gbp
            && player.market_value as i64 != run.cfg.to_eur(value)
        {
            field_failures.push(format!(
                "{id} value {} != {}",
                player.market_value,
                run.cfg.to_eur(value)
            ));
        }
        if player.contract_start() != row.start_date.as_deref() {
            contract_failures.push(format!(
                "{id} contract start {:?} != {:?}",
                player.contract_start(),
                row.start_date
            ));
        }
        if player.contract_end() != plan.def.contract_end.as_deref() {
            contract_failures.push(format!(
                "{id} contract end {:?} != {:?}",
                player.contract_end(),
                plan.def.contract_end
            ));
        }
        if let (Some(start), Some(end)) = (player.contract_start(), player.contract_end())
            && end <= start
        {
            contract_failures.push(format!(
                "{id} contract ends {end} on/before it starts {start}"
            ));
        }
        if let Some(end) = player.contract_end()
            && end < run.cfg.start.date.as_str()
        {
            contract_failures.push(format!(
                "{id} contract already expired at the configured start ({end})"
            ));
        }
        // Nationality: the player's own code, or OFM's football identity for it (e.g. WAL).
        let expected = &plan.def.nationality;
        if &player.nationality != expected && &player.football_nation != expected {
            nation_failures.push(format!(
                "{id} nationality {} / {} != {expected}",
                player.nationality, player.football_nation
            ));
        }
        if nation_by_code(&player.nationality).is_none() {
            nation_failures.push(format!(
                "{id} nationality {} is not an OFM code",
                player.nationality
            ));
        }
        let a = &player.attributes;
        let all = [
            a.pace,
            a.stamina,
            a.strength,
            a.agility,
            a.passing,
            a.shooting,
            a.tackling,
            a.dribbling,
            a.defending,
            a.positioning,
            a.vision,
            a.decisions,
            a.composure,
            a.aggression,
            a.teamwork,
            a.leadership,
            a.handling,
            a.reflexes,
            a.aerial,
        ];
        if all.iter().any(|v| !(1..=99).contains(v))
            || !(1..=99).contains(&player.ovr)
            || player.potential < player.ovr
        {
            bound_failures.push(format!("{id} attribute/ovr/potential out of bounds"));
        }
        if player.ovr != plan.ovr_expected {
            bound_failures.push(format!(
                "{id} ovr {} != expected {}",
                player.ovr, plan.ovr_expected
            ));
        }
        if player.wage() > 5_000_000 || player.market_value > 1_000_000_000 {
            bound_failures.push(format!(
                "{id} wage/value implausible ({}, {})",
                player.wage(),
                player.market_value
            ));
        }
        let expected_youth = plan.def.youth;
        if expected_youth && player.squad_role != SquadRole::Youth {
            field_failures.push(format!(
                "{id} was an FM youth-team player but is not in the academy"
            ));
        }
    }
    checks.push(check(
        "players.fields",
        true,
        field_failures,
        format!(
            "{} players: dob, attributes, wage, value, youth",
            run.plans.len()
        ),
    ));
    checks.push(check(
        "players.club_relations",
        true,
        club_failures,
        "every imported player is at the club of his current FM contract",
    ));
    checks.push(check(
        "players.positions_valid",
        true,
        position_failures,
        "every position is a specific OFM position equal to the plan",
    ));
    checks.push(check(
        "players.nationalities_valid",
        true,
        nation_failures,
        "every nationality is an OFM country code, equal to the plan",
    ));
    checks.push(check(
        "players.contracts",
        true,
        contract_failures,
        "contract dates preserved, end after start, none expired at the configured start",
    ));
    checks.push(check(
        "players.bounds",
        true,
        bound_failures,
        "attributes 1-99, potential >= rating, wage/value plausible",
    ));

    // --- squads ----------------------------------------------------------------------------------
    let mut squad_failures = Vec::new();
    let mut by_team: BTreeMap<&str, Vec<&domain::player::Player>> = BTreeMap::new();
    for p in &world.players {
        match &p.team_id {
            Some(team) => by_team.entry(team.as_str()).or_default().push(p),
            None => {
                let is_a_pool_free_agent = ids::internal_key(&p.id, "fm-p-")
                    .is_some_and(|key| free_agent_keys.contains(&key));
                if !is_a_pool_free_agent {
                    squad_failures.push(format!("{} has no club", p.id));
                }
            }
        }
    }
    for team in &world.teams {
        let members = by_team
            .get(team.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if members.is_empty() {
            squad_failures.push(format!("{} has an empty squad", team.id));
            continue;
        }
        let seniors = members
            .iter()
            .filter(|p| p.squad_role == SquadRole::Senior)
            .count();
        if seniors < ofm_core::squad_floor::MIN_SENIOR_PLAYERS {
            squad_failures.push(format!("{} has only {seniors} seniors", team.id));
        }
        for (group, floor) in ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP {
            let n = members
                .iter()
                .filter(|p| {
                    p.squad_role == SquadRole::Senior && p.position.to_group_position() == group
                })
                .count();
            if n < floor {
                squad_failures.push(format!("{} has {n} {group:?} (floor {floor})", team.id));
            }
        }
    }
    checks.push(check(
        "squads.floor_and_not_empty",
        true,
        squad_failures,
        "every club has >= 15 seniors and OFM's per-group floor",
    ));

    let generated_marked: Vec<String> = world
        .players
        .iter()
        .filter(|p| !p.id.starts_with("fm-p-"))
        .filter(|p| !run.generated_players.iter().any(|g| g.id == p.id))
        .map(|p| p.id.clone())
        .collect();
    checks.push(check(
        "squads.generated_players_all_listed",
        true,
        generated_marked,
        format!(
            "{} generated players, each identified in the report",
            run.generated_players.len()
        ),
    ));

    // --- free agents ---------------------------------------------------------------------------
    let mut fa_failures = Vec::new();
    let club_person_keys: BTreeSet<i64> = run.players.iter().map(|p| p.person_id).collect();
    for plan in &run.free_agent_plans {
        let id = &plan.def.id;
        if club_person_keys.contains(&plan.person_id) {
            fa_failures.push(format!("{id} is also a contracted player"));
            continue;
        }
        let Some(player) = world_players.get(id.as_str()) else {
            fa_failures.push(format!("{id} absent from the world"));
            continue;
        };
        if player.team_id.is_some() {
            fa_failures.push(format!("{id} is attached to a club"));
        }
        if player.contract_end().is_some() || player.contract_start().is_some() {
            fa_failures.push(format!("{id} carries a contract"));
        }
        if player.retired {
            fa_failures.push(format!("{id} is retired"));
        }
        if player.ovr != plan.ovr_expected {
            fa_failures.push(format!("{id} ovr {} != {}", player.ovr, plan.ovr_expected));
        }
        if player.market_value == 0 {
            fa_failures.push(format!("{id} has no market value"));
        }
        if let Some(asked) = plan.asked_wage
            && player.wage() != asked
        {
            fa_failures.push(format!("{id} asks {} instead of {asked}", player.wage()));
        }
    }
    let pool_in_world = world
        .players
        .iter()
        .filter(|p| {
            p.team_id.is_none()
                && ids::internal_key(&p.id, "fm-p-")
                    .is_some_and(|key| free_agent_keys.contains(&key))
        })
        .count();
    if pool_in_world != run.free_agent_stats.chosen {
        fa_failures.push(format!(
            "{} chosen but {pool_in_world} in the world",
            run.free_agent_stats.chosen
        ));
    }
    let stats = &run.free_agent_stats;
    checks.push(check(
        "free_agents.pool",
        true,
        fa_failures,
        format!(
            "{} of {} unattached players enter as free agents ({} eligible; left in world.db: {} outside the age range, {} below the level, {} without a known nation, {} without birth date, {} without a position)",
            stats.chosen,
            stats.candidates,
            stats.eligible,
            stats.outside_age_range,
            stats.below_level,
            stats.no_known_nation,
            stats.no_birth_date,
            stats.no_position
        ),
    ));

    // --- clubs -------------------------------------------------------------------------------------
    let mut club_field_failures = Vec::new();
    let team_by_id: BTreeMap<&str, &domain::team::Team> =
        world.teams.iter().map(|t| (t.id.as_str(), t)).collect();
    for club in &run.clubs {
        let id = ids::club(club.id);
        let Some(team) = team_by_id.get(id.as_str()) else {
            club_field_failures.push(format!("{id} missing"));
            continue;
        };
        if team.name != club.name {
            club_field_failures.push(format!("{id} name {} != {}", team.name, club.name));
        }
        if i64::from(team.reputation) != (club.reputation / 10).clamp(0, 1000) {
            club_field_failures.push(format!(
                "{id} reputation {} != {}",
                team.reputation,
                club.reputation / 10
            ));
        }
        if let Some(colours) = &club.colours
            && !team
                .colors
                .primary
                .eq_ignore_ascii_case(&format!("#{}", colours.primary))
        {
            club_field_failures.push(format!(
                "{id} primary colour {} != #{}",
                team.colors.primary, colours.primary
            ));
        }
        if let Some(f) = &club.finance {
            if run.cfg.finance.restore_balance && team.finance != run.cfg.to_eur(f.balance_gbp) {
                club_field_failures.push(format!(
                    "{id} balance {} != {}",
                    team.finance,
                    run.cfg.to_eur(f.balance_gbp)
                ));
            }
            if run.cfg.finance.restore_transfer_budget
                && team.transfer_budget != run.cfg.to_eur(f.transfer_budget_gbp)
            {
                club_field_failures.push(format!(
                    "{id} transfer budget {} != {}",
                    team.transfer_budget,
                    run.cfg.to_eur(f.transfer_budget_gbp)
                ));
            }
            let source_budget = run.cfg.to_eur(f.wage_budget_gbp_week);
            // The last record is the budget the club ends with: a budget raised to the day-one
            // commitments is documented as such, and is no longer the source's value.
            let restored = run
                .finance
                .iter()
                .rfind(|r| r.club_id == club.id && r.field == "wage_budget")
                .is_some_and(|r| r.provenance == crate::finance::Provenance::Imported);
            if restored && team.wage_budget != source_budget {
                club_field_failures.push(format!(
                    "{id} wage budget {} != {source_budget}",
                    team.wage_budget
                ));
            }
            if !restored && team.wage_budget <= 0 {
                club_field_failures.push(format!("{id} has no usable wage budget"));
            }
        }
    }
    checks.push(check("clubs.fields", true, club_field_failures, "name, reputation, colours, balance and budgets match the source (or are documented OFM values)"));

    // --- staff and managers ------------------------------------------------------------------------
    let world_staff: BTreeMap<&str, &domain::staff::Staff> =
        world.staff.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut staff_failures = Vec::new();
    for (def, person) in &run.staff_plans {
        match world_staff.get(def.id.as_str()) {
            None => staff_failures.push(format!("{} absent from the world", def.id)),
            Some(member) => {
                if member.team_id.as_deref() != Some(def.club.as_str()) {
                    staff_failures.push(format!(
                        "{} club {:?} != {}",
                        def.id, member.team_id, def.club
                    ));
                }
                if member.role != def.role {
                    staff_failures.push(format!(
                        "{} role {:?} != {:?}",
                        def.id, member.role, def.role
                    ));
                }
                if let Some(row) = run.staff_rows.iter().find(|r| r.person_id == *person)
                    && i64::from(member.wage) != run.cfg.to_eur(row.wage_gbp_week)
                {
                    staff_failures.push(format!(
                        "{} wage {} != {}",
                        def.id,
                        member.wage,
                        run.cfg.to_eur(row.wage_gbp_week)
                    ));
                }
            }
        }
    }
    checks.push(check(
        "staff.fields",
        true,
        staff_failures,
        format!(
            "{} imported staff members present with role, club and wage",
            run.staff_plans.len()
        ),
    ));

    let mut manager_failures = Vec::new();
    for manager in &world.managers {
        let team = manager.team_id.as_deref().and_then(|t| team_by_id.get(t));
        match team {
            Some(t) if t.manager_id.as_deref() == Some(manager.id.as_str()) => {}
            _ => manager_failures.push(format!("{} is not wired to a club", manager.id)),
        }
    }
    if world.managers.len() + run.manager_clubs_missing.len() != run.clubs.len() {
        manager_failures.push(format!(
            "{} managers + {} clubs without a head coach != {} clubs",
            world.managers.len(),
            run.manager_clubs_missing.len(),
            run.clubs.len()
        ));
    }
    checks.push(check(
        "managers.wired",
        true,
        manager_failures,
        format!(
            "{} real head coaches are the clubs' managers",
            world.managers.len()
        ),
    ));

    // --- informational -------------------------------------------------------------------------------
    let expired_at_start = world
        .players
        .iter()
        .filter(|p| {
            p.contract_end()
                .is_some_and(|e| e < run.cfg.start.date.as_str())
        })
        .count();
    checks.push(check(
        "contracts.expired_at_start",
        false,
        if expired_at_start == 0 {
            vec![]
        } else {
            vec![format!("{expired_at_start}")]
        },
        "no contract expired at the configured start date",
    ));
    let future_start = world
        .players
        .iter()
        .filter(|p| {
            p.contract_start()
                .is_some_and(|s| s > run.cfg.start.date.as_str())
        })
        .count();
    checks.push(Check {
        name: "contracts.future_start",
        blocking: false,
        ok: true,
        detail: format!(
            "{future_start} contracts start after the configured start date (kept as in FM)"
        ),
    });
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_with_no_failure_is_ok_and_reports_its_detail() {
        let c = check("x", true, vec![], "fine");
        assert!(c.ok);
        assert_eq!(c.detail, "fine");
    }

    #[test]
    fn a_check_with_failures_lists_at_most_five() {
        let failures: Vec<String> = (0..9).map(|i| format!("f{i}")).collect();
        let c = check("x", true, failures, "fine");
        assert!(!c.ok);
        assert!(c.detail.starts_with("9 failure(s)"));
        assert!(!c.detail.contains("f5"));
    }
}
