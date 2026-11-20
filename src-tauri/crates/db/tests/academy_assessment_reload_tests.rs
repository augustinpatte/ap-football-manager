//! What the youth staff tell the manager of an academy player's ceiling, saved to a real `.db` file
//! and read back by a fresh `SaveManager`: the assessment is a pure function of what the save
//! holds, so it must read the same after a reload, while the save keeps the exact ceiling the
//! development model needs.

#[path = "../../ofm_core/tests/market_fixtures/mod.rs"]
#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use db::save_manager::SaveManager;
use domain::player::SquadRole;
use market_fixtures::{game_with, rated_player};
use ofm_core::academy_review::academy_review;
use ofm_core::game::Game;
use ofm_core::rating_view::RatingView;

/// The manager's club with three academy players: two whose true ceilings (62 and 71) fall in one
/// assessment band and one far above (90). No youth staff, so the bands are the widest.
fn academy_career() -> Game {
    let mut game = game_with(rated_player("2000-01-01", 70));
    for (id, ovr, ceiling) in [("kid-low", 50, 62), ("kid-high", 50, 71), ("star", 55, 90)] {
        let mut kid = rated_player("2008-01-01", ovr);
        kid.id = id.to_string();
        kid.team_id = Some("team-1".to_string());
        kid.squad_role = SquadRole::Youth;
        kid.ovr = ovr;
        kid.potential = ceiling;
        game.players.push(kid);
    }
    game
}

fn ceiling_of(game: &Game, id: &str) -> (u8, u8) {
    let review = academy_review(game).expect("the manager has a club");
    let row = review
        .rows
        .iter()
        .find(|row| row.player_id == id)
        .expect("an academy row");
    match row.ceiling {
        RatingView::Estimate { low, high, .. } => (low, high),
        ref other => panic!("an assessed band, not {other:?}"),
    }
}

/// Every academy row's assessed ceiling and the department's summary, as the screens receive them.
fn assessments(game: &Game) -> (Vec<(String, serde_json::Value)>, serde_json::Value) {
    let review = academy_review(game).expect("the manager has a club");
    let rows = review
        .rows
        .iter()
        .map(|row| {
            (
                row.player_id.clone(),
                serde_json::to_value(&row.ceiling).expect("a ceiling serializes"),
            )
        })
        .collect();
    (
        rows,
        serde_json::to_value(&review.summary).expect("a summary serializes"),
    )
}

fn saved_and_loaded(game: &Game, dir: &std::path::Path, name: &str) -> Game {
    let saves = dir.join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(game, name)
        .unwrap();
    SaveManager::init(&saves).unwrap().load_game(&id).unwrap()
}

/// Given an academy of three players saved to disk and loaded by a fresh save manager
/// When the assessments are read before and after, and the loaded game is saved and loaded again
/// Then every ceiling band and the youth summary are identical across the reload, two ceilings in
///      one band still read alike, the whole review is identical across the second reload, and the
///      save still holds each player's exact potential.
///
/// (The whole review is not compared with the unsaved game: the fixture's legacy depth positions
/// are rewritten on load, which moves the squad cover and so the verdicts, not the assessments.)
#[test]
fn the_assessments_survive_a_save_and_a_reload_and_the_save_keeps_the_truth() {
    let game = academy_career();
    assert_eq!(ceiling_of(&game, "kid-low"), ceiling_of(&game, "kid-high"));
    assert_ne!(ceiling_of(&game, "kid-low"), ceiling_of(&game, "star"));
    let dir = tempfile::tempdir().unwrap();

    let reloaded = saved_and_loaded(&game, dir.path(), "Academy");

    assert_eq!(assessments(&game), assessments(&reloaded));
    for (id, ceiling) in [("kid-low", 62), ("kid-high", 71), ("star", 90)] {
        let stored = reloaded.players.iter().find(|p| p.id == id).unwrap();
        assert_eq!(
            stored.potential, ceiling,
            "{id}: the save must keep the exact ceiling"
        );
        let (low, high) = ceiling_of(&reloaded, id);
        assert!(low <= ceiling && ceiling <= high, "{id}: {low}-{high}");
    }
    let again = saved_and_loaded(&reloaded, dir.path(), "Academy again");
    let first = serde_json::to_string(&academy_review(&reloaded).unwrap()).unwrap();
    let second = serde_json::to_string(&academy_review(&again).unwrap()).unwrap();
    assert_eq!(first, second);
    assert!(!first.contains("\"potential\""), "{first}");
}
