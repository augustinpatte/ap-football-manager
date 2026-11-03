//! Run F4.2, scenario 6: a season of interactive matches, played through the application layer.
//!
//! Every fixture of the user's club is played with the M12 engine as an interactive match (no
//! orders: the club plays its own plan), finalised through the one canonical path, and the days
//! between them advance as the game advances them (training, recovery, injuries, everybody else's
//! results). The test asks what a season must keep true: each fixture finalised once, the table
//! equal to the results, condition that rises and falls but does not spiral, injured players that
//! do not play, and a save and reload in the middle that changes nothing.
//!
//! `#[ignore]`d: it plays a whole season. Run it with
//! `cargo test --release --lib interactive_season -- --ignored --nocapture`.

use std::collections::BTreeMap;

use ofm_core::game::Game;
use ofm_core::interactive_match::{AdvanceRequest, AdvanceTarget, StopReason};
use ofm_core::state::StateManager;
use season_harness::worlds::WorldSpec;

use crate::application::interactive_match::{
    advance_interactive_match_internal, finish_interactive_match_internal,
    start_interactive_match_internal,
};
use crate::application::time_advancement::advance_time_with_mode;

/// What one played match left in the career, measured on the user's club.
#[derive(Debug, Clone, Default)]
struct MatchRecord {
    /// Squad average (bench included) before and after the match.
    starters_condition_before: f64,
    starters_condition_after: f64,
    /// Condition, before the match, of the players who played in it: the fatigue they arrived with.
    arrived_with: Vec<u8>,
    injured_after: usize,
    reds_after: u32,
    played_while_injured: usize,
}

fn user_squad(game: &Game) -> Vec<&domain::player::Player> {
    let team = game.manager.team_id.as_deref().unwrap_or_default();
    game.players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team) && !p.retired)
        .collect()
}

fn appearances(game: &Game) -> BTreeMap<String, u32> {
    user_squad(game)
        .into_iter()
        .map(|p| (p.id.clone(), p.stats.appearances))
        .collect()
}

fn mean_condition(game: &Game) -> f64 {
    let squad = user_squad(game);
    squad.iter().map(|p| f64::from(p.condition)).sum::<f64>() / squad.len().max(1) as f64
}

fn play_interactive(state: &StateManager) {
    start_interactive_match_internal(state).expect("the interactive match starts");
    loop {
        let out = advance_interactive_match_internal(
            state,
            AdvanceRequest {
                target: AdvanceTarget::ToEnd,
                stop_at_key_events: false,
            },
            0,
        )
        .expect("advance");
        if out.stop == StopReason::Finished {
            break;
        }
    }
    finish_interactive_match_internal(state).expect("the match is finalised");
}

/// Plays user matches until `matches` have been played (or the season is over). Returns the records.
fn play_matches(state: &StateManager, matches: usize) -> Vec<MatchRecord> {
    let mut records = Vec::new();
    let mut guard = 0;
    while records.len() < matches {
        guard += 1;
        assert!(guard < 600, "the season never ends");
        if state
            .get_game(ofm_core::end_of_season::is_season_complete)
            .unwrap_or(true)
        {
            break;
        }
        let response = advance_time_with_mode(state, "live").expect("a day advances");
        if response.action != "live_match" {
            continue;
        }
        let before = state.get_game(|g| g.clone()).expect("game");
        let appearances_before = appearances(&before);
        let injured_ids: Vec<String> = user_squad(&before)
            .iter()
            .filter(|p| p.injury.is_some())
            .map(|p| p.id.clone())
            .collect();
        let reds_before: u32 = user_squad(&before).iter().map(|p| p.stats.red_cards).sum();
        play_interactive(state);
        let after = state.get_game(|g| g.clone()).expect("game");
        let appearances_after = appearances(&after);
        let arrived_with: Vec<u8> = user_squad(&before)
            .iter()
            .filter(|p| appearances_after.get(&p.id) > appearances_before.get(&p.id))
            .map(|p| p.condition)
            .collect();
        let played_while_injured = injured_ids
            .iter()
            .filter(|id| appearances_after.get(*id) > appearances_before.get(*id))
            .count();
        records.push(MatchRecord {
            starters_condition_before: mean_condition(&before),
            starters_condition_after: mean_condition(&after),
            arrived_with,
            injured_after: user_squad(&after)
                .iter()
                .filter(|p| p.injury.is_some())
                .count(),
            reds_after: user_squad(&after)
                .iter()
                .map(|p| p.stats.red_cards)
                .sum::<u32>()
                - reds_before,
            played_while_injured,
        });
    }
    records
}

fn career(seed: u64) -> StateManager {
    let game = WorldSpec::gate(seed).build().expect("a generated world");
    let state = StateManager::new();
    state.set_game(game);
    state
}

fn table_played(game: &Game) -> u32 {
    let team = game.manager.team_id.clone().unwrap_or_default();
    game.competitions
        .iter()
        .flat_map(|c| c.standings.iter())
        .filter(|s| s.team_id == team)
        .map(|s| s.played)
        .sum()
}

/// Given a generated world and the user's club,
/// When a season of its fixtures is played as interactive M12 matches, with a save and a reload
/// of the career after the tenth,
/// Then: every match is finalised once (the club's table row counts exactly the matches played),
/// nobody plays while injured, condition stays inside 0..100 and the squad average neither
/// collapses nor saturates, and the reloaded career plays the rest exactly as the uninterrupted one.
#[test]
#[ignore = "study: plays a season; minutes in release"]
fn a_season_of_interactive_matches_carries_everything_forward() {
    let seed = 7;
    // A: straight through
    let a = career(seed);
    let records = play_matches(&a, 38);
    let played = records.len();
    assert!(
        played >= 30,
        "a season has at least thirty league matches: {played}"
    );

    let game = a.get_game(|g| g.clone()).expect("game");
    // the club also plays cup ties: the table counts the league fixtures, once each
    let team = game.manager.team_id.clone().unwrap_or_default();
    let league_fixtures_completed = game
        .competitions
        .iter()
        .filter(|c| !c.standings.is_empty())
        .flat_map(|c| c.fixtures.iter())
        .filter(|f| {
            f.status == domain::league::FixtureStatus::Completed
                && f.counts_for_league_standings()
                && (f.home_team_id == team || f.away_team_id == team)
        })
        .count();
    assert_eq!(
        table_played(&game) as usize,
        league_fixtures_completed,
        "the table counts each completed league fixture of the club exactly once"
    );
    assert!(
        league_fixtures_completed >= 30,
        "a league season: {league_fixtures_completed}"
    );
    let faults = season_harness::lifecycle::standings_match_results(&game);
    assert!(
        faults.is_empty(),
        "every table row equals its results: {faults:?}"
    );
    let violators: usize = records.iter().map(|r| r.played_while_injured).sum();
    assert_eq!(violators, 0, "an injured player played");
    for p in user_squad(&game) {
        assert!(p.condition <= 100);
    }
    let low = records
        .iter()
        .map(|r| r.starters_condition_after)
        .fold(f64::MAX, f64::min);
    let mean_after = records
        .iter()
        .map(|r| r.starters_condition_after)
        .sum::<f64>()
        / played as f64;
    let mean_before = records
        .iter()
        .map(|r| r.starters_condition_before)
        .sum::<f64>()
        / played as f64;
    println!(
        "season: {played} matches; squad condition before {mean_before:.1}, after {mean_after:.1}, lowest after {low:.1}; injured after a match (mean) {:.2}; reds {}",
        records.iter().map(|r| r.injured_after as f64).sum::<f64>() / played as f64,
        records.iter().map(|r| r.reds_after).sum::<u32>()
    );
    // the players who play arrive tired when matches come thick, but the schedule does not wear
    // them down match after match: compare the first and the last third of the season
    let arrivals: Vec<f64> = records
        .iter()
        .map(|r| {
            r.arrived_with.iter().map(|c| f64::from(*c)).sum::<f64>()
                / r.arrived_with.len().max(1) as f64
        })
        .collect();
    let third = (played / 3).max(1);
    let early = arrivals[..third].iter().sum::<f64>() / third as f64;
    let late = arrivals[played - third..].iter().sum::<f64>() / third as f64;
    let worst = records
        .iter()
        .flat_map(|r| r.arrived_with.iter().copied())
        .min()
        .unwrap_or(100);
    println!(
        "players who played arrived with condition {early:.1} (first third of the season), {late:.1} (last third); lowest {worst}"
    );
    assert!(
        late > early - 15.0,
        "no spiral: arrivals {early:.1} -> {late:.1}"
    );
    assert!(worst >= 30, "nobody plays on empty: lowest arrival {worst}");
    assert!(
        low > 20.0,
        "no fatigue spiral: lowest squad average {low:.1}"
    );
    assert!(mean_before > mean_after, "a match costs condition");
    assert!(
        mean_before > 45.0,
        "recovery brings the squad back between matches"
    );

    // B: the same career, saved and reloaded after ten matches
    let directory = tempfile::TempDir::new().expect("dir");
    let mut saves = db::save_manager::SaveManager::init(directory.path()).expect("saves");
    let b = career(seed);
    let first = play_matches(&b, 10);
    assert_eq!(first.len(), 10);
    let save_id = b
        .get_game(|g| saves.create_save(g, "season").expect("save"))
        .expect("game");
    b.set_save_id(save_id.clone());
    crate::application::saving::persist_active_game(&b, &mut saves).expect("persist");
    let reloaded = StateManager::new();
    let loaded = saves.load_game(&save_id).expect("load");
    let stats = saves.load_stats_state(&save_id).expect("stats");
    crate::application::career::install_career(&reloaded, loaded, stats, Some(save_id));
    let rest = play_matches(&reloaded, 38 - 10);
    let game_b = reloaded.get_game(|g| g.clone()).expect("game");
    assert_eq!(
        table_played(&game_b),
        table_played(&game),
        "the reloaded career played the same number of matches"
    );
    let (fa, fb) = (
        season_harness::fingerprint::fingerprint(&game),
        season_harness::fingerprint::fingerprint(&game_b),
    );
    assert!(
        fa == fb,
        "save and reload in the middle of the season changes nothing: {}",
        season_harness::fingerprint::first_difference(&fa, &fb)
    );
    assert_eq!(first.len() + rest.len(), played);
}
