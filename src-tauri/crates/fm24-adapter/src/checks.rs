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
    let european_clubs = run.european.as_ref().map_or(0, |facts| {
        facts.continental_only.len() + facts.generated_clubs.len()
    });
    let domestic_clubs = run
        .clubs
        .iter()
        .filter(|club| club.competition_id != crate::pipeline::CONTINENTAL_ONLY)
        .count();
    checks.push(check(
        "perimeter.clubs",
        true,
        if domestic_clubs == crate::pipeline::EXPECTED_CLUBS
            && world.teams.len() == crate::pipeline::EXPECTED_CLUBS + european_clubs
        {
            vec![]
        } else {
            vec![format!(
                "source {domestic_clubs} domestic clubs / world {} clubs, expected {} + {european_clubs} European-only",
                world.teams.len(),
                crate::pipeline::EXPECTED_CLUBS
            )]
        },
        format!(
            "240 clubs in the source perimeter, {european_clubs} more only for the European competitions, {} in the world",
            world.teams.len()
        ),
    ));
    let comp_failures = {
        let mut failures = Vec::new();
        if run.competitions.len() != crate::pipeline::EXPECTED_COMPETITIONS {
            failures.push(format!("{} competitions", run.competitions.len()));
        }
        let count_of = |kind: domain::league::CompetitionType| {
            world.competition_definitions.as_ref().map_or(0, |d| {
                d.competitions.iter().filter(|c| c.r#type == kind).count()
            })
        };
        let leagues = count_of(domain::league::CompetitionType::League);
        let cups = count_of(domain::league::CompetitionType::Cup);
        let continental = count_of(domain::league::CompetitionType::ContinentalClub);
        if leagues != run.competitions.len() {
            failures.push(format!("world holds {leagues} league definitions"));
        }
        let expected_cups = if run.cfg.competitions.authored_rules {
            crate::pipeline::EXPECTED_CUPS
        } else {
            0
        };
        if cups != expected_cups {
            failures.push(format!("world holds {cups} cup definitions"));
        }
        let expected_continental = if run.european.is_some() {
            crate::europe::COMPETITIONS.len()
        } else {
            0
        };
        if continental != expected_continental {
            failures.push(format!("world holds {continental} European definitions"));
        }
        failures
    };
    checks.push(check(
        "perimeter.competitions",
        true,
        comp_failures,
        "12 league definitions, 6 national cups and the European competitions",
    ));
    if let Some(facts) = &run.european {
        checks.extend(european_checks(run, facts));
    }

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
        for def in defs
            .competitions
            .iter()
            .filter(|def| def.r#type == domain::league::CompetitionType::League)
        {
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
        // Where he plays is where the save registers him (his owner's club when he is not on a
        // loan); a loan carries his owner and ends within his contract.
        let placement = &plan.placement;
        if player.team_id.as_deref() != Some(ids::club(placement.playing_club).as_str()) {
            club_failures.push(format!(
                "{id} club {:?} != fm-c-{} ({})",
                player.team_id,
                placement.playing_club,
                placement.kind.label()
            ));
        }
        if !placement.is_loan()
            && player.team_id.as_deref() != Some(ids::club(row.club_id).as_str())
            && placement.kind == crate::placement::Kind::Own
        {
            club_failures.push(format!(
                "{id} owns a contract at fm-c-{} but plays for {:?}",
                row.club_id, player.team_id
            ));
        }
        match (
            &player.active_loan,
            placement.is_loan() && run.cfg.loans.enabled,
        ) {
            (Some(loan), true) => {
                if Some(loan.parent_team_id.as_str())
                    != placement.owner_club.map(ids::club).as_deref()
                    || Some(loan.loan_team_id.as_str()) != player.team_id.as_deref()
                {
                    club_failures.push(format!("{id} loan has the wrong owner or borrower"));
                }
                if loan.end_date.as_str() <= loan.start_date.as_str() {
                    club_failures.push(format!("{id} loan ends before it starts"));
                }
            }
            (None, true) => club_failures.push(format!("{id} should be on loan")),
            (Some(_), false) => club_failures.push(format!("{id} is on loan but should not be")),
            (None, false) => {}
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
        if Some(player.market_value) != plan.def.value {
            field_failures.push(format!(
                "{id} value {} != planned {:?}",
                player.market_value, plan.def.value
            ));
        }
        // A value labelled as the save's own must be the save's, to the euro.
        if plan.value_source == "save"
            && let Some(value) = row.value_gbp
            && player.market_value as i64 != run.cfg.to_eur(value)
        {
            field_failures.push(format!(
                "{id} value {} is labelled as the save's but the save says {}",
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
        let expected_youth = plan.def.youth && !run.promoted_academy_players.contains(id);
        if expected_youth && player.squad_role != SquadRole::Youth {
            field_failures.push(format!(
                "{id} was registered with a reserve or youth side in the save but is not in the academy"
            ));
        }
        if !plan.def.youth && player.squad_role != SquadRole::Senior {
            field_failures.push(format!(
                "{id} was registered with a first team (or past the academy age) but is not senior"
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
        "every imported player plays for the club the save registers him with, loans carry their owner",
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

    // --- academy and loans: one effective club per player -------------------------------------------
    let mut coherence_failures = Vec::new();
    if let Ok(on) = chrono::NaiveDate::parse_from_str(&run.cfg.start.date, "%Y-%m-%d") {
        for p in world.players.iter().filter(|p| p.id.starts_with("fm-p-")) {
            if p.squad_role == SquadRole::Youth
                && p.team_id.is_some()
                && domain::age::age_on_or_unknown(on, &p.date_of_birth)
                    > ofm_core::roster::YOUTH_ACADEMY_MAX_AGE
            {
                coherence_failures.push(format!(
                    "{} is past the academy age but in the academy",
                    p.id
                ));
            }
            let Some(loan) = &p.active_loan else { continue };
            if p.team_id.as_deref() != Some(loan.loan_team_id.as_str()) {
                coherence_failures.push(format!(
                    "{} is on loan to {} but plays for {:?}",
                    p.id, loan.loan_team_id, p.team_id
                ));
            }
            if !world.teams.iter().any(|t| t.id == loan.loan_team_id)
                && loan.loan_team_name.is_none()
            {
                coherence_failures.push(format!(
                    "{} is on loan to {} which is neither a team of the world nor named",
                    p.id, loan.loan_team_id
                ));
            }
            if loan.parent_team_id == loan.loan_team_id {
                coherence_failures.push(format!("{} is on loan to his own club", p.id));
            }
            match world.teams.iter().find(|t| t.id == loan.parent_team_id) {
                None => coherence_failures.push(format!(
                    "{} has an owner ({}) that is not in the world",
                    p.id, loan.parent_team_id
                )),
                Some(owner) => {
                    let still_listed = owner.starting_xi_ids.contains(&p.id)
                        || owner.player_roles.contains_key(&p.id)
                        || owner
                            .training_groups
                            .iter()
                            .any(|g| g.player_ids.contains(&p.id));
                    if still_listed {
                        coherence_failures.push(format!(
                            "{} is on loan but still in his owner's lineup, roles or training",
                            p.id
                        ));
                    }
                }
            }
        }
    }
    checks.push(check(
        "squads.academy_and_loans_coherent",
        true,
        coherence_failures,
        "no imported academy player is past the academy age; a loaned player plays for exactly one club (the borrower) and is in none of his owner's lineups",
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

    // --- generic roster audit, shared with the generated-world tests -------------------------------
    // Not blocking: it also applies a heuristic ceiling (a bloated roster) and a few cross-checks the
    // blocking checks above do not cover (identity clashes, loan records, lineups). It tells the
    // reader what to look at; it never decides that the world is unusable.
    let audit_failures = match chrono::NaiveDate::parse_from_str(&run.cfg.start.date, "%Y-%m-%d") {
        Ok(on) => {
            let report = ofm_core::world_integrity::audit_players(
                &world.teams,
                &world.players,
                &ofm_core::world_integrity::IntegrityLimits::new(on),
            );
            report.findings.iter().map(ToString::to_string).collect()
        }
        Err(error) => vec![format!("start date {:?}: {error}", run.cfg.start.date)],
    };
    checks.push(check(
        "integrity.world_audit",
        false,
        audit_failures,
        "no duplicate identity, impossible rating, unknown club, bad loan record or oversized roster",
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

type Teams<'a> = BTreeMap<&'a str, &'a domain::team::Team>;
type Definitions<'a> = Vec<&'a ofm_core::generator::CompetitionDefinition>;

/// The explicit participants a competition definition lists.
fn explicit_of(def: &ofm_core::generator::CompetitionDefinition) -> Vec<String> {
    serde_json::to_value(def)
        .ok()
        .and_then(|value| value["participants"]["explicit"].as_array().cloned())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The European competitions against the mapping and the teams actually in the world: three
/// disjoint fields of 36 whose every club exists, carries the source's identity and name, plays in
/// no domestic league and can field a squad, and whose competitions carry their berths.
fn european_checks(run: &Run, facts: &crate::pipeline::EuropeanFacts) -> Vec<Check> {
    let world = &run.world;
    let teams: Teams = world
        .teams
        .iter()
        .map(|team| (team.id.as_str(), team))
        .collect();
    let definitions: Definitions = world
        .competition_definitions
        .iter()
        .flat_map(|d| d.competitions.iter())
        .collect();
    let mut checks = field_checks(facts, &teams, &definitions);
    checks.extend(identity_checks(facts, &teams, &definitions));
    checks.extend(berth_and_squad_checks(run, facts, &definitions));
    checks
}

fn field_checks(
    facts: &crate::pipeline::EuropeanFacts,
    teams: &Teams,
    definitions: &Definitions,
) -> Vec<Check> {
    let mut field_failures = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (key, short, _) in crate::europe::COMPETITIONS {
        let id = ids::european_competition(short);
        let Some(def) = definitions.iter().find(|d| d.id == id) else {
            field_failures.push(format!("{id} is not in the world"));
            continue;
        };
        let field = explicit_of(def);
        let expected: Vec<String> = facts.fields.field_of(key).map(|p| p.club_id()).collect();
        if field != expected {
            field_failures.push(format!("{id}: field differs from the mapping"));
        }
        if field.len() != crate::europe::FIELD_SIZE {
            field_failures.push(format!("{id}: {} clubs", field.len()));
        }
        for club in &field {
            if !teams.contains_key(club.as_str()) {
                field_failures.push(format!("{id}: {club} is not a team of the world"));
            }
            if !seen.insert(club.clone()) {
                field_failures.push(format!("{club} is in two European fields"));
            }
        }
    }
    vec![check(
        "europe.fields",
        true,
        field_failures,
        "three disjoint fields of 36, every club a team of the world",
    )]
}

fn identity_checks(
    facts: &crate::pipeline::EuropeanFacts,
    teams: &Teams,
    definitions: &Definitions,
) -> Vec<Check> {
    let mut out = Vec::new();
    let mut identity_failures = Vec::new();
    for participant in &facts.fields.participants {
        let Some(team) = teams.get(participant.club_id().as_str()) else {
            continue;
        };
        if team.country != participant.association {
            identity_failures.push(format!(
                "{}: country {} but association {}",
                participant.uefa_name, team.country, participant.association
            ));
        }
        match participant.fm_club_id {
            Some(_) if team.name != participant.fm_name => identity_failures.push(format!(
                "{}: world team `{}` but the mapping names `{}`",
                participant.uefa_name, team.name, participant.fm_name
            )),
            None if team.name != participant.uefa_name => identity_failures.push(format!(
                "{}: generated team carries `{}`",
                participant.uefa_name, team.name
            )),
            _ => {}
        }
    }
    out.push(check(
        "europe.identity",
        true,
        identity_failures,
        "every participant is the source club the mapping names (or a generated club under its UEFA name)",
    ));

    let in_domestic_league: BTreeSet<String> = definitions
        .iter()
        .filter(|d| d.r#type == domain::league::CompetitionType::League)
        .flat_map(|d| explicit_of(d))
        .collect();
    let only = european_only(facts);
    out.push(check(
        "europe.no_domestic_league",
        true,
        only.iter()
            .filter(|club| in_domestic_league.contains(*club))
            .map(|club| format!("{club} is in a domestic league"))
            .collect(),
        format!(
            "{} clubs exist only for the European competitions: none is in a domestic league",
            only.len()
        ),
    ));
    out
}

/// The clubs that exist only for the European competitions: the continental-only source clubs and
/// the generated ones.
fn european_only(facts: &crate::pipeline::EuropeanFacts) -> Vec<String> {
    facts
        .continental_only
        .iter()
        .map(|id| ids::club(*id))
        .chain(facts.generated_clubs.iter().cloned())
        .collect()
}

fn berth_and_squad_checks(
    run: &Run,
    facts: &crate::pipeline::EuropeanFacts,
    definitions: &Definitions,
) -> Vec<Check> {
    let world = &run.world;
    let mut checks = Vec::new();
    // The qualification rules of later seasons need their berths in the shipped world: each
    // European competition keeps its unsimulated clubs and sends its winner up, and the first
    // divisions award the places.
    let mut berth_failures = Vec::new();
    let berths_of = |id: &str| -> Vec<(String, domain::league::BerthRule)> {
        definitions
            .iter()
            .find(|d| d.id == id)
            .map(|d| {
                d.berths
                    .iter()
                    .map(|b| (b.target.clone(), b.rule.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    for (_, short, _) in crate::europe::COMPETITIONS {
        let id = ids::european_competition(short);
        let berths = berths_of(&id);
        if !berths.contains(&(id.clone(), domain::league::BerthRule::Incumbent)) {
            berth_failures.push(format!("{id} has no incumbent berth"));
        }
        if !berths
            .iter()
            .any(|(_, rule)| *rule == domain::league::BerthRule::Titleholder)
        {
            berth_failures.push(format!("{id} has no titleholder berth"));
        }
    }
    let fed_leagues = definitions
        .iter()
        .filter(|d| {
            d.berths
                .iter()
                .any(|b| b.target == ids::european_competition("ucl"))
                && d.r#type == domain::league::CompetitionType::League
        })
        .count();
    if fed_leagues != crate::europe::ACCESS.len() {
        berth_failures.push(format!(
            "{fed_leagues} leagues feed the Champions League, expected {}",
            crate::europe::ACCESS.len()
        ));
    }
    checks.push(check(
        "europe.berths",
        true,
        berth_failures,
        "the European competitions carry their incumbent and titleholder berths and five first divisions feed them",
    ));

    let mut unmanned = Vec::new();
    for club in &european_only(facts) {
        let players = world
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(club.as_str()))
            .count();
        if players < ofm_core::squad_floor::MIN_SENIOR_PLAYERS {
            unmanned.push(format!("{club} has {players} players"));
        }
    }
    checks.push(check(
        "europe.playable_squads",
        true,
        unmanned,
        "every European-only club has a squad at OFM's floor, so every European match can be played",
    ));
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
