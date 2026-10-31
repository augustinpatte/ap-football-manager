//! Run 3: the transfer systems (instalments, sell-on clauses, signing terms) and the tactics
//! systems (separate defensive shape, phase instructions, custom tactics) were built on separate
//! branches. These tests keep them honest *together*, against a real `.db` file read back by a
//! fresh `SaveManager`: one career that holds both kinds of state, saved, reloaded and advanced.

#[path = "../../ofm_core/tests/market_fixtures/mod.rs"]
mod market_fixtures;

use db::save_manager::SaveManager;
use domain::custom_tactic::CustomTactic;
use domain::finance::CashKind;
use domain::team::{DefensiveLine, PitchWidth, TacticsPhaseSettings};
use domain::transfer_terms::{TransferObligation, TransferTerms};
use market_fixtures::{PLAYER_ID, game_with, player, prospect, set_date};
use ofm_core::game::Game;
use ofm_core::transfers::{TransferNegotiationDecision, make_transfer_bid_with_terms};

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
    assert_eq!(team.out_of_possession_formation, "5-4-1", "{context}: defensive shape");
    assert_eq!(team.tactics_phase.width, PitchWidth::Wide, "{context}: width");
    assert_eq!(team.tactics_phase.defensive_line, DefensiveLine::High, "{context}: line");
    assert_eq!(game.custom_tactics.len(), 1, "{context}: saved tactics");
    assert_eq!(game.custom_tactics[0].out_of_possession_formation, "5-4-1");
    assert_eq!(
        game.custom_tactics[0].tactics_phase.as_ref().map(|p| &p.width),
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
    assert_eq!(obligations_before.len(), 3, "two later instalments and one sell-on clause");

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
            TransferObligation::Instalment { number: 2, paid_on, .. } => Some(paid_on.clone()),
            _ => None,
        })
        .next()
        .expect("the second instalment is still on the books");
    assert!(first_due.is_some(), "the due instalment was settled by the daily turn");
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
