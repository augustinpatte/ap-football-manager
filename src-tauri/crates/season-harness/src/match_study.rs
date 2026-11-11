//! Controlled match studies: one pairing, played by either engine, read as numbers.
//!
//! A career plays two engines. Unwatched fixtures (every AI match, every delegated match, every
//! long-career study) are played by the five-zone live engine; the match a person watches is played
//! by M12. A study that wants to say how the two differ has to put the same two prepared sides into
//! both, so this module installs one fixture in the game and plays it through each entry point the
//! game itself uses (`play_unwatched_fixture`, `spatial_runner::build_fixture_report`).
//!
//! Matches are seeded from `Game::seed` and the fixture id: a study varies the seed to get a new
//! match and keeps everything else still.

use domain::league::{CompetitionType, Fixture, FixtureStatus, League};
use ofm_core::game::Game;
use ofm_core::live_match_manager::play_unwatched_fixture;
use ofm_core::spatial_runner::build_fixture_report;

/// Which engine plays the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Football {
    /// The five-zone live engine every unwatched fixture is played by.
    FiveZone,
    /// M12, the spatial engine the user's watched match is played by.
    M12,
}

/// One side of one match.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SideResult {
    pub points: f64,
    pub goals_for: f64,
    pub goals_against: f64,
    pub shots: f64,
    pub on_target: f64,
    /// Share of the ball, 0 to 100.
    pub possession: f64,
}

impl SideResult {
    fn of(goals_for: u8, goals_against: u8, shots: u16, on_target: u16, possession: f64) -> Self {
        let points = match goals_for.cmp(&goals_against) {
            std::cmp::Ordering::Greater => 3.0,
            std::cmp::Ordering::Equal => 1.0,
            std::cmp::Ordering::Less => 0.0,
        };
        Self {
            points,
            goals_for: f64::from(goals_for),
            goals_against: f64::from(goals_against),
            shots: f64::from(shots),
            on_target: f64::from(on_target),
            possession,
        }
    }

    /// The mean of each field over `results`; all zero for none.
    pub fn mean(results: &[SideResult]) -> Self {
        if results.is_empty() {
            return Self::default();
        }
        let n = results.len() as f64;
        let sum = |field: fn(&SideResult) -> f64| results.iter().map(field).sum::<f64>() / n;
        Self {
            points: sum(|r| r.points),
            goals_for: sum(|r| r.goals_for),
            goals_against: sum(|r| r.goals_against),
            shots: sum(|r| r.shots),
            on_target: sum(|r| r.on_target),
            possession: sum(|r| r.possession),
        }
    }
}

/// Both sides of a match, home first, from the numbers a report carries: (goals, shots, shots on
/// target) of the home side and of the away side, and the home side's share of the ball.
fn pair(
    home: (u8, u16, u16),
    away: (u8, u16, u16),
    home_possession: f64,
) -> (SideResult, SideResult) {
    (
        SideResult::of(home.0, away.0, home.1, home.2, home_possession),
        SideResult::of(away.0, home.0, away.1, away.2, 100.0 - home_possession),
    )
}

/// Makes `home` v `away`, today, the one fixture of the game's league.
fn install_fixture(game: &mut Game, home: &str, away: &str) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.league = Some(League {
        id: "match-study".to_string(),
        name: "Match study".to_string(),
        kind: CompetitionType::League,
        fixtures: vec![Fixture {
            id: "match-study-fixture".to_string(),
            competition_id: "match-study".to_string(),
            matchday: 1,
            date: today,
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            status: FixtureStatus::Scheduled,
            ..Fixture::default()
        }],
        ..League::default()
    });
}

/// Plays `home` v `away` once with `football` and returns (home side, away side). The game's
/// standings and club state are only touched by the five-zone path (as in a career); M12's
/// report is built without applying it.
pub fn play(
    game: &mut Game,
    football: Football,
    home: &str,
    away: &str,
) -> Result<(SideResult, SideResult), String> {
    install_fixture(game, home, away);
    match football {
        Football::FiveZone => {
            let r = play_unwatched_fixture(game, 0)?.report;
            Ok(pair(
                (
                    r.home_goals,
                    r.home_stats.shots,
                    r.home_stats.shots_on_target,
                ),
                (
                    r.away_goals,
                    r.away_stats.shots,
                    r.away_stats.shots_on_target,
                ),
                r.home_possession,
            ))
        }
        Football::M12 => {
            let r = build_fixture_report(game, 0).map_err(|error| format!("{error:?}"))?;
            Ok(pair(
                (
                    r.home_goals,
                    r.home_stats.shots,
                    r.home_stats.shots_on_target,
                ),
                (
                    r.away_goals,
                    r.away_stats.shots,
                    r.away_stats.shots_on_target,
                ),
                r.home_possession,
            ))
        }
    }
}

/// Mean and standard error of the mean of `values`.
pub fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n.max(1.0);
    if values.len() < 2 {
        return (mean, f64::NAN);
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, (variance / n).sqrt())
}
