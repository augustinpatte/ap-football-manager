//! The yearly review of an AI club's wage budget.
//!
//! A club's `wage_budget` was set once, when the world was made, and never revisited: a club whose
//! gate grew, or whose squad cost more to keep each time a contract was renewed, kept the budget
//! of its first season and sat at 200 to 350 % of it by the tenth, while holding a hundred times
//! that in cash. The board of a real club resets the figure every summer from what the club
//! earned. That is all this does.
//!
//! - **what is counted**: the club's matchday, sponsorship and prize income over the last 365
//!   days, as the cash journal recorded it. Transfer fees, loan fees and one-off payments are not
//!   income the board can plan wages on;
//! - **what share goes to wages**: a small club lives close to its income, an elite one keeps
//!   more of it ([`wage_share`], 80 % falling to 55 % with reputation); a club in debt is held
//!   to 85 % of that, a club holding more than a year of income in cash is allowed 5 % more;
//! - **how fast it moves**: at most [`MAX_RISE_PCT`] up and [`MAX_CUT_PCT`] down a year, so a
//!   one-off season does not reset a squad's pay scale and a club is never ordered to shed half
//!   its wage bill overnight. Wages above the new budget are not cut: contracts run their course
//!   and the wage policy keeps the club from adding to the excess.
//!
//! The player's own club is left alone: its board is the player's to negotiate with.

use std::collections::HashMap;

use chrono::Duration;
use domain::finance::CashKind;

use crate::game::Game;

pub(crate) const MAX_RISE_PCT: i64 = 10;
pub(crate) const MAX_CUT_PCT: i64 = 5;
/// A club in debt is ordered to come back to its income faster than a solvent one drifts down.
pub(crate) const MAX_CUT_IN_DEBT_PCT: i64 = 15;
const WEEKS_PER_YEAR: i64 = 52;

/// The share of income an AI club plans to spend on wages, from its reputation.
pub(crate) fn wage_share(reputation: u32) -> f64 {
    let stature = ((f64::from(reputation) - 250.0) / 650.0).clamp(0.0, 1.0);
    0.80 - 0.25 * stature
}

/// The wage budget a club with this income, cash and reputation plans on, before the yearly limits.
pub(crate) fn target_weekly_budget(yearly_income: i64, cash: i64, reputation: u32) -> i64 {
    let mut share = wage_share(reputation);
    if cash < 0 {
        share *= 0.85;
    } else if yearly_income > 0 && cash > yearly_income {
        share *= 1.05;
    }
    ((yearly_income.max(0) / WEEKS_PER_YEAR) as f64 * share) as i64
}

/// The budget after the yearly limits: it moves towards `target` by no more than the caps.
pub(crate) fn limited_budget(old: i64, target: i64, in_debt: bool) -> i64 {
    let cut = if in_debt {
        MAX_CUT_IN_DEBT_PCT
    } else {
        MAX_CUT_PCT
    };
    let floor = old - old * cut / 100;
    let ceiling = old + old * MAX_RISE_PCT / 100;
    target.clamp(floor.min(ceiling), ceiling)
}

pub(crate) fn review_wage_budgets(game: &mut Game) {
    let since = (game.clock.current_date.date_naive() - Duration::days(365))
        .format("%Y-%m-%d")
        .to_string();
    let mut income: HashMap<&str, i64> = HashMap::new();
    for post in game.cash_journal.iter() {
        if post.date.as_str() > since.as_str()
            && matches!(
                post.kind,
                CashKind::Matchday | CashKind::Sponsorship | CashKind::PrizeMoney
            )
        {
            *income.entry(post.club_id.as_str()).or_default() += post.amount;
        }
    }
    let reviewed: Vec<(String, i64)> = game
        .teams
        .iter()
        .filter(|team| Some(team.id.as_str()) != game.manager.team_id.as_deref())
        .filter(|team| team.wage_budget > 0)
        .map(|team| {
            let yearly = income.get(team.id.as_str()).copied().unwrap_or(0);
            let target = target_weekly_budget(yearly, team.finance, team.reputation);
            (
                team.id.clone(),
                limited_budget(team.wage_budget, target, team.finance < 0),
            )
        })
        .collect();
    for (team_id, budget) in reviewed {
        if let Some(team) = game.teams.iter_mut().find(|team| team.id == team_id) {
            team.wage_budget = budget;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_club_plans_more_of_its_income_on_wages_than_an_elite_one() {
        assert!(wage_share(250) > wage_share(900));
        assert!((wage_share(250) - 0.80).abs() < 1e-9);
        assert!((wage_share(900) - 0.55).abs() < 1e-9);
    }

    #[test]
    fn debt_lowers_the_budget_and_a_large_reserve_raises_it() {
        let normal = target_weekly_budget(52_000_000, 1_000_000, 500);
        assert!(target_weekly_budget(52_000_000, -1, 500) < normal);
        assert!(target_weekly_budget(52_000_000, 60_000_000, 500) > normal);
    }

    #[test]
    fn the_budget_moves_by_at_most_the_yearly_limits() {
        assert_eq!(limited_budget(100_000, 1_000_000, false), 110_000);
        assert_eq!(limited_budget(100_000, 10_000, false), 95_000);
        assert_eq!(limited_budget(100_000, 103_000, false), 103_000);
    }

    /// Given a club in debt whose income has fallen far below its budget, when the board resets
    /// the budget, then it may cut three times as fast as a solvent club (15 % against 5 %), so a
    /// bill that outgrew the club's income is closed in years, not decades. The rise is unchanged.
    #[test]
    fn a_club_in_debt_may_cut_its_budget_faster() {
        assert_eq!(limited_budget(100_000, 10_000, true), 85_000);
        assert_eq!(limited_budget(100_000, 10_000, false), 95_000);
        assert_eq!(limited_budget(100_000, 1_000_000, true), 110_000);
    }
}
