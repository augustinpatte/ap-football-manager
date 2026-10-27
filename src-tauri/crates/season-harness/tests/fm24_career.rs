//! A career in the FM24 world, played across a rollover: the cases a synthetic world cannot raise.
//!
//! The FM24 world is the player's own file (`APFM_WORLD_JSON`, built by `apfm-import` from their
//! `world.db`) and is never in the repository, so each test returns at once when it is not set.
//! Run them with `APFM_WORLD_JSON=<file> cargo test --release -p season-harness --test fm24_career`.

use db::save_manager::SaveManager;
use domain::league::{CompetitionType, FixtureStatus};
use domain::stats::StatsState;
use season_harness::driver::{RunOptions, play_day, roll_season_over, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

const PSG: &str = "fm-c-498";

/// Given the FM24 world and a Ligue 1 career,
/// When two seasons are played through the production day and rollover,
/// Then every invariant of the harness holds, and at each rollover all twelve leagues have played
///      their season out and carry the same season: a league that ends later than the user's must
///      not be left behind (it used to sit idle for a year and then play May to January).
#[test]
fn every_league_of_the_fm24_world_plays_every_season_in_step() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let options = RunOptions::seasons(1);

    for season in 1..=2 {
        let mut day = 0;
        while !ofm_core::end_of_season::is_season_complete(&game) {
            day += 1;
            assert!(day < 460, "season {season} did not complete");
            play_day(&mut game, options, season, day, &mut invariants).expect("a day plays");
        }
        let leagues: Vec<_> = game
            .competitions
            .iter()
            .filter(|c| {
                c.kind == CompetitionType::League
                    && c.rules.format == domain::league::CompetitionFormat::LeagueTable
            })
            .collect();
        assert_eq!(leagues.len(), 12, "the twelve championships of the world");
        for league in &leagues {
            let unplayed = league
                .fixtures
                .iter()
                .filter(|f| f.counts_for_league_standings() && f.status != FixtureStatus::Completed)
                .count();
            assert_eq!(
                unplayed, 0,
                "{} still has {unplayed} fixtures at the rollover",
                league.name
            );
        }
        let seasons: std::collections::BTreeSet<u32> = leagues.iter().map(|c| c.season).collect();
        assert_eq!(
            seasons.len(),
            1,
            "leagues on different seasons: {seasons:?}"
        );
        roll_season_over(&mut game, options, season, &mut invariants).expect("the rollover runs");
    }

    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given a career saved once and then played through a rollover,
/// When it is saved again over the same file and read back,
/// Then the save succeeds. Retirees leave their shirts to the recruits of the same club, and the
///      file still holds the retirees' rows until the archive step: the save used to fail, for
///      good, from the first rollover on.
#[test]
fn a_career_saves_over_its_own_file_after_a_rollover() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, "fm-c-501", 7).expect("the career builds");
    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "career")
        .expect("the opening save");
    let mut invariants = Invariants::new(&game);
    run_seasons(&mut game, RunOptions::seasons(1), &mut invariants).expect("a season plays");

    manager
        .save_game_with_stats(&game, &StatsState::default(), &id)
        .expect("saving over the same file after the rollover");

    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&id)
        .unwrap();
    assert_eq!(reloaded.clock.current_date, game.clock.current_date);
    assert_eq!(
        reloaded.players.len(),
        game.players.len(),
        "no player is lost by the save"
    );
    assert_eq!(reloaded.retired_players.len(), game.retired_players.len());
}
