//! Deadline day: what the user must still answer before the window shuts, what is already
//! scheduled, and what the rest of the market has just done; and the sweep that ends every
//! unanswered negotiation when the last day is over.

use super::*;
use domain::message::{InboxMessage, MessageCategory, MessagePriority};
use serde::Serialize;

/// How far back "market activity" reaches.
const RECENT_DEAL_DAYS: i64 = 3;
const MAX_RECENT_DEALS: usize = 8;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineItemKind {
    Transfer,
    Loan,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineItemState {
    /// A club has bid for one of the user's players and awaits an answer.
    IncomingOffer,
    /// A club has countered the user's bid and awaits an answer.
    CounterReceived,
    /// The user's bid is with the other club.
    AwaitingClub,
    /// Agreed, to be registered on the date shown.
    ScheduledRegistration,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadlineItem {
    pub kind: DeadlineItemKind,
    pub state: DeadlineItemState,
    pub player_id: String,
    pub player_name: String,
    pub counterparty_team_id: String,
    pub counterparty_team_name: String,
    pub offer_id: String,
    /// The fee on the table; `None` for a loan, which has no fee.
    pub fee: Option<u64>,
    /// Whether the user must act, or the negotiation lapses with the window.
    pub needs_response: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadlineDeal {
    pub player_id: String,
    pub player_name: String,
    pub from_team_name: String,
    pub to_team_name: String,
    pub fee: u64,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadlineSummary {
    pub window_open: bool,
    /// `early`, `mid`, `last_week`, `last_72h` or `deadline_day`; only meaningful when open.
    pub phase: String,
    pub days_remaining: Option<i64>,
    pub closes_on: Option<String>,
    /// The last three days: the interface raises its alarm.
    pub urgent: bool,
    pub items: Vec<DeadlineItem>,
    pub urgent_response_count: usize,
    pub recent_deals: Vec<DeadlineDeal>,
}

fn transfer_item(
    game: &Game,
    player: &Player,
    offer: &domain::player::TransferOffer,
    state: DeadlineItemState,
    counterparty_id: &str,
) -> DeadlineItem {
    DeadlineItem {
        kind: DeadlineItemKind::Transfer,
        needs_response: matches!(
            state,
            DeadlineItemState::IncomingOffer | DeadlineItemState::CounterReceived
        ),
        state,
        player_id: player.id.clone(),
        player_name: player.full_name.clone(),
        counterparty_team_id: counterparty_id.to_string(),
        counterparty_team_name: game.team_name_or_id(counterparty_id),
        offer_id: offer.id.clone(),
        fee: Some(offer.fee),
        registration_date: offer.registration_date.clone(),
    }
}

fn loan_item(
    game: &Game,
    player: &Player,
    offer: &domain::player::LoanOffer,
    state: DeadlineItemState,
    counterparty_id: &str,
) -> DeadlineItem {
    DeadlineItem {
        kind: DeadlineItemKind::Loan,
        needs_response: matches!(
            state,
            DeadlineItemState::IncomingOffer | DeadlineItemState::CounterReceived
        ),
        state,
        player_id: player.id.clone(),
        player_name: player.full_name.clone(),
        counterparty_team_id: counterparty_id.to_string(),
        counterparty_team_name: game.team_name_or_id(counterparty_id),
        offer_id: offer.id.clone(),
        fee: None,
        registration_date: None,
    }
}

/// The user's side of the market as the window closes. Read-only.
pub fn transfer_deadline_summary(game: &Game) -> DeadlineSummary {
    let window = &game.season_context.transfer_window;
    let window_open = transfer_window_is_open(game);
    let phase = window_phase(window.days_remaining);
    let mut items = Vec::new();
    if let Some(user_id) = game.manager.team_id.as_deref() {
        for player in &game.players {
            let owned = player.team_id.as_deref() == Some(user_id);
            for offer in &player.transfer_offers {
                match (offer.status.clone(), owned) {
                    (TransferOfferStatus::Pending, true) => items.push(transfer_item(
                        game,
                        player,
                        offer,
                        DeadlineItemState::IncomingOffer,
                        &offer.from_team_id,
                    )),
                    (TransferOfferStatus::Pending, false) if offer.from_team_id == user_id => {
                        let state = if offer.suggested_counter_fee.is_some() {
                            DeadlineItemState::CounterReceived
                        } else {
                            DeadlineItemState::AwaitingClub
                        };
                        let owner = player.team_id.clone().unwrap_or_default();
                        items.push(transfer_item(game, player, offer, state, &owner));
                    }
                    (TransferOfferStatus::PendingRegistration, _)
                        if owned || offer.from_team_id == user_id =>
                    {
                        let other = if owned {
                            offer.from_team_id.clone()
                        } else {
                            user_id.to_string()
                        };
                        items.push(transfer_item(
                            game,
                            player,
                            offer,
                            DeadlineItemState::ScheduledRegistration,
                            &other,
                        ));
                    }
                    _ => {}
                }
            }
            for offer in &player.loan_offers {
                if offer.status != LoanOfferStatus::Pending {
                    continue;
                }
                if owned {
                    items.push(loan_item(
                        game,
                        player,
                        offer,
                        DeadlineItemState::IncomingOffer,
                        &offer.from_team_id,
                    ));
                } else if offer.from_team_id == user_id {
                    let state = if offer.suggested_wage_contribution_pct.is_some()
                        || offer.suggested_end_date.is_some()
                        || offer.suggested_buy_option_fee.is_some()
                    {
                        DeadlineItemState::CounterReceived
                    } else {
                        DeadlineItemState::AwaitingClub
                    };
                    items.push(loan_item(game, player, offer, state, &offer.parent_team_id));
                }
            }
        }
    }
    items.sort_by(|a, b| {
        b.needs_response
            .cmp(&a.needs_response)
            .then_with(|| a.player_name.cmp(&b.player_name))
    });
    let urgent_response_count = items.iter().filter(|item| item.needs_response).count();

    DeadlineSummary {
        window_open,
        phase: phase.name().to_string(),
        days_remaining: window.days_remaining,
        closes_on: window.closes_on.clone(),
        urgent: window_open && phase >= WindowPhase::Last72h,
        items,
        urgent_response_count,
        recent_deals: recent_deals(game),
    }
}

fn recent_deals(game: &Game) -> Vec<DeadlineDeal> {
    let today = game.clock.current_date.date_naive();
    let mut deals: Vec<DeadlineDeal> = game
        .competitions
        .iter()
        .flat_map(|competition| competition.transfer_log.iter())
        .chain(
            game.league
                .iter()
                .flat_map(|league| league.transfer_log.iter()),
        )
        .filter(|deal| {
            NaiveDate::parse_from_str(&deal.date, "%Y-%m-%d")
                .is_ok_and(|day| (today - day).num_days() < RECENT_DEAL_DAYS)
        })
        .map(|deal| DeadlineDeal {
            player_id: deal.player_id.clone(),
            player_name: game
                .players
                .iter()
                .find(|player| player.id == deal.player_id)
                .map_or_else(|| deal.player_id.clone(), |player| player.full_name.clone()),
            from_team_name: game.team_name_or_id(&deal.from_team_id),
            to_team_name: game.team_name_or_id(&deal.to_team_id),
            fee: deal.fee,
            date: deal.date.clone(),
        })
        .collect();
    // The league mirror repeats a competition's log, so the same deal can appear twice.
    deals.sort_by(|a, b| {
        b.fee
            .cmp(&a.fee)
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
    deals.dedup_by(|a, b| a.player_id == b.player_id && a.date == b.date && a.fee == b.fee);
    deals.truncate(MAX_RECENT_DEALS);
    deals
}

/// Ends every negotiation still unanswered when the closing day is over: nothing may stay live
/// into a window that has shut, and an offer that cannot be completed must say so rather than
/// hang. Agreed deals (pending registration) are untouched; they register when the window next
/// opens. Runs once, on the closing day itself, and only ever moves `Pending` offers, so a second
/// run changes nothing. An offer that arrived today is left alone: the user has not yet had a
/// chance to see it, and it lapses by the ordinary staleness rule if it is never answered.
pub fn expire_negotiations_at_deadline(game: &mut Game) {
    if game.season_context.transfer_window.status != TransferWindowStatus::DeadlineDay {
        return;
    }
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let user_id = game.manager.team_id.clone();
    let mut user_involved = 0usize;
    for player in &mut game.players {
        let owned_by_user = player.team_id.is_some() && player.team_id == user_id;
        for offer in &mut player.transfer_offers {
            if offer.status == TransferOfferStatus::Pending && offer.date != today {
                user_involved +=
                    usize::from(owned_by_user || Some(&offer.from_team_id) == user_id.as_ref());
                close_transfer_offer(offer, TransferOfferStatus::Withdrawn, &today);
            }
        }
        for offer in &mut player.loan_offers {
            if offer.status == LoanOfferStatus::Pending && offer.date != today {
                user_involved +=
                    usize::from(owned_by_user || Some(&offer.from_team_id) == user_id.as_ref());
                close_loan_offer(offer, LoanOfferStatus::Withdrawn, &today);
            }
        }
    }
    if user_involved == 0 {
        return;
    }
    let mut params = std::collections::HashMap::new();
    params.insert("count".to_string(), user_involved.to_string());
    game.messages.push(
        InboxMessage::new(
            format!("transfer_deadline_expired_{today}"),
            String::new(),
            String::new(),
            String::new(),
            today,
        )
        .with_category(MessageCategory::Transfer)
        .with_priority(MessagePriority::Normal)
        .with_sender_role("")
        .with_i18n(
            "be.msg.deadlineExpired.subject",
            "be.msg.deadlineExpired.body",
            params,
        )
        .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager"),
    );
}
