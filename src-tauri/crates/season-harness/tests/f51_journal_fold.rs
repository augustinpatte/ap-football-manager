//! Run F5.1: the cash journal is folded at rollovers, and a folded career reloads exactly.
//!
//! A career is played for several seasons and saved the way the game saves (a first save, then
//! overwrites), with no load in between: loading a copy mid-run re-seeds opening balances and would
//! make the live game and the file disagree for reasons that have nothing to do with the fold. At
//! the end the file is loaded and compared with the game that wrote it.
//!
//! `cargo test --release -p season-harness --test f51_journal_fold -- --ignored --nocapture`

use domain::finance::CashKind;
use domain::stats::StatsState;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

struct Quiet;
impl SeasonObserver for Quiet {}

#[test]
#[ignore = "plays five seasons; run in release with --ignored"]
fn a_folded_career_reloads_with_every_clubs_cash_and_posts_exactly() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut manager = db::save_manager::SaveManager::init(dir.path()).expect("saves");
    let mut game = WorldSpec::gate(5).build().expect("world");
    let stats = StatsState::default();

    // Season by season, saved after each, as a player's game is.
    run_seasons(&mut game, RunOptions::seasons(1), &mut Quiet).expect("season 1");
    let id = manager
        .create_save_with_stats(&game, &stats, "fold")
        .expect("create");
    for _ in 2..=5 {
        run_seasons(&mut game, RunOptions::seasons(1), &mut Quiet).expect("a season");
        manager
            .save_game_with_stats(&game, &stats, &id)
            .expect("save");
    }

    let folds = game
        .cash_journal
        .iter()
        .filter(|p| p.id.starts_with("fold:"))
        .count();
    assert!(
        folds > 0,
        "five seasons in, the oldest months should be folded"
    );
    let cutoff = game
        .cash_journal_fold_pending
        .clone()
        .expect("the harness never clears the pending marker, so the last fold is still owed");
    assert!(
        game.cash_journal
            .iter()
            .filter(|p| p.date < cutoff && p.kind != CashKind::OpeningBalance)
            .all(|p| p.id.starts_with("fold:")),
        "nothing before the cutoff is left as a single payment"
    );

    let mut fresh = db::save_manager::SaveManager::init(dir.path()).expect("init");
    let loaded = fresh.load_game(&id).expect("load");

    assert_eq!(loaded.cash_journal.len(), game.cash_journal.len());
    for team in &game.teams {
        assert_eq!(
            loaded.cash_journal.cash_for(&team.id),
            team.finance,
            "{}",
            team.id
        );
        assert_eq!(
            loaded.cash_journal.cash_for(&team.id),
            game.cash_journal.cash_for(&team.id)
        );
    }
    eprintln!(
        "five seasons: {} posts ({} folds), every club's cash reloaded exactly",
        game.cash_journal.len(),
        folds
    );
}
