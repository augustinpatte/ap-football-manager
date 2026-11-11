//! P5: what a formation and a tactical dial do to a match when nothing else changes. A controlled
//! experiment, measurement only (`#[ignore]`d).
//!
//! `P5_SEEDS=1,2,3 P5_CLUBS=8 P5_PANEL=6 P5_REPS=4 cargo test --release -p season-harness --test p5_formation_matches -- --ignored --nocapture`
//!
//! Each subject club of a freshly built world plays the same panel of opponents, home and away, in
//! every one of the eight formations (then, in its best-scoring formation, with one tactical
//! dial moved at a time). The squads, the opponents, the venue and the opponents' own shapes never
//! change; only the subject's formation (or one dial) does, and each repetition re-seeds the
//! match. The matches are played by the engine every unwatched fixture of a career is played by
//! (`play_unwatched_fixture`), so what is measured is what a long career rewards.
//!
//! Uncertainty is stated per **club** (clubs are the independent units; the matches of one club
//! share its squad), and the worlds are the seeds. A table's standard error is the spread of the
//! per-club contrasts, never of the matches.

use std::collections::BTreeMap;

use domain::league::{CompetitionType, Fixture, FixtureStatus, League};
use domain::team::{DefensiveLine, PitchWidth, PlayStyle, PressingIntensity, Tempo};
use ofm_core::ai_formation::{freeze_reviews_for_study, read_all_clubs};
use ofm_core::game::Game;
use ofm_core::live_match_manager::play_unwatched_fixture;
use ofm_core::player_rating::FORMATIONS;
use season_harness::env;

/// One side of one match, as the study reads it.
#[derive(Clone, Copy, Default)]
struct Side {
    points: f64,
    goals_for: f64,
    goals_against: f64,
    shots: f64,
    on_target: f64,
    possession: f64,
}

fn side(goals_for: u8, goals_against: u8, shots: u16, on_target: u16, possession: f64) -> Side {
    Side {
        points: match goals_for.cmp(&goals_against) {
            std::cmp::Ordering::Greater => 3.0,
            std::cmp::Ordering::Equal => 1.0,
            std::cmp::Ordering::Less => 0.0,
        },
        goals_for: f64::from(goals_for),
        goals_against: f64::from(goals_against),
        shots: f64::from(shots),
        on_target: f64::from(on_target),
        possession,
    }
}

/// One match, as (the home side's, the away side's) numbers.
fn play(game: &mut Game, home: &str, away: &str) -> (Side, Side) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.league = Some(League {
        id: "p5-study".to_string(),
        name: "P5 study".to_string(),
        kind: CompetitionType::League,
        fixtures: vec![Fixture {
            id: "p5-fixture".to_string(),
            competition_id: "p5-study".to_string(),
            matchday: 1,
            date: today,
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            status: FixtureStatus::Scheduled,
            ..Fixture::default()
        }],
        ..League::default()
    });
    let report = play_unwatched_fixture(game, 0)
        .expect("the fixture is playable")
        .report;
    (
        side(
            report.home_goals,
            report.away_goals,
            report.home_stats.shots,
            report.home_stats.shots_on_target,
            report.home_possession,
        ),
        side(
            report.away_goals,
            report.home_goals,
            report.away_stats.shots,
            report.away_stats.shots_on_target,
            100.0 - report.home_possession,
        ),
    )
}

/// The mean of each field over `matches`.
fn mean(matches: &[Side]) -> Side {
    let n = matches.len() as f64;
    let mut total = Side::default();
    for side in matches {
        total.points += side.points;
        total.goals_for += side.goals_for;
        total.goals_against += side.goals_against;
        total.shots += side.shots;
        total.on_target += side.on_target;
        total.possession += side.possession;
    }
    Side {
        points: total.points / n,
        goals_for: total.goals_for / n,
        goals_against: total.goals_against / n,
        shots: total.shots / n,
        on_target: total.on_target / n,
        possession: total.possession / n,
    }
}

/// Mean and standard error (spread of the values over their count) of `values`.
fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let m = values.iter().sum::<f64>() / n;
    if values.len() < 2 {
        return (m, f64::NAN);
    }
    let var = values.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (n - 1.0);
    (m, (var / n).sqrt())
}

/// A club's whole schedule of study matches under the current settings of its team row: the
/// panel home and away, `reps` times, each repetition a different match.
fn season_of_matches(
    game: &mut Game,
    club: &str,
    panel: &[String],
    reps: u64,
    base_seed: u64,
) -> Vec<Side> {
    let mut sides = Vec::new();
    for rep in 0..reps {
        game.seed = base_seed.wrapping_mul(1_000).wrapping_add(rep);
        for opponent in panel {
            sides.push(play(game, club, opponent).0);
            sides.push(play(game, opponent, club).1);
        }
    }
    sides
}

/// One setting to try on a club, with its name.
type Variation<'a> = (&'a str, Box<dyn Fn(&mut domain::team::Team)>);

struct Subject {
    /// The squad model's score for each formation, in `FORMATIONS` order.
    scores: Vec<f64>,
    /// Per formation: the mean over the club's matches.
    by_formation: Vec<Side>,
}

fn set_formation(game: &mut Game, club: &str, formation: &str) {
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == club) {
        team.formation = formation.to_string();
        // The AI re-picks its eleven for every match; a saved one from another shape must not
        // follow the club into this one.
        team.starting_xi_ids.clear();
    }
}

/// The subject clubs and the panel of one league of `game`: subjects spread over the strength
/// order, the panel the clubs nearest the middle of it.
fn choose_clubs(game: &Game, subjects: usize, panel: usize) -> (Vec<String>, Vec<String>) {
    let user = game.manager.team_id.clone();
    let wanted = std::env::var("P5_LEAGUE").unwrap_or_default();
    let league = game
        .competitions
        .iter()
        .filter(|c| c.kind == CompetitionType::League && c.name.contains(&wanted))
        .max_by_key(|c| c.participant_ids.len())
        .expect("the world has a league");
    let mut strength: Vec<(String, f64)> = league
        .participant_ids
        .iter()
        .filter(|id| Some(*id) != user.as_ref())
        .map(|id| {
            let mut ovr: Vec<u8> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(id.as_str()))
                .map(|p| p.ovr)
                .collect();
            ovr.sort_unstable_by(|a, b| b.cmp(a));
            ovr.truncate(14);
            (
                id.clone(),
                ovr.iter().map(|o| f64::from(*o)).sum::<f64>() / ovr.len().max(1) as f64,
            )
        })
        .collect();
    strength.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let n = strength.len();
    let chosen: Vec<String> = if std::env::var("P5_MID_SUBJECTS").is_ok() {
        // The clubs a panel is made of play each other: the mid-table baseline.
        (0..subjects)
            .map(|k| strength[n / 2 - subjects / 2 + k].0.clone())
            .collect()
    } else {
        (0..subjects)
            .map(|k| strength[(k * (n - 1)) / (subjects - 1).max(1)].0.clone())
            .collect()
    };
    let middle = n / 2;
    let panel_ids: Vec<String> = strength
        .iter()
        .enumerate()
        .filter(|(index, _)| index.abs_diff(middle) <= panel.div_ceil(2))
        .map(|(_, (id, _))| id.clone())
        .take(panel + subjects)
        .collect();
    (chosen, panel_ids)
}

#[test]
#[ignore = "measurement study"]
fn p5_formation_matches() {
    let reps = env::first("P5_REPS", "4");
    let subject_count = env::first("P5_CLUBS", "8") as usize;
    let panel_size = env::first("P5_PANEL", "6") as usize;
    let mut all_subjects: Vec<(u64, Subject)> = Vec::new();
    let mut all_matches: Vec<Side> = Vec::new();
    let mut score_rows: Vec<(u64, String, String, Side)> = Vec::new();

    for seed in env::list("P5_SEEDS", "1") {
        let mut game = env::world("P5_WORLD", seed).build().expect("world builds");
        // Real match days are not opening days: the AI clubs have set the tactics their managers
        // play, players carry fatigue and injuries. Warm the world up by playing days.
        {
            use season_harness::driver::{RunOptions, SeasonObserver, play_day};
            struct Quiet;
            impl SeasonObserver for Quiet {}
            let options = RunOptions::seasons(1);
            for day in 1..=env::first("P5_WARMUP", "0") as u32 {
                play_day(&mut game, options, 1, day, &mut Quiet).expect("the day plays");
            }
        }
        if let Ok(reputation) = std::env::var("P5_REPUTATION") {
            // Every club at one standing, to see what the manager's standing does to a match.
            let reputation: u32 = reputation.parse().expect("P5_REPUTATION is a number");
            for team in &mut game.teams {
                team.reputation = reputation;
            }
        }
        if let Ok(condition) = std::env::var("P5_CONDITION") {
            // A match day mid-season, not an opening day: every player at this condition.
            let condition: u8 = condition.parse().expect("P5_CONDITION is a number");
            for player in &mut game.players {
                player.condition = condition;
            }
        }
        let _frozen = freeze_reviews_for_study();
        let readings = read_all_clubs(&game);
        let (subjects, panel_pool) = choose_clubs(&game, subject_count, panel_size);
        for club in &subjects {
            let panel: Vec<String> = panel_pool
                .iter()
                .filter(|id| *id != club)
                .take(panel_size)
                .cloned()
                .collect();
            let scores: Vec<f64> = FORMATIONS
                .iter()
                .map(|formation| {
                    readings
                        .iter()
                        .find(|r| &r.team_id == club)
                        .and_then(|r| r.scores.iter().find(|s| s.formation == *formation))
                        .map_or(f64::NAN, |s| s.squad)
                })
                .collect();
            let mut by_formation = Vec::new();
            for formation in FORMATIONS {
                set_formation(&mut game, club, formation);
                let matches = season_of_matches(&mut game, club, &panel, reps, seed);
                all_matches.extend(matches.iter().copied());
                by_formation.push(mean(&matches));
            }
            let best = scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(0, |(index, _)| index);
            // The tactical dials, one at a time, in the club's best-scoring formation. Each is
            // set on its own against the neutral setting it is measured against.
            set_formation(&mut game, club, FORMATIONS[best]);
            let neutral_style = game
                .teams
                .iter()
                .find(|t| &t.id == club)
                .map(|t| (t.play_style.clone(), t.tactics_phase.clone()))
                .expect("the club exists");
            let neutral = {
                let matches = season_of_matches(&mut game, club, &panel, reps, seed);
                mean(&matches)
            };
            let variations: Vec<Variation> = vec![
                (
                    "style Defensive",
                    Box::new(|t| t.play_style = PlayStyle::Defensive),
                ),
                (
                    "style Attacking",
                    Box::new(|t| t.play_style = PlayStyle::Attacking),
                ),
                (
                    "style Possession",
                    Box::new(|t| t.play_style = PlayStyle::Possession),
                ),
                (
                    "style Counter",
                    Box::new(|t| t.play_style = PlayStyle::Counter),
                ),
                (
                    "style HighPress",
                    Box::new(|t| t.play_style = PlayStyle::HighPress),
                ),
                (
                    "line VeryLow",
                    Box::new(|t| t.tactics_phase.defensive_line = DefensiveLine::VeryLow),
                ),
                (
                    "line High",
                    Box::new(|t| t.tactics_phase.defensive_line = DefensiveLine::High),
                ),
                (
                    "press Passive",
                    Box::new(|t| t.tactics_phase.pressing_intensity = PressingIntensity::Passive),
                ),
                (
                    "press Aggressive",
                    Box::new(|t| {
                        t.tactics_phase.pressing_intensity = PressingIntensity::Aggressive
                    }),
                ),
                (
                    "width Narrow",
                    Box::new(|t| t.tactics_phase.width = PitchWidth::Narrow),
                ),
                (
                    "width Wide",
                    Box::new(|t| t.tactics_phase.width = PitchWidth::Wide),
                ),
                (
                    "tempo Patient",
                    Box::new(|t| t.tactics_phase.tempo = Tempo::Patient),
                ),
            ];
            for (name, apply) in &variations {
                if let Some(team) = game.teams.iter_mut().find(|t| &t.id == club) {
                    team.play_style = neutral_style.0.clone();
                    team.tactics_phase = neutral_style.1.clone();
                    apply(team);
                }
                let matches = season_of_matches(&mut game, club, &panel, reps, seed);
                let m = mean(&matches);
                score_rows.push((
                    seed,
                    club.clone(),
                    (*name).to_string(),
                    Side {
                        points: m.points - neutral.points,
                        goals_for: m.goals_for - neutral.goals_for,
                        goals_against: m.goals_against - neutral.goals_against,
                        shots: m.shots - neutral.shots,
                        on_target: m.on_target - neutral.on_target,
                        possession: m.possession - neutral.possession,
                    },
                ));
            }
            if let Some(team) = game.teams.iter_mut().find(|t| &t.id == club) {
                team.play_style = neutral_style.0.clone();
                team.tactics_phase = neutral_style.1.clone();
            }
            all_subjects.push((
                seed,
                Subject {
                    scores,
                    by_formation,
                },
            ));
        }
    }

    report_formations(&all_subjects);
    report_slope(&all_subjects);
    report_score_distribution(&all_matches);
    report_dials(&score_rows);
}

fn report_formations(subjects: &[(u64, Subject)]) {
    eprintln!(
        "\n== Formation, club's own mean removed ({} subject clubs over {} worlds) ==",
        subjects.len(),
        subjects
            .iter()
            .map(|(s, _)| *s)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    eprintln!(
        "formation | ppg (se over clubs) | goals for | goals against | shots | on target | possession"
    );
    for (index, formation) in FORMATIONS.iter().enumerate() {
        let column = |field: fn(&Side) -> f64| -> Vec<f64> {
            subjects
                .iter()
                .map(|(_, s)| {
                    let club_mean =
                        s.by_formation.iter().map(field).sum::<f64>() / s.by_formation.len() as f64;
                    field(&s.by_formation[index]) - club_mean
                })
                .collect()
        };
        let (ppg, ppg_se) = mean_se(&column(|s| s.points));
        let cell = |field: fn(&Side) -> f64| mean_se(&column(field)).0;
        eprintln!(
            "{formation:<8} | {ppg:+.3} (±{ppg_se:.3}) | {:+.3} | {:+.3} | {:+.2} | {:+.2} | {:+.2}",
            cell(|s| s.goals_for),
            cell(|s| s.goals_against),
            cell(|s| s.shots),
            cell(|s| s.on_target),
            cell(|s| s.possession)
        );
    }
    // How much does the club's own choice of shape move its results at all? The spread of a
    // club's per-formation ppg, against what the matches' own noise would give.
    let spreads: Vec<f64> = subjects
        .iter()
        .map(|(_, s)| {
            let values: Vec<f64> = s.by_formation.iter().map(|m| m.points).collect();
            let (m, _) = mean_se(&values);
            (values.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (values.len() - 1) as f64)
                .sqrt()
        })
        .collect();
    let (spread, spread_se) = mean_se(&spreads);
    eprintln!(
        "a club's ppg varies by {spread:.3} (±{spread_se:.3}) between its eight shapes (one standard deviation, points a game)"
    );
}

/// The within-club slope of points on the squad model's score for the shape played, and the
/// agreement between the model's ranking of a club's shapes and the results'.
fn report_slope(subjects: &[(u64, Subject)]) {
    let mut slopes = Vec::new();
    let mut rank_agreement = Vec::new();
    for (_, s) in subjects {
        let usable: Vec<(f64, f64)> = s
            .scores
            .iter()
            .zip(&s.by_formation)
            .filter(|(score, _)| score.is_finite())
            .map(|(score, m)| (*score, m.points))
            .collect();
        if usable.len() < 4 {
            continue;
        }
        let n = usable.len() as f64;
        let mx = usable.iter().map(|u| u.0).sum::<f64>() / n;
        let my = usable.iter().map(|u| u.1).sum::<f64>() / n;
        let sxx = usable.iter().map(|u| (u.0 - mx) * (u.0 - mx)).sum::<f64>();
        let sxy = usable.iter().map(|u| (u.0 - mx) * (u.1 - my)).sum::<f64>();
        if sxx > 1e-9 {
            slopes.push(sxy / sxx);
        }
        let best_model = usable
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.0.total_cmp(&b.1.0))
            .map(|(i, _)| i);
        let best_result = usable
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .map(|(i, _)| i);
        rank_agreement.push(f64::from(u8::from(best_model == best_result)));
    }
    let (slope, se) = mean_se(&slopes);
    eprintln!(
        "\n== Does the squad model's score for a shape predict how the shape does? ==\nwithin-club slope: {slope:+.4} points a game per rating point of squad-model score (se over {} clubs ±{se:.4})",
        slopes.len()
    );
    let (agree, _) = mean_se(&rank_agreement);
    eprintln!(
        "the shape the model rates best is the shape with the best results for {:.0} % of clubs (chance would be {:.1} %)",
        agree * 100.0,
        100.0 / FORMATIONS.len() as f64
    );
}

fn report_score_distribution(matches: &[Side]) {
    let n = matches.len() as f64;
    let total = |s: &Side| s.goals_for + s.goals_against;
    let share =
        |test: &dyn Fn(&Side) -> bool| matches.iter().filter(|s| test(s)).count() as f64 / n;
    let mut histogram: BTreeMap<u32, usize> = BTreeMap::new();
    for side in matches {
        *histogram.entry(total(side) as u32).or_default() += 1;
    }
    eprintln!(
        "\n== Score distribution, all {} study matches (the subject's side of each) ==",
        matches.len()
    );
    eprintln!(
        "goals a match {:.2} | 0-0 {:.1} % | 5 or more goals {:.1} % | subject win/draw/loss {:.1}/{:.1}/{:.1} %",
        matches.iter().map(total).sum::<f64>() / n,
        share(&|s| total(s) == 0.0) * 100.0,
        share(&|s| total(s) >= 5.0) * 100.0,
        share(&|s| s.points == 3.0) * 100.0,
        share(&|s| s.points == 1.0) * 100.0,
        share(&|s| s.points == 0.0) * 100.0
    );
    let line: Vec<String> = histogram
        .iter()
        .map(|(goals, count)| format!("{goals}:{:.1}%", *count as f64 / n * 100.0))
        .collect();
    eprintln!("total goals histogram {}", line.join(" "));
}

fn report_dials(rows: &[(u64, String, String, Side)]) {
    eprintln!(
        "\n== One tactical dial at a time, in each club's best-scoring shape, against its neutral settings =="
    );
    eprintln!(
        "setting | d ppg (se over clubs) | d goals for | d goals against | d shots | d possession"
    );
    let mut names: Vec<&String> = rows.iter().map(|r| &r.2).collect();
    names.dedup();
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        if !seen.insert(name.clone()) {
            continue;
        }
        let of: Vec<&Side> = rows.iter().filter(|r| &r.2 == name).map(|r| &r.3).collect();
        let col = |field: fn(&Side) -> f64| of.iter().map(|s| field(s)).collect::<Vec<_>>();
        let (ppg, se) = mean_se(&col(|s| s.points));
        eprintln!(
            "{name:<18} | {ppg:+.3} (±{se:.3}) | {:+.3} | {:+.3} | {:+.2} | {:+.2}",
            mean_se(&col(|s| s.goals_for)).0,
            mean_se(&col(|s| s.goals_against)).0,
            mean_se(&col(|s| s.shots)).0,
            mean_se(&col(|s| s.possession)).0
        );
    }
}

/// What the same engine does over a whole season of its own league, for the study's matches to be
/// read against: goals a match and the shares of home wins, draws and away wins, from the
/// standings of every league table at the season's end.
#[test]
#[ignore = "measurement study"]
fn p5_reference_season() {
    use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};

    #[derive(Default)]
    struct Totals {
        matches: f64,
        goals: f64,
        draws: f64,
    }
    impl SeasonObserver for Totals {
        fn on_before_rollover(&mut self, game: &Game, _season: u32) {
            for league in game
                .competitions
                .iter()
                .filter(|c| c.kind == CompetitionType::League)
            {
                let (matches_before, goals_before) = (self.matches, self.goals);
                for entry in &league.standings {
                    // Each match is in two rows: a goal is one row's goal for, a match and a draw
                    // are counted in both.
                    self.matches += f64::from(entry.played) / 2.0;
                    self.goals += f64::from(entry.goals_for);
                    self.draws += f64::from(entry.drawn) / 2.0;
                }
                eprintln!(
                    "  {} ({} clubs): {:.2} goals a match",
                    league.name,
                    league.standings.len(),
                    (self.goals - goals_before) / (self.matches - matches_before).max(1.0)
                );
            }
        }
    }
    for seed in env::list("P5_SEEDS", "1") {
        let mut game = env::world("P5_WORLD", seed).build().expect("world builds");
        let mut totals = Totals::default();
        run_seasons(&mut game, RunOptions::seasons(1), &mut totals).expect("a season plays");
        eprintln!(
            "reference season, world {seed}: {:.0} league matches, {:.2} goals a match, {:.1} % drawn",
            totals.matches,
            totals.goals / totals.matches,
            totals.draws / totals.matches * 100.0
        );
    }
}

/// The same pairings played two ways: by `play_unwatched_fixture` on its own, and by the day's
/// sweep (`simulate_other_matches`), which is how a career plays every AI fixture. A study that
/// is meant to measure what a career rewards has to produce a career's goal rate.
#[test]
#[ignore = "measurement study"]
fn p5_probe_match_paths() {
    let seed = env::first("P5_SEEDS", "1");
    let mut base = env::world("P5_WORLD", seed).build().expect("world builds");
    {
        use season_harness::driver::{RunOptions, SeasonObserver, play_day};
        struct Quiet;
        impl SeasonObserver for Quiet {}
        for day in 1..=env::first("P5_WARMUP", "30") as u32 {
            play_day(&mut base, RunOptions::seasons(1), 1, day, &mut Quiet).expect("day plays");
        }
    }
    if let Ok(reputation) = std::env::var("P5_REPUTATION") {
        let reputation: u32 = reputation.parse().expect("P5_REPUTATION is a number");
        for team in &mut base.teams {
            team.reputation = reputation;
        }
    }
    let (subjects, pool) = choose_clubs(&base, 8, 20);
    let today = base.clock.current_date.format("%Y-%m-%d").to_string();
    let opponents = env::first("P5_PROBE_OPP", "4") as usize;
    // One shared game for every match, as the study plays them, instead of a clone for each.
    let mut shared = base.clone();
    let (mut direct, mut swept, mut n, mut shared_goals) = (0.0, 0.0, 0.0, 0.0);
    let mut by_gap: Vec<(f64, f64)> = Vec::new();
    let (mut shots, mut on_target) = (0.0, 0.0);
    for (index, home) in subjects.iter().enumerate() {
        for away in pool.iter().filter(|id| *id != home).take(opponents) {
            let mut game = base.clone();
            game.seed = 7_000 + index as u64;
            if let Ok(formation) = std::env::var("P5_SET") {
                set_formation(&mut game, home, &formation);
            }
            let (h, a) = play(&mut game, home, away);
            direct += h.goals_for + a.goals_for;
            shots += h.shots + a.shots;
            on_target += h.on_target + a.on_target;
            let strength = |id: &str| -> f64 {
                let mut ovr: Vec<u8> = base
                    .players
                    .iter()
                    .filter(|p| p.team_id.as_deref() == Some(id))
                    .map(|p| p.ovr)
                    .collect();
                ovr.sort_unstable_by(|a, b| b.cmp(a));
                ovr.truncate(14);
                ovr.iter().map(|o| f64::from(*o)).sum::<f64>() / ovr.len().max(1) as f64
            };
            by_gap.push((
                (strength(home) - strength(away)).abs(),
                h.goals_for + a.goals_for,
            ));
            shared.seed = 7_000 + index as u64;
            let (h2, a2) = play(&mut shared, home, away);
            shared_goals += h2.goals_for + a2.goals_for;
            // The sweep plays what is scheduled today in the legacy slot, then writes the result.
            let mut game = base.clone();
            game.seed = 7_000 + index as u64;
            game.league = Some(League {
                id: "p5-study".to_string(),
                name: "P5 study".to_string(),
                kind: CompetitionType::League,
                fixtures: vec![Fixture {
                    id: "p5-fixture".to_string(),
                    competition_id: "p5-study".to_string(),
                    matchday: 1,
                    date: today.clone(),
                    home_team_id: home.clone(),
                    away_team_id: away.clone(),
                    status: FixtureStatus::Scheduled,
                    ..Fixture::default()
                }],
                ..League::default()
            });
            ofm_core::turn::simulate_other_matches(&mut game, &today, None);
            let result = game
                .league
                .as_ref()
                .and_then(|l| l.fixtures[0].result.clone());
            if let Some(result) = result {
                swept += f64::from(result.home_goals) + f64::from(result.away_goals);
            }
            n += 1.0;
        }
    }
    by_gap.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (label, chunk) in ["smallest", "middle", "largest"]
        .iter()
        .zip(by_gap.chunks(by_gap.len().div_ceil(3)))
    {
        eprintln!(
            "strength gap, {label} third (gap {:.1} to {:.1}): {:.2} goals a match",
            chunk.first().map_or(0.0, |c| c.0),
            chunk.last().map_or(0.0, |c| c.0),
            chunk.iter().map(|c| c.1).sum::<f64>() / chunk.len() as f64
        );
    }
    eprintln!(
        "{n} identical pairings: {:.2} shots, {:.2} on target, {:.2} goals a match direct, {:.2} through the day's sweep, {:.2} on one shared game",
        shots / n,
        on_target / n,
        direct / n,
        swept / n,
        shared_goals / n
    );
}

/// How good each league's attackers, defenders and goalkeepers are on the attributes a shot is
/// resolved by, for the goal rate to be read against: the engine's shot accuracy asks only how
/// good the shooter is, so a league of better shooters scores more however good its defenders
/// are.
#[test]
#[ignore = "measurement study"]
fn p5_league_quality() {
    use domain::player::Position;
    for seed in env::list("P5_SEEDS", "1") {
        let game = env::world("P5_WORLD", seed).build().expect("world builds");
        eprintln!(
            "world {seed}: league | forwards (shoot+compose+decide)/3 | defenders (defend+tackle+position+aerial)/4 | keepers (handle+reflex+position)/3"
        );
        for league in game
            .competitions
            .iter()
            .filter(|c| c.kind == CompetitionType::League)
        {
            let mut shoot = Vec::new();
            let mut defend = Vec::new();
            let mut keep = Vec::new();
            for id in &league.participant_ids {
                let mut club: Vec<&domain::player::Player> = game
                    .players
                    .iter()
                    .filter(|p| p.team_id.as_deref() == Some(id.as_str()))
                    .collect();
                club.sort_by_key(|p| std::cmp::Reverse(p.ovr));
                for (position, count, out) in [
                    (Position::Forward, 3, &mut shoot),
                    (Position::Defender, 4, &mut defend),
                    (Position::Goalkeeper, 1, &mut keep),
                ] {
                    for p in club.iter().filter(|p| p.position == position).take(count) {
                        let a = &p.attributes;
                        let value = match position {
                            Position::Forward => {
                                (f64::from(a.shooting)
                                    + f64::from(a.composure)
                                    + f64::from(a.decisions))
                                    / 3.0
                            }
                            Position::Defender => {
                                (f64::from(a.defending)
                                    + f64::from(a.tackling)
                                    + f64::from(a.positioning)
                                    + f64::from(a.aerial))
                                    / 4.0
                            }
                            _ => {
                                (f64::from(a.handling)
                                    + f64::from(a.reflexes)
                                    + f64::from(a.positioning))
                                    / 3.0
                            }
                        };
                        out.push(value);
                    }
                }
            }
            let m = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len().max(1) as f64;
            eprintln!(
                "  {:<28} | {:.1} | {:.1} | {:.1}",
                league.name,
                m(&shoot),
                m(&defend),
                m(&keep)
            );
        }
    }
}

/// Does a manager who changes shape for the players he has beat one who does not? The same club,
/// the same panel, with its two best defenders (then midfielders, then forwards) out: in its own
/// shape, in the shape the match-day rule picks, and in each of the eight (the best of which is
/// the most any rule could have found, selected on the very matches it is measured on, so an
/// upper bound that flatters it).
#[test]
#[ignore = "measurement study"]
fn p5_absence_formation() {
    use domain::player::{Injury, Position};
    use ofm_core::matchday_formation::shape_for_the_day;

    let reps = env::first("P5_REPS", "8");
    let mut rows: Vec<(String, f64, f64, f64, bool)> = Vec::new();
    for seed in env::list("P5_SEEDS", "1") {
        let mut game = env::world("P5_WORLD", seed).build().expect("world builds");
        {
            use season_harness::driver::{RunOptions, SeasonObserver, play_day};
            struct Quiet;
            impl SeasonObserver for Quiet {}
            for day in 1..=env::first("P5_WARMUP", "60") as u32 {
                play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("day");
            }
        }
        let _frozen = freeze_reviews_for_study();
        let (subjects, pool) = choose_clubs(&game, env::first("P5_CLUBS", "8") as usize, 6);
        for club in &subjects {
            let panel: Vec<String> = pool
                .iter()
                .filter(|id| *id != club)
                .take(6)
                .cloned()
                .collect();
            for (label, group) in [
                ("defenders out", Position::Defender),
                ("midfielders out", Position::Midfielder),
                ("forwards out", Position::Forward),
            ] {
                // The two best of the group are out, for as long as the scenario lasts.
                let mut squad: Vec<(u8, String)> = game
                    .players
                    .iter()
                    .filter(|p| p.team_id.as_deref() == Some(club.as_str()) && p.position == group)
                    .map(|p| (p.ovr, p.id.clone()))
                    .collect();
                squad.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                let out: Vec<String> = squad.iter().take(2).map(|p| p.1.clone()).collect();
                let before: Vec<(String, Option<Injury>)> = game
                    .players
                    .iter()
                    .filter(|p| out.contains(&p.id))
                    .map(|p| (p.id.clone(), p.injury.clone()))
                    .collect();
                for player in game.players.iter_mut().filter(|p| out.contains(&p.id)) {
                    player.injury = Some(Injury {
                        name: "Study".to_string(),
                        days_remaining: 30,
                    });
                }
                let own = game
                    .teams
                    .iter()
                    .find(|t| &t.id == club)
                    .map(|t| t.formation.clone())
                    .expect("club exists");
                let chosen = shape_for_the_day(&game, club);
                let ppg_in = |game: &mut Game, formation: &str| -> f64 {
                    set_formation(game, club, formation);
                    mean(&season_of_matches(game, club, &panel, reps, seed)).points
                };
                let own_ppg = ppg_in(&mut game, &own);
                let rule_ppg = if chosen == own {
                    own_ppg
                } else {
                    ppg_in(&mut game, &chosen)
                };
                let best = FORMATIONS
                    .iter()
                    .map(|f| ppg_in(&mut game, f))
                    .fold(f64::MIN, f64::max);
                set_formation(&mut game, club, &own);
                for (id, injury) in before {
                    if let Some(player) = game.players.iter_mut().find(|p| p.id == id) {
                        player.injury = injury;
                    }
                }
                rows.push((label.to_string(), own_ppg, rule_ppg, best, chosen != own));
            }
        }
    }
    eprintln!("\n== Players missing: the shape for the day against the club's own ==");
    eprintln!("scenario | share changed | rule - own ppg (se over clubs) | best of eight - own");
    for label in ["defenders out", "midfielders out", "forwards out"] {
        let of: Vec<&(String, f64, f64, f64, bool)> =
            rows.iter().filter(|r| r.0 == label).collect();
        let gain: Vec<f64> = of.iter().map(|r| r.2 - r.1).collect();
        let (g, se) = mean_se(&gain);
        let changed = of.iter().filter(|r| r.4).count() as f64 / of.len() as f64;
        let upper = of.iter().map(|r| r.3 - r.1).sum::<f64>() / of.len() as f64;
        eprintln!(
            "{label:<16} | {:.0} % | {g:+.3} (±{se:.3}) | {upper:+.3}   ({} club-scenarios)",
            changed * 100.0,
            of.len()
        );
        let changed_only: Vec<f64> = of.iter().filter(|r| r.4).map(|r| r.2 - r.1).collect();
        if changed_only.len() > 1 {
            let (g, se) = mean_se(&changed_only);
            eprintln!(
                "{:<16} |   where it changed: {g:+.3} (±{se:.3}), {} cases",
                "",
                changed_only.len()
            );
        }
    }
}
