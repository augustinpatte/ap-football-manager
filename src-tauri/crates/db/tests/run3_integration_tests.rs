//! Run 3: the transfer systems (instalments, sell-on clauses, signing terms) and the tactics
//! systems (separate defensive shape, phase instructions, custom tactics) were built on separate
//! branches. These tests keep them honest *together*, against a real `.db` file read back by a
//! fresh `SaveManager`: one career that holds both kinds of state, saved, reloaded and advanced.

#[path = "../../ofm_core/tests/market_fixtures/mod.rs"]
mod market_fixtures;

use db::save_manager::SaveManager;
use domain::custom_tactic::CustomTactic;
use domain::finance::CashKind;
use domain::season::TransferWindowStatus;
use domain::team::{DefensiveLine, PitchWidth, TacticsPhaseSettings};
use domain::transfer_terms::{TransferObligation, TransferTerms};
use market_fixtures::{PLAYER_ID, game_with, pending_offer, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{
    TransferNegotiationDecision, make_transfer_bid_with_terms, respond_to_offer,
};

const FEE: u64 = 3_000_000;
const USER: &str = "team-1";

fn user_team(game: &Game) -> &domain::team::Team {
    game.teams.iter().find(|team| team.id == USER).unwrap()
}

/// The fixture prospect owned by `small-club`, and a user club with a non-default tactical set-up:
/// an attacking shape, a deeper defensive shape, wide and high instructions and one saved tactic.
fn career_with_tactics() -> Game {
    let mut owned = player("1999-01-01");
    owned.team_id = Some("small-club".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = 70;
    owned.potential = 70;
    let mut game = game_with(owned);

    let team = game.teams.iter_mut().find(|t| t.id == USER).unwrap();
    team.formation = "4-3-3".to_string();
    team.out_of_possession_formation = "5-4-1".to_string();
    team.tactics_phase = TacticsPhaseSettings {
        width: PitchWidth::Wide,
        defensive_line: DefensiveLine::High,
        ..TacticsPhaseSettings::default()
    };
    game.custom_tactics = vec![CustomTactic {
        id: "ct-1".to_string(),
        name: "Park the bus".to_string(),
        description: String::new(),
        formation: "4-4-2".to_string(),
        play_style: "Counter".to_string(),
        out_of_possession_formation: "5-4-1".to_string(),
        tactics_phase: Some(TacticsPhaseSettings {
            width: PitchWidth::Narrow,
            ..TacticsPhaseSettings::default()
        }),
        source_preset_name: None,
    }];
    game
}

fn assert_tactics_intact(game: &Game, context: &str) {
    let team = user_team(game);
    assert_eq!(team.formation, "4-3-3", "{context}: attacking shape");
    assert_eq!(
        team.out_of_possession_formation, "5-4-1",
        "{context}: defensive shape"
    );
    assert_eq!(
        team.tactics_phase.width,
        PitchWidth::Wide,
        "{context}: width"
    );
    assert_eq!(
        team.tactics_phase.defensive_line,
        DefensiveLine::High,
        "{context}: line"
    );
    assert_eq!(game.custom_tactics.len(), 1, "{context}: saved tactics");
    assert_eq!(game.custom_tactics[0].out_of_possession_formation, "5-4-1");
    assert_eq!(
        game.custom_tactics[0]
            .tactics_phase
            .as_ref()
            .map(|p| &p.width),
        Some(&PitchWidth::Narrow),
        "{context}: saved tactic instructions"
    );
}

fn terms() -> TransferTerms {
    TransferTerms {
        instalments: 3,
        sell_on_pct: 20,
        signing_bonus: 0,
        promise_stepping_stone: false,
        ..TransferTerms::default()
    }
}

/// Given a career that has just completed a three-instalment, sell-on transfer while running a
/// non-default two-shape tactic,
/// When it is saved to disk and loaded by a fresh manager, then saved and loaded again,
/// Then the player's move, every obligation and every tactical field survive both round trips.
#[test]
fn a_career_with_open_transfer_obligations_and_custom_tactics_survives_two_round_trips() {
    let mut game = career_with_tactics();
    let decision = make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms())
        .expect("the bid resolves")
        .decision;
    assert_eq!(decision, TransferNegotiationDecision::Accepted);
    let obligations_before = game.transfer_obligations.clone();
    assert_eq!(
        obligations_before.len(),
        3,
        "two later instalments and one sell-on clause"
    );

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let mut manager = SaveManager::init(&saves).unwrap();
    let id = manager.create_save(&game, "Run 3").unwrap();

    let mut loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(loaded.transfer_obligations, obligations_before);
    assert_eq!(prospect(&loaded).team_id.as_deref(), Some(USER));
    assert_tactics_intact(&loaded, "first reload");

    let mut manager = SaveManager::init(&saves).unwrap();
    manager.save_game(&loaded, &id).unwrap();
    loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(loaded.transfer_obligations, obligations_before);
    assert_tactics_intact(&loaded, "second reload");
}

/// Given the same career reloaded from disk a year on, when the daily turn reaches the date the
/// second instalment falls due,
/// Then the seller is paid exactly once, the settlement is saved, and a reload neither pays it
/// again nor loses the tactical set-up the calendar has been running under.
#[test]
fn an_instalment_falling_due_after_a_reload_is_paid_once_and_tactics_are_kept() {
    let mut game = career_with_tactics();
    make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Run 3")
        .unwrap();
    let mut loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    set_date(&mut loaded, 2027, 8, 1);
    ofm_core::turn::process_day(&mut loaded);

    let first_due = loaded
        .transfer_obligations
        .iter()
        .filter_map(|o| match o {
            TransferObligation::Instalment {
                number: 2, paid_on, ..
            } => Some(paid_on.clone()),
            _ => None,
        })
        .next()
        .expect("the second instalment is still on the books");
    assert!(
        first_due.is_some(),
        "the due instalment was settled by the daily turn"
    );
    let paid_to_seller = |game: &Game| -> i64 {
        game.cash_journal
            .iter()
            .filter(|p| p.kind == CashKind::TransferFeeIn && p.club_id == "small-club")
            .map(|p| p.amount)
            .sum()
    };
    let received = paid_to_seller(&loaded);

    let mut manager = SaveManager::init(&saves).unwrap();
    manager.save_game(&loaded, &id).unwrap();
    let mut reloaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(reloaded.transfer_obligations, loaded.transfer_obligations);
    assert_tactics_intact(&reloaded, "after the due day");

    ofm_core::turn::process_day(&mut reloaded);
    assert_eq!(
        paid_to_seller(&reloaded),
        received,
        "the reloaded career does not pay the same instalment twice"
    );
}

/// Given the user's club bought the prospect with a 20% sell-on clause and fielded him under a
/// two-shape tactic,
/// When he is sold on through an incoming offer answered from the inbox, and the career is saved
/// and reloaded,
/// Then he is gone from the XI, the original seller's clause is settled once, and the user's
/// attacking shape, defensive shape and instructions are exactly as the manager left them.
#[test]
fn selling_a_fielded_signing_settles_the_clause_and_leaves_the_tactics_alone() {
    let mut game = career_with_tactics();
    make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms()).unwrap();
    game.teams
        .iter_mut()
        .find(|t| t.id == USER)
        .unwrap()
        .starting_xi_ids
        .push(PLAYER_ID.to_string());

    set_date(&mut game, 2027, 2, 1);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    pending_offer(&mut game, "resale", "big-club", 4_000_000, "2027-02-01");
    respond_to_offer(&mut game, PLAYER_ID, "resale", true).unwrap();

    assert_eq!(prospect(&game).team_id.as_deref(), Some("big-club"));
    assert!(
        !user_team(&game)
            .starting_xi_ids
            .iter()
            .any(|id| id == PLAYER_ID),
        "a sold player cannot stay in the XI"
    );
    assert!(game.transfer_obligations.iter().any(|o| matches!(
        o,
        TransferObligation::SellOn {
            settled_on: Some(_),
            ..
        }
    )));

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Run 3")
        .unwrap();
    let reloaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    assert_eq!(reloaded.transfer_obligations, game.transfer_obligations);
    assert_eq!(prospect(&reloaded).team_id.as_deref(), Some("big-club"));
    assert!(
        !user_team(&reloaded)
            .starting_xi_ids
            .iter()
            .any(|id| id == PLAYER_ID)
    );
    assert_tactics_intact(&reloaded, "after selling a fielded signing");
}

// ── Careers written by earlier builds, read by the integrated one ────────────────────────────────

/// The schema a given earlier build left a real career file in, made by taking a current career
/// file and removing exactly what that build did not have yet. `user_version` is then set to what
/// that build wrote.
#[derive(Clone, Copy, Debug)]
enum WrittenBy {
    /// Released `apfm/main`: schema 61, no obligations table, no defensive-shape column.
    V061,
    /// Run 1 development build: schema 62 with the obligations table, no defensive-shape column.
    Run1Dev,
    /// Run 2 development build: schema 62 with the defensive-shape column, no obligations table.
    Run2Dev,
}

fn rewrite_file_as(saves: &std::path::Path, id: &str, build: WrittenBy) -> std::path::PathBuf {
    let path = saves.join(format!("{id}.db"));
    let conn = rusqlite::Connection::open(&path).unwrap();
    let drop_column = "ALTER TABLE teams DROP COLUMN out_of_possession_formation;";
    let drop_table = "DROP TABLE transfer_obligations;";
    // Added after every build this helper imitates (v064 the checkpoint table, v065 the suspension
    // column): such a file does not have them.
    let drop_checkpoint = "DROP TABLE interactive_match_checkpoint;\
         ALTER TABLE players DROP COLUMN suspended_matches;";
    let (sql, version) = match build {
        WrittenBy::V061 => (format!("{drop_column}{drop_table}"), 61),
        WrittenBy::Run1Dev => (drop_column.to_string(), 62),
        WrittenBy::Run2Dev => (drop_table.to_string(), 62),
    };
    conn.execute_batch(&format!("{sql}{drop_checkpoint}"))
        .unwrap();
    conn.pragma_update(None, "user_version", version).unwrap();
    path
}

fn has_table(path: &std::path::Path, table: &str) -> bool {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 1
}

fn has_team_column(path: &std::path::Path, column: &str) -> bool {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('teams') WHERE name = ?1",
            [column],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 1
}

/// Given a real career file left by the Run 2 development build (defensive shape, custom tactics,
///       no obligations table),
/// When a fresh `SaveManager` loads it, the integrated build then books a transfer with
///      instalments, and the career is saved and loaded again,
/// Then the tactics survive every step, the file gained the obligations table, a backup of the
///      Run 2 file exists and is untouched, and the new obligations come back.
#[test]
fn a_run2_development_career_loads_keeps_its_tactics_and_then_books_obligations() {
    let game = career_with_tactics();
    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Run 2 dev")
        .unwrap();
    let path = rewrite_file_as(&saves, &id, WrittenBy::Run2Dev);
    assert!(
        has_team_column(&path, "out_of_possession_formation")
            && !has_table(&path, "transfer_obligations")
    );
    let original = std::fs::read(&path).unwrap();

    let mut loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    assert_tactics_intact(&loaded, "Run 2 dev career loaded");
    assert!(
        has_table(&path, "transfer_obligations"),
        "the file was repaired"
    );
    assert_eq!(
        std::fs::read(saves.join(format!("{id}.db.pre-v063.bak"))).unwrap(),
        original,
        "the backup is the Run 2 file as it was"
    );

    // The window's status is derived from the calendar on load; the fixture world has none.
    loaded.season_context.transfer_window.status = TransferWindowStatus::Open;
    make_transfer_bid_with_terms(&mut loaded, PLAYER_ID, FEE, terms()).unwrap();
    let booked = loaded.transfer_obligations.clone();
    assert_eq!(booked.len(), 3);
    SaveManager::init(&saves)
        .unwrap()
        .save_game(&loaded, &id)
        .unwrap();
    let again = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(again.transfer_obligations, booked);
    assert_tactics_intact(&again, "after booking obligations");
}

/// Given a real career file left by the Run 1 development build (open obligations, no
///       defensive-shape column),
/// When it is loaded by the integrated build and then saved and loaded again,
/// Then the obligations and the transfer survive, the attacking shape is kept, the defensive shape
///      reads as "none" (the build never had one) and the file gained the column.
#[test]
fn a_run1_development_career_keeps_its_obligations_and_gains_the_defensive_shape_column() {
    let mut game = career_with_tactics();
    make_transfer_bid_with_terms(&mut game, PLAYER_ID, FEE, terms()).unwrap();
    let obligations = game.transfer_obligations.clone();
    assert_eq!(obligations.len(), 3);
    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Run 1 dev")
        .unwrap();
    let path = rewrite_file_as(&saves, &id, WrittenBy::Run1Dev);
    assert!(
        !has_team_column(&path, "out_of_possession_formation")
            && has_table(&path, "transfer_obligations")
    );

    let loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    assert_eq!(loaded.transfer_obligations, obligations);
    assert_eq!(prospect(&loaded).team_id.as_deref(), Some(USER));
    assert_eq!(user_team(&loaded).formation, "4-3-3");
    assert_eq!(user_team(&loaded).out_of_possession_formation, "");
    assert!(has_team_column(&path, "out_of_possession_formation"));
    SaveManager::init(&saves)
        .unwrap()
        .save_game(&loaded, &id)
        .unwrap();
    let again = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(again.transfer_obligations, obligations);
}

/// Given a real career file left by the released v061 build,
/// When it is loaded by the integrated build,
/// Then it loads, both new structures exist in the file, and the career is playable on: a transfer
///      with instalments books obligations that survive a reload.
#[test]
fn a_released_v061_career_loads_and_gains_both_structures() {
    let game = career_with_tactics();
    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "v061")
        .unwrap();
    let path = rewrite_file_as(&saves, &id, WrittenBy::V061);
    assert!(
        !has_team_column(&path, "out_of_possession_formation")
            && !has_table(&path, "transfer_obligations")
    );

    let mut loaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    assert!(has_team_column(&path, "out_of_possession_formation"));
    assert!(has_table(&path, "transfer_obligations"));
    assert!(loaded.transfer_obligations.is_empty());
    assert_eq!(user_team(&loaded).formation, "4-3-3");
    loaded.season_context.transfer_window.status = TransferWindowStatus::Open;
    make_transfer_bid_with_terms(&mut loaded, PLAYER_ID, FEE, terms()).unwrap();
    let booked = loaded.transfer_obligations.clone();
    assert_eq!(booked.len(), 3);
    SaveManager::init(&saves)
        .unwrap()
        .save_game(&loaded, &id)
        .unwrap();
    let again = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();
    assert_eq!(again.transfer_obligations, booked);
}

/// Given a career file whose schema contradicts its version (claims 63, lacks the obligations
///       table),
/// When a fresh manager tries to load it,
/// Then the load is refused and the file is left byte-identical.
#[test]
fn a_career_file_that_contradicts_its_version_is_refused_untouched() {
    let game = career_with_tactics();
    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Broken")
        .unwrap();
    let path = saves.join(format!("{id}.db"));
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE transfer_obligations;")
        .unwrap();
    let before = std::fs::read(&path).unwrap();

    let outcome = SaveManager::init(&saves).unwrap().load_game(&id);

    assert!(outcome.is_err(), "a contradictory file must not load");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
