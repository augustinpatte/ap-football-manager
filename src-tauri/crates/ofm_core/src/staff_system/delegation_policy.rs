//! What the assistant may commit to on his own: the one place that answers "does this need the
//! manager?".
//!
//! Every delegated step that spends money or binds the club goes through [`authorize`] before it
//! is carried out, so a duty's own setting and the manager's spending limits cannot disagree. The
//! transfer engine is meant to call it too: a delegated signing is a [`Spend::Transfer`].

use domain::delegation::{DelegationState, Duty};

/// Weeks a weekly wage is committed over a year.
const WEEKS_PER_YEAR: i64 = 52;

/// A decision that commits the club's money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spend {
    /// Hiring or renewing a staff member.
    StaffContract { weekly_wage: u32, years: u8 },
    /// Renewing a player.
    PlayerContract { weekly_wage: u32, years: u8 },
    /// Paying a transfer fee (and the contract that comes with it).
    Transfer {
        fee: i64,
        weekly_wage: u32,
        years: u8,
    },
}

/// The answer to "may the assistant do this by himself?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// Yes: the duty is delegated, no approval is asked for, and the cost is within the limits.
    Auto,
    /// Not alone: the duty asks for approval, or the cost is above a limit. Propose it.
    NeedsApproval(Reason),
    /// The duty is not delegated at all; the assistant does not act.
    NotDelegated,
}

/// Why a step is not made on the assistant's own authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The duty is set to "ask me before major decisions".
    ApprovalRequired,
    /// The wage is above the weekly ceiling for that kind of contract.
    WageAboveLimit,
    /// The whole contract is above the commitment ceiling.
    CommitmentAboveLimit,
    /// The fee is above the transfer ceiling (zero by default: transfers are the manager's), or,
    /// under Total authority, above the cash the club can spend.
    FeeAboveLimit,
    /// The duty acts alone, but the manager has not given the assistant Total authority, and
    /// selling or listing a player is not something a ceiling can measure: it is proposed.
    NoAuthorityToSell,
    /// Under Total authority: the club cannot carry the money this step commits it to (a transfer
    /// the wage room cannot take, a staff wage past the staff budget).
    BeyondTheClubsMeans,
}

/// What the club can carry, read from its books on the day: the authority Total gives the
/// assistant instead of a ceiling per deal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Books {
    /// Cash for a fee: the lesser of the transfer budget and the balance (never negative).
    pub transfer_cash: i64,
    /// Weekly wage the player wage budget still has room for (never negative).
    pub wage_room: i64,
    /// Weekly wage the staff budget still has room for (never negative).
    pub staff_room: i64,
}

/// May `duty` carry out `spend` without asking the manager, given what the club can carry?
///
/// Under Total authority (`limits.finance_bound`) the answer is the club's books: the step is
/// automatic when it fits the cash, the wage room and the staff budget, whatever its size. The
/// transfer engine, the board's wage policy, registration and the player's consent still apply to
/// the step itself, exactly as to the manager's. `wage_increase` is what the step adds to the
/// weekly wage bill (a signing adds his whole wage, a renewal only the raise). Without Total
/// authority this is [`authorize`], ceilings and all.
pub fn authorize_in_books(
    delegation: &DelegationState,
    duty: Duty,
    spend: Spend,
    books: &Books,
    wage_increase: u32,
) -> Authorization {
    if !delegation.limits.finance_bound {
        return authorize(delegation, duty, spend);
    }
    if !delegation.is_delegated(duty) {
        return Authorization::NotDelegated;
    }
    if delegation.needs_confirmation(duty) {
        return Authorization::NeedsApproval(Reason::ApprovalRequired);
    }
    let within = match spend {
        Spend::StaffContract { weekly_wage, .. } => i64::from(weekly_wage) <= books.staff_room,
        Spend::PlayerContract { .. } => i64::from(wage_increase) <= books.wage_room,
        Spend::Transfer { fee, .. } => {
            if fee > books.transfer_cash {
                return Authorization::NeedsApproval(Reason::FeeAboveLimit);
            }
            i64::from(wage_increase) <= books.wage_room
        }
    };
    if within {
        Authorization::Auto
    } else {
        Authorization::NeedsApproval(Reason::BeyondTheClubsMeans)
    }
}

/// May `duty` sell, counter or list a player by itself? Only with Total authority: every sale is a
/// major decision, and the manager's ceilings are about money going out, not about a player going.
pub fn authorize_sale(delegation: &DelegationState, duty: Duty) -> Authorization {
    if !delegation.is_delegated(duty) {
        return Authorization::NotDelegated;
    }
    if delegation.needs_confirmation(duty) {
        return Authorization::NeedsApproval(Reason::ApprovalRequired);
    }
    if !delegation.limits.finance_bound {
        return Authorization::NeedsApproval(Reason::NoAuthorityToSell);
    }
    Authorization::Auto
}

/// Total a contract commits the club to.
pub fn commitment(weekly_wage: u32, years: u8) -> i64 {
    i64::from(weekly_wage) * WEEKS_PER_YEAR * i64::from(years)
}

/// May `duty` carry out `spend` without asking the manager?
///
/// Pure: reads the delegation state only. A step the duty would never take is still judged by
/// the limits, so a limit cannot be bypassed by choosing a different duty.
pub fn authorize(delegation: &DelegationState, duty: Duty, spend: Spend) -> Authorization {
    if !delegation.is_delegated(duty) {
        return Authorization::NotDelegated;
    }
    if delegation.needs_confirmation(duty) {
        return Authorization::NeedsApproval(Reason::ApprovalRequired);
    }
    let limits = &delegation.limits;
    let (weekly_wage, years, fee) = match spend {
        Spend::StaffContract { weekly_wage, years } => (weekly_wage, years, 0),
        Spend::PlayerContract { weekly_wage, years } => (weekly_wage, years, 0),
        Spend::Transfer {
            fee,
            weekly_wage,
            years,
        } => (weekly_wage, years, fee),
    };
    let wage_ceiling = match spend {
        Spend::StaffContract { .. } => limits.max_staff_wage,
        Spend::PlayerContract { .. } | Spend::Transfer { .. } => limits.max_player_wage,
    };
    if weekly_wage > wage_ceiling {
        return Authorization::NeedsApproval(Reason::WageAboveLimit);
    }
    // A zero ceiling is "every signing is the manager's", fee or no fee: a free agent costs no
    // fee but binds the club to a contract all the same, so a fee of zero must not slip under it.
    if matches!(spend, Spend::Transfer { .. })
        && (limits.max_transfer_fee <= 0 || fee > limits.max_transfer_fee)
    {
        return Authorization::NeedsApproval(Reason::FeeAboveLimit);
    }
    if commitment(weekly_wage, years) + fee.max(0) > limits.max_commitment {
        return Authorization::NeedsApproval(Reason::CommitmentAboveLimit);
    }
    Authorization::Auto
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> DelegationState {
        let mut state = DelegationState::default();
        state.set(Duty::StaffRecruitment, true, false);
        state.set(Duty::PlayerRenewals, true, false);
        state
    }

    /// Given a duty that is not delegated, when a spend is judged, then the assistant does not act.
    #[test]
    fn an_undelegated_duty_never_acts() {
        let state = DelegationState::default();
        let spend = Spend::StaffContract {
            weekly_wage: 1,
            years: 1,
        };
        assert_eq!(
            authorize(&state, Duty::StaffRecruitment, spend),
            Authorization::NotDelegated
        );
    }

    /// Given a duty that asks for approval, when even a trivial spend is judged, then it is
    /// proposed.
    #[test]
    fn a_duty_that_asks_for_approval_proposes_even_a_cheap_step() {
        let mut state = full();
        state.set(Duty::StaffRecruitment, true, true);
        let spend = Spend::StaffContract {
            weekly_wage: 100,
            years: 1,
        };
        assert_eq!(
            authorize(&state, Duty::StaffRecruitment, spend),
            Authorization::NeedsApproval(Reason::ApprovalRequired)
        );
    }

    /// Given full delegation, when spends are judged either side of each ceiling, then only the
    /// ones within every ceiling are automatic.
    #[test]
    fn each_ceiling_turns_an_automatic_step_into_a_proposal() {
        let state = full();
        let staff = |weekly_wage| Spend::StaffContract {
            weekly_wage,
            years: 2,
        };
        assert_eq!(
            authorize(&state, Duty::StaffRecruitment, staff(5_000)),
            Authorization::Auto
        );
        assert_eq!(
            authorize(&state, Duty::StaffRecruitment, staff(5_001)),
            Authorization::NeedsApproval(Reason::WageAboveLimit)
        );
        let player = |weekly_wage, years| Spend::PlayerContract { weekly_wage, years };
        assert_eq!(
            authorize(&state, Duty::PlayerRenewals, player(15_000, 3)),
            Authorization::Auto
        );
        // 15 000 x 52 x 4 years = 3 120 000, above the 3 000 000 commitment ceiling.
        assert_eq!(
            authorize(&state, Duty::PlayerRenewals, player(15_000, 4)),
            Authorization::NeedsApproval(Reason::CommitmentAboveLimit)
        );
        assert_eq!(
            authorize(&state, Duty::PlayerRenewals, player(15_001, 1)),
            Authorization::NeedsApproval(Reason::WageAboveLimit)
        );
    }

    /// Given the default limits, when a transfer is judged, then it always goes to the manager;
    /// once the manager raises the fee ceiling, a fee within it is automatic.
    #[test]
    fn transfers_are_the_managers_until_a_fee_ceiling_is_set() {
        let mut state = full();
        state.set(Duty::Scouting, true, false);
        let deal = Spend::Transfer {
            fee: 500_000,
            weekly_wage: 5_000,
            years: 2,
        };
        assert_eq!(
            authorize(&state, Duty::Scouting, deal),
            Authorization::NeedsApproval(Reason::FeeAboveLimit)
        );
        // Not even a signing that costs nothing slips under a ceiling of zero.
        let free = Spend::Transfer {
            fee: 0,
            weekly_wage: 5_000,
            years: 2,
        };
        assert_eq!(
            authorize(&state, Duty::Scouting, free),
            Authorization::NeedsApproval(Reason::FeeAboveLimit)
        );
        state.limits.max_transfer_fee = 1_000_000;
        state.limits.max_commitment = 2_000_000;
        assert_eq!(authorize(&state, Duty::Scouting, deal), Authorization::Auto);
        assert_eq!(authorize(&state, Duty::Scouting, free), Authorization::Auto);
        // The fee counts toward the commitment.
        state.limits.max_commitment = 900_000;
        assert_eq!(
            authorize(&state, Duty::Scouting, deal),
            Authorization::NeedsApproval(Reason::CommitmentAboveLimit)
        );
    }

    fn books() -> Books {
        Books {
            transfer_cash: 10_000_000,
            wage_room: 40_000,
            staff_room: 6_000,
        }
    }

    fn total() -> DelegationState {
        let mut state = DelegationState::default();
        state.apply_mode(domain::delegation::DelegationMode::Full);
        state
    }

    fn deal(fee: i64, wage: u32) -> Spend {
        Spend::Transfer {
            fee,
            weekly_wage: wage,
            years: 4,
        }
    }

    /// Given Total authority and a transfer far above every default ceiling
    /// When it fits the club's cash and wage room
    /// Then the assistant may make it by himself; the same transfer is a proposal under Prudent.
    #[test]
    fn total_authority_is_the_books_not_a_ceiling() {
        let spend = deal(8_000_000, 35_000);
        assert_eq!(
            authorize_in_books(&total(), Duty::PlayerRecruitment, spend, &books(), 35_000),
            Authorization::Auto
        );
        let mut prudent = DelegationState::default();
        prudent.apply_mode(domain::delegation::DelegationMode::Approval);
        assert_eq!(
            authorize_in_books(&prudent, Duty::PlayerRecruitment, spend, &books(), 35_000),
            Authorization::NeedsApproval(Reason::ApprovalRequired)
        );
    }

    /// Given Total authority
    /// When a transfer is one pound over the cash, or one pound a week over the wage room
    /// Then it is a proposal: Total is not unlimited.
    #[test]
    fn total_authority_stops_at_the_cash_and_the_wage_room() {
        let state = total();
        let over_cash = deal(10_000_001, 1_000);
        assert_eq!(
            authorize_in_books(&state, Duty::PlayerRecruitment, over_cash, &books(), 1_000),
            Authorization::NeedsApproval(Reason::FeeAboveLimit)
        );
        let over_wages = deal(1_000, 40_001);
        assert_eq!(
            authorize_in_books(
                &state,
                Duty::PlayerRecruitment,
                over_wages,
                &books(),
                40_001
            ),
            Authorization::NeedsApproval(Reason::BeyondTheClubsMeans)
        );
        let exactly = deal(10_000_000, 40_000);
        assert_eq!(
            authorize_in_books(&state, Duty::PlayerRecruitment, exactly, &books(), 40_000),
            Authorization::Auto
        );
    }

    /// Given Total authority
    /// When a renewal raises the wage bill by more than the room, and one that raises it by less
    /// Then only the first is a proposal: a renewal adds the raise, not the whole wage.
    #[test]
    fn a_renewal_is_held_to_the_raise_not_the_whole_wage() {
        let state = total();
        let renewal = Spend::PlayerContract {
            weekly_wage: 90_000,
            years: 3,
        };
        assert_eq!(
            authorize_in_books(&state, Duty::PlayerRenewals, renewal, &books(), 5_000),
            Authorization::Auto
        );
        assert_eq!(
            authorize_in_books(&state, Duty::PlayerRenewals, renewal, &books(), 41_000),
            Authorization::NeedsApproval(Reason::BeyondTheClubsMeans)
        );
    }

    /// Given a state that acts alone but has no Total authority (a Full saved before it existed)
    /// When a transfer is judged against the books
    /// Then the old ceilings apply: nothing is silently granted.
    #[test]
    fn without_total_authority_the_ceilings_still_apply() {
        let mut state = total();
        state.limits.finance_bound = false;
        assert_eq!(
            authorize_in_books(
                &state,
                Duty::PlayerRecruitment,
                deal(500_000, 5_000),
                &books(),
                5_000
            ),
            Authorization::NeedsApproval(Reason::FeeAboveLimit),
            "the default transfer ceiling is zero"
        );
    }

    /// Given each way a duty can be set
    /// When a sale is judged
    /// Then only a duty that acts alone under Total authority may sell by itself.
    #[test]
    fn only_total_authority_may_sell() {
        assert_eq!(authorize_sale(&total(), Duty::Offers), Authorization::Auto);
        let mut acts_but_no_authority = total();
        acts_but_no_authority.limits.finance_bound = false;
        assert_eq!(
            authorize_sale(&acts_but_no_authority, Duty::Offers),
            Authorization::NeedsApproval(Reason::NoAuthorityToSell)
        );
        let mut prudent = DelegationState::default();
        prudent.apply_mode(domain::delegation::DelegationMode::Approval);
        assert_eq!(
            authorize_sale(&prudent, Duty::Offers),
            Authorization::NeedsApproval(Reason::ApprovalRequired)
        );
        assert_eq!(
            authorize_sale(&DelegationState::default(), Duty::Offers),
            Authorization::NotDelegated
        );
    }
}
