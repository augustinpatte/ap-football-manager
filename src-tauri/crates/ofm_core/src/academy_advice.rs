//! For each action open to the manager on an academy player, whether it is recommended, merely
//! available or blocked, and why.
//!
//! Nothing here decides anything new. Every refusal is one the backend already enforces (a player
//! out on loan cannot be promoted or listed; a promotion needs a senior place), and every
//! recommendation restates the review's verdict. The point is that the screen can say *why* a
//! button is enabled, advised against or disabled, in the manager's terms, instead of only greying
//! it out. The reasons are translation-key suffixes under `academy.advice.`.

use serde::Serialize;

use crate::academy_review::Verdict;

/// How the review stands on one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdviceState {
    /// The review's verdict points to it.
    Recommended,
    /// Allowed, with nothing in the review pushing for or against it.
    Available,
    /// The game would refuse it.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionAdvice {
    pub state: AdviceState,
    pub reasons: Vec<String>,
}

impl ActionAdvice {
    fn new(state: AdviceState, reasons: &[&str]) -> Self {
        Self {
            state,
            reasons: reasons.iter().map(|reason| reason.to_string()).collect(),
        }
    }
}

/// What the manager can do with a youngster, and what the review makes of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowAdvice {
    pub promote: ActionAdvice,
    pub loan: ActionAdvice,
    /// Staying in the academy is what happens when the manager does nothing, so it has no button:
    /// this says whether doing nothing is the advice.
    pub keep: ActionAdvice,
    /// Present only when his contract ends within the renewal horizon.
    pub renew: Option<ActionAdvice>,
}

/// The facts an action's advice rests on, all of them already on the row.
pub(crate) struct AdviceFacts<'a> {
    pub verdict: Verdict,
    pub age: i32,
    pub injured: bool,
    pub on_loan: bool,
    pub loan_listed: bool,
    /// The refusal key when no senior place is free.
    pub place_refusal: Option<&'a str>,
    pub contract_days_left: Option<i64>,
}

pub(crate) fn advise(facts: &AdviceFacts) -> RowAdvice {
    RowAdvice {
        promote: promotion_advice(facts),
        loan: loan_advice(facts),
        keep: keep_advice(facts),
        renew: renewal_advice(facts),
    }
}

fn promotion_advice(facts: &AdviceFacts) -> ActionAdvice {
    let mut blockers = Vec::new();
    if facts.on_loan {
        blockers.push("promote.onLoan");
    }
    if facts.place_refusal.is_some() {
        blockers.push("promote.noPlace");
    }
    if !blockers.is_empty() {
        return ActionAdvice::new(AdviceState::Blocked, &blockers);
    }
    if facts.age > crate::roster::YOUTH_ACADEMY_MAX_AGE {
        return ActionAdvice::new(AdviceState::Recommended, &["promote.pastAcademyAge"]);
    }
    match (facts.verdict, facts.injured) {
        (Verdict::ReadyForSenior, true) => ActionAdvice::new(
            AdviceState::Recommended,
            &["promote.ready", "promote.injured"],
        ),
        (Verdict::ReadyForSenior, false) => {
            ActionAdvice::new(AdviceState::Recommended, &["promote.ready"])
        }
        (_, true) => ActionAdvice::new(
            AdviceState::Available,
            &["promote.notNeeded", "promote.injured"],
        ),
        (_, false) => ActionAdvice::new(AdviceState::Available, &["promote.notNeeded"]),
    }
}

fn loan_advice(facts: &AdviceFacts) -> ActionAdvice {
    if facts.on_loan {
        return ActionAdvice::new(AdviceState::Blocked, &["loan.alreadyOut"]);
    }
    match facts.verdict {
        Verdict::LoanCandidate if facts.loan_listed => ActionAdvice::new(
            AdviceState::Recommended,
            &["loan.recommended", "loan.listed"],
        ),
        Verdict::LoanCandidate => {
            ActionAdvice::new(AdviceState::Recommended, &["loan.recommended"])
        }
        Verdict::ReadyForSenior => {
            ActionAdvice::new(AdviceState::Available, &["loan.promoteInstead"])
        }
        _ if facts.loan_listed => ActionAdvice::new(AdviceState::Available, &["loan.listed"]),
        _ => ActionAdvice::new(AdviceState::Available, &["loan.notNeeded"]),
    }
}

fn keep_advice(facts: &AdviceFacts) -> ActionAdvice {
    if facts.age > crate::roster::YOUTH_ACADEMY_MAX_AGE {
        return ActionAdvice::new(AdviceState::Available, &["keep.pastAcademyAge"]);
    }
    let last_year = facts.age == crate::roster::YOUTH_ACADEMY_MAX_AGE;
    match (facts.verdict, last_year) {
        (Verdict::KeepDeveloping, true) => ActionAdvice::new(
            AdviceState::Recommended,
            &["keep.developing", "keep.lastYear"],
        ),
        (Verdict::KeepDeveloping, false) => {
            ActionAdvice::new(AdviceState::Recommended, &["keep.developing"])
        }
        (_, true) => ActionAdvice::new(AdviceState::Available, &["keep.lastYear"]),
        _ => ActionAdvice::new(AdviceState::Available, &[]),
    }
}

fn renewal_advice(facts: &AdviceFacts) -> Option<ActionAdvice> {
    let days = facts.contract_days_left?;
    if days > crate::ai_contracts::RENEWAL_HORIZON_DAYS {
        return None;
    }
    Some(if facts.verdict == Verdict::UnlikelyToProgress {
        ActionAdvice::new(AdviceState::Available, &["renew.letRun"])
    } else {
        ActionAdvice::new(AdviceState::Recommended, &["renew.endsSoon"])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(verdict: Verdict) -> AdviceFacts<'static> {
        AdviceFacts {
            verdict,
            age: 19,
            injured: false,
            on_loan: false,
            loan_listed: false,
            place_refusal: None,
            contract_days_left: Some(900),
        }
    }

    /// Given a youngster out on loan
    /// When the actions are advised
    /// Then promotion and a loan listing are blocked, with the reason the game would refuse for.
    #[test]
    fn a_player_out_on_loan_can_be_neither_promoted_nor_listed() {
        let advice = advise(&AdviceFacts {
            on_loan: true,
            ..facts(Verdict::ReadyForSenior)
        });
        assert_eq!(advice.promote.state, AdviceState::Blocked);
        assert_eq!(advice.promote.reasons, vec!["promote.onLoan"]);
        assert_eq!(advice.loan.state, AdviceState::Blocked);
    }

    /// Given a ready youngster and a club with no senior place
    /// When promotion is advised
    /// Then it is blocked for the place, even though the verdict says ready.
    #[test]
    fn a_full_squad_blocks_a_ready_promotion() {
        let advice = advise(&AdviceFacts {
            place_refusal: Some("be.error.squadFull"),
            ..facts(Verdict::ReadyForSenior)
        });
        assert_eq!(advice.promote.state, AdviceState::Blocked);
        assert_eq!(advice.promote.reasons, vec!["promote.noPlace"]);
    }

    /// Given each verdict
    /// When the actions are advised
    /// Then the one action the verdict points to is recommended and the others are not.
    #[test]
    fn the_verdict_decides_which_action_is_recommended() {
        let ready = advise(&facts(Verdict::ReadyForSenior));
        assert_eq!(ready.promote.state, AdviceState::Recommended);
        assert_eq!(ready.loan.reasons, vec!["loan.promoteInstead"]);
        let loan = advise(&facts(Verdict::LoanCandidate));
        assert_eq!(loan.loan.state, AdviceState::Recommended);
        assert_eq!(loan.promote.state, AdviceState::Available);
        let keep = advise(&facts(Verdict::KeepDeveloping));
        assert_eq!(keep.keep.state, AdviceState::Recommended);
        assert_eq!(keep.promote.reasons, vec!["promote.notNeeded"]);
    }

    /// Given a contract that runs on, and one that ends within the renewal horizon
    /// When renewal is advised
    /// Then only the expiring one has advice, and a youngster unlikely to progress is not pressed.
    #[test]
    fn renewal_is_advised_only_when_the_contract_is_ending() {
        assert!(advise(&facts(Verdict::KeepDeveloping)).renew.is_none());
        let ending = AdviceFacts {
            contract_days_left: Some(90),
            ..facts(Verdict::KeepDeveloping)
        };
        assert_eq!(
            advise(&ending).renew.unwrap().state,
            AdviceState::Recommended
        );
        let lost = AdviceFacts {
            contract_days_left: Some(90),
            ..facts(Verdict::UnlikelyToProgress)
        };
        assert_eq!(advise(&lost).renew.unwrap().reasons, vec!["renew.letRun"]);
    }

    /// Given a youngster past the academy's age limit
    /// When promotion and staying are advised
    /// Then promotion is recommended and staying says he is past the limit.
    #[test]
    fn a_player_past_the_academy_age_is_advised_to_move_on() {
        let old = AdviceFacts {
            age: crate::roster::YOUTH_ACADEMY_MAX_AGE + 1,
            ..facts(Verdict::KeepDeveloping)
        };
        let advice = advise(&old);
        assert_eq!(advice.promote.reasons, vec!["promote.pastAcademyAge"]);
        assert_eq!(advice.keep.reasons, vec!["keep.pastAcademyAge"]);
    }
}
