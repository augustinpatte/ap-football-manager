//! Ordering of a league table by an authored list of tiebreakers.
//!
//! The first criterion splits the table into groups of clubs that are level; each group is then
//! split by the next criterion, and so on. Head-to-head criteria are read from the matches the
//! clubs of the group (and only they) played against each other, so a three-way tie is settled by a
//! three-club mini table, not by the clubs' full records. A tie nothing breaks keeps the order the
//! clubs were listed in.

use crate::competition_rules::Tiebreaker;
use crate::league::{Fixture, FixtureStatus, StandingEntry};
use std::collections::HashSet;

struct Context<'a> {
    /// The league matches played, with their score.
    played: Vec<(&'a Fixture, u8, u8)>,
    competition_id: &'a str,
    season: u32,
}

#[derive(Default)]
struct Record {
    points: i64,
    goals_for: i64,
    goals_against: i64,
    away_goals: i64,
}

impl Context<'_> {
    /// The record of `team` over the matches both sides of which are in `within` (every match
    /// when `within` is `None`).
    fn record(&self, team: &str, within: Option<&HashSet<&str>>) -> Record {
        let mut record = Record::default();
        for (fixture, home_goals, away_goals) in &self.played {
            let (home, away) = (fixture.home_team_id.as_str(), fixture.away_team_id.as_str());
            if home != team && away != team {
                continue;
            }
            if let Some(group) = within
                && !(group.contains(home) && group.contains(away))
            {
                continue;
            }
            let (scored, conceded) = if home == team {
                (*home_goals, *away_goals)
            } else {
                (*away_goals, *home_goals)
            };
            record.goals_for += i64::from(scored);
            record.goals_against += i64::from(conceded);
            record.points += match scored.cmp(&conceded) {
                std::cmp::Ordering::Greater => 3,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Less => 0,
            };
            if away == team {
                record.away_goals += i64::from(scored);
            }
        }
        record
    }

    fn away_wins(&self, team: &str) -> i64 {
        self.played
            .iter()
            .filter(|(fixture, home, away)| fixture.away_team_id == team && away > home)
            .count() as i64
    }

    fn lots(&self, team: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in format!("{}/{}/{}", self.season, self.competition_id, team).bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    /// The value of `criterion` for `entry`; higher is better. `group` is the set of clubs level
    /// so far.
    fn value(&self, criterion: Tiebreaker, entry: &StandingEntry, group: &HashSet<&str>) -> i64 {
        match criterion {
            Tiebreaker::Points => i64::from(entry.points),
            Tiebreaker::GoalDifference => i64::from(entry.goal_difference()),
            Tiebreaker::GoalsFor => i64::from(entry.goals_for),
            Tiebreaker::Wins => i64::from(entry.won),
            Tiebreaker::AwayWins => self.away_wins(&entry.team_id),
            Tiebreaker::AwayGoalsFor => self.record(&entry.team_id, None).away_goals,
            Tiebreaker::HeadToHeadPoints => self.record(&entry.team_id, Some(group)).points,
            Tiebreaker::HeadToHeadGoalDifference => {
                let record = self.record(&entry.team_id, Some(group));
                record.goals_for - record.goals_against
            }
            Tiebreaker::HeadToHeadGoalsFor => self.record(&entry.team_id, Some(group)).goals_for,
            Tiebreaker::HeadToHeadAwayGoals => self.record(&entry.team_id, Some(group)).away_goals,
            // Resolved by `split` itself: a draw of lots has no value to compare.
            Tiebreaker::Lots => 0,
        }
    }
}

fn split(
    group: Vec<StandingEntry>,
    criteria: &[Tiebreaker],
    context: &Context<'_>,
) -> Vec<StandingEntry> {
    if group.len() < 2 {
        return group;
    }
    let Some((criterion, rest)) = criteria.split_first() else {
        return group;
    };
    if *criterion == Tiebreaker::Lots {
        let mut group = group;
        group.sort_by_key(|entry| context.lots(&entry.team_id));
        return group;
    }
    let members: HashSet<&str> = group.iter().map(|entry| entry.team_id.as_str()).collect();
    let mut keyed: Vec<(i64, StandingEntry)> = group
        .iter()
        .map(|entry| (context.value(*criterion, entry, &members), entry.clone()))
        .collect();
    keyed.sort_by_key(|(value, _)| std::cmp::Reverse(*value));

    let mut ordered = Vec::with_capacity(keyed.len());
    let mut run: Vec<StandingEntry> = Vec::new();
    let mut run_value = None;
    for (value, entry) in keyed {
        if run_value.is_some_and(|current| current != value) {
            ordered.extend(split(std::mem::take(&mut run), rest, context));
        }
        run_value = Some(value);
        run.push(entry);
    }
    ordered.extend(split(run, rest, context));
    ordered
}

/// `standings` in finishing order under `criteria`. `fixtures` are the competition's own; only
/// completed league matches are read.
pub fn order_table(
    standings: Vec<StandingEntry>,
    criteria: &[Tiebreaker],
    fixtures: &[Fixture],
    competition_id: &str,
    season: u32,
) -> Vec<StandingEntry> {
    let context = Context {
        played: fixtures
            .iter()
            .filter(|fixture| {
                fixture.counts_for_league_standings() && fixture.status == FixtureStatus::Completed
            })
            .filter_map(|fixture| {
                let result = fixture.result.as_ref()?;
                Some((fixture, result.home_goals, result.away_goals))
            })
            .collect(),
        competition_id,
        season,
    };
    split(standings, criteria, &context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league::{FixtureCompetition, MatchResult};

    fn entry(team: &str, points: u32, goals_for: u32, goals_against: u32) -> StandingEntry {
        StandingEntry {
            team_id: team.to_string(),
            played: 0,
            won: 0,
            drawn: 0,
            lost: 0,
            goals_for,
            goals_against,
            points,
        }
    }

    fn game(home: &str, away: &str, home_goals: u8, away_goals: u8) -> Fixture {
        Fixture {
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            status: FixtureStatus::Completed,
            competition: FixtureCompetition::League,
            result: Some(MatchResult {
                home_goals,
                away_goals,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn order(
        standings: Vec<StandingEntry>,
        criteria: &[Tiebreaker],
        fixtures: &[Fixture],
    ) -> Vec<String> {
        order_table(standings, criteria, fixtures, "league", 2026)
            .into_iter()
            .map(|entry| entry.team_id)
            .collect()
    }

    use Tiebreaker::*;

    #[test]
    fn goal_difference_first_is_the_generic_order() {
        let table = vec![
            entry("a", 60, 50, 40),
            entry("b", 60, 55, 40),
            entry("c", 61, 30, 40),
        ];
        assert_eq!(
            order(table, &[Points, GoalDifference, GoalsFor], &[]),
            ["c", "b", "a"]
        );
    }

    /// LaLiga and Serie A: level on points, the head-to-head decides before the goal difference
    /// of the whole season does.
    #[test]
    fn head_to_head_points_come_before_goal_difference() {
        // a has the better season goal difference, b won both matches between the two.
        let table = vec![entry("a", 70, 80, 30), entry("b", 70, 60, 40)];
        let fixtures = vec![game("a", "b", 0, 1), game("b", "a", 1, 1)];
        assert_eq!(
            order(
                table.clone(),
                &[Points, HeadToHeadPoints, GoalDifference],
                &fixtures
            ),
            ["b", "a"]
        );
        assert_eq!(
            order(
                table,
                &[Points, GoalDifference, HeadToHeadPoints],
                &fixtures
            ),
            ["a", "b"]
        );
    }

    /// Three clubs level: only the matches among the three count, not the matches against the rest.
    #[test]
    fn a_three_way_tie_is_a_mini_table_of_the_three() {
        let table = vec![
            entry("a", 50, 0, 0),
            entry("b", 50, 0, 0),
            entry("c", 50, 0, 0),
            entry("d", 10, 0, 0),
        ];
        let fixtures = vec![
            game("a", "b", 1, 0),
            game("b", "c", 2, 0),
            game("c", "a", 3, 0),
            // A heavy win over the outsider must not matter in the mini table.
            game("a", "d", 9, 0),
        ];
        // Mini table: a 3 pts (+1-3 = -2), b 3 pts (-1+2 = +1), c 3 pts (+3-2 = +1)... all level on
        // points, so the head-to-head goal difference decides: b and c (+1) over a (-2).
        let result = order(
            table,
            &[Points, HeadToHeadPoints, HeadToHeadGoalDifference],
            &fixtures,
        );
        assert_eq!(result[3], "d");
        assert_eq!(result[2], "a");
    }

    #[test]
    fn a_tie_nothing_breaks_keeps_the_listed_order() {
        let table = vec![entry("x", 10, 5, 5), entry("y", 10, 5, 5)];
        assert_eq!(
            order(table, &[Points, GoalDifference, GoalsFor], &[]),
            ["x", "y"]
        );
    }

    #[test]
    fn lots_are_the_same_on_every_call() {
        let table = vec![
            entry("x", 10, 5, 5),
            entry("y", 10, 5, 5),
            entry("z", 10, 5, 5),
        ];
        let first = order(table.clone(), &[Points, Lots], &[]);
        assert_eq!(first, order(table, &[Points, Lots], &[]));
    }

    #[test]
    fn away_goals_count_only_the_clubs_own_away_matches() {
        let table = vec![entry("a", 10, 5, 5), entry("b", 10, 5, 5)];
        let fixtures = vec![game("b", "a", 0, 3), game("a", "b", 1, 1)];
        assert_eq!(order(table, &[Points, AwayGoalsFor], &fixtures), ["a", "b"]);
    }
}
