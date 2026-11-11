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

/// The goal-rate gap between the match a person watches and the one a career is played with, on
/// real squads: the same fixtures of a warmed-up world, through both engines. Reports goals, shots,
/// draws and possession, and how each engine's goals follow the strength gap between the sides.
#[test]
#[ignore = "measurement study"]
fn p6_goal_rate_both_engines() {
    use season_harness::driver::{RunOptions, SeasonObserver, play_day};
    struct Quiet;
    impl SeasonObserver for Quiet {}

    let pairings = env::first("P6_PAIRINGS", "150") as usize;
    let warmup = env::first("P6_WARMUP", "60") as u32;
    let mut rows: Vec<(f64, [SideResult; 2], [SideResult; 2])> = Vec::new();
    for seed in env::list("P6_SEEDS", "1") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=warmup {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let user = game.manager.team_id.clone();
        let league = game
            .competitions
            .iter()
            .filter(|c| {
                c.kind == domain::league::CompetitionType::League && c.participant_ids.len() >= 16
            })
            .max_by_key(|c| c.participant_ids.len())
            .expect("a league")
            .clone();
        let clubs: Vec<String> = league
            .participant_ids
            .iter()
            .filter(|id| Some(*id) != user.as_ref())
            .cloned()
            .collect();
        let top14 = |game: &Game, club: &str| {
            let mut ovr: Vec<f64> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(club))
                .map(|p| f64::from(p.ovr))
                .collect();
            ovr.sort_by(|a, b| b.total_cmp(a));
            ovr.truncate(14);
            ovr.iter().sum::<f64>() / ovr.len().max(1) as f64
        };
        for n in 0..pairings {
            let home = &clubs[n % clubs.len()];
            let away = &clubs[(n * 7 + 3) % clubs.len()];
            if home == away {
                continue;
            }
            game.seed = seed * 1_000_000 + n as u64;
            let gap = top14(&game, home) - top14(&game, away);
            let mut five_game = game.clone();
            let five = play(&mut five_game, Football::FiveZone, home, away).expect("five-zone");
            let mut m12_game = game.clone();
            let m12 = play(&mut m12_game, Football::M12, home, away).expect("m12");
            rows.push((gap, [five.0, five.1], [m12.0, m12.1]));
        }
    }
    let report = |name: &str,
                  pick: &dyn Fn(&(f64, [SideResult; 2], [SideResult; 2])) -> [SideResult; 2],
                  rows: &[&(f64, [SideResult; 2], [SideResult; 2])]| {
        let n = rows.len() as f64;
        let goals: Vec<f64> = rows
            .iter()
            .map(|r| pick(r)[0].goals_for + pick(r)[1].goals_for)
            .collect();
        let (mean_goals, goals_se) = mean_se(&goals);
        let draws = rows
            .iter()
            .filter(|r| pick(r)[0].goals_for == pick(r)[1].goals_for)
            .count() as f64
            / n;
        let home_pts = rows.iter().map(|r| pick(r)[0].points).sum::<f64>() / n;
        let shots = rows
            .iter()
            .map(|r| pick(r)[0].shots + pick(r)[1].shots)
            .sum::<f64>()
            / n;
        let on_target = rows
            .iter()
            .map(|r| pick(r)[0].on_target + pick(r)[1].on_target)
            .sum::<f64>()
            / n;
        let home_poss = rows.iter().map(|r| pick(r)[0].possession).sum::<f64>() / n;
        eprintln!(
            "  {name:>9}: n {:>4} | goals {mean_goals:.2} (se {goals_se:.2}) | draws {:>4.1}% | home pts {home_pts:.2} | shots {shots:.1} on target {on_target:.1} | home possession {home_poss:.1}",
            rows.len(),
            draws * 100.0
        );
    };
    eprintln!(
        "\n== goal rate on real squads, same fixtures, {} pairings ==",
        rows.len()
    );
    let all: Vec<&_> = rows.iter().collect();
    report("five-zone", &|r| r.1, &all);
    report("M12", &|r| r.2, &all);
    let mut sorted: Vec<&_> = rows.iter().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (label, chunk) in ["home weaker third", "level third", "home stronger third"]
        .iter()
        .zip(sorted.chunks(sorted.len().div_ceil(3).max(1)))
    {
        eprintln!(
            "-- {label} (gap {:+.1} to {:+.1}) --",
            chunk[0].0,
            chunk[chunk.len() - 1].0
        );
        report("five-zone", &|r| r.1, chunk);
        report("M12", &|r| r.2, chunk);
    }
}

/// How much the squad model's score for a club varies across the eight formations, which is the
/// whole range a "formation suitability" slope has to work with.
#[test]
#[ignore = "measurement study"]
fn p6_formation_score_spread() {
    use ofm_core::ai_formation::{freeze_reviews_for_study, read_all_clubs};
    let mut spreads: Vec<f64> = Vec::new();
    let mut ranges: Vec<f64> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let game = env::world("P6_WORLD", seed).build().expect("world builds");
        let _frozen = freeze_reviews_for_study();
        for reading in read_all_clubs(&game) {
            let scores: Vec<f64> = reading.scores.iter().map(|s| s.squad).collect();
            let (_, se) = mean_se(&scores);
            spreads.push(se * (scores.len() as f64).sqrt());
            let (lo, hi) = scores
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), s| (lo.min(*s), hi.max(*s)));
            ranges.push(hi - lo);
        }
    }
    let (sd, _) = mean_se(&spreads);
    let (range, _) = mean_se(&ranges);
    eprintln!(
        "\n== squad-model score across a club's eight formations ({} clubs) ==\nmean within-club standard deviation {sd:.3} rating points, mean range {range:.3}",
        spreads.len()
    );
}

/// Within a club, across its eight formations, which number predicts its points a match: the
/// squad model's score for the shape, or the engine's own ratings of the side the shape fields?
/// The slope is per rating point; its standard error is the spread over clubs.
#[test]
#[ignore = "measurement study"]
fn p6_what_predicts_a_formation() {
    use ofm_core::ai_formation::{freeze_reviews_for_study, read_all_clubs};
    use season_harness::driver::{RunOptions, SeasonObserver, play_day};
    use season_harness::match_study::engine_ratings;
    struct Quiet;
    impl SeasonObserver for Quiet {}

    let reps = env::first("P6_REPS", "8");
    let clubs_per_world = env::first("P6_CLUBS", "8") as usize;
    let panel_size = 6usize;
    // One row per (club, formation): squad score, attack, midfield, defence, points a match.
    let mut by_club: Vec<Vec<[f64; 5]>> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=env::first("P6_WARMUP", "60") as u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let _frozen = freeze_reviews_for_study();
        let readings = read_all_clubs(&game);
        let user = game.manager.team_id.clone();
        let league = game
            .competitions
            .iter()
            .filter(|c| {
                c.kind == domain::league::CompetitionType::League && c.participant_ids.len() >= 16
            })
            .max_by_key(|c| c.participant_ids.len())
            .expect("a league")
            .clone();
        let ids: Vec<String> = league
            .participant_ids
            .iter()
            .filter(|id| Some(*id) != user.as_ref())
            .cloned()
            .collect();
        let step = (ids.len() / clubs_per_world).max(1);
        for subject in ids.iter().step_by(step).take(clubs_per_world) {
            let panel: Vec<&String> = ids
                .iter()
                .filter(|id| *id != subject)
                .skip(ids.len() / 2 - panel_size / 2)
                .take(panel_size)
                .collect();
            let mut rows = Vec::new();
            for formation in FORMATIONS {
                set_formation(&mut game, subject, formation);
                let score = readings
                    .iter()
                    .find(|r| &r.team_id == subject)
                    .and_then(|r| r.scores.iter().find(|s| s.formation == formation))
                    .map_or(f64::NAN, |s| s.squad);
                let ratings = engine_ratings(&mut game, subject, panel[0]).unwrap_or_else(|error| {
                    let seniors = game
                        .players
                        .iter()
                        .filter(|p| p.team_id.as_deref() == Some(subject.as_str()))
                        .count();
                    panic!("ratings for {subject} in {formation} v {}: {error} ({seniors} players)", panel[0])
                });
                let mut points = Vec::new();
                for rep in 0..reps {
                    game.seed = seed * 100_000 + rep;
                    for opponent in &panel {
                        points.push(
                            play(&mut game, Football::FiveZone, subject, opponent)
                                .expect("plays")
                                .0
                                .points,
                        );
                        points.push(
                            play(&mut game, Football::FiveZone, opponent, subject)
                                .expect("plays")
                                .1
                                .points,
                        );
                    }
                }
                rows.push([
                    score,
                    ratings.attack,
                    ratings.midfield,
                    ratings.defence,
                    points.iter().sum::<f64>() / points.len() as f64,
                ]);
            }
            by_club.push(rows);
        }
    }
    // Within-club slope of points on a column, over the eight shapes.
    let slope = |rows: &[[f64; 5]], column: &dyn Fn(&[f64; 5]) -> f64| {
        let n = rows.len() as f64;
        let mx = rows.iter().map(column).sum::<f64>() / n;
        let my = rows.iter().map(|r| r[4]).sum::<f64>() / n;
        let sxy: f64 = rows.iter().map(|r| (column(r) - mx) * (r[4] - my)).sum();
        let sxx: f64 = rows.iter().map(|r| (column(r) - mx).powi(2)).sum();
        if sxx > 1e-9 { sxy / sxx } else { 0.0 }
    };
    eprintln!(
        "\n== within-club slope of points a match on each number, over {} clubs x 8 shapes ==",
        by_club.len()
    );
    let columns: [(&str, Box<dyn Fn(&[f64; 5]) -> f64>); 5] = [
        ("squad-model score", Box::new(|r| r[0])),
        ("engine attack", Box::new(|r| r[1])),
        ("engine midfield", Box::new(|r| r[2])),
        ("engine defence", Box::new(|r| r[3])),
        ("engine a+m+d", Box::new(|r| r[1] + r[2] + r[3])),
    ];
    for (name, column) in &columns {
        let slopes: Vec<f64> = by_club
            .iter()
            .map(|rows| slope(rows, column.as_ref()))
            .collect();
        let (m, se) = mean_se(&slopes);
        let sd: Vec<f64> = by_club
            .iter()
            .map(|rows| {
                let values: Vec<f64> = rows.iter().map(|r| column(r)).collect();
                let (_, se) = mean_se(&values);
                se * (values.len() as f64).sqrt()
            })
            .collect();
        let (spread, _) = mean_se(&sd);
        eprintln!(
            "  {name:>18}: slope {m:+.4} (se {se:.4}) | within-club sd of the number {spread:.2}"
        );
    }
}
