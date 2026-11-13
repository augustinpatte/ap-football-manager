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
    let club = state
        .get_game(|g| g.manager.team_id.clone())
        .flatten()
        .unwrap_or_default();
    while records.len() < matches {
        // a board can dismiss the manager: that is a career outcome, not a fault of the season
        if state
            .get_game(|g| g.manager.team_id.is_none())
            .unwrap_or(true)
        {
            let (played, points) = state
                .get_game(|g| {
                    g.competitions
                        .iter()
                        .flat_map(|c| c.standings.iter())
                        .filter(|s| s.team_id == club)
                        .fold((0, 0), |a, s| (a.0 + s.played, a.1 + s.points))
                })
                .unwrap_or_default();
            println!(
                "the manager was dismissed after {} matches ({played} league matches, {points} points)",
                records.len()
            );
            break;
        }
        guard += 1;
        if guard >= 600 {
            let game = state.get_game(|g| g.clone()).expect("game");
            let squad = user_squad(&game);
            panic!(
                "the season never ends: {} matches played, date {}, squad {} of whom {} can play, table rows played {}",
                records.len(),
                game.clock.current_date,
                squad.len(),
                squad.iter().filter(|p| p.can_play()).count(),
                table_played(&game)
            );
        }
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

/// What the squad's condition over the season must show: it falls in a match and recovers between matches,
/// the players who play do not arrive more tired as the season goes on, nobody plays on empty. Returns what
/// a match costs the squad on average (points of condition per match played).
fn check_condition(records: &[MatchRecord], game: &Game, label: &str) -> f64 {
    let played = records.len();
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
    report_congestion(records, label);
    let (world_goals, club_goals, _) = league_goals_per_match(game);
    println!(
        "[{label}] goals a league match: {club_goals:.2} in the club's matches (played by the engine), {world_goals:.2} over the whole world (mostly the statistical model outside the followed leagues)"
    );
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

    mean_before - mean_after
}

/// What a season of the user's club must keep true, whatever world and club it is played in; returns
/// the number of matches played and the game afterwards. `label` names the world in the output.
fn check_season(state: &StateManager, label: &str) -> (usize, Game, Option<f64>) {
    let records = play_matches(state, 38);
    let played = records.len();
    if state
        .get_game(|g| g.manager.team_id.is_none())
        .unwrap_or(true)
    {
        println!("[{label}] DISMISSED: the manager lost the club after {played} matches");
        return (played, state.get_game(|g| g.clone()).expect("game"), None);
    }
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
    // 38 matches of the club include its cup ties, so the league share varies with the cup run
    assert!(
        league_fixtures_completed >= 25,
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
    let cost = check_condition(&records, &game, label);
    (played, game, Some(cost))
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
        let mut costs: Vec<(&str, Option<f64>)> = Vec::new();
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
        if costs.iter().any(|c| c.1.is_none()) {
            println!("[seed {seed}] a plan ended in the manager's dismissal: no cost comparison for this seed (a career outcome, reported above)");
            continue;
        }
        let cost = |n: &str| {
            costs
                .iter()
                .find(|c| c.0 == n)
                .and_then(|c| c.1)
                .expect("a plan")
        };
        assert!(
            cost("high press") > cost("low block"),
            "a high press costs the squad more condition per match than a low block: {costs:?}"
        );
    }
}

/// Goals a league match, over every completed league fixture of the world (the user's club and the others).
fn league_goals_per_match(game: &Game) -> (f64, f64, usize) {
    let (mut goals, mut matches, mut user_goals, mut user_matches) = (0.0, 0usize, 0.0, 0usize);
    let team = game.manager.team_id.clone().unwrap_or_default();
    for fixture in game
        .competitions
        .iter()
        .filter(|c| !c.standings.is_empty())
        .flat_map(|c| c.fixtures.iter())
        .filter(|f| f.status == domain::league::FixtureStatus::Completed)
    {
        if let Some(r) = &fixture.result {
            goals += f64::from(r.home_goals) + f64::from(r.away_goals);
            matches += 1;
            if fixture.home_team_id == team || fixture.away_team_id == team {
                user_goals += f64::from(r.home_goals) + f64::from(r.away_goals);
                user_matches += 1;
            }
        }
    }
    (
        goals / matches.max(1) as f64,
        user_goals / user_matches.max(1) as f64,
        matches,
    )
}

/// Debug aid for a season that does not end: one plan, one seed.
#[test]
#[ignore = "debug aid: plays one season of one seed and prints its summary (F43_SEASON_SEED)"]
fn debug_one_season() {
    let seed: u64 = std::env::var("F43_SEASON_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(11);
    let state = career(seed);
    let (_, game, _) = check_season(&state, &format!("debug, seed {seed}"));
    let (all, user, n) = league_goals_per_match(&game);
    println!("goals a league match: {all:.2} over {n} completed fixtures of the world; {user:.2} in the user's club's matches");
}

/// What the engine reads of a player: the plain mean of twelve attributes the adapter passes to M12.
/// Deliberately not `ofm_core::player_rating` (position-weighted `ovr`, which the statistical model reads):
/// the study compares the two sources, so it must not measure them with the model's own yardstick.
fn engine_input_strength(p: &domain::player::Player) -> f64 {
    let a = &p.attributes;
    [
        a.pace,
        a.stamina,
        a.strength,
        a.passing,
        a.shooting,
        a.tackling,
        a.dribbling,
        a.defending,
        a.positioning,
        a.vision,
        a.decisions,
        a.composure,
    ]
    .iter()
    .map(|v| f64::from(*v))
    .sum::<f64>()
        / 12.0
}

/// (engine-input strength of the best eleven non-keepers, keeper rating) of a club. Not
/// `ofm_core::catchup::club_strength`, which is the statistical model's own measure and not public.
fn club_input_strength(game: &Game, team: &str) -> (f64, f64) {
    let mut outfield: Vec<f64> = Vec::new();
    let mut keepers: Vec<f64> = Vec::new();
    for p in game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team) && !p.retired)
    {
        if matches!(p.natural_position, domain::player::Position::Goalkeeper) {
            keepers
                .push((f64::from(p.attributes.reflexes) + f64::from(p.attributes.handling)) / 2.0);
        } else {
            outfield.push(engine_input_strength(p));
        }
    }
    outfield.sort_by(|a, b| b.total_cmp(a));
    keepers.sort_by(|a, b| b.total_cmp(a));
    let top = outfield.iter().take(11).sum::<f64>() / outfield.len().clamp(1, 11) as f64;
    (top, keepers.first().copied().unwrap_or(0.0))
}

/// F4.4 instrument: goals of every completed league fixture against the quality gap of its two clubs,
/// for the user's club's engine matches and for the world's statistical-model matches, side by side.
#[test]
#[ignore = "F4.4 study: plays a season per seed (F44_SEEDS, default 7,11,23)"]
fn scoring_response_by_quality_gap() {
    let seeds: Vec<u64> = std::env::var("F44_SEEDS")
        .unwrap_or_else(|_| "7,11,23".into())
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    // bins of (stronger side's rating - weaker side's rating): [0,2) [2,4) [4,7) [7,+)
    let edges = [0.0, 2.0, 4.0, 7.0, f64::MAX];
    // [source: 0 engine, 1 model][bin] -> (n, strong goals, weak goals, strong wins, draws)
    let mut acc = [[(0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64); 4]; 2];
    for seed in seeds {
        let state = career(seed);
        let (_, game, _) = check_season(&state, &format!("gap study, seed {seed}"));
        let team = game.manager.team_id.clone().unwrap_or_default();
        let mut ratings: Vec<(String, f64, f64)> = game
            .teams
            .iter()
            .map(|t| {
                let (o, k) = club_input_strength(&game, &t.id);
                (t.id.clone(), o, k)
            })
            .filter(|r| r.1 > 0.0)
            .collect();
        ratings.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mean = ratings.iter().map(|r| r.1).sum::<f64>() / ratings.len().max(1) as f64;
        let user = ratings.iter().position(|r| r.0 == team);
        println!(
            "[seed {seed}] {} clubs; outfield rating best {:.1}, median {:.1}, worst {:.1}, mean {mean:.1}; user rank {user:?}; keepers of the top 3: {:?}",
            ratings.len(),
            ratings[0].1,
            ratings[ratings.len() / 2].1,
            ratings[ratings.len() - 1].1,
            ratings.iter().take(3).map(|r| (r.2 * 10.0).round() / 10.0).collect::<Vec<_>>(),
        );
        let lookup = |id: &str| ratings.iter().find(|r| r.0 == id).map(|r| r.1);
        for fixture in game
            .competitions
            .iter()
            .filter(|c| !c.standings.is_empty())
            .flat_map(|c| c.fixtures.iter())
            .filter(|f| f.status == domain::league::FixtureStatus::Completed)
        {
            let (Some(r), Some(h), Some(a)) = (
                &fixture.result,
                lookup(&fixture.home_team_id),
                lookup(&fixture.away_team_id),
            ) else {
                continue;
            };
            let engine = usize::from(fixture.home_team_id != team && fixture.away_team_id != team);
            let (strong, weak) = if h >= a {
                (f64::from(r.home_goals), f64::from(r.away_goals))
            } else {
                (f64::from(r.away_goals), f64::from(r.home_goals))
            };
            let gap = (h - a).abs();
            let bin = edges
                .windows(2)
                .position(|w| gap >= w[0] && gap < w[1])
                .unwrap_or(3);
            let cell = &mut acc[engine][bin];
            cell.0 += 1.0;
            cell.1 += strong;
            cell.2 += weak;
            cell.3 += f64::from(u8::from(strong > weak));
            cell.4 += f64::from(u8::from(strong == weak));
        }
    }
    for (src, name) in [
        (0, "engine (user's club)"),
        (1, "statistical model (others)"),
    ] {
        for (bin, label) in ["gap 0-2", "gap 2-4", "gap 4-7", "gap 7+"]
            .iter()
            .enumerate()
        {
            let c = acc[src][bin];
            let n = c.0.max(1.0);
            println!(
                "{name:28} {label}: n={:4} strong {:.2} weak {:.2} total {:.2} win {:.0}% draw {:.0}%",
                c.0, c.1 / n, c.2 / n, (c.1 + c.2) / n, 100.0 * c.3 / n, 100.0 * c.4 / n
            );
        }
    }
}

/// F4.4 instrument: writes the M12 input of sampled scheduled league fixtures of a generated world
/// (F44_DUMP_DIR, F44_SEED, F44_SAMPLE) so the engine can be run on the exact career inputs offline.
#[test]
#[ignore = "F4.4 study: writes adapted match inputs to F44_DUMP_DIR"]
fn dump_adapted_career_inputs() {
    let dir = std::env::var("F44_DUMP_DIR").expect("F44_DUMP_DIR");
    let seed: u64 = std::env::var("F44_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7);
    let sample: usize = std::env::var("F44_SAMPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    std::fs::create_dir_all(&dir).expect("dump dir");
    let game = WorldSpec::gate(seed).build().expect("a generated world");
    let mut picks = Vec::new();
    for competition in game.competitions.iter().filter(|c| !c.standings.is_empty()) {
        for (index, fixture) in competition.fixtures.iter().enumerate() {
            if fixture.status == domain::league::FixtureStatus::Scheduled {
                picks.push((competition.id.clone(), index, fixture.id.clone()));
            }
        }
    }
    let step = (picks.len() / sample.max(1)).max(1);
    let mut written = 0;
    for (competition_id, index, fixture_id) in picks.into_iter().step_by(step).take(sample) {
        let Ok(adapted) = ofm_core::spatial_runner::adapt_selected_fixture(
            &game,
            index,
            &competition_id,
            &fixture_id,
        ) else {
            continue;
        };
        let json = serde_json::to_string(&adapted.input).expect("serialise the input");
        std::fs::write(format!("{dir}/{written:03}.json"), json).expect("write");
        written += 1;
    }
    println!("wrote {written} inputs of seed {seed} to {dir}");
}
