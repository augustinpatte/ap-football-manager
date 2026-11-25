//! The club's finished matches, read from the competitions' fixtures.

use domain::league::FixtureStatus;

use crate::game::Game;

use super::tracker::PreRun;
use super::types::{Listed, MatchLine, MatchOutcome, MatchesReport, ReportEvent};

/// The matches the club finished on `[from, until)`, and the knockout ties among them that sent
/// the club on or out. Both come from the same pass over the fixtures.
pub(super) fn matches_of(
    game: &Game,
    club_id: &str,
    from: &str,
    until: &str,
    pre_run: &PreRun,
) -> (MatchesReport, Vec<ReportEvent>) {
    let mut lines = Vec::new();
    let mut events = Vec::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if fixture.status != FixtureStatus::Completed
                || fixture.date.as_str() < from
                || fixture.date.as_str() >= until
            {
                continue;
            }
            if pre_run.fixtures.contains(&fixture.id) {
                continue;
            }
            let at_home = fixture.home_team_id == club_id;
            if !at_home && fixture.away_team_id != club_id {
                continue;
            }
            let Some(result) = &fixture.result else {
                continue;
            };
            let opponent_id = if at_home {
                &fixture.away_team_id
            } else {
                &fixture.home_team_id
            };
            let (goals_for, goals_against) = if at_home {
                (result.home_goals, result.away_goals)
            } else {
                (result.away_goals, result.home_goals)
            };
            let (penalties_for, penalties_against) = if at_home {
                (result.home_penalties, result.away_penalties)
            } else {
                (result.away_penalties, result.home_penalties)
            };
            lines.push(MatchLine {
                fixture_id: fixture.id.clone(),
                competition_id: competition.id.clone(),
                competition_name: competition.name.clone(),
                competition_name_key: competition.name_key.clone(),
                date: fixture.date.clone(),
                at_home,
                opponent_id: opponent_id.clone(),
                opponent_name: game.team_name_or_id(opponent_id),
                goals_for,
                goals_against,
                penalties_for,
                penalties_against,
                outcome: match goals_for.cmp(&goals_against) {
                    std::cmp::Ordering::Greater => MatchOutcome::Win,
                    std::cmp::Ordering::Equal => MatchOutcome::Draw,
                    std::cmp::Ordering::Less => MatchOutcome::Loss,
                },
            });
            if competition.is_knockout_fixture(&fixture.id) {
                events.push(knockout_event(fixture, competition, club_id));
            }
        }
    }
    lines.sort_by(|a, b| (&a.date, &a.fixture_id).cmp(&(&b.date, &b.fixture_id)));
    (totals(lines), events)
}

fn knockout_event(
    fixture: &domain::league::Fixture,
    competition: &domain::league::League,
    club_id: &str,
) -> ReportEvent {
    let date = fixture.date.clone();
    let fixture_id = fixture.id.clone();
    let competition_id = competition.id.clone();
    let competition_name = competition.name.clone();
    if fixture.advancing_team_id() == Some(club_id) {
        ReportEvent::KnockoutAdvanced {
            date,
            fixture_id,
            competition_id,
            competition_name,
        }
    } else {
        ReportEvent::KnockoutEliminated {
            date,
            fixture_id,
            competition_id,
            competition_name,
        }
    }
}

fn totals(lines: Vec<MatchLine>) -> MatchesReport {
    let mut report = MatchesReport {
        played: u32::try_from(lines.len()).unwrap_or(u32::MAX),
        ..MatchesReport::default()
    };
    for line in &lines {
        report.goals_for += u32::from(line.goals_for);
        report.goals_against += u32::from(line.goals_against);
        match line.outcome {
            MatchOutcome::Win => report.won += 1,
            MatchOutcome::Draw => report.drawn += 1,
            MatchOutcome::Loss => report.lost += 1,
        }
    }
    report.fixtures = Listed::capped(lines);
    report
}
