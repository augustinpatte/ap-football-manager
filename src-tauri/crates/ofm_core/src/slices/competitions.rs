use crate::game::Game;
use domain::league::CompetitionState;
use domain::world_history::WorldCupChampionRecord;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Deserialize)]
pub struct CompetitionsQuery {}

/// Minimal player name info needed to display top-scorer rows.
#[derive(Debug, Serialize)]
pub struct PlayerNameEntry {
    pub match_name: String,
    pub full_name: String,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
}

/// How one archived edition of a competition ended: who won it, who it was won against, and what
/// it moved between divisions.
#[derive(Debug, Serialize)]
pub struct EditionSummary {
    pub competition_id: String,
    pub season: u32,
    pub completed_on: String,
    pub champion_id: String,
    /// The beaten finalist of a cup, the second of a table.
    pub runner_up_id: Option<String>,
    /// Home club, away club and score of a cup final: `(home id, away id, home goals, away
    /// goals, home penalties, away penalties)`.
    pub final_result: Option<FinalResult>,
    pub movements: Option<domain::competition_rules::BoundaryMovements>,
}

#[derive(Debug, Serialize)]
pub struct FinalResult {
    pub home_team_id: String,
    pub away_team_id: String,
    pub home_goals: u8,
    pub away_goals: u8,
    pub home_penalties: Option<u8>,
    pub away_penalties: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct CompetitionsView {
    pub competitions: Vec<CompetitionState>,
    /// Finished editions, most recent first: champions, finals, promotions and relegations.
    pub history: Vec<EditionSummary>,
    /// Club team names keyed by ID (only teams appearing in competitions).
    pub team_names: BTreeMap<String, String>,
    /// National team names keyed by ID.
    pub national_team_names: BTreeMap<String, String>,
    /// i18n keys for national team nation names, keyed by national team ID.
    /// Frontend resolves these via `t(key)` and the `nations.nationalTeamTemplate`
    /// template rather than displaying the raw English `national_team_names` entry.
    pub national_team_name_keys: BTreeMap<String, String>,
    /// Name info for every player who has scored in a competition fixture.
    pub player_names: BTreeMap<String, PlayerNameEntry>,
    pub world_cup_champions: Vec<WorldCupChampionRecord>,
    pub manager_team_id: Option<String>,
    pub active_competition_ids: Vec<String>,
}

/// The finished editions the game remembers, most recent first.
fn edition_summaries(game: &Game) -> Vec<EditionSummary> {
    let mut editions: Vec<EditionSummary> = game
        .edition_archive
        .iter()
        .map(|edition| {
            let final_fixture = edition
                .knockout_rounds
                .last()
                .and_then(|round| round.fixture_ids.first())
                .and_then(|id| edition.fixtures.iter().find(|fixture| &fixture.id == id));
            let final_result = final_fixture.and_then(|fixture| {
                let result = fixture.result.as_ref()?;
                Some(FinalResult {
                    home_team_id: fixture.home_team_id.clone(),
                    away_team_id: fixture.away_team_id.clone(),
                    home_goals: result.home_goals,
                    away_goals: result.away_goals,
                    home_penalties: result.home_penalties,
                    away_penalties: result.away_penalties,
                })
            });
            let runner_up_id = match &final_result {
                Some(result) => [&result.home_team_id, &result.away_team_id]
                    .into_iter()
                    .find(|team| **team != edition.champion_id)
                    .cloned(),
                None => edition
                    .standings
                    .get(1)
                    .map(|entry| entry.team_id.clone())
                    .filter(|team| *team != edition.champion_id),
            };
            EditionSummary {
                competition_id: edition.competition_id.clone(),
                season: edition.season,
                completed_on: edition.completed_on.clone(),
                champion_id: edition.champion_id.clone(),
                runner_up_id,
                final_result,
                movements: edition.movements.clone(),
            }
        })
        .collect();
    editions.sort_by(|a, b| {
        b.season
            .cmp(&a.season)
            .then(a.competition_id.cmp(&b.competition_id))
    });
    editions
}

pub fn query_competitions(game: &Game, _query: &CompetitionsQuery) -> CompetitionsView {
    let mut competitions = game.competitions.clone();
    for competition in &mut competitions {
        if !competition.rules.tiebreakers.is_empty() {
            competition.standings = competition.sorted_standings();
        }
    }
    let history = edition_summaries(game);

    // Build club team lookup from all competitions' participants and fixtures.
    let club_team_ids: BTreeSet<&str> = competitions
        .iter()
        .flat_map(|c| {
            c.participant_ids.iter().map(String::as_str).chain(
                c.fixtures
                    .iter()
                    .flat_map(|f| [f.home_team_id.as_str(), f.away_team_id.as_str()]),
            )
        })
        .chain(history.iter().flat_map(|edition| {
            [
                Some(edition.champion_id.as_str()),
                edition.runner_up_id.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(edition.movements.iter().flat_map(|movements| {
                movements
                    .promoted()
                    .chain(movements.relegated())
                    .map(String::as_str)
            }))
        }))
        .collect();

    let team_map: BTreeMap<&str, &str> = game
        .teams
        .iter()
        .map(|t| (t.id.as_str(), t.name.as_str()))
        .collect();

    let team_names: BTreeMap<String, String> = team_map
        .iter()
        .filter(|(id, _)| club_team_ids.contains(*id))
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect();

    let national_team_names: BTreeMap<String, String> = game
        .national_teams
        .iter()
        .map(|nt| (nt.id.clone(), nt.name.clone()))
        .collect();

    let national_team_name_keys: BTreeMap<String, String> = game
        .national_teams
        .iter()
        .filter_map(|nt| nt.name_key.as_ref().map(|key| (nt.id.clone(), key.clone())))
        .collect();

    // Collect scorer IDs across all competition fixtures.
    let scorer_ids: BTreeSet<&str> = competitions
        .iter()
        .flat_map(|c| c.fixtures.iter())
        .filter_map(|f| f.result.as_ref())
        .flat_map(|r| {
            r.home_scorers
                .iter()
                .chain(r.away_scorers.iter())
                .map(|s| s.player_id.as_str())
        })
        .collect();

    let on_roster = game
        .players
        .iter()
        .filter(|p| scorer_ids.contains(p.id.as_str()))
        .map(|p| {
            let team_name = p
                .team_id
                .as_deref()
                .and_then(|tid| team_map.get(tid))
                .map(|n| n.to_string());
            (
                p.id.clone(),
                PlayerNameEntry {
                    match_name: p.match_name.clone(),
                    full_name: p.full_name.clone(),
                    team_id: p.team_id.clone(),
                    team_name,
                },
            )
        });
    // A scorer who has since retired into the archive still scored that goal.
    let retired = game
        .retired_players
        .iter()
        .filter(|p| scorer_ids.contains(p.id.as_str()))
        .map(|p| {
            (
                p.id.clone(),
                PlayerNameEntry {
                    match_name: p.match_name.clone(),
                    full_name: p.full_name.clone(),
                    team_id: None,
                    team_name: None,
                },
            )
        });
    let player_names: BTreeMap<String, PlayerNameEntry> = retired.chain(on_roster).collect();

    CompetitionsView {
        competitions,
        history,
        team_names,
        national_team_names,
        national_team_name_keys,
        player_names,
        world_cup_champions: game.world_history.world_cup_champions.clone(),
        manager_team_id: game.manager.team_id.clone(),
        active_competition_ids: game.active_competition_ids.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::league::{Fixture, FixtureStatus, GoalEvent, League, MatchResult};
    use domain::manager::Manager;
    use domain::player::{Player, PlayerAttributes, Position};
    use domain::retired_player::RetiredPlayer;

    use crate::clock::GameClock;

    fn scorer(id: &str, name: &str) -> Player {
        let value = 60;
        let attributes = PlayerAttributes {
            pace: value,
            stamina: value,
            strength: value,
            agility: value,
            passing: value,
            shooting: value,
            tackling: value,
            dribbling: value,
            defending: value,
            positioning: value,
            vision: value,
            decisions: value,
            composure: value,
            aggression: value,
            teamwork: value,
            leadership: value,
            handling: value,
            reflexes: value,
            aerial: value,
        };
        Player::new(
            id.into(),
            format!("{name} (match)"),
            name.into(),
            "1988-01-01".into(),
            "England".into(),
            Position::Forward,
            attributes,
        )
    }

    fn game_where_both_scored(active: Player, retired: Player) -> Game {
        let scored = |player: &Player| GoalEvent {
            player_id: player.id.clone(),
            minute: 30,
        };
        let fixture = Fixture {
            status: FixtureStatus::Completed,
            home_team_id: "home".into(),
            away_team_id: "away".into(),
            result: Some(MatchResult {
                home_goals: 1,
                away_goals: 1,
                home_scorers: vec![scored(&active)],
                away_scorers: vec![scored(&retired)],
                ..Default::default()
            }),
            ..Default::default()
        };
        let league = League {
            id: "league".into(),
            participant_ids: vec!["home".into(), "away".into()],
            fixtures: vec![fixture],
            ..Default::default()
        };
        let manager = Manager::new(
            "mgr".into(),
            "Alex".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        let clock = GameClock::new(Utc.with_ymd_and_hms(2040, 6, 1, 12, 0, 0).unwrap());
        let mut game = Game::new(clock, manager, vec![], vec![active], vec![], vec![]);
        game.competitions = vec![league];
        game.retired_players
            .push(RetiredPlayer::from_player(&retired, "2039-06-30", 2039));
        game
    }

    /// Given an old result whose scorer has since retired into the archive, when the competitions
    /// view is built, then both scorers' names resolve and the retiree has no club.
    #[test]
    fn a_retired_scorer_in_an_old_result_still_has_a_name() {
        let game =
            game_where_both_scored(scorer("p-active", "Ann Active"), scorer("p-old", "Ada Old"));

        let view = query_competitions(&game, &CompetitionsQuery {});

        assert_eq!(view.player_names["p-active"].full_name, "Ann Active");
        let retired = &view.player_names["p-old"];
        assert_eq!(retired.full_name, "Ada Old");
        assert_eq!(retired.match_name, "Ada Old (match)");
        assert_eq!(retired.team_id, None);
        assert_eq!(retired.team_name, None);
    }
}
