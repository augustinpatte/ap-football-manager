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
    /// The fee is above the transfer ceiling (zero by default: transfers are the manager's).
    FeeAboveLimit,
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
    if matches!(spend, Spend::Transfer { .. }) && fee > limits.max_transfer_fee {
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
        state.limits.max_transfer_fee = 1_000_000;
        state.limits.max_commitment = 2_000_000;
        assert_eq!(authorize(&state, Duty::Scouting, deal), Authorization::Auto);
        // The fee counts toward the commitment.
        state.limits.max_commitment = 900_000;
        assert_eq!(
            authorize(&state, Duty::Scouting, deal),
            Authorization::NeedsApproval(Reason::CommitmentAboveLimit)
        );
    }
}
