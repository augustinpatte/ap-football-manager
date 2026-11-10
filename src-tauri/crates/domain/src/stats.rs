use crate::league::FixtureCompetition;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Seasons of match-by-match lines a career keeps (the current one and the four before it).
///
/// A line costs about 500 bytes of memory and 45 on disk, and a season plays some 50,000 of them
/// in a populated world; kept for every season, thirty seasons hold over 800 MB. What the screens
/// read of older seasons is only what they add up to, which [`StatsState::fold_old_seasons`]
/// keeps; the recent matches they list are never older than this.
pub const SEASONS_OF_LINES_KEPT: u32 = 5;

/// The most recent lines of one player or team kept whatever their age, so "recent matches" of
/// someone who has not played for years still lists his last ones.
pub const LATEST_LINES_KEPT_PER_SUBJECT: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StatsState {
    pub player_matches: Vec<PlayerMatchStatsRecord>,
    pub team_matches: Vec<TeamMatchStatsRecord>,
    /// What the lines of seasons no longer kept add up to, per player.
    pub player_totals: Vec<PlayerStatsTotals>,
    /// What the lines of seasons no longer kept add up to, per team.
    pub team_totals: Vec<TeamStatsTotals>,
    /// How many times lines have been folded into the totals. The file keeps the epoch its totals
    /// were written at; a memory that is ahead of it is saved in full, because a fold removes lines
    /// from anywhere in the history, not just its end.
    pub totals_epoch: u64,
    /// The newest season a line has been appended for: when a newer one arrives, the seasons that
    /// fall out of the window are folded. Not saved; the first append after a load sets it.
    #[serde(skip)]
    pub newest_season: u32,
}

/// The sums the statistics overviews read, over lines that are no longer kept one by one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PlayerStatsTotals {
    pub player_id: String,
    pub minutes_played: u32,
    pub shots: u32,
    pub shots_on_target: u32,
    pub passes_completed: u32,
    pub passes_attempted: u32,
    pub tackles_won: u32,
    pub interceptions: u32,
    pub fouls_committed: u32,
}

impl PlayerStatsTotals {
    pub fn add(&mut self, record: &PlayerMatchStatsRecord) {
        self.minutes_played += u32::from(record.minutes_played);
        self.shots += u32::from(record.shots);
        self.shots_on_target += u32::from(record.shots_on_target);
        self.passes_completed += u32::from(record.passes_completed);
        self.passes_attempted += u32::from(record.passes_attempted);
        self.tackles_won += u32::from(record.tackles_won);
        self.interceptions += u32::from(record.interceptions);
        self.fouls_committed += u32::from(record.fouls_committed);
    }
}

/// The sums the team overview reads, over lines that are no longer kept one by one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TeamStatsTotals {
    pub team_id: String,
    pub matches_played: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub possession_total: u32,
    pub shots: u32,
    pub shots_on_target: u32,
    pub passes_completed: u32,
    pub passes_attempted: u32,
    pub tackles_won: u32,
    pub interceptions: u32,
    pub fouls_committed: u32,
}

impl TeamStatsTotals {
    pub fn add(&mut self, record: &TeamMatchStatsRecord) {
        self.matches_played += 1;
        self.goals_for += u32::from(record.goals_for);
        self.goals_against += u32::from(record.goals_against);
        self.possession_total += u32::from(record.possession_pct);
        self.shots += u32::from(record.shots);
        self.shots_on_target += u32::from(record.shots_on_target);
        self.passes_completed += u32::from(record.passes_completed);
        self.passes_attempted += u32::from(record.passes_attempted);
        self.tackles_won += u32::from(record.tackles_won);
        self.interceptions += u32::from(record.interceptions);
        self.fouls_committed += u32::from(record.fouls_committed);
    }
}

impl StatsState {
    /// A history of exactly these lines, with nothing folded.
    pub fn with_lines(
        player_matches: Vec<PlayerMatchStatsRecord>,
        team_matches: Vec<TeamMatchStatsRecord>,
    ) -> Self {
        Self {
            player_matches,
            team_matches,
            ..Self::default()
        }
    }

    /// Adds `other`'s lines (a capture: its totals and epoch are not carried over); when they reach a newer season than any before, the seasons that have
    /// fallen out of the window of [`SEASONS_OF_LINES_KEPT`] are folded into the totals.
    pub fn append(&mut self, other: StatsState) {
        let newest = other
            .player_matches
            .iter()
            .map(|r| r.season)
            .chain(other.team_matches.iter().map(|r| r.season))
            .max()
            .unwrap_or(0);
        self.player_matches.extend(other.player_matches);
        self.team_matches.extend(other.team_matches);
        if newest > self.newest_season {
            self.newest_season = newest;
            self.fold_old_seasons(newest);
        }
    }

    /// Folds the lines older than the window ending at `current_season` into the totals, except the
    /// latest [`LATEST_LINES_KEPT_PER_SUBJECT`] of each player and team. What the overviews add up
    /// is unchanged by it; the lines it removes are the ones no screen lists.
    pub fn fold_old_seasons(&mut self, current_season: u32) {
        let first_kept = current_season.saturating_sub(SEASONS_OF_LINES_KEPT - 1);
        if first_kept == 0 {
            return;
        }
        let lines_before = self.player_matches.len() + self.team_matches.len();
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut folded_players = vec![false; self.player_matches.len()];
        for (index, record) in self.player_matches.iter().enumerate().rev() {
            let latest = seen.entry(record.player_id.as_str()).or_default();
            *latest += 1;
            folded_players[index] =
                record.season < first_kept && *latest > LATEST_LINES_KEPT_PER_SUBJECT;
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut folded_teams = vec![false; self.team_matches.len()];
        for (index, record) in self.team_matches.iter().enumerate().rev() {
            let latest = seen.entry(record.team_id.as_str()).or_default();
            *latest += 1;
            folded_teams[index] =
                record.season < first_kept && *latest > LATEST_LINES_KEPT_PER_SUBJECT;
        }

        let mut players: HashMap<String, PlayerStatsTotals> = self
            .player_totals
            .drain(..)
            .map(|totals| (totals.player_id.clone(), totals))
            .collect();
        let mut index = 0;
        self.player_matches.retain(|record| {
            let fold = folded_players[index];
            index += 1;
            if fold {
                players
                    .entry(record.player_id.clone())
                    .or_insert_with(|| PlayerStatsTotals {
                        player_id: record.player_id.clone(),
                        ..PlayerStatsTotals::default()
                    })
                    .add(record);
            }
            !fold
        });
        let mut teams: HashMap<String, TeamStatsTotals> = self
            .team_totals
            .drain(..)
            .map(|totals| (totals.team_id.clone(), totals))
            .collect();
        let mut index = 0;
        self.team_matches.retain(|record| {
            let fold = folded_teams[index];
            index += 1;
            if fold {
                teams
                    .entry(record.team_id.clone())
                    .or_insert_with(|| TeamStatsTotals {
                        team_id: record.team_id.clone(),
                        ..TeamStatsTotals::default()
                    })
                    .add(record);
            }
            !fold
        });
        if self.player_matches.len() + self.team_matches.len() != lines_before {
            self.totals_epoch += 1;
        }
        // Sorted, so the same history always saves the same bytes.
        self.player_totals = players.into_values().collect();
        self.player_totals
            .sort_by(|a, b| a.player_id.cmp(&b.player_id));
        self.team_totals = teams.into_values().collect();
        self.team_totals.sort_by(|a, b| a.team_id.cmp(&b.team_id));
    }

    /// The totals of each of `player_ids` that has any history: what was folded plus every line
    /// still kept. A player with no history at all is not in the result.
    pub fn player_history_totals(
        &self,
        player_ids: &std::collections::HashSet<String>,
    ) -> HashMap<String, PlayerStatsTotals> {
        let mut totals: HashMap<String, PlayerStatsTotals> = self
            .player_totals
            .iter()
            .filter(|t| player_ids.contains(&t.player_id))
            .map(|t| (t.player_id.clone(), t.clone()))
            .collect();
        for record in &self.player_matches {
            if player_ids.contains(&record.player_id) {
                totals
                    .entry(record.player_id.clone())
                    .or_insert_with(|| PlayerStatsTotals {
                        player_id: record.player_id.clone(),
                        ..PlayerStatsTotals::default()
                    })
                    .add(record);
            }
        }
        totals
    }

    /// The totals of `team_id`: what was folded plus every line still kept. `None` when the team
    /// has no history at all.
    pub fn team_history_totals(&self, team_id: &str) -> Option<TeamStatsTotals> {
        let mut totals = self
            .team_totals
            .iter()
            .find(|t| t.team_id == team_id)
            .cloned();
        for record in self.team_matches.iter().filter(|r| r.team_id == team_id) {
            totals
                .get_or_insert_with(|| TeamStatsTotals {
                    team_id: team_id.to_string(),
                    ..TeamStatsTotals::default()
                })
                .add(record);
        }
        totals
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerMatchStatsRecord {
    pub fixture_id: String,
    pub season: u32,
    pub matchday: u32,
    pub date: String,
    pub competition: FixtureCompetition,
    pub player_id: String,
    pub team_id: String,
    pub opponent_team_id: String,
    pub home_team_id: String,
    pub away_team_id: String,
    pub home_goals: u8,
    pub away_goals: u8,
    pub minutes_played: u8,
    pub goals: u8,
    pub assists: u8,
    pub shots: u8,
    pub shots_on_target: u8,
    pub passes_completed: u8,
    pub passes_attempted: u8,
    pub tackles_won: u8,
    pub interceptions: u8,
    pub fouls_committed: u8,
    pub yellow_cards: u8,
    pub red_cards: u8,
    pub rating: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamMatchStatsRecord {
    pub fixture_id: String,
    pub season: u32,
    pub matchday: u32,
    pub date: String,
    pub competition: FixtureCompetition,
    pub team_id: String,
    pub opponent_team_id: String,
    pub home_team_id: String,
    pub away_team_id: String,
    pub goals_for: u8,
    pub goals_against: u8,
    pub possession_pct: u8,
    pub shots: u16,
    pub shots_on_target: u16,
    pub passes_completed: u16,
    pub passes_attempted: u16,
    pub tackles_won: u16,
    pub interceptions: u16,
    pub fouls_committed: u16,
    pub yellow_cards: u8,
    pub red_cards: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn line(player: &str, season: u32, matchday: u32, minutes: u8) -> PlayerMatchStatsRecord {
        PlayerMatchStatsRecord {
            fixture_id: format!("{season}-{matchday}"),
            season,
            matchday,
            date: format!("{season}-09-{matchday:02}"),
            competition: FixtureCompetition::League,
            player_id: player.to_string(),
            team_id: "club".to_string(),
            opponent_team_id: "other".to_string(),
            home_team_id: "club".to_string(),
            away_team_id: "other".to_string(),
            home_goals: 1,
            away_goals: 0,
            minutes_played: minutes,
            goals: 0,
            assists: 0,
            shots: 2,
            shots_on_target: 1,
            passes_completed: 20,
            passes_attempted: 25,
            tackles_won: 1,
            interceptions: 1,
            fouls_committed: 1,
            yellow_cards: 0,
            red_cards: 0,
            rating: 6.5,
        }
    }

    fn team_line(team: &str, season: u32, matchday: u32) -> TeamMatchStatsRecord {
        TeamMatchStatsRecord {
            fixture_id: format!("{season}-{matchday}"),
            season,
            matchday,
            date: format!("{season}-09-{matchday:02}"),
            competition: FixtureCompetition::League,
            team_id: team.to_string(),
            opponent_team_id: "other".to_string(),
            home_team_id: team.to_string(),
            away_team_id: "other".to_string(),
            goals_for: 2,
            goals_against: 1,
            possession_pct: 55,
            shots: 12,
            shots_on_target: 5,
            passes_completed: 300,
            passes_attempted: 350,
            tackles_won: 10,
            interceptions: 7,
            fouls_committed: 9,
            yellow_cards: 1,
            red_cards: 0,
        }
    }

    /// Ten seasons of ten matches for two players and a team, one season at a time.
    fn ten_seasons() -> StatsState {
        let mut stats = StatsState::default();
        for season in 2030..2040 {
            for matchday in 1..=10 {
                stats.append(StatsState::with_lines(
                    vec![
                        line("regular", season, matchday, 90),
                        line("another", season, matchday, 45),
                    ],
                    vec![team_line("club", season, matchday)],
                ));
            }
        }
        stats
    }

    fn all(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    /// Given ten seasons of match lines appended season by season,
    /// When the history is read as the overviews read it,
    /// Then the sums are those of every line ever played, though only five seasons of lines are kept.
    #[test]
    fn folding_old_seasons_changes_no_sum_an_overview_reads() {
        let kept = ten_seasons();
        let player = &kept.player_history_totals(&all(&["regular"]))["regular"];
        assert_eq!(player.minutes_played, 100 * 90);
        assert_eq!(player.shots, 100 * 2);
        assert_eq!(player.passes_attempted, 100 * 25);
        let team = kept.team_history_totals("club").unwrap();
        assert_eq!(team.matches_played, 100);
        assert_eq!(team.goals_for, 200);
        assert_eq!(team.possession_total, 5_500);

        assert_eq!(
            kept.player_matches.len(),
            2 * 5 * 10,
            "five seasons of lines are kept"
        );
        assert!(kept.player_matches.iter().all(|r| r.season >= 2035));
        assert_eq!(kept.team_matches.len(), 5 * 10);
    }

    /// Given a player whose last lines are all older than the window,
    /// When old seasons are folded,
    /// Then his latest five lines are kept, so "recent matches" still lists them.
    #[test]
    fn a_player_who_stopped_playing_keeps_his_latest_lines() {
        let mut stats = StatsState::default();
        for matchday in 1..=8 {
            stats.append(StatsState::with_lines(
                vec![line("retired", 2030, matchday, 90)],
                vec![],
            ));
        }
        stats.append(StatsState::with_lines(
            vec![line("regular", 2040, 1, 90)],
            vec![],
        ));

        let retired: Vec<u32> = stats
            .player_matches
            .iter()
            .filter(|r| r.player_id == "retired")
            .map(|r| r.matchday)
            .collect();
        assert_eq!(retired, vec![4, 5, 6, 7, 8]);
        assert_eq!(
            stats.player_history_totals(&all(&["retired"]))["retired"].minutes_played,
            8 * 90
        );
    }

    /// Given a season still being played,
    /// When more of its lines are appended,
    /// Then nothing is folded: folding happens when a newer season arrives, not on every match.
    #[test]
    fn lines_of_the_same_season_fold_nothing() {
        let mut stats = ten_seasons();
        let kept = stats.player_matches.len();
        stats.append(StatsState::with_lines(
            vec![line("regular", 2039, 99, 90)],
            vec![],
        ));
        assert_eq!(stats.player_matches.len(), kept + 1);
    }

    /// Given a history already folded,
    /// When it is folded again for the same season,
    /// Then nothing changes.
    #[test]
    fn folding_twice_is_folding_once() {
        let mut stats = ten_seasons();
        let before = (stats.player_matches.len(), stats.player_totals.clone());
        stats.fold_old_seasons(2039);
        assert_eq!(
            (stats.player_matches.len(), stats.player_totals.clone()),
            before
        );
    }
}
