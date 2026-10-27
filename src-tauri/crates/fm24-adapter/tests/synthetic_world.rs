//! End-to-end test on a **synthetic** world.db (no FM data): the schema subset the adapter reads,
//! filled with invented players at real club ids (the city table is keyed by them).
//!
//! It checks what the real run also relies on: ids, positions, goalkeepers, attribute hierarchy,
//! squads below 22 (generated players identified), finance restoration, managers, determinism,
//! and that the source file is never modified.

use fm24_adapter::{Config, checks, pipeline, report};
use rusqlite::{Connection, params};
use std::path::Path;

const SCHEMA: &str = "
CREATE TABLE import_run (id INTEGER PRIMARY KEY, source_file TEXT, source_sha256 TEXT, game_version TEXT,
  game_date TEXT, start_season INTEGER, importer_version TEXT, imported_at TEXT);
CREATE TABLE nation (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE competition (id INTEGER PRIMARY KEY, fm_uid INTEGER, name TEXT, nation_id INTEGER, kind TEXT, playable INTEGER, simulated INTEGER);
CREATE TABLE competition_target (competition_key TEXT PRIMARY KEY, level INTEGER, fm_uid TEXT);
CREATE TABLE competition_rule (competition_key TEXT, rule_key TEXT, value TEXT, status TEXT);
CREATE TABLE competition_season (id INTEGER PRIMARY KEY, competition_id INTEGER, season INTEGER);
CREATE TABLE competition_season_team (competition_season_id INTEGER, team_id INTEGER);
CREATE TABLE club (id INTEGER PRIMARY KEY, fm_uid INTEGER, name TEXT, short_name TEXT, nation_id INTEGER, reputation INTEGER);
CREATE TABLE club_colours (club_id INTEGER PRIMARY KEY, primary_rgb TEXT, secondary_rgb TEXT, source TEXT);
CREATE TABLE club_finance_start (club_id INTEGER PRIMARY KEY, tv_gbp INTEGER, other_revenue_gbp INTEGER, wage_bill_gbp_week INTEGER,
  other_costs_gbp INTEGER, wage_budget_gbp_week INTEGER, transfer_budget_gbp INTEGER, balance_gbp INTEGER, source TEXT);
CREATE TABLE team (id INTEGER PRIMARY KEY, club_id INTEGER, slot INTEGER, is_youth INTEGER);
CREATE TABLE person (id INTEGER PRIMARY KEY, fm_uid INTEGER, first_name TEXT, last_name TEXT, common_name TEXT,
  birth_date TEXT, nation_id INTEGER, height_cm INTEGER, fm_uid_status TEXT);
CREATE TABLE player (person_id INTEGER PRIMARY KEY, ca INTEGER, pa INTEGER, rep_current INTEGER, rep_home INTEGER, rep_world INTEGER,
  foot_left INTEGER, foot_right INTEGER, value_gbp INTEGER, value_source TEXT);
CREATE TABLE player_attributes (person_id INTEGER PRIMARY KEY, %ATTRS%);
CREATE TABLE player_position (person_id INTEGER, position TEXT, rating INTEGER);
CREATE TABLE staff (person_id INTEGER PRIMARY KEY, ca INTEGER, pa INTEGER, rep_current INTEGER);
CREATE TABLE staff_attribute (person_id INTEGER, attr_key TEXT, value_raw INTEGER);
CREATE TABLE contract (id INTEGER PRIMARY KEY, person_id INTEGER, club_id INTEGER, team_id INTEGER, is_current INTEGER,
  role_code TEXT, wage_gbp_week INTEGER, wage_original INTEGER, wage_source TEXT, start_date TEXT, end_date TEXT,
  squad_status_code INTEGER, contract_type_code INTEGER);
";

fn attribute_columns() -> String {
    fm24_adapter::source::ATTRIBUTE_COLUMNS
        .iter()
        .map(|c| format!("{c} INTEGER"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One synthetic player: (person id, club, position code, level 0-100 applied to the relevant attributes).
struct Spec {
    id: i64,
    club: i64,
    position: &'static str,
    level: i64,
    ca: i64,
    pa: i64,
    nation: Option<i64>,
}

fn build_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(&SCHEMA.replace("%ATTRS%", &attribute_columns()))
        .unwrap();
    conn.execute(
        "INSERT INTO import_run VALUES (1,'synthetic','0','24.3.0',NULL,2026,'test','2026-01-01')",
        [],
    )
    .unwrap();
    for (id, name) in [(1, "France"), (2, "England")] {
        conn.execute("INSERT INTO nation VALUES (?,?)", params![id, name])
            .unwrap();
    }
    conn.execute(
        "INSERT INTO competition VALUES (12,16,'Ligue 1',1,'league',1,0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO competition_target VALUES ('fr_ligue1',1,'16')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO competition_rule VALUES ('fr_ligue1','season_start','2026-08-21','confirmée')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO competition_season VALUES (1,12,2026)", [])
        .unwrap();
    // 498 = a big club with a full squad, 464 = a small club with only 6 players, 466 = a club with none.
    for (club, name, short, rep) in [
        (498, "Paris Saint-Germain", "PSG", 9000),
        (464, "AJ Auxerre", "Auxerre", 3000),
        (466, "AS Monaco", "Monaco", 6000),
    ] {
        conn.execute(
            "INSERT INTO club VALUES (?,?,?,?,1,?)",
            params![club, club + 1000, name, short, rep],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO club_colours VALUES (?, '0000CD', 'FFFFFF', 'saisie')",
            params![club],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO club_finance_start VALUES (?,0,0,100000,0,?,?,?, 'générée')",
            params![club, 1_500_000, 2_000_000, 50_000_000],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO team VALUES (?,?,0,0)",
            params![club * 10, club],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO competition_season_team VALUES (1,?)",
            params![club * 10],
        )
        .unwrap();
    }

    let mut specs: Vec<Spec> = Vec::new();
    // PSG: 2 keepers, 5 defenders, 5 midfielders, 4 forwards, one with an unknown nationality; a star and a prospect.
    let layout = [("GK", 2), ("DC", 5), ("MC", 5), ("STC", 4)];
    let mut id = 1000;
    for (position, count) in layout {
        for n in 0..count {
            specs.push(Spec {
                id,
                club: 498,
                position,
                level: if n == 0 { 88 } else { 70 },
                ca: if n == 0 { 185 } else { 140 },
                pa: if n == 0 { 190 } else { 150 },
                nation: Some(1),
            });
            id += 1;
        }
    }
    specs.push(Spec {
        id: 2000,
        club: 498,
        position: "STC",
        level: 45,
        ca: 70,
        pa: 160,
        nation: None,
    });
    // Auxerre: 6 players only, lower division level.
    for (n, position) in ["GK", "DC", "DC", "MC", "MC", "STC"]
        .into_iter()
        .enumerate()
    {
        specs.push(Spec {
            id: 3000 + n as i64,
            club: 464,
            position,
            level: 52,
            ca: 80,
            pa: 85,
            nation: Some(2),
        });
    }
    for spec in &specs {
        conn.execute(
            "INSERT INTO person VALUES (?,?,?,?,?,?,?,?,'unique')",
            params![
                spec.id,
                spec.id + 500_000,
                format!("First{}", spec.id),
                format!("Last{}", spec.id),
                Option::<String>::None,
                "1998-05-04",
                spec.nation,
                180
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO player VALUES (?,?,?,?,?,?,?,?,?,'save')",
            params![
                spec.id, spec.ca, spec.pa, 5000, 5000, 5000, 60, 100, 10_000_000
            ],
        )
        .unwrap();
        let values = vec![spec.level.to_string(); 52].join(",");
        conn.execute_batch(&format!(
            "INSERT INTO player_attributes VALUES ({},{values})",
            spec.id
        ))
        .unwrap();
        for code in [
            "GK", "DC", "DL", "DR", "WBL", "WBR", "DM", "MC", "ML", "MR", "AMC", "AML", "AMR",
            "STC", "SW",
        ] {
            conn.execute(
                "INSERT INTO player_position VALUES (?,?,?)",
                params![spec.id, code, if code == spec.position { 20 } else { 1 }],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO contract VALUES (?,?,?,?,1,NULL,?,?,'save',?,?,?,1)",
            params![
                spec.id,
                spec.id,
                spec.club,
                spec.club * 10,
                50_000,
                50_000,
                "2025-07-01",
                if spec.id % 7 == 0 {
                    None
                } else {
                    Some("2029-06-30")
                },
                13
            ],
        )
        .unwrap();
    }
    // A head coach for PSG, and a scout.
    for (id, role) in [
        (9000_i64, "entraineur_principal"),
        (9001, "recruteur"),
        (9002, "president"),
    ] {
        conn.execute(
            "INSERT INTO person VALUES (?,?,?,?,NULL,'1975-02-02',1,180,'unique')",
            params![id, id + 500_000, format!("S{id}"), "Staff"],
        )
        .unwrap();
        conn.execute("INSERT INTO staff VALUES (?,100,100,6000)", params![id])
            .unwrap();
        for key in [
            "attacking",
            "defending",
            "fitness",
            "mental",
            "tactical",
            "technical",
            "judging_player_ability",
            "judging_player_potential",
            "physiotherapy",
            "sports_science",
        ] {
            conn.execute(
                "INSERT INTO staff_attribute VALUES (?,?,60)",
                params![id, key],
            )
            .unwrap();
        }
        conn.execute("INSERT INTO contract VALUES (?,?,498,4980,1,?,9000,9000,'save','2025-07-01','2028-06-30',NULL,1)", params![9000 + id, id, role]).unwrap();
    }
}

fn run_once(dir: &Path) -> pipeline::Run {
    let db = dir.join("world.db");
    if !db.exists() {
        build_db(&db);
    }
    pipeline::run(&db, &dir.join("package"), &Config::default_config())
        .expect("the synthetic world imports")
}

#[test]
fn the_synthetic_world_builds_with_real_ids_and_the_source_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    assert_eq!(
        run.source_sha256_before, run.source_sha256_after,
        "world.db is never modified"
    );
    assert_eq!(run.clubs.len(), 3);
    assert!(run.world.players.iter().any(|p| p.id == "fm-p-1000"));
    assert!(run.world.teams.iter().all(|t| t.id.starts_with("fm-c-")));
    assert_eq!(run.world.managers.len(), 1, "only PSG has a head coach");
    assert_eq!(run.manager_clubs_missing.len(), 2);
}

#[test]
fn a_goalkeeper_stays_a_goalkeeper_and_carries_no_weak_foot() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    let keeper = run.plans.iter().find(|p| p.def.id == "fm-p-1000").unwrap();
    assert_eq!(keeper.def.position, domain::player::Position::Goalkeeper);
    assert!(keeper.def.weak_foot.is_none() && keeper.def.alternate_positions.is_empty());
}

#[test]
fn the_hierarchy_of_levels_survives_the_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    let ovr = |id: &str| run.world.players.iter().find(|p| p.id == id).unwrap().ovr;
    // PSG's level-88 keeper/defender/midfielder/forward all rate above the level-70 team-mates, which
    // rate above Auxerre's level-52 players, in every role.
    for (star, regular, lower) in [
        ("fm-p-1000", "fm-p-1001", "fm-p-3000"),
        ("fm-p-1002", "fm-p-1003", "fm-p-3001"),
        ("fm-p-1007", "fm-p-1008", "fm-p-3003"),
        ("fm-p-1012", "fm-p-1013", "fm-p-3005"),
    ] {
        assert!(
            ovr(star) > ovr(regular) + 5,
            "{star} {} vs {regular} {}",
            ovr(star),
            ovr(regular)
        );
        assert!(
            ovr(regular) > ovr(lower) + 5,
            "{regular} {} vs {lower} {}",
            ovr(regular),
            ovr(lower)
        );
    }
}

#[test]
fn clubs_below_the_floor_get_identified_generated_players_and_an_empty_club_is_not_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    for club in ["fm-c-464", "fm-c-466"] {
        let seniors = run
            .world
            .players
            .iter()
            .filter(|p| {
                p.team_id.as_deref() == Some(club)
                    && p.squad_role == domain::player::SquadRole::Senior
            })
            .count();
        assert!(
            seniors >= ofm_core::squad_floor::MIN_SENIOR_PLAYERS,
            "{club} has {seniors} seniors"
        );
    }
    let generated: Vec<_> = run
        .world
        .players
        .iter()
        .filter(|p| !p.id.starts_with("fm-p-"))
        .collect();
    assert_eq!(
        generated.len(),
        run.generated_players.len(),
        "every generated player is listed"
    );
    // PSG has 17 real players: no more than the floor needs.
    assert!(
        run.generated_players
            .iter()
            .filter(|g| g.club_id == 498)
            .count()
            <= 2
    );
    // Auxerre keeps its 6 real players.
    assert_eq!(
        run.world
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some("fm-c-464") && p.id.starts_with("fm-p-"))
            .count(),
        6
    );
}

#[test]
fn a_missing_nationality_and_contract_end_are_filled_visibly() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    assert_eq!(
        run.journal.count("player.nation_fallback"),
        1,
        "the player with no nation is journaled"
    );
    let plan = run.plans.iter().find(|p| p.def.id == "fm-p-2000").unwrap();
    assert_eq!(plan.nationality_source, "club (repli)");
    let no_end = run.plans.iter().find(|p| p.def.id == "fm-p-1001").unwrap(); // 1001 % 7 == 0
    assert_eq!(no_end.contract_end_source, "générée");
    assert_eq!(no_end.def.contract_end.as_deref(), Some("2027-06-30"));
}

#[test]
fn finance_is_restored_in_euros_with_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    let team = run.world.teams.iter().find(|t| t.id == "fm-c-498").unwrap();
    assert_eq!(team.finance, 58_500_000);
    assert_eq!(team.transfer_budget, 2_340_000);
    assert_eq!(team.wage_budget, 1_755_000);
    assert!(run.finance.iter().any(|r| r.club_id == 498
        && r.field == "stadium_capacity"
        && r.provenance == fm24_adapter::finance::Provenance::OfmGenerated));
}

#[test]
fn the_world_file_is_byte_identical_across_independent_runs() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = report::canonical_world_json(&run_once(a.path()).world).unwrap();
    let second = report::canonical_world_json(&run_once(b.path()).world).unwrap();
    assert_eq!(
        report::sha256_hex(first.as_bytes()),
        report::sha256_hex(second.as_bytes())
    );
}

#[test]
fn the_integrity_checks_pass_except_the_240_club_perimeter_of_the_real_world() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_once(dir.path());
    let results = checks::run_checks(&run);
    let failed: Vec<_> = results.iter().filter(|c| !c.ok).map(|c| c.name).collect();
    assert_eq!(
        failed,
        vec!["perimeter.clubs", "perimeter.competitions"],
        "{results:?}"
    );
}
