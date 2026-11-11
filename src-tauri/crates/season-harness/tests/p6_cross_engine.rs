//! P6: the same two sides in the five-zone engine and in M12.
//!
//! Two clubs of a freshly built world are given identical players (one rating, every attribute),
//! neutral tactics and full condition. Each formation then plays the same reference formation, home
//! and away, in each engine. Whatever differs between the rows is what the engine prices a *shape*
//! at; whatever differs between the engines is the gap between the football a person watches and
//! the football a career is played with.
//!
//! `P6_SEEDS=1 P6_MATCHES=200 P6_RATING=70 cargo test --release -p season-harness --test p6_cross_engine -- --ignored --nocapture`

use domain::team::{PlayStyle, TacticsPhaseSettings};
use ofm_core::game::Game;
use ofm_core::player_rating::FORMATIONS;
use season_harness::env;
use season_harness::match_study::{Football, SideResult, mean_se, play};

/// Every player of `club` at one rating, fit and uncontracted from the injury list.
fn make_uniform(game: &mut Game, club: &str, rating: u8) {
    for player in game
        .players
        .iter_mut()
        .filter(|p| p.team_id.as_deref() == Some(club))
    {
        let a = &mut player.attributes;
        for attribute in [
            &mut a.pace,
            &mut a.stamina,
            &mut a.strength,
            &mut a.agility,
            &mut a.passing,
            &mut a.shooting,
            &mut a.tackling,
            &mut a.dribbling,
            &mut a.defending,
            &mut a.positioning,
            &mut a.vision,
            &mut a.decisions,
            &mut a.composure,
            &mut a.aggression,
            &mut a.teamwork,
            &mut a.leadership,
            &mut a.handling,
            &mut a.reflexes,
            &mut a.aerial,
        ] {
            *attribute = rating;
        }
        player.ovr = rating;
        player.condition = 100;
        player.injury = None;
    }
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == club) {
        team.play_style = PlayStyle::Balanced;
        team.tactics_phase = TacticsPhaseSettings::default();
    }
}

fn set_formation(game: &mut Game, club: &str, formation: &str) {
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == club) {
        team.formation = formation.to_string();
        team.starting_xi_ids.clear();
    }
}

/// Two clubs of the first league that are not the user's.
fn two_clubs(game: &Game) -> (String, String) {
    let user = game.manager.team_id.clone();
    let league = game
        .competitions
        .iter()
        .find(|c| c.participant_ids.len() >= 4)
        .expect("the world has a league");
    let mut ids = league
        .participant_ids
        .iter()
        .filter(|id| Some(*id) != user.as_ref());
    (
        ids.next().expect("a first club").clone(),
        ids.next().expect("a second club").clone(),
    )
}

/// The subject's mean result against the reference over `matches`, home and away alternating.
fn subject_results(
    game: &mut Game,
    football: Football,
    subject: &str,
    reference: &str,
    matches: u64,
    seed: u64,
) -> Vec<SideResult> {
    (0..matches)
        .map(|n| {
            game.seed = seed.wrapping_mul(100_000).wrapping_add(n);
            if n % 2 == 0 {
                play(game, football, subject, reference).expect("plays").0
            } else {
                play(game, football, reference, subject).expect("plays").1
            }
        })
        .collect()
}

#[test]
#[ignore = "measurement study"]
fn p6_equal_players_both_engines() {
    let matches = env::first("P6_MATCHES", "200");
    let rating = env::first("P6_RATING", "70") as u8;
    // Unset: as many M12 matches as five-zone ones. 0: none (M12 costs about a second a match).
    let m12_matches = match std::env::var("P6_M12_MATCHES") {
        Ok(_) => env::first("P6_M12_MATCHES", "0"),
        Err(_) => matches,
    };
    // A comma list of formations to play, so a study can be split over processes.
    let only: Option<Vec<String>> = std::env::var("P6_SHAPES")
        .ok()
        .map(|list| list.split(',').map(|s| s.trim().to_string()).collect());
    for seed in env::list("P6_SEEDS", "1") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        let (a, b) = two_clubs(&game);
        make_uniform(&mut game, &a, rating);
        make_uniform(&mut game, &b, rating);
        set_formation(&mut game, &b, "4-4-2");
        eprintln!(
            "\n== world {seed}, {a} (subject) v {b} (4-4-2), players all {rating}: \
             {matches} five-zone / {m12_matches} M12 matches a row =="
        );
        eprintln!(
            "{:>8} | {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} | {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            "",
            "5z pts",
            "se",
            "gf",
            "ga",
            "shots",
            "poss",
            "M12pts",
            "se",
            "gf",
            "ga",
            "shots",
            "poss"
        );
        for formation in FORMATIONS
            .into_iter()
            .filter(|f| only.as_ref().is_none_or(|only| only.iter().any(|o| o == f)))
        {
            set_formation(&mut game, &a, formation);
            let five = subject_results(&mut game, Football::FiveZone, &a, &b, matches, seed);
            let m12 = subject_results(&mut game, Football::M12, &a, &b, m12_matches, seed);
            let cell = |results: &[SideResult]| {
                let mean = SideResult::mean(results);
                let (_, se) = mean_se(&results.iter().map(|r| r.points).collect::<Vec<_>>());
                (mean, se)
            };
            let (f, fse) = cell(&five);
            let (m, mse) = if m12.is_empty() {
                (SideResult::default(), f64::NAN)
            } else {
                cell(&m12)
            };
            eprintln!(
                "{:>8} | {:>6.3} {:>6.3} {:>6.2} {:>6.2} {:>6.1} {:>6.1} | {:>6.3} {:>6.3} {:>6.2} {:>6.2} {:>6.1} {:>6.1}",
                formation,
                f.points,
                fse,
                f.goals_for,
                f.goals_against,
                f.shots,
                f.possession,
                m.points,
                mse,
                m.goals_for,
                m.goals_against,
                m.shots,
                m.possession
            );
        }
    }
}
