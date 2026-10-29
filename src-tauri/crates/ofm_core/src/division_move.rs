//! What a promotion or a relegation means to an AI club's market and books (mission 12).
//!
//! The signal is **read, not stored**: the edition archive (already saved with the game) records,
//! for each division boundary and season, which clubs went up and which went down. A club is
//! `Promoted` or `Relegated` for the whole season that follows the rollover, and nothing is added
//! to the save format. Berth moves between pyramids carry no such record and give no signal.
//!
//! Two things use it:
//!
//! - the **books**, once, when the divisions are redrawn ([`rebase_wage_budgets`]): the board
//!   moves the wage budget towards what the clubs of the new division plan on, by a bounded step
//!   on top of the yearly review. Nothing is cut from a contract: a relegated club that is over its
//!   new budget stops adding to the excess, and the market's existing rules (surplus, unwanted,
//!   loan lists) take the rest;
//! - the **market sweep** (`transfers::market`), through [`DivisionMove`]: a promoted club
//!   shops for starters, a relegated one is held to its wage budget.

use std::collections::HashMap;

use crate::game::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivisionMove {
    Promoted,
    Relegated,
}

/// The largest extra step, per cent of the old budget, the board takes at a division change on
/// top of the yearly review's own limits.
pub(crate) const EXTRA_STEP_PCT: i64 = 12;

/// A relegated club plans on this share of what the median club of its new division plans on, at
/// most: it keeps some of its old scale, but not all of it.
pub(crate) const RELEGATED_SCALE_PCT: i64 = 140;
/// A promoted club moves towards this share of its new division's median budget (never beyond the
/// extra step), so a step up in division is a step in pay, not a leap.
pub(crate) const PROMOTED_SCALE_PCT: i64 = 75;

/// Clubs moved by the latest season's division boundaries.
pub fn recent_division_moves(game: &Game) -> HashMap<String, DivisionMove> {
    let latest = game
        .edition_archive
        .iter()
        .map(|edition| edition.season)
        .max();
    let mut moves = HashMap::new();
    let Some(latest) = latest else {
        return moves;
    };
    for edition in game
        .edition_archive
        .iter()
        .filter(|edition| edition.season == latest)
    {
        let Some(movements) = edition.movements.as_ref() else {
            continue;
        };
        for club in movements.promoted() {
            moves.insert(club.clone(), DivisionMove::Promoted);
        }
        for club in movements.relegated() {
            moves.insert(club.clone(), DivisionMove::Relegated);
        }
    }
    moves
}

/// The budget a division change moves a club to: `old` towards `reference` (the new division's
/// median budget scaled), by at most [`EXTRA_STEP_PCT`] of `old` in either direction. A promoted
/// club only ever rises, a relegated club only ever falls.
pub(crate) fn rebased_budget(old: i64, reference: i64, change: DivisionMove) -> i64 {
    let step = old * EXTRA_STEP_PCT / 100;
    match change {
        DivisionMove::Promoted => {
            let target = reference * PROMOTED_SCALE_PCT / 100;
            (old + step).min(target).max(old)
        }
        DivisionMove::Relegated => {
            let target = reference * RELEGATED_SCALE_PCT / 100;
            (old - step).max(target).min(old)
        }
    }
}

fn median(values: &mut [i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// Called once the divisions have been redrawn for the new season: moves the wage budget of each
/// promoted or relegated AI club towards its new division. The player's club is left alone, as in
/// the yearly review: its board is the player's to negotiate with. A promoted club in debt gets
/// no raise.
pub(crate) fn rebase_wage_budgets(game: &mut Game) {
    let moves = recent_division_moves(game);
    if moves.is_empty() {
        return;
    }
    let user = game.manager.team_id.clone();
    let budget_of: HashMap<&str, i64> = game
        .teams
        .iter()
        .map(|team| (team.id.as_str(), team.wage_budget))
        .collect();
    let mut changes: Vec<(String, i64)> = Vec::new();
    for (club, change) in &moves {
        if Some(club) == user.as_ref() {
            continue;
        }
        let Some(&old) = budget_of.get(club.as_str()) else {
            continue;
        };
        let Some(division) = crate::end_of_season::club_league_table(game, club) else {
            continue;
        };
        let mut peers: Vec<i64> = division
            .participant_ids
            .iter()
            .filter(|id| *id != club)
            .filter_map(|id| budget_of.get(id.as_str()).copied())
            .filter(|budget| *budget > 0)
            .collect();
        if peers.is_empty() || old <= 0 {
            continue;
        }
        let reference = median(&mut peers);
        let new = rebased_budget(old, reference, *change);
        if new != old {
            changes.push((club.clone(), new));
        }
    }
    for (club, budget) in changes {
        if let Some(team) = game.teams.iter_mut().find(|team| team.id == club) {
            // A promoted club in debt is not rewarded with a bigger bill.
            if team.finance < 0 && budget > team.wage_budget {
                continue;
            }
            team.wage_budget = budget;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_promoted_club_rises_by_at_most_the_step_and_never_beyond_its_new_division() {
        assert_eq!(rebased_budget(100, 1_000, DivisionMove::Promoted), 112);
        assert_eq!(rebased_budget(100, 140, DivisionMove::Promoted), 105);
        // Already above what its new division plans on: unchanged, never cut by a promotion.
        assert_eq!(rebased_budget(100, 60, DivisionMove::Promoted), 100);
    }

    #[test]
    fn a_relegated_club_falls_by_at_most_the_step_and_never_rises() {
        assert_eq!(rebased_budget(100, 10, DivisionMove::Relegated), 88);
        assert_eq!(rebased_budget(100, 70, DivisionMove::Relegated), 98);
        assert_eq!(rebased_budget(100, 1_000, DivisionMove::Relegated), 100);
    }

    #[test]
    fn the_signal_is_read_from_the_latest_archived_season_only() {
        use chrono::{TimeZone, Utc};
        use domain::competition_rules::BoundaryMovements;
        use domain::edition_archive::CompletedEdition;
        use domain::manager::Manager;

        let edition = |season: u32, up: &str, down: &str| CompletedEdition {
            competition_id: "d1".into(),
            season,
            completed_on: String::new(),
            champion_id: String::new(),
            participant_ids: vec![],
            standings: vec![],
            groups: vec![],
            knockout_rounds: vec![],
            fixtures: vec![],
            movements: Some(BoundaryMovements {
                season,
                promoted_direct: vec![up.to_string()],
                relegated_direct: vec![down.to_string()],
                ..Default::default()
            }),
        };
        let mut game = Game::new(
            crate::clock::GameClock::new(Utc.with_ymd_and_hms(2027, 7, 1, 12, 0, 0).unwrap()),
            Manager::new(
                "m".into(),
                "A".into(),
                "B".into(),
                "1980-01-01".into(),
                "England".into(),
            ),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        game.edition_archive = vec![
            edition(2025, "old-up", "old-down"),
            edition(2026, "up", "down"),
        ];
        let moves = recent_division_moves(&game);
        assert_eq!(moves.get("up"), Some(&DivisionMove::Promoted));
        assert_eq!(moves.get("down"), Some(&DivisionMove::Relegated));
        assert!(
            !moves.contains_key("old-up"),
            "last season's moves are no longer news"
        );
        game.edition_archive.clear();
        assert!(recent_division_moves(&game).is_empty());
    }
}
