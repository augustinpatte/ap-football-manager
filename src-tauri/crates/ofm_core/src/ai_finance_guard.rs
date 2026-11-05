//! What an AI club checks about its books before it takes on a cost.
//!
//! The board's wage policy (`contract_wage_policy`) answers one question — does the weekly wage
//! bill stay within the budget? A club can pass it and still sign what it cannot carry: the budget
//! says nothing about the cash in the bank, about what the club earns, or about how long a
//! contract runs. This is the second question, asked of every signing, renewal and purchase an AI
//! club makes on its own:
//!
//! - **the wage budget, without the absolute grace**: the board's policy tolerates a club that is
//!   already over its budget a further `max(3 %, 25 000)` a week at every decision. The 3 % is
//!   proportional; the 25 000 is a margin sized for clubs paying hundreds of thousands, which at a
//!   club whose whole budget is 13 000 is a licence to triple the bill one signing at a time. An AI
//!   club that adds to its bill stays under the policy's soft cap ([`WAGE_SOFT_CAP_PCT`] of the
//!   budget) or adds no more than the 3 % ([`LEGACY_OVER_BUDGET_GRACE_PCT`]) — the same policy,
//!   with the grace scaled to the club;
//! - **income it can count on**: its recurring sponsorship plus the gate its scheduled home league
//!   matches bring, at the attendance and price the game itself uses to estimate one;
//! - **the weekly result after the cost**: that income against the wage bill with the new wage;
//! - **a safety margin on a purchase**: a club that would run a weekly loss must hold
//!   [`SAFETY_WEEKS`] of that loss in cash after paying the fee — a season of it, since a fee is
//!   spent at once and the player is kept for several;
//! - **no new wages in debt**: a club with cash below zero takes on no new weekly cost.
//!
//! A cost that does not add to the bill (a renewal at the same wage, a cheaper replacement) always
//! passes: this is a guard against growth, not a reason to let a squad fall apart. The squad
//! floor's emergency signing does not ask it either — a club that cannot put a side out has no
//! margin worth protecting — but it chooses the cheapest player who will do (see `squad_floor`).

use crate::contract_wage_policy::{LEGACY_OVER_BUDGET_GRACE_PCT, WAGE_SOFT_CAP_PCT};
use crate::finances::calc_matchday;
use crate::game::Game;
use domain::league::FixtureCompetition;
use domain::team::Team;
use std::collections::HashMap;

/// Weeks of a weekly loss the club must hold in cash after a signing that makes it run one.
pub(crate) const SAFETY_WEEKS: i64 = 52;

/// Attendance and ticket price the game's own estimate of a gate uses (`finances`).
const ESTIMATED_ATTENDANCE: f64 = 0.76;
const ESTIMATED_TICKET: f64 = 20.0;

/// Scheduled home league matches in the current season, by club. A sweep over many clubs reads
/// it once; one decision reads it through [`SigningBooks::of`].
pub(crate) fn home_league_matches_by_club(game: &Game) -> HashMap<&str, i64> {
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if fixture.competition == FixtureCompetition::League {
                *counts.entry(fixture.home_team_id.as_str()).or_default() += 1;
            }
        }
    }
    counts
}

/// Weekly income a club can count on: its recurring sponsorship and its estimated gate, spread
/// over the year. Prize money is not counted — it is a result, not a plan.
pub(crate) fn expected_weekly_income(team: &Team, home_matches_this_season: i64) -> i64 {
    let sponsor = team
        .sponsorship
        .as_ref()
        .map_or(0, |sponsorship| sponsorship.base_value);
    let gate = calc_matchday(
        team.stadium_capacity,
        home_matches_this_season,
        ESTIMATED_ATTENDANCE,
        ESTIMATED_TICKET * crate::club_economy::ticket_price_factor(team.reputation),
    ) / 52;
    sponsor + gate
}

/// A club's books as a signing finds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SigningBooks {
    pub cash: i64,
    pub weekly_bill: i64,
    pub weekly_income: i64,
    pub wage_budget: i64,
}

impl SigningBooks {
    /// A club's books from its bill and its home matches this season, both worked out by a caller
    /// that judges many decisions at once.
    pub(crate) fn given(team: &Team, weekly_bill: i64, home_matches_this_season: i64) -> Self {
        Self {
            cash: team.finance,
            weekly_bill,
            weekly_income: expected_weekly_income(team, home_matches_this_season),
            wage_budget: team.wage_budget,
        }
    }

    /// Whether the club may add `extra_weekly` to its wage bill: not while it is in debt, and not
    /// past the policy's soft cap. The question for a signing on a free transfer or a renewal at a
    /// raise — a wage, with no fee to pay and no reason to ask for a year of cash in hand: a club
    /// that earns less than it pays is held in check by its budget, as clubs have always been.
    pub(crate) fn can_add_wage(&self, extra_weekly: i64) -> bool {
        if extra_weekly <= 0 {
            return true;
        }
        self.cash >= 0 && self.weekly_bill + extra_weekly <= self.bill_ceiling()
    }

    /// The most the weekly bill may reach by adding wages: the policy's soft cap, or, for a club
    /// already past it, what it pays plus the proportional grace.
    fn bill_ceiling(&self) -> i64 {
        (self.wage_budget * WAGE_SOFT_CAP_PCT / 100)
            .max(self.weekly_bill + self.wage_budget * LEGACY_OVER_BUDGET_GRACE_PCT / 100)
    }

    /// Whether the club can take on `extra_weekly` of wages, having paid `upfront` (a fee) now.
    pub(crate) fn can_carry(&self, extra_weekly: i64, upfront: i64) -> bool {
        if extra_weekly <= 0 && upfront <= 0 {
            return true;
        }
        let cash_after = self.cash - upfront.max(0);
        if extra_weekly > 0 && self.cash < 0 {
            return false;
        }
        if extra_weekly > 0 && self.weekly_bill + extra_weekly > self.bill_ceiling() {
            return false;
        }
        if cash_after < 0 {
            return false;
        }
        let weekly_result = self.weekly_income - (self.weekly_bill + extra_weekly.max(0));
        weekly_result >= 0 || cash_after >= -weekly_result * SAFETY_WEEKS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn books(cash: i64, weekly_bill: i64, weekly_income: i64) -> SigningBooks {
        SigningBooks {
            cash,
            weekly_bill,
            weekly_income,
            wage_budget: 10_000_000,
        }
    }

    /// Given a club that earns more than it pays
    /// When it takes on a wage that keeps it in profit
    /// Then it can, whatever its cash.
    #[test]
    fn a_wage_a_profitable_club_can_pay_out_of_income_is_carried() {
        assert!(books(0, 90_000, 100_000).can_carry(5_000, 0));
    }

    /// Given a club that would run a weekly loss of 10 000 after the signing
    /// When it holds more or less than a season of that loss in cash
    /// Then it can only with the season's worth: 520 000.
    #[test]
    fn a_loss_making_club_needs_a_season_of_the_loss_in_cash() {
        let club = |cash| books(cash, 108_000, 100_000);
        assert!(club(520_000).can_carry(2_000, 0));
        assert!(!club(519_999).can_carry(2_000, 0));
    }

    /// Given a club in debt
    /// When it takes on any new weekly cost
    /// Then it cannot; one that adds nothing still can.
    #[test]
    fn a_club_in_debt_adds_no_wage() {
        let club = books(-1, 50_000, 100_000);
        assert!(!club.can_carry(1, 0));
        assert!(club.can_carry(0, 0));
        assert!(club.can_carry(-500, 0));
    }

    /// Given a club already over its wage budget (13 000), with cash and income to spare
    /// When it takes on a wage
    /// Then it may add no more than the proportional grace, 3 % of the budget — not the 25 000 the
    ///      board's own policy allows — and a club under the soft cap may rise to it.
    #[test]
    fn a_club_over_its_wage_budget_adds_no_more_than_the_proportional_grace() {
        let club = SigningBooks {
            wage_budget: 13_000,
            ..books(50_000_000, 42_000, 1_000_000)
        };
        assert!(club.can_carry(390, 0)); // 3 % of 13 000
        assert!(!club.can_carry(391, 0));
        assert!(club.can_carry(-1_000, 0));
        let under = SigningBooks {
            weekly_bill: 12_000,
            ..club
        };
        assert!(under.can_carry(2_300, 0)); // 14 300 = 110 %
        assert!(!under.can_carry(2_400, 0));
    }

    /// Given a club that runs a weekly loss and holds little cash
    /// When it adds a wage without a fee
    /// Then it may, within its budget: the year of cash is asked of a purchase, not of a wage; and
    ///      it may not once it is in debt.
    #[test]
    fn a_wage_without_a_fee_asks_for_a_budget_and_no_debt() {
        let club = SigningBooks {
            wage_budget: 100_000,
            ..books(10_000, 100_000, 100_000)
        };
        assert!(club.can_add_wage(10_000)); // 110 000 = 110 % of the budget
        assert!(!club.can_add_wage(10_001));
        assert!(club.can_add_wage(-5_000));
        assert!(!SigningBooks { cash: -1, ..club }.can_add_wage(1));
        assert!(SigningBooks { cash: -1, ..club }.can_add_wage(0));
    }

    /// Given a fee that would take the club's cash below zero
    /// When it is paid
    /// Then the purchase is refused, wages or not.
    #[test]
    fn a_fee_larger_than_the_cash_is_refused() {
        assert!(!books(1_000_000, 10_000, 100_000).can_carry(0, 1_000_001));
        assert!(books(1_000_000, 10_000, 100_000).can_carry(0, 1_000_000));
    }

    /// Given a fee that leaves a club running a weekly loss with too little cash
    /// When a player is bought
    /// Then the cash left after the fee is what must cover the season's loss.
    #[test]
    fn the_cash_left_after_the_fee_is_what_covers_the_loss() {
        let club = books(1_000_000, 105_000, 100_000); // already -5 000 a week: needs 260 000
        assert!(club.can_carry(0, 740_000));
        assert!(!club.can_carry(0, 740_001));
    }
}
