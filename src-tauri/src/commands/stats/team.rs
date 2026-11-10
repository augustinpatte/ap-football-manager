use domain::stats::{TeamMatchStatsRecord, TeamStatsTotals};
use ofm_core::state::StateManager;

use super::dto::{
    TeamAdvancedMetricDto, TeamAdvancedPassMetricDto, TeamMatchHistoryEntryDto,
    TeamStatsOverviewDto, TeamStatsOverviewMetricsDto,
};
use super::shared::{
    calculate_average, calculate_pass_accuracy, competition_label, ensure_team_exists,
    opponent_name,
};

#[derive(Debug, Clone, Default)]
struct TeamAggregate {
    matches_played: u32,
    goals_for: u32,
    goals_against: u32,
    possession_total: u32,
    shots: u32,
    shots_on_target: u32,
    passes_completed: u32,
    passes_attempted: u32,
    tackles_won: u32,
    interceptions: u32,
    fouls_committed: u32,
}

/// What the history holds of a team: the totals folded from seasons whose lines are no longer
/// kept, plus every line still kept (see `domain::stats::StatsState::team_history_totals`).
fn aggregate_from_totals(totals: &TeamStatsTotals) -> TeamAggregate {
    TeamAggregate {
        matches_played: totals.matches_played,
        goals_for: totals.goals_for,
        goals_against: totals.goals_against,
        possession_total: totals.possession_total,
        shots: totals.shots,
        shots_on_target: totals.shots_on_target,
        passes_completed: totals.passes_completed,
        passes_attempted: totals.passes_attempted,
        tackles_won: totals.tackles_won,
        interceptions: totals.interceptions,
        fouls_committed: totals.fouls_committed,
    }
}

fn build_team_overview(aggregate: &TeamAggregate) -> TeamStatsOverviewDto {
    TeamStatsOverviewDto {
        matches_played: aggregate.matches_played,
        goals_for: aggregate.goals_for,
        goals_against: aggregate.goals_against,
        goal_difference: aggregate.goals_for as i32 - aggregate.goals_against as i32,
        possession_average: calculate_average(aggregate.possession_total, aggregate.matches_played),
        metrics: TeamStatsOverviewMetricsDto {
            shots: TeamAdvancedMetricDto {
                total: aggregate.shots,
                per_match: calculate_average(aggregate.shots, aggregate.matches_played),
            },
            shots_on_target: TeamAdvancedMetricDto {
                total: aggregate.shots_on_target,
                per_match: calculate_average(aggregate.shots_on_target, aggregate.matches_played),
            },
            passes: TeamAdvancedPassMetricDto {
                completed: aggregate.passes_completed,
                attempted: aggregate.passes_attempted,
                accuracy: calculate_pass_accuracy(
                    aggregate.passes_completed,
                    aggregate.passes_attempted,
                ),
            },
            tackles_won: TeamAdvancedMetricDto {
                total: aggregate.tackles_won,
                per_match: calculate_average(aggregate.tackles_won, aggregate.matches_played),
            },
            interceptions: TeamAdvancedMetricDto {
                total: aggregate.interceptions,
                per_match: calculate_average(aggregate.interceptions, aggregate.matches_played),
            },
            fouls_committed: TeamAdvancedMetricDto {
                total: aggregate.fouls_committed,
                per_match: calculate_average(aggregate.fouls_committed, aggregate.matches_played),
            },
        },
    }
}

fn to_team_history_dto(
    state: &StateManager,
    record: &TeamMatchStatsRecord,
) -> TeamMatchHistoryEntryDto {
    TeamMatchHistoryEntryDto {
        fixture_id: record.fixture_id.clone(),
        date: record.date.clone(),
        competition: competition_label(&record.competition),
        matchday: record.matchday,
        opponent_team_id: record.opponent_team_id.clone(),
        opponent_name: opponent_name(state, &record.opponent_team_id),
        goals_for: record.goals_for,
        goals_against: record.goals_against,
        possession_pct: record.possession_pct,
        shots: record.shots,
        shots_on_target: record.shots_on_target,
    }
}

pub fn get_team_stats_overview_internal(
    state: &StateManager,
    team_id: &str,
) -> Result<Option<TeamStatsOverviewDto>, String> {
    ensure_team_exists(state, team_id)?;

    let Some(totals) = state.get_stats_state(|stats| stats.team_history_totals(team_id)) else {
        return Ok(None);
    };

    Ok(totals.map(|totals| build_team_overview(&aggregate_from_totals(&totals))))
}

pub fn get_team_match_history_internal(
    state: &StateManager,
    team_id: &str,
    limit: Option<usize>,
) -> Result<Vec<TeamMatchHistoryEntryDto>, String> {
    ensure_team_exists(state, team_id)?;

    let Some(mut history) = state.get_stats_state(|stats| {
        stats
            .team_matches
            .iter()
            .filter(|record| record.team_id == team_id)
            .cloned()
            .collect::<Vec<_>>()
    }) else {
        return Ok(Vec::new());
    };

    history.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then(right.matchday.cmp(&left.matchday))
            .then(right.fixture_id.cmp(&left.fixture_id))
    });

    let limit = limit.unwrap_or(5);
    Ok(history
        .into_iter()
        .take(limit)
        .map(|record| to_team_history_dto(state, &record))
        .collect())
}
