//! A career in which the assistant recruits: a season and a rollover on the generated world, with
//! the club's signings handed over inside a fee ceiling, and a save/reload in the middle of the
//! window.
//!
//! The assertions are about what delegation promises, not about who is signed: the assistant never
//! pays above the ceiling, never sells anyone, never leaves two copies of a player, and what he
//! has in hand (shelved targets, open negotiations, the log) survives the SQLite round trip and the
//! rollover.

use db::save_manager::SaveManager;
use domain::delegation::DelegationMode;
use domain::staff::StaffPost;
use domain::stats::StatsState;
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, play_day, roll_season_over};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;
use season_harness::worlds::WorldSpec;

const FEE_CEILING: i64 = 4_000_000;

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn ensure_assistant(game: &mut Game) {
    let team = user_team(game);
    if profile::team_staff(game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager)
    {
        return;
    }
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

/// Every transfer the user's club is party to, from the competitions' logs.
fn user_moves(game: &Game) -> Vec<(String, String, String, u64)> {
    let user = user_team(game);
    game.competitions
        .iter()
        .flat_map(|c| &c.transfer_log)
        .filter(|t| t.from_team_id == user || t.to_team_id == user)
        .map(|t| {
            (
                t.date.clone(),
                t.from_team_id.clone(),
                t.to_team_id.clone(),
                t.fee,
            )
        })
        .collect()
}

/// The generated club is balanced and short of money, so the assistant would have nothing to do: a
/// real transfer budget, and a back line cut to four.
fn give_the_club_a_need(game: &mut Game) {
    let team = user_team(game);
    for club in game.teams.iter_mut().filter(|t| t.id == team) {
        club.transfer_budget = 30_000_000;
        club.finance = 40_000_000;
    }
    let other = game
        .teams
        .iter()
        .find(|t| t.id != team)
        .map(|t| t.id.clone())
        .expect("another club");
    let mut defenders: Vec<(u8, String)> = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team.as_str())
                && p.natural_position.to_group_position() == domain::player::Position::Defender
        })
        .map(|p| (p.ovr, p.id.clone()))
        .collect();
    defenders.sort_by(|a, b| b.cmp(a));
    // Four are left, which is a back line too short to play with: a need the squad review names.
    for (_, id) in defenders.into_iter().skip(4) {
        if let Some(club) = game.teams.iter_mut().find(|t| t.id == team) {
            club.remove_player_references(&id);
        }
        if let Some(player) = game.players.iter_mut().find(|p| p.id == id) {
            player.team_id = Some(other.clone());
            // The new club has its own shirt numbers.
            player.jersey_number = None;
        }
    }
}

fn every_player_has_one_club(game: &Game) {
    let mut seen = std::collections::HashSet::new();
    for player in &game.players {
        assert!(seen.insert(&player.id), "{} appears twice", player.id);
    }
}

/// Given the whole job handed over with a fee ceiling
/// When a season is played with a reload in the middle of the window and another after the rollover
/// Then nothing above the ceiling is paid, nobody is sold by the assistant, the world stays
///      consistent, and the assistant's memory is exactly what it was before each reload.
#[test]
fn a_delegated_career_recruits_within_its_ceiling_and_remembers_across_reloads() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    ensure_assistant(&mut game);
    game.delegation.apply_mode(DelegationMode::Full);
    game.delegation.limits.max_transfer_fee = FEE_CEILING;
    game.delegation.limits.max_commitment = 12_000_000;
    game.delegation.limits.max_player_wage = 60_000;
    give_the_club_a_need(&mut game);
    let options = RunOptions::seasons(1);
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);
    let user = user_team(&game);
    let squad_at_start: Vec<String> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(user.as_str()))
        .map(|p| p.id.clone())
        .collect();

    let dir = tempfile::tempdir().unwrap();
    let mut saves = SaveManager::init(dir.path()).unwrap();
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "recruiting")
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
            loaded.delegation.recruitment, game.delegation.recruitment,
            "{when}: the assistant forgot what he had in hand"
        );
        assert_eq!(
            loaded.delegation, game.delegation,
            "{when}: delegation changed"
        );
        *game = loaded;
    };

    let mut day = 0;
    let mut signed_by_day_60 = 0;
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
        every_player_has_one_club(&game);
        if day == 25 {
            reload(&mut game, "inside the first window");
        }
        if day == 60 {
            // Arrived, not merely agreed: a deal waits for its registration date, and the log
            // says "scheduled" until then.
            signed_by_day_60 = user_moves(&game)
                .iter()
                .filter(|(_, _, to, _)| *to == user)
                .count();
        }
    }

    assert!(
        signed_by_day_60 >= 1,
        "the assistant never signed anyone in the first window: the test would prove nothing"
    );
    // The manager did nothing by hand all season, so every move of the club's is the assistant's.
    for (date, from, to, fee) in user_moves(&game) {
        if to == user {
            assert!(
                i64::try_from(fee).unwrap() <= FEE_CEILING,
                "{date}: {fee} is above the ceiling"
            );
        }
        assert_ne!(
            from, user,
            "{date}: someone was sold with nobody to sell him"
        );
    }
    for id in &squad_at_start {
        let player = game.players.iter().find(|p| &p.id == id).unwrap();
        assert!(
            player.retired
                || player.team_id.as_deref() == Some(user.as_str())
                || player.team_id.is_none(),
            "{id} was moved on while nobody sold him"
        );
    }

    roll_season_over(
        &mut game,
        options,
        1,
        &mut (&mut invariants, &mut lifecycle),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut game, "after the rollover");
    assert_eq!(game.delegation.mode(), DelegationMode::Full);
    assert_eq!(game.delegation.limits.max_transfer_fee, FEE_CEILING);

    ensure_assistant(&mut game);
    for day in 1..=30 {
        play_day(
            &mut game,
            options,
            2,
            day,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        every_player_has_one_club(&game);
    }
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
}
