//! How the assistant lets the manager know what he has done, without filling the inbox.
//!
//! The decision history (`DelegationState::log`) holds every step. The inbox gets three kinds of
//! message, each rate-limited: one digest a fortnight when anything happened, one reminder at most
//! every three days while proposals are waiting, and one notice a season if the assistant
//! leaves with duties still handed to him.

use chrono::NaiveDate;
use domain::delegation::DecisionOutcome;
use domain::message::{InboxMessage, MessageCategory, MessageContext, MessagePriority};
use std::collections::HashMap;

use super::delegation::{date, stamp};
use crate::game::Game;

/// Days between two digests: a fortnight keeps a season to about two dozen messages.
const DIGEST_PERIOD_DAYS: i64 = 14;
/// Fewest days between two reminders that proposals are waiting.
const REMINDER_PERIOD_DAYS: i64 = 3;

fn days_since(game: &Game, then: Option<&str>) -> Option<i64> {
    let then = NaiveDate::parse_from_str(then?, "%Y-%m-%d").ok()?;
    Some((date(game) - then).num_days())
}

fn message(
    game: &Game,
    team_id: &str,
    id: String,
    keys: (&str, &str),
    params: &[(&str, String)],
    priority: MessagePriority,
) -> InboxMessage {
    InboxMessage::new(id, String::new(), String::new(), String::new(), stamp(game))
        .with_category(MessageCategory::System)
        .with_priority(priority)
        .with_i18n(
            keys.0,
            keys.1,
            params
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect::<HashMap<_, _>>(),
        )
        .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
        .with_context(MessageContext {
            team_id: Some(team_id.to_string()),
            ..Default::default()
        })
}

/// The end-of-day messages: the digest and the reminder about waiting proposals.
pub(super) fn notify(game: &mut Game, team_id: &str) {
    let today = stamp(game);
    periodic_digest(game, team_id, &today);
    proposal_reminder(game, team_id, &today);
}

fn periodic_digest(game: &mut Game, team_id: &str, today: &str) {
    let due = match game.delegation.last_digest.as_deref() {
        None => !game.delegation.log.is_empty(),
        Some(last) => days_since(game, Some(last)).is_some_and(|d| d >= DIGEST_PERIOD_DAYS),
    };
    if !due {
        return;
    }
    let since = game.delegation.last_digest.clone().unwrap_or_default();
    let (mut done, mut left) = (0, 0);
    let (mut signings, mut sales) = (0, 0);
    for entry in game.delegation.log.iter().filter(|e| e.date > since) {
        match entry.outcome {
            DecisionOutcome::Executed | DecisionOutcome::Approved => {
                done += 1;
                match transfer_kind(&entry.title_key) {
                    Some(TransferKind::Signing) => signings += 1,
                    Some(TransferKind::Sale) => sales += 1,
                    None => {}
                }
            }
            DecisionOutcome::LeftToManager => left += 1,
            // A declined proposal was the manager's own answer: nothing to report back.
            DecisionOutcome::Declined => {}
        }
    }
    game.delegation.last_digest = Some(today.to_string());
    if done + left == 0 {
        return;
    }
    let waiting = game.delegation.proposals.len();
    // A fortnight with signings or sales says so; a quiet one stays a single short line.
    let body = if signings + sales > 0 {
        "be.msg.assistantDigest.bodyTransfers"
    } else {
        "be.msg.assistantDigest.body"
    };
    let message = message(
        game,
        team_id,
        format!("assistant_digest_{team_id}_{today}"),
        ("be.msg.assistantDigest.subject", body),
        &[
            ("done", done.to_string()),
            ("left", left.to_string()),
            ("waiting", waiting.to_string()),
            ("signings", signings.to_string()),
            ("sales", sales.to_string()),
        ],
        MessagePriority::Low,
    );
    crate::inbox::emit(game, message);
}

/// The two kinds of decision a manager wants named in the digest.
enum TransferKind {
    Signing,
    Sale,
}

/// Whether a history entry is a signing or a sale the assistant made (agreed or done).
fn transfer_kind(title_key: &str) -> Option<TransferKind> {
    match title_key {
        "delegation.log.recruitSigned"
        | "delegation.log.recruitScheduled"
        | "delegation.proposal.signPlayer.accepted" => Some(TransferKind::Signing),
        key if key.starts_with("delegation.log.offerAccepted")
            || key == "delegation.log.offerCounteredAccepted"
            || key == "delegation.proposal.sellPlayer.accepted" =>
        {
            Some(TransferKind::Sale)
        }
        _ => None,
    }
}

fn proposal_reminder(game: &mut Game, team_id: &str, today: &str) {
    let waiting = game.delegation.proposals.len();
    if waiting == 0 {
        return;
    }
    let recently = days_since(game, game.delegation.last_proposal_notice.as_deref())
        .is_some_and(|d| d < REMINDER_PERIOD_DAYS);
    if recently {
        return;
    }
    game.delegation.last_proposal_notice = Some(today.to_string());
    let message = message(
        game,
        team_id,
        format!("assistant_proposals_{team_id}_{today}"),
        (
            "be.msg.assistantProposals.subject",
            "be.msg.assistantProposals.body",
        ),
        &[("count", waiting.to_string())],
        MessagePriority::Normal,
    );
    crate::inbox::emit(game, message);
}

/// The assistant is gone but duties are still handed to him: say so once a season, since nothing
/// will run until another is hired.
pub(super) fn note_missing_assistant(game: &mut Game, team_id: &str) {
    let handed_over = domain::delegation::Duty::ALL
        .iter()
        .any(|duty| game.delegation.is_delegated(*duty));
    if !handed_over {
        return;
    }
    let key = format!(
        "assistant_missing_{team_id}_{}",
        crate::inbox::recurrence_season(game)
    );
    if crate::inbox::already_emitted(game, &key) {
        return;
    }
    let message = message(
        game,
        team_id,
        key,
        (
            "be.msg.assistantMissing.subject",
            "be.msg.assistantMissing.body",
        ),
        &[],
        MessagePriority::High,
    );
    crate::inbox::emit(game, message);
}
