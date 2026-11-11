//! What the end-of-season screen is told about the season that just ended.
//!
//! Everything here is read from the state the rollover produced (the renewed divisions, the
//! edition archive), so the screen presents the backend's decisions and recomputes none of them.

use super::user_division;
use crate::game::Game;
use domain::league::{BerthRule, CompetitionType};

/// How the player's club moved between divisions at the rollover.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UserDivisionMove {
    #[default]
    Stayed,
    Promoted,
    Relegated,
}

/// A domestic cup the player's club entered, as it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserCupResult {
    pub competition_id: String,
    /// The stored name; the screen renders the competition's translated name from its id and
    /// falls back to this.
    pub competition_name: String,
    pub champion_id: String,
    pub champion_name: String,
    pub user_won: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EndOfSeasonSummary {
    pub season: u32,
    pub league_name: String,
    pub champion_id: String,
    pub champion_name: String,
    pub user_position: u32,
    pub user_points: u32,
    pub user_won: u32,
    pub user_drawn: u32,
    pub user_lost: u32,
    pub user_goals_for: u32,
    pub user_goals_against: u32,
    pub golden_boot_player: String,
    pub golden_boot_goals: u32,
    pub poty_player: String,
    pub poty_rating: f64,
    pub total_teams: u32,
    pub season_awards: crate::season_awards::SeasonAwards,
    /// Whether the club was promoted, relegated or kept its division. Absent in a summary
    /// written before the field existed, which reads as `Stayed`.
    #[serde(default)]
    pub user_division_move: UserDivisionMove,
    /// The division the club starts the next season in (its own when it stayed), by id so the
    /// screen can render its translated name, and by stored name as the fallback.
    #[serde(default)]
    pub user_next_league_id: String,
    #[serde(default)]
    pub user_next_league_name: String,
    /// Every domestic cup of this season the club entered, whoever won it.
    #[serde(default)]
    pub user_cup_results: Vec<UserCupResult>,
}

/// The user's division change at a rollover, with what the inbox message and the summary say.
pub(super) struct DivisionChange {
    pub promoted: bool,
    pub division_name: String,
    pub division_id: String,
    pub renewed_season: u32,
}

/// Rank decides direction, but two divisions can share a rank: a berth moves clubs into its
/// target, and nothing makes a target outrank its feeder. A move along a berth is the promotion it
/// is, rather than telling a champion they have been relegated.
pub(super) fn division_change_of(
    game: &Game,
    division_before: Option<&(String, u32)>,
    user_team_id: &str,
) -> Option<DivisionChange> {
    let (old_division_id, old_priority) = division_before?;
    let new_division = user_division(game, user_team_id)?;
    if &new_division.id == old_division_id {
        return None;
    }
    let promoted_by_berth = game
        .competitions
        .iter()
        .find(|competition| &competition.id == old_division_id)
        .is_some_and(|old_division| {
            old_division.berths.iter().any(|berth| {
                berth.target == new_division.id
                    && matches!(berth.rule, BerthRule::PositionRange { .. })
            })
        });
    Some(DivisionChange {
        promoted: new_division.priority < *old_priority || promoted_by_berth,
        division_name: new_division.name.clone(),
        division_id: new_division.id.clone(),
        // The renewed edition's own label: a delayed edition renews to the calendar year, which is
        // not always last season plus one.
        renewed_season: new_division.season,
    })
}

/// Fills the parts of the summary that exist only once the competitions have been renewed.
pub(super) fn describe_the_renewed_world(
    summary: &mut EndOfSeasonSummary,
    game: &Game,
    change: Option<&DivisionChange>,
    user_team_id: &str,
) {
    summary.user_division_move = match change {
        None => UserDivisionMove::Stayed,
        Some(change) if change.promoted => UserDivisionMove::Promoted,
        Some(_) => UserDivisionMove::Relegated,
    };
    match change {
        Some(change) => {
            summary.user_next_league_name = change.division_name.clone();
            summary.user_next_league_id = change.division_id.clone();
        }
        None => {
            if let Some(division) = user_division(game, user_team_id) {
                summary.user_next_league_name = division.name.clone();
                summary.user_next_league_id = division.id.clone();
            }
        }
    }
    summary.user_cup_results = user_cup_results(game, summary.season, user_team_id);
}

/// The domestic cups of `season` that the club played, from the immutable edition archive.
fn user_cup_results(game: &Game, season: u32, user_team_id: &str) -> Vec<UserCupResult> {
    let mut results: Vec<UserCupResult> = game
        .edition_archive
        .iter()
        .filter(|edition| edition.season == season)
        .filter_map(|edition| {
            let cup = game
                .competitions
                .iter()
                .find(|competition| competition.id == edition.competition_id)?;
            if cup.kind != CompetitionType::Cup {
                return None;
            }
            let entered = edition.participant_ids.iter().any(|id| id == user_team_id)
                || edition.fixtures.iter().any(|fixture| {
                    fixture.home_team_id == user_team_id || fixture.away_team_id == user_team_id
                });
            entered.then(|| UserCupResult {
                competition_id: cup.id.clone(),
                competition_name: cup.name.clone(),
                champion_id: edition.champion_id.clone(),
                champion_name: game
                    .teams
                    .iter()
                    .find(|team| team.id == edition.champion_id)
                    .map(|team| team.name.clone())
                    .unwrap_or_default(),
                user_won: edition.champion_id == user_team_id,
            })
        })
        .collect();
    results.sort_by(|a, b| a.competition_id.cmp(&b.competition_id));
    results
}
