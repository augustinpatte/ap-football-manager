//! What the assistant did on the manager's behalf, read from the delegation log.

use domain::delegation::DecisionOutcome;

use crate::game::Game;

use super::tracker::PreRun;
use super::types::{DelegationLine, DelegationReport, Listed};

pub(super) fn delegation_of(
    game: &Game,
    from: &str,
    until: &str,
    pre_run: &PreRun,
) -> DelegationReport {
    let mut report = DelegationReport::default();
    let mut lines = Vec::new();
    let mut before_the_run = pre_run.delegation_entries;
    for entry in &game.delegation.log {
        // The first of the first day's entries were logged before the run began.
        if entry.date.as_str() == from && before_the_run > 0 {
            before_the_run -= 1;
            continue;
        }
        if entry.date.as_str() < from || entry.date.as_str() >= until {
            continue;
        }
        match entry.outcome {
            DecisionOutcome::Executed => report.executed += 1,
            DecisionOutcome::Approved => report.approved += 1,
            DecisionOutcome::Declined => report.declined += 1,
            DecisionOutcome::LeftToManager => report.left_to_manager += 1,
        }
        lines.push(DelegationLine {
            date: entry.date.clone(),
            duty: entry.duty,
            title_key: entry.title_key.clone(),
            params: entry.params.clone(),
            outcome: entry.outcome,
        });
    }
    lines.sort_by(|a, b| a.date.cmp(&b.date));
    report.entries = Listed::capped(lines);
    report
}
