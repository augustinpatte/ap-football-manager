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
    /// The day the match was played (days since the Unix epoch), for the rest between matches.
    day: i64,
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
            .filter(|p| !p.can_play())
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
            day: before.clock.current_date.timestamp() / 86_400,
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

/// The club's own rows of the league tables.
fn own_standing_rows(game: &Game) -> impl Iterator<Item = &domain::league::StandingEntry> {
    let team = game.manager.team_id.clone().unwrap_or_default();
    game.competitions
        .iter()
        .flat_map(|c| c.standings.iter())
        .filter(move |s| s.team_id == team)
}

fn table_played(game: &Game) -> u32 {
    own_standing_rows(game).map(|s| s.played).sum()
}

/// Scenario J: matches within four days of the last one against matches after a rest of six days or more,
/// by the condition the players who played arrived with.
fn report_congestion(records: &[MatchRecord], label: &str) {
    let arrival = |r: &MatchRecord| {
        r.arrived_with.iter().map(|c| f64::from(*c)).sum::<f64>()
            / r.arrived_with.len().max(1) as f64
    };
    let (mut short, mut long) = (Vec::new(), Vec::new());
    for pair in records.windows(2) {
        let gap = pair[1].day - pair[0].day;
        if gap <= 4 {
            short.push(arrival(&pair[1]));
        } else if gap >= 6 {
            long.push(arrival(&pair[1]));
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    println!(
        "[{label}] congestion: {} matches within 4 days of the last arrive with condition {:.1}; {} after a rest of 6 days or more arrive with {:.1}",
        short.len(),
        mean(&short),
        long.len(),
        mean(&long)
    );
}

/// What a season of the user's club must keep true, whatever world and club it is played in; returns
/// the number of matches played and the game afterwards. `label` names the world in the output.
fn check_season(state: &StateManager, label: &str) -> (usize, Game, f64) {
    let records = play_matches(state, 38);
    let played = records.len();
    assert!(
        played >= 30,
        "a season has at least thirty league matches: {played}"
    );

    let game = state.get_game(|g| g.clone()).expect("game");
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
    assert_eq!(violators, 0, "an injured or suspended player played");
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
        "[{label}] season: {played} matches; squad condition before {mean_before:.1}, after {mean_after:.1}, lowest after {low:.1}; injured after a match (mean) {:.2}; reds {}",
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
    report_congestion(&records, label);
    let worst = records
        .iter()
        .flat_map(|r| r.arrived_with.iter().copied())
        .min()
        .unwrap_or(100);
    println!(
        "[{label}] players who played arrived with condition {early:.1} (first third of the season), {late:.1} (last third); lowest {worst}"
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

    // what a match costs the squad, on average (points of condition per match played)
    (played, game, mean_before - mean_after)
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
    let (played, game, _) = check_season(&a, "generated, seed 7");

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

/// Given generated worlds of three different seeds, and (when `APFM_WORLD_JSON` names a copy of the FM24
/// world, which is only read) the FM24 world with two different clubs,
/// When a season is played in each as interactive matches,
/// Then every one keeps what a season must keep true: each fixture finalised once, the table equal
/// to the results, nobody playing while injured or suspended, condition that does not spiral.
#[test]
#[ignore = "study: plays several seasons; many minutes in release"]
fn seasons_hold_across_seeds_clubs_and_the_fm24_world() {
    for seed in [11, 23, 37] {
        let state = career(seed);
        check_season(&state, &format!("generated, seed {seed}"));
    }
    let Some(path) = season_harness::fm24::world_path_from_env() else {
        println!("APFM_WORLD_JSON is not set: the FM24 world is not played (stated limitation)");
        return;
    };
    // the strongest and a middling club of the world's first league, by the squads' own ratings
    let (world, _) = season_harness::fm24::build_fm24_world(&path, 5).expect("the FM24 world");
    let mut clubs: Vec<(f64, String)> = world
        .teams
        .iter()
        .map(|t| {
            let ovr: Vec<f64> = world
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(t.id.as_str()))
                .map(|p| f64::from(p.ovr))
                .collect();
            (
                ovr.iter().sum::<f64>() / ovr.len().max(1) as f64,
                t.id.clone(),
            )
        })
        .filter(|(o, _)| *o > 0.0)
        .collect();
    clubs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let picks = [clubs.first().cloned(), clubs.get(clubs.len() / 2).cloned()];
    for (rating, club) in picks.into_iter().flatten() {
        let game = season_harness::fm24::build_fm24_career(&path, &club, 5).expect("a career");
        let state = StateManager::new();
        state.set_game(game);
        check_season(&state, &format!("FM24, club {club} (rating {rating:.1})"));
    }
}

/// The club's own row of the league table after the season: (points, goals for, goals against).
fn table_row(game: &Game) -> (u32, u32, u32) {
    own_standing_rows(game).fold((0, 0, 0), |a, s| {
        (a.0 + s.points, a.1 + s.goals_for, a.2 + s.goals_against)
    })
}

/// Given one generated world and club, and the club's plan set to a low block, to the balanced
/// default and to a high press,
/// When the same season is played under each,
/// Then the table shows what each plan earned and the squad's arrival condition shows what it cost:
/// a press that wins points does so by wearing the squad down, a low block saves it.
#[test]
#[ignore = "study: plays three seasons; many minutes in release"]
fn a_high_press_pays_for_itself_in_condition_across_a_season() {
    use domain::team::{PlayStyle, PressingIntensity};
    let plans = [
        (
            "low block",
            PlayStyle::Defensive,
            PressingIntensity::Passive,
        ),
        ("balanced", PlayStyle::Balanced, PressingIntensity::Medium),
        (
            "high press",
            PlayStyle::HighPress,
            PressingIntensity::Aggressive,
        ),
    ];
    for seed in [11u64, 23] {
        println!("--- generated world, seed {seed}");
        let mut costs: Vec<(&str, f64)> = Vec::new();
        for (name, style, pressing) in &plans {
            let mut game = WorldSpec::gate(seed).build().expect("a generated world");
            let id = game.manager.team_id.clone().expect("a club");
            let team = game
                .teams
                .iter_mut()
                .find(|t| t.id == id)
                .expect("the club");
            team.play_style = style.clone();
            team.tactics_phase.pressing_intensity = pressing.clone();
            let state = StateManager::new();
            state.set_game(game);
            let (played, after, cost) = check_season(&state, &format!("{name}, seed {seed}"));
            costs.push((*name, cost));
            let (points, gf, ga) = table_row(&after);
            println!("[{name}, seed {seed}] {played} matches: {points} points, goals {gf}-{ga}");
        }
        let cost = |n: &str| {
            costs
                .iter()
                .find(|c| c.0 == n)
                .map(|c| c.1)
                .expect("a plan")
        };
        assert!(
            cost("high press") > cost("low block"),
            "a high press costs the squad more condition per match than a low block: {costs:?}"
        );
    }
}
