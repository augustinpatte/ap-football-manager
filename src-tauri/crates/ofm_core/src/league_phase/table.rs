//! The league-phase table and its ranking (Article 18).
//!
//! The table is a function of the completed league-phase fixtures. It is rebuilt from them on
//! every call, never incremented, so a result that is finalised twice cannot count twice and a
//! reloaded save ranks the same as the one that was saved.
//!
//! Order after points: goal difference, goals scored, away goals scored, wins, away wins, the
//! collective points, goal difference and goals scored of the club's league-phase opponents, a
//! lower disciplinary total, a higher club coefficient. The coefficient is never invented: when
//! none was supplied the seeding order (the pots were themselves drawn from the coefficient
//! ranking) stands in, and the outcome says so. A final comparison on the club id keeps the order
//! total and deterministic; it is reached only by clubs identical in every criterion.

use domain::league::{Fixture, FixtureStatus};
use domain::league_phase::{LeaguePhaseOutcome, LeaguePhaseRow};
use std::cmp::Ordering;
use std::collections::BTreeMap;

const YELLOW_POINTS: u32 = 1;
/// A second yellow is a 3-point expulsion in total; the first yellow already counted 1.
const SECOND_YELLOW_EXTRA_POINTS: u32 = 2;
const RED_POINTS: u32 = 3;

/// Disciplinary points of one side from a match report's events. Matches settled by scoreline have
/// no report and contribute none: the data does not exist, and none is made up.
fn disciplinary_points(fixture: &Fixture, home: bool) -> u32 {
    let side = if home { "Home" } else { "Away" };
    let Some(report) = fixture
        .result
        .as_ref()
        .and_then(|result| result.report.as_ref())
    else {
        return 0;
    };
    report
        .events
        .iter()
        .filter(|event| event.side == side)
        .map(|event| match event.event_type.as_str() {
            "YellowCard" => YELLOW_POINTS,
            "SecondYellow" => SECOND_YELLOW_EXTRA_POINTS,
            "RedCard" => RED_POINTS,
            _ => 0,
        })
        .sum()
}

fn bare_rows(seeding: &[String], coefficients: &BTreeMap<String, u32>) -> Vec<LeaguePhaseRow> {
    seeding
        .iter()
        .map(|team_id| LeaguePhaseRow {
            team_id: team_id.clone(),
            coefficient: coefficients.get(team_id).copied(),
            ..Default::default()
        })
        .collect()
}

fn record(row: &mut LeaguePhaseRow, goals_for: u8, goals_against: u8, away: bool) {
    row.played += 1;
    row.goals_for += u32::from(goals_for);
    row.goals_against += u32::from(goals_against);
    if away {
        row.away_goals_for += u32::from(goals_for);
    }
    match goals_for.cmp(&goals_against) {
        Ordering::Greater => {
            row.won += 1;
            row.points += 3;
            row.away_wins += u32::from(away);
        }
        Ordering::Equal => {
            row.drawn += 1;
            row.points += 1;
        }
        Ordering::Less => row.lost += 1,
    }
}

/// The ranked rows of the table over `fixtures` (the league-phase fixtures only; unplayed ones are
/// ignored). `seeding` lists every club, strongest first.
pub fn ranked_rows<'a>(
    fixtures: impl Iterator<Item = &'a Fixture> + Clone,
    seeding: &[String],
    coefficients: &BTreeMap<String, u32>,
) -> Vec<LeaguePhaseRow> {
    let mut rows = bare_rows(seeding, coefficients);
    let index: BTreeMap<&str, usize> = seeding
        .iter()
        .enumerate()
        .map(|(position, id)| (id.as_str(), position))
        .collect();
    let played =
        |fixture: &&Fixture| fixture.status == FixtureStatus::Completed && fixture.result.is_some();
    for fixture in fixtures.clone().filter(played) {
        let (Some(result), Some(&home), Some(&away)) = (
            fixture.result.as_ref(),
            index.get(fixture.home_team_id.as_str()),
            index.get(fixture.away_team_id.as_str()),
        ) else {
            continue;
        };
        record(&mut rows[home], result.home_goals, result.away_goals, false);
        record(&mut rows[away], result.away_goals, result.home_goals, true);
        rows[home].disciplinary_points += disciplinary_points(fixture, true);
        rows[away].disciplinary_points += disciplinary_points(fixture, false);
    }
    // Opponent aggregates need every club's totals first.
    let totals: Vec<(u32, i32, u32)> = rows
        .iter()
        .map(|row| (row.points, row.goal_difference(), row.goals_for))
        .collect();
    for fixture in fixtures.filter(played) {
        let (Some(&home), Some(&away)) = (
            index.get(fixture.home_team_id.as_str()),
            index.get(fixture.away_team_id.as_str()),
        ) else {
            continue;
        };
        for (club, opponent) in [(home, away), (away, home)] {
            rows[club].opponents_points += totals[opponent].0;
            rows[club].opponents_goal_difference += totals[opponent].1;
            rows[club].opponents_goals_for += totals[opponent].2;
        }
    }
    let stand_in: BTreeMap<String, usize> = index
        .iter()
        .map(|(id, position)| ((*id).to_string(), *position))
        .collect();
    rows.sort_by(|a, b| compare(a, b, &stand_in));
    for (position, row) in rows.iter_mut().enumerate() {
        row.position = position as u32 + 1;
    }
    rows
}

/// Best first. `seed_position` is each club's place in the seeding order, standing in for the
/// coefficient when one is missing.
fn compare(
    a: &LeaguePhaseRow,
    b: &LeaguePhaseRow,
    seed_position: &BTreeMap<String, usize>,
) -> Ordering {
    b.points
        .cmp(&a.points)
        .then_with(|| b.goal_difference().cmp(&a.goal_difference()))
        .then_with(|| b.goals_for.cmp(&a.goals_for))
        .then_with(|| b.away_goals_for.cmp(&a.away_goals_for))
        .then_with(|| b.won.cmp(&a.won))
        .then_with(|| b.away_wins.cmp(&a.away_wins))
        .then_with(|| b.opponents_points.cmp(&a.opponents_points))
        .then_with(|| {
            b.opponents_goal_difference
                .cmp(&a.opponents_goal_difference)
        })
        .then_with(|| b.opponents_goals_for.cmp(&a.opponents_goals_for))
        .then_with(|| a.disciplinary_points.cmp(&b.disciplinary_points))
        .then_with(|| coefficient_order(a, b, seed_position))
        .then_with(|| a.team_id.cmp(&b.team_id))
}

fn coefficient_order(
    a: &LeaguePhaseRow,
    b: &LeaguePhaseRow,
    seed_position: &BTreeMap<String, usize>,
) -> Ordering {
    match (a.coefficient, b.coefficient) {
        (Some(x), Some(y)) => y.cmp(&x),
        _ => seed_position
            .get(&a.team_id)
            .cmp(&seed_position.get(&b.team_id)),
    }
}

/// Whether two rows are level on every criterion before the coefficient: the pairs that make the
/// coefficient (or its stand-in) decisive.
fn level_before_coefficient(a: &LeaguePhaseRow, b: &LeaguePhaseRow) -> bool {
    (
        a.points,
        a.goal_difference(),
        a.goals_for,
        a.away_goals_for,
        a.won,
        a.away_wins,
        a.opponents_points,
        a.opponents_goal_difference,
        a.opponents_goals_for,
        a.disciplinary_points,
    ) == (
        b.points,
        b.goal_difference(),
        b.goals_for,
        b.away_goals_for,
        b.won,
        b.away_wins,
        b.opponents_points,
        b.opponents_goal_difference,
        b.opponents_goals_for,
        b.disciplinary_points,
    )
}

/// Who goes where, from the final ranked rows: ranks 1–8 direct, 9–24 to the play-offs, the rest
/// out.
pub fn outcome_of(
    rows: &[LeaguePhaseRow],
    direct_places: usize,
    playoff_places: usize,
) -> LeaguePhaseOutcome {
    let ids: Vec<String> = rows.iter().map(|row| row.team_id.clone()).collect();
    let direct_end = direct_places.min(ids.len());
    let playoff_end = (direct_places + playoff_places).min(ids.len());
    let fallback = rows.windows(2).any(|pair| {
        level_before_coefficient(&pair[0], &pair[1])
            && !(pair[0].coefficient.is_some() && pair[1].coefficient.is_some())
    });
    LeaguePhaseOutcome {
        direct: ids[..direct_end].to_vec(),
        playoff: ids[direct_end..playoff_end].to_vec(),
        eliminated: ids[playoff_end..].to_vec(),
        ranking: ids,
        coefficient_fallback_used: fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::league::{
        CompactMatchEvent, CompactMatchReport, CompactTeamMatchStats, MatchResult,
    };

    fn game(home: &str, away: &str, home_goals: u8, away_goals: u8) -> Fixture {
        Fixture {
            id: format!("{home}-{away}"),
            home_team_id: home.into(),
            away_team_id: away.into(),
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_goals,
                away_goals,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn with_cards(mut fixture: Fixture, events: &[(&str, &str)]) -> Fixture {
        let stats = CompactTeamMatchStats {
            possession_pct: 50,
            shots: 0,
            shots_on_target: 0,
            fouls: 0,
            corners: 0,
            yellow_cards: 0,
            red_cards: 0,
        };
        fixture.result.as_mut().unwrap().report = Some(CompactMatchReport {
            total_minutes: 90,
            home_stats: stats.clone(),
            away_stats: stats,
            events: events
                .iter()
                .map(|(side, kind)| CompactMatchEvent {
                    minute: 10,
                    event_type: (*kind).into(),
                    side: (*side).into(),
                    player_id: None,
                    secondary_player_id: None,
                    own_goal_by: None,
                })
                .collect(),
        });
        fixture
    }

    fn ids(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("t{i}")).collect()
    }

    fn order(rows: &[LeaguePhaseRow]) -> Vec<&str> {
        rows.iter().map(|row| row.team_id.as_str()).collect()
    }

    fn rank(fixtures: &[Fixture], clubs: usize) -> Vec<LeaguePhaseRow> {
        ranked_rows(fixtures.iter(), &ids(clubs), &BTreeMap::new())
    }

    fn position_of(rows: &[LeaguePhaseRow], id: &str) -> u32 {
        rows.iter().find(|row| row.team_id == id).unwrap().position
    }

    /// Given two clubs on equal points, one with the better difference, the other with more goals
    /// When they are ranked
    /// Then goal difference decides, not goals scored.
    #[test]
    fn goal_difference_comes_before_goals_scored() {
        let fixtures = [game("t0", "t2", 2, 0), game("t1", "t3", 5, 4)];

        let rows = rank(&fixtures, 4);

        assert!(position_of(&rows, "t0") < position_of(&rows, "t1"));
        assert_eq!(
            rows.iter().find(|r| r.team_id == "t1").unwrap().goals_for,
            5
        );
    }

    /// Given clubs level on points, difference and goals scored
    /// When one scored its goals away from home
    /// Then more away goals ranks first.
    #[test]
    fn away_goals_scored_break_a_tie_on_goals_scored() {
        let fixtures = [game("t0", "t2", 2, 1), game("t3", "t1", 1, 2)];

        let rows = rank(&fixtures, 4);

        let t0 = rows.iter().find(|r| r.team_id == "t0").unwrap();
        let t1 = rows.iter().find(|r| r.team_id == "t1").unwrap();
        assert_eq!((t0.points, t0.goal_difference(), t0.goals_for), (3, 1, 2));
        assert_eq!((t1.points, t1.goal_difference(), t1.goals_for), (3, 1, 2));
        assert_eq!((t0.away_goals_for, t1.away_goals_for), (0, 2));
        assert!(t1.position < t0.position);
    }

    /// Given clubs level through away goals, with the same number of wins
    /// When one of the wins came away from home
    /// Then more away wins ranks first.
    #[test]
    fn away_wins_break_a_tie_on_wins() {
        // t0: home win 2-1, away loss 1-2.  t1: away win 1-0, home loss 2-3.
        let fixtures = [
            game("t0", "t2", 2, 1),
            game("t2", "t0", 2, 1),
            game("t3", "t1", 0, 1),
            game("t1", "t3", 2, 3),
        ];

        let rows = rank(&fixtures, 4);

        let t0 = rows.iter().find(|r| r.team_id == "t0").unwrap();
        let t1 = rows.iter().find(|r| r.team_id == "t1").unwrap();
        assert_eq!(
            (
                t0.points,
                t0.goal_difference(),
                t0.goals_for,
                t0.away_goals_for,
                t0.won
            ),
            (
                t1.points,
                t1.goal_difference(),
                t1.goals_for,
                t1.away_goals_for,
                t1.won
            )
        );
        assert_eq!((t0.away_wins, t1.away_wins), (0, 1));
        assert!(t1.position < t0.position);
    }

    /// Given two clubs level through wins and away wins, the better-seeded one with the weaker opponent
    /// When their opponents' collective results differ
    /// Then the club whose opponent earned more points ranks first, against the seeding order.
    #[test]
    fn opponents_collective_points_break_a_complete_tie() {
        // t0 and t1 both win 1-0 at home, against t2 and t3. t3 (t1's opponent) also beats t4,
        // so t3 has 3 points to t2's none. t0 leads t1 in the seeding, which is the last resort.
        let fixtures = [
            game("t0", "t2", 1, 0),
            game("t1", "t3", 1, 0),
            game("t3", "t4", 1, 0),
        ];

        let rows = rank(&fixtures, 5);

        let t0 = rows.iter().find(|r| r.team_id == "t0").unwrap();
        let t1 = rows.iter().find(|r| r.team_id == "t1").unwrap();
        assert_eq!(
            (
                t0.points,
                t0.goal_difference(),
                t0.goals_for,
                t0.away_goals_for,
                t0.won,
                t0.away_wins
            ),
            (
                t1.points,
                t1.goal_difference(),
                t1.goals_for,
                t1.away_goals_for,
                t1.won,
                t1.away_wins
            )
        );
        assert_eq!((t0.opponents_points, t1.opponents_points), (0, 3));
        assert!(
            t1.position < t0.position,
            "the weaker-seeded club ranks first on its opponents' points"
        );
    }

    /// Given two clubs identical in every sporting criterion, the cleaner one worse-seeded
    /// When their disciplinary totals differ
    /// Then the cleaner club ranks first against the seeding (yellow 1, second yellow 3 in all).
    #[test]
    fn a_lower_disciplinary_total_ranks_first() {
        let fixtures = [
            with_cards(
                game("t0", "t2", 1, 0),
                &[("Home", "YellowCard"), ("Home", "SecondYellow")],
            ),
            with_cards(
                game("t1", "t3", 1, 0),
                &[("Home", "YellowCard"), ("Home", "YellowCard")],
            ),
        ];

        let rows = rank(&fixtures, 4);

        let points = |id: &str| {
            rows.iter()
                .find(|r| r.team_id == id)
                .unwrap()
                .disciplinary_points
        };
        assert_eq!((points("t0"), points("t1")), (3, 2));
        assert!(position_of(&rows, "t1") < position_of(&rows, "t0"));
    }

    /// Given a straight red
    /// When disciplinary points are totalled
    /// Then it costs 3, and scoreline-settled matches (no report) cost nothing.
    #[test]
    fn red_cards_cost_three_and_missing_reports_cost_nothing() {
        let fixtures = [
            with_cards(game("t0", "t1", 0, 0), &[("Away", "RedCard")]),
            game("t2", "t3", 0, 0),
        ];

        let rows = rank(&fixtures, 4);

        let points = |id: &str| {
            rows.iter()
                .find(|r| r.team_id == id)
                .unwrap()
                .disciplinary_points
        };
        assert_eq!((points("t1"), points("t0"), points("t2")), (3, 0, 0));
    }

    /// Given clubs identical in every criterion
    /// When no coefficient was supplied, or one was
    /// Then the seeding order stands in and the outcome says so; a supplied coefficient decides.
    #[test]
    fn the_coefficient_is_never_invented() {
        let fixtures = [game("t0", "t1", 0, 0)];
        let seeding = ids(2);

        let without = ranked_rows(fixtures.iter(), &seeding, &BTreeMap::new());
        let with: BTreeMap<String, u32> = [("t0".to_string(), 10), ("t1".to_string(), 90)].into();
        let supplied = ranked_rows(fixtures.iter(), &seeding, &with);

        assert_eq!(order(&without), ["t0", "t1"], "seeding order stands in");
        assert!(outcome_of(&without, 1, 1).coefficient_fallback_used);
        assert_eq!(
            order(&supplied),
            ["t1", "t0"],
            "the supplied coefficient decides"
        );
        assert!(!outcome_of(&supplied, 1, 1).coefficient_fallback_used);
    }

    /// Given a table of 36 clubs
    /// When the outcome is read
    /// Then 8 go direct, 16 to the play-offs and 12 out, each club exactly once.
    #[test]
    fn the_outcome_splits_thirty_six_into_eight_sixteen_twelve() {
        let rows = rank(&[], 36);

        let outcome = outcome_of(&rows, 8, 16);

        assert_eq!(
            (
                outcome.direct.len(),
                outcome.playoff.len(),
                outcome.eliminated.len()
            ),
            (8, 16, 12)
        );
        let ids: Vec<String> = rows.iter().map(|row| row.team_id.clone()).collect();
        assert_eq!(outcome.direct, ids[..8]);
        assert_eq!(outcome.playoff, ids[8..24]);
        assert_eq!(outcome.eliminated, ids[24..]);
        let mut all: Vec<&String> = outcome
            .direct
            .iter()
            .chain(&outcome.playoff)
            .chain(&outcome.eliminated)
            .collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 36);
    }

    /// Given the same results presented twice
    /// When the table is rebuilt
    /// Then it is identical (derived, not incremented).
    #[test]
    fn rebuilding_the_table_is_idempotent() {
        let fixtures = [game("t0", "t1", 2, 1), game("t2", "t3", 0, 3)];

        assert_eq!(rank(&fixtures, 4), rank(&fixtures, 4));
    }
}
