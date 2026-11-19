//! I5: a career with the tactical intelligence on, season by season, and what its managers do in a
//! match. Measurement only (`#[ignore]`d): nothing here changes how the game plays.
//!
//! ```text
//! P6_WORLD=gate P6_SEEDS=1,2,3 I5_SEASONS=5 \
//!   cargo test --release -p season-harness --test i5_career -- --ignored --nocapture
//! ```
//!
//! The first test plays seasons exactly as a career does (`run_seasons`), with the AI's weekly
//! formation review, its tactical plans and the match-day adjustments all on, and reads two things:
//!
//! - **what a league's matches look like** from the game's own match statistics (goals, shots, on
//!   target, possession, draws, home advantage, red cards). Every applied league match is a record
//!   in `StatsState::team_matches`, played by the same engine as an unwatched career fixture;
//! - **what the managers do to their shapes**: how many formations the AI clubs play, how much of
//!   the league the commonest shape holds, how many clubs change shape between seasons, and how many
//!   distinct tactical plans are in use.
//!
//! The second test plays a panel of real league matches and reads the reports for what a match
//! manager does when a match gets away from him: substitutions, red cards, and the goals that
//! arrive after the hour.
//!
//! Uncertainty is over **worlds** (the independent units), never over matches or clubs: one world's
//! clubs share a payroll, a fixture list and a level of talent.
//!
//! Against the I4 baseline these two runs are the comparison: the same command at the commit the
//! branch was forked from, in a separate worktree, prints the same columns.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use domain::stats::StatsState;
use ofm_core::ai_formation::freeze_reviews_for_study;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::env;
use season_harness::match_study::{mean_se, play_fixture};

struct Quiet;
impl SeasonObserver for Quiet {}

/// One season's league matches, summed over every league the world applies.
#[derive(Debug, Clone, Copy, Default)]
struct Matches {
    fixtures: f64,
    goals: f64,
    shots: f64,
    on_target: f64,
    home_goals: f64,
    away_goals: f64,
    home_possession: f64,
    possession_readings: f64,
    draws: f64,
    home_points: f64,
    red_cards: f64,
}

impl Matches {
    fn per_match(&self, value: f64) -> f64 {
        value / self.fixtures.max(1.0)
    }

    fn absorb(&mut self, other: &Matches) {
        self.fixtures += other.fixtures;
        self.goals += other.goals;
        self.shots += other.shots;
        self.on_target += other.on_target;
        self.home_goals += other.home_goals;
        self.away_goals += other.away_goals;
        self.home_possession += other.home_possession;
        self.possession_readings += other.possession_readings;
        self.draws += other.draws;
        self.home_points += other.home_points;
        self.red_cards += other.red_cards;
    }
}

/// What one season of one world is worth to this study.
#[derive(Debug, Clone, Default)]
struct SeasonRow {
    season: u32,
    formations: usize,
    commonest_formation: String,
    commonest_formation_share: f64,
    formation_changes: usize,
    tactical_plans: usize,
    seconds: f64,
    matches: Matches,
}

/// Watches a career and reads each season as it closes.
struct Reader {
    /// The matches captured since the last day ended, which belong to the day's season. A statistics
    /// line is dated the competition's own season (a year), not the run's season number, so the
    /// driver's number is the key and the captures are folded into it when the day closes.
    pending: Matches,
    /// League matches by the run's season number.
    matches: BTreeMap<u32, Matches>,
    /// The shape each AI club played last season, to count changes.
    formation_of: HashMap<String, String>,
    previous_formation_of: HashMap<String, String>,
    /// Every club-season of the measured seasons, by the shape the club played.
    shapes: BTreeMap<String, usize>,
    rows: Vec<SeasonRow>,
    started: Instant,
}

impl Reader {
    fn new() -> Self {
        Self {
            pending: Matches::default(),
            matches: BTreeMap::new(),
            formation_of: HashMap::new(),
            previous_formation_of: HashMap::new(),
            shapes: BTreeMap::new(),
            rows: Vec::new(),
            started: Instant::now(),
        }
    }

    fn ai_clubs(game: &Game) -> impl Iterator<Item = &domain::team::Team> {
        let user = game.manager.team_id.clone();
        game.teams
            .iter()
            .filter(move |team| Some(&team.id) != user.as_ref())
    }
}

impl SeasonObserver for Reader {
    fn on_day_end(&mut self, _game: &Game, season: u32, _day: u32) {
        let pending = std::mem::take(&mut self.pending);
        self.matches.entry(season).or_default().absorb(&pending);
    }

    fn on_stats_captured(&mut self, stats: &StatsState) {
        let day = &mut self.pending;
        for record in &stats.team_matches {
            if record.competition != domain::league::FixtureCompetition::League {
                continue;
            }
            // Each fixture writes two lines, one per club: only the home club's line counts the
            // fixture, its goals for both, and its share of the ball.
            if record.team_id != record.home_team_id {
                day.red_cards += f64::from(record.red_cards);
                day.shots += f64::from(record.shots);
                day.on_target += f64::from(record.shots_on_target);
                continue;
            }
            day.fixtures += 1.0;
            day.goals += f64::from(record.goals_for) + f64::from(record.goals_against);
            day.home_goals += f64::from(record.goals_for);
            day.away_goals += f64::from(record.goals_against);
            day.shots += f64::from(record.shots);
            day.on_target += f64::from(record.shots_on_target);
            day.home_possession += f64::from(record.possession_pct);
            day.possession_readings += 1.0;
            day.red_cards += f64::from(record.red_cards);
            if record.goals_for == record.goals_against {
                day.draws += 1.0;
            }
            day.home_points += match record.goals_for.cmp(&record.goals_against) {
                std::cmp::Ordering::Greater => 3.0,
                std::cmp::Ordering::Equal => 1.0,
                std::cmp::Ordering::Less => 0.0,
            };
        }
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let clubs: Vec<_> = Self::ai_clubs(game).collect();
        let mut formations: BTreeMap<&str, usize> = BTreeMap::new();
        let mut plans = BTreeMap::new();
        let mut changes = 0usize;
        for team in &clubs {
            *formations.entry(team.formation.as_str()).or_default() += 1;
            plans.insert(format!("{:?}", team.tactics_phase), ());
            let before = self.previous_formation_of.get(&team.id);
            if before.is_some_and(|before| *before != team.formation) {
                changes += 1;
            }
            self.formation_of
                .insert(team.id.clone(), team.formation.clone());
        }
        for (shape, count) in &formations {
            *self.shapes.entry((*shape).to_string()).or_default() += *count;
        }
        let matches = self.matches.get(&season).copied().unwrap_or_default();
        let commonest = formations
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(shape, _)| (*shape).to_string())
            .unwrap_or_default();
        self.rows.push(SeasonRow {
            season,
            formations: formations.len(),
            commonest_formation: commonest,
            commonest_formation_share: formations.values().copied().max().unwrap_or(0) as f64
                / clubs.len().max(1) as f64,
            formation_changes: changes,
            tactical_plans: plans.len(),
            seconds: self.started.elapsed().as_secs_f64(),
            matches,
        });
        self.previous_formation_of = self.formation_of.clone();
        self.started = Instant::now();
    }
}

#[test]
#[ignore = "measurement study"]
fn a_career_season_by_season() {
    let seasons = env::first("I5_SEASONS", "5") as u32;
    let mut per_world: Vec<Vec<SeasonRow>> = Vec::new();
    let mut shapes_per_world: Vec<BTreeMap<String, usize>> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        let mut reader = Reader::new();
        // The warm-up season is the world settling (contracts, squads, the first reviews), not a
        // measurement: every study that reads a world reads it after it.
        reader.started = Instant::now();
        run_seasons(&mut game, RunOptions::seasons(seasons + 1), &mut reader)
            .expect("the career plays");
        let Reader {
            mut rows, shapes, ..
        } = reader;
        rows.remove(0);
        per_world.push(rows);
        shapes_per_world.push(shapes);
    }
    println!(
        "\n== a career with the tactical intelligence on: {} worlds, {seasons} measured seasons ==",
        per_world.len()
    );
    println!(
        "{:<6}{:<8}{:>7} {:<9}{:>7}{:>7}{:>7}{:>7}{:>8}{:>8}{:>8}{:>8}{:>7}{:>8}{:>6}",
        "world",
        "season",
        "shapes",
        "commonest",
        "share%",
        "chg",
        "plans",
        "sec",
        "goals",
        "shots",
        "on tgt",
        "poss%",
        "draws",
        "home pt",
        "red"
    );
    for (world, rows) in per_world.iter().enumerate() {
        for row in rows {
            let m = &row.matches;
            println!(
                "{:<6}{:<8}{:>7} {:<9}{:>7.0}{:>7}{:>7}{:>7.0}{:>8.3}{:>8.2}{:>8.2}{:>8.2}{:>7.3}{:>8.3}{:>6.2}",
                world + 1,
                row.season,
                row.formations,
                row.commonest_formation,
                100.0 * row.commonest_formation_share,
                row.formation_changes,
                row.tactical_plans,
                row.seconds,
                m.per_match(m.goals),
                m.per_match(m.shots),
                m.per_match(m.on_target),
                m.home_possession / m.possession_readings.max(1.0),
                m.per_match(m.draws),
                m.per_match(m.home_points),
                m.per_match(m.red_cards),
            );
        }
    }
    // The shapes the measured seasons were played in, over every world: a club-season each.
    let mut shapes: BTreeMap<&str, usize> = BTreeMap::new();
    for world in &shapes_per_world {
        for (shape, count) in world {
            *shapes.entry(shape.as_str()).or_default() += count;
        }
    }
    let club_seasons: usize = shapes.values().sum();
    let mut ordered: Vec<(&str, usize)> = shapes.into_iter().collect();
    ordered.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    println!("\nshapes played, {club_seasons} club-seasons over the measured seasons:");
    for (shape, count) in ordered {
        println!(
            "  {shape:<9}{:>6}  {:>5.1} %",
            count,
            100.0 * count as f64 / club_seasons.max(1) as f64
        );
    }

    // The columns a baseline run is compared on: the mean over worlds of each season's mean.
    let column = |read: &dyn Fn(&SeasonRow) -> f64| -> (f64, f64) {
        mean_se(
            &per_world
                .iter()
                .map(|rows| read_of(rows, read))
                .collect::<Vec<_>>(),
        )
    };
    println!("\nmean over worlds, and the spread of the worlds' means (se):");
    let report =
        |name: &str, (mean, se): (f64, f64)| println!("  {name:<26}{mean:>9.3}  (se {se:.3})");
    report(
        "clubs changing shape",
        column(&|r| r.formation_changes as f64),
    );
    report("formations in the league", column(&|r| r.formations as f64));
    report(
        "commonest shape share %",
        column(&|r| 100.0 * r.commonest_formation_share),
    );
    report("tactical plans", column(&|r| r.tactical_plans as f64));
    report("seconds a season", column(&|r| r.seconds));
    report(
        "goals a match",
        column(&|r| r.matches.per_match(r.matches.goals)),
    );
    report(
        "shots a match",
        column(&|r| r.matches.per_match(r.matches.shots)),
    );
    report(
        "on target a match",
        column(&|r| r.matches.per_match(r.matches.on_target)),
    );
    report(
        "home possession %",
        column(&|r| r.matches.home_possession / r.matches.possession_readings.max(1.0)),
    );
    report("draws", column(&|r| r.matches.per_match(r.matches.draws)));
    report(
        "home points a match",
        column(&|r| r.matches.per_match(r.matches.home_points)),
    );
    report(
        "home minus away goals",
        column(&|r| {
            r.matches
                .per_match(r.matches.home_goals - r.matches.away_goals)
        }),
    );
    report(
        "red cards a match",
        column(&|r| r.matches.per_match(r.matches.red_cards)),
    );
    let fixtures: f64 = per_world.iter().flatten().map(|r| r.matches.fixtures).sum();
    assert!(
        fixtures > 0.0,
        "no league match was applied: not a measurement"
    );
}

/// The mean over the seasons of one world of one column.
fn read_of(rows: &[SeasonRow], read: &dyn Fn(&SeasonRow) -> f64) -> f64 {
    rows.iter().map(read).sum::<f64>() / rows.len().max(1) as f64
}

// ---------------------------------------------------------------------------
// What a manager does during a match
// ---------------------------------------------------------------------------

/// One played match, as far as the management study reads it.
#[derive(Debug, Clone, Copy, Default)]
struct MatchFacts {
    substitutions: f64,
    goals_after_60: f64,
    shots_after_60: f64,
    sent_off: f64,
    /// The side ahead at the hour won, was held, or lost; only fixtures with a leader at the hour
    /// count.
    led_at_60: f64,
    held_the_lead: f64,
}

/// Given a match's report
/// When its facts are read
/// Then they are counted from the events the engine emitted: substitutions and sending-offs are in
///      the log, and the score at the hour is the goals scored in the first sixty minutes.
fn facts(report: &engine::report::MatchReport) -> MatchFacts {
    use engine::event::EventType;
    let mut facts = MatchFacts::default();
    let mut goals = (0u32, 0u32);
    for event in &report.events {
        let home = event.side == engine::types::Side::Home;
        match event.event_type {
            EventType::Substitution => facts.substitutions += 1.0,
            EventType::RedCard => facts.sent_off += 1.0,
            EventType::Goal | EventType::PenaltyGoal if event.minute > 60 => {
                facts.goals_after_60 += 1.0;
            }
            EventType::ShotOnTarget | EventType::ShotOffTarget | EventType::ShotBlocked
                if event.minute > 60 =>
            {
                facts.shots_after_60 += 1.0;
            }
            _ => {}
        }
        if event.minute <= 60 {
            match event.event_type {
                EventType::Goal | EventType::PenaltyGoal => {
                    if home {
                        goals.0 += 1;
                    } else {
                        goals.1 += 1;
                    }
                }
                _ => {}
            }
        }
    }
    if goals.0 != goals.1 {
        facts.led_at_60 = 1.0;
        let leader_won = if goals.0 > goals.1 {
            report.home_goals > report.away_goals
        } else {
            report.away_goals > report.home_goals
        };
        if leader_won {
            facts.held_the_lead = 1.0;
        }
    }
    facts
}

/// Recovered for another match: a study plays a panel, not a season, so nobody carries fatigue or a
/// knock from one fixture into the next.
fn refresh(game: &mut Game, clubs: [&str; 2]) {
    for player in game
        .players
        .iter_mut()
        .filter(|p| p.team_id.as_deref().is_some_and(|id| clubs.contains(&id)))
    {
        player.condition = 100;
        player.injury = None;
    }
}

#[test]
#[ignore = "measurement study"]
fn what_a_manager_does_during_a_match() {
    let clubs_per_world = env::first("P61_CLUBS", "8") as usize;
    let mut per_world: Vec<[f64; 6]> = Vec::new();
    for seed in env::list("P6_SEEDS", "1,2,3") {
        let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
        for day in 1..=env::first("P6_WARMUP", "60") as u32 {
            play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
        }
        let _frozen = freeze_reviews_for_study();
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
        let subjects: Vec<String> = ids
            .iter()
            .step_by(step)
            .take(clubs_per_world)
            .cloned()
            .collect();
        let mut sums = [0.0f64; 6];
        let mut fixtures = 0.0;
        let mut played = 0u64;
        for subject in &subjects {
            let panel: Vec<&String> = ids
                .iter()
                .filter(|id| *id != subject)
                .skip(ids.len() / 3)
                .take(6)
                .collect();
            for opponent in panel {
                for (home, away) in [
                    (subject.as_str(), opponent.as_str()),
                    (opponent.as_str(), subject.as_str()),
                ] {
                    refresh(&mut game, [home, away]);
                    game.seed = seed * 100_000 + played;
                    played += 1;
                    let outcome = play_fixture(&mut game, home, away).expect("plays");
                    let facts = facts(&outcome.report);
                    fixtures += 1.0;
                    sums[0] += facts.substitutions;
                    sums[1] += facts.goals_after_60;
                    sums[2] += facts.shots_after_60;
                    sums[3] += f64::from(facts.sent_off > 0.0);
                    sums[4] += facts.led_at_60;
                    sums[5] += facts.held_the_lead;
                }
            }
        }
        per_world.push([
            sums[0] / fixtures,
            sums[1] / fixtures,
            sums[2] / fixtures,
            sums[3] / fixtures,
            sums[4] / fixtures,
            if sums[4] > 0.0 {
                sums[5] / sums[4]
            } else {
                f64::NAN
            },
        ]);
    }
    println!(
        "\n== what a manager does during a match: {} worlds, {} clubs a world ==",
        per_world.len(),
        clubs_per_world
    );
    let column = |index: usize| -> (f64, f64) {
        mean_se(&per_world.iter().map(|w| w[index]).collect::<Vec<_>>())
    };
    let report =
        |name: &str, (mean, se): (f64, f64)| println!("  {name:<34}{mean:>8.3}  (se {se:.3})");
    report("substitutions a match", column(0));
    report("goals after the hour", column(1));
    report("shots after the hour", column(2));
    report("matches with a sending-off", column(3));
    report("matches with a leader at the hour", column(4));
    report("leads held to the end", column(5));
    assert!(
        per_world.iter().all(|w| w[0].is_finite()),
        "no match was played: not a measurement"
    );
}
