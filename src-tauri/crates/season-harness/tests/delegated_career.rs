//! A season of managing nothing by hand: the player hands every duty to the assistant coach, the
//! career is saved and reloaded in the middle of the season and across the rollover, and the
//! assistant keeps working throughout.
//!
//! The assertions are about what delegation promises, not about results: the eleven named is
//! always a legal one, nothing above the spending limits is signed, the settings and history
//! survive the SQLite round trip and the rollover, and the assistant is still acting in the
//! season after.

use db::save_manager::SaveManager;
use domain::delegation::{DelegationMode, Duty};
use domain::staff::StaffPost;
use domain::stats::StatsState;
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, play_day, roll_season_over};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;
use season_harness::worlds::WorldSpec;

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn has_assistant(game: &Game) -> bool {
    profile::team_staff(game, &user_team(game))
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager)
}

/// Gives the club an assistant coach if the generated staff has none.
fn ensure_assistant(game: &mut Game) {
    if has_assistant(game) {
        return;
    }
    let team = user_team(game);
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team)
        .map_or(0, |t| t.reputation);
    let (id, wage) = game
        .staff
        .iter()
        .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
        .max_by_key(|s| (s.overall(), s.id.clone()))
        .map(|s| (s.id.clone(), market::asking_wage(s, reputation)))
        .expect("a free assistant coach exists in the world");
    market::hire(game, &team, &id, wage, 3).expect("the assistant can be hired");
}

fn xi_is_legal(game: &Game) {
    let team_id = user_team(game);
    let team = game.teams.iter().find(|t| t.id == team_id).unwrap();
    let xi = &team.starting_xi_ids;
    if xi.is_empty() {
        return;
    }
    let distinct: std::collections::HashSet<_> = xi.iter().collect();
    assert_eq!(distinct.len(), xi.len(), "a player named twice");
    assert_eq!(xi.len(), 11, "the delegated eleven is not eleven");
    for id in xi {
        let player = game
            .players
            .iter()
            .find(|p| &p.id == id)
            .expect("a real player");
        assert_eq!(
            player.team_id.as_deref(),
            Some(team_id.as_str()),
            "not a club player"
        );
    }
}

/// Given a generated world and every duty handed over with full delegation,
/// When a season is played with a save and reload in the middle and another after the rollover,
/// Then the settings, limits and history survive both, the eleven is always legal, the assistant
///      acts again in the next season, and nothing over the spending limits is ever signed.
#[test]
fn a_fully_delegated_season_survives_reload_and_rollover() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    ensure_assistant(&mut game);
    game.delegation.apply_mode(DelegationMode::Full);
    game.delegation.limits.max_staff_wage = 4_000;
    let options = RunOptions::seasons(1);
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);

    let dir = tempfile::tempdir().unwrap();
    let mut saves = SaveManager::init(dir.path()).unwrap();
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "delegated")
        .unwrap();
    let mut reload = |game: &mut Game, when: &str| {
        saves
            .save_game_with_stats(game, &StatsState::default(), &save_id)
            .unwrap_or_else(|e| panic!("{when}: save failed: {e}"));
        let loaded = SaveManager::init(dir.path())
            .unwrap()
            .load_game(&save_id)
            .unwrap_or_else(|e| panic!("{when}: load failed: {e}"));
        assert_eq!(
            loaded.delegation, game.delegation,
            "{when}: delegation changed in the round trip"
        );
        *game = loaded;
    };

    let mut day = 0;
    while !is_season_complete(&game) {
        day += 1;
        assert!(day < 460, "the season did not complete");
        play_day(
            &mut game,
            options,
            1,
            day,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        xi_is_legal(&game);
        if day == 50 {
            // Seven weeks in: the assistant has been acting on the duties that fall due weekly.
            assert!(game.delegation.last_run_on(Duty::Lineup).is_some());
            assert!(
                game.delegation
                    .last_run_on(Duty::MatchPreparation)
                    .is_some()
            );
            reload(&mut game, "mid-season");
        }
        for entry in &game.delegation.log {
            assert!(
                !entry.title_key.contains("transfer"),
                "the assistant made a transfer: {}",
                entry.title_key
            );
        }
    }
    let history_before = game.delegation.log.len();
    assert!(
        history_before > 0,
        "a whole season of delegation left no history"
    );

    roll_season_over(
        &mut game,
        options,
        1,
        &mut (&mut invariants, &mut lifecycle),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut game, "after the rollover");
    assert_eq!(game.delegation.mode(), DelegationMode::Full);
    assert_eq!(game.delegation.limits.max_staff_wage, 4_000);
    assert_eq!(game.delegation.log.len(), history_before);

    // The next season: with an assistant still in post, the delegated duties run again.
    ensure_assistant(&mut game);
    let rolled_on = game.clock.current_date.date_naive();
    for day in 1..=30 {
        play_day(
            &mut game,
            options,
            2,
            day,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        xi_is_legal(&game);
    }
    let last_lineup = game
        .delegation
        .last_run_on(Duty::Lineup)
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .expect("the lineup duty ran");
    assert!(
        last_lineup > rolled_on,
        "the assistant did not resume after the rollover"
    );
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
}
