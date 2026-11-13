use crate::contract_wage_policy::{
    BuyerWageFacts, loan_borrower_wage_policy_error_message, purchase_wage_policy_verdict,
    renewal_wage_policy_error_message, wage_policy_allows_projection,
};
use crate::contracts::Verdict;
use crate::contracts::{
    ERR_UNABLE_TO_CALCULATE_CONTRACT_END_DATE, contract_entry, contract_record, record_movement,
    standard_contract_terms,
};
use crate::finances::calc_wages;
use crate::game::Game;
use chrono::{Duration, NaiveDate};
use domain::contract_ledger::ContractSource;
use domain::league::CompletedTransfer;
use domain::negotiation::{NegotiationFeedback, NegotiationMood};
use domain::player::{
    ActiveLoan, LoanOfferStatus, Player, PlayerMovementEntry, PlayerMovementKind, Position,
    TransferOfferStatus,
};
use domain::season::TransferWindowStatus;
use domain::team::Team;
use domain::transfer_terms::TransferTerms;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

mod affordability;
mod bids;
mod buyer_view;
mod consent;
mod consts;
mod deadline;
mod execution;
mod inbox_actions;
mod lifecycle;
mod loans;
mod market;
mod notifications;
mod processing;
mod promises;
mod realism;
mod registration;
mod sale_view;
mod strategy;
mod terms;

pub(crate) use affordability::{
    FeeBlock, can_afford_fee, fee_and_bonus_block_after_pending, transfer_fee_ceiling,
};
pub use bids::*;
pub use consent::ERR_PLAYER_REFUSES_MOVE;
pub use consent::PlayerOutlook;
pub(crate) use consent::{
    ClubView, MoveJudgement, SweepConsent, incoming_deal_terms, judge_in_game, push_to_leave,
    still_agrees,
};
use consts::*;
pub use deadline::{
    DeadlineDeal, DeadlineItem, DeadlineItemKind, DeadlineItemState, DeadlineSummary,
    expire_negotiations_at_deadline, transfer_deadline_summary,
};
pub use execution::*;
pub use inbox_actions::{TransferOfferResponseEffect, apply_transfer_offer_response};
use lifecycle::*;
pub use loans::*;
pub use market::*;
use notifications::*;
use processing::{deal_registration_date, loan_deal_registration_date};
pub use promises::{ERR_PROMISE_NOT_WANTED, process_stepping_stone_promises, wants_stepping_stone};
pub(crate) use promises::{discounted_wage, make_promise, on_offer_refused, on_player_sold};
use realism::*;
pub(crate) use realism::{DESIRED_DEPTH, LONG_INJURY_DAYS, group_is_thin, window_phase};
use registration::*;
pub(crate) use registration::{
    has_pending_transfer_registration, pending_fee_commitments, pending_wage_commitments,
    player_has_active_or_pending_loan, player_has_pending_registration, signing_place_refusal,
    transfer_window_is_open,
};
pub(crate) use sale_view::reinvestment_gain;
pub(crate) use strategy::{Ambition, ClubProfile, Means, profile_in, profile_of, strategies_of};
pub use strategy::{ClubStrategyReport, strategy_reports};
pub use terms::{
    ERR_INVALID_DEAL_TERMS, ERR_SELL_ON_UNAFFORDABLE, ensure_bonus_affordable,
    process_transfer_obligations,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferNegotiationDecision {
    Accepted,
    Rejected,
    CounterOffer,
    /// The clubs agree on the fee and nothing has moved: the player is next. `player_demand`
    /// carries the contract he would sign, and the deal completes only on a bid that puts
    /// contract terms to him and that he accepts.
    PlayerTalks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferNegotiationOutcome {
    pub decision: TransferNegotiationDecision,
    pub suggested_fee: Option<u64>,
    pub is_terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_date: Option<String>,
    pub feedback: NegotiationFeedback,
    /// The player's side, when it is the player and not the clubs who stopped the deal: the
    /// wage and length he would sign for, if there are any that would bring him round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_demand: Option<PlayerDemand>,
    /// What stood in his way, when he turned the move down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_obstacle: Option<domain::free_agent_talk::FreeAgentTalkReason>,
}

/// The contract a player asks for in place of the one offered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerDemand {
    pub weekly_wage: u32,
    pub contract_years: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferBidFinancialProjection {
    pub transfer_budget_before: i64,
    pub transfer_budget_after: i64,
    pub finance_before: i64,
    pub finance_after: i64,
    pub annual_wage_bill_before: i64,
    pub annual_wage_bill_after: i64,
    pub annual_wage_budget: i64,
    /// Weekly wage figures (stored wages are already weekly), so the UI can
    /// show the same before → after breakdown the renewal projection uses
    /// instead of a lone usage %.
    pub current_weekly_wage_spend: i64,
    pub projected_weekly_wage_spend: i64,
    pub weekly_wage_budget: i64,
    pub incoming_player_weekly_wage: i64,
    pub projected_wage_budget_usage_pct: i64,
    pub exceeds_transfer_budget: bool,
    pub exceeds_finance: bool,
    /// The board would refuse the player's standard wage at this club: the verdict the bid itself
    /// is held to, so the preview and the refusal cannot disagree.
    #[serde(default)]
    pub blocked_by_wage_policy: bool,
    /// Set when the window is closed and this bid would land as
    /// PendingRegistration: the date the debit will actually fire. `None` when
    /// the window is open and the debit fires immediately on acceptance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_registration_date: Option<String>,
    /// Cash that leaves on completion: the first instalment and any signing bonus.
    #[serde(default)]
    pub cash_due_on_completion: u64,
    #[serde(default)]
    pub signing_bonus: u64,
    /// Payments still to come after completion, a year apart.
    #[serde(default)]
    pub later_instalments: u8,
    #[serde(default)]
    pub later_instalment_amount: u64,
    /// The player would value a stepping-stone promise; the bid is refused for anyone else.
    #[serde(default)]
    pub stepping_stone_available: bool,
    /// How the player takes the move on these terms: keen, hesitant or unlikely, and what he
    /// would ask for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_outlook: Option<PlayerOutlook>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoanOfferDecision {
    Accepted,
    Rejected,
    CounterOffer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoanOfferOutcome {
    pub decision: LoanOfferDecision,
    pub offer_id: String,
    pub suggested_wage_contribution_pct: Option<u8>,
    pub suggested_end_date: Option<String>,
    pub suggested_buy_option_fee: Option<u64>,
    pub is_terminal: bool,
}

pub(crate) enum PlayerImportance {
    Key,
    Regular,
    Fringe,
}

pub(crate) struct MarketCandidate {
    player_id: String,
    owner_team_id: String,
    fee: u64,
}

/// A player worth pursuing, with buyer-independent appeal precomputed once so
/// every club can reuse it instead of re-scoring the whole world.
pub(crate) struct MarketTarget {
    player_id: String,
    owner_team_id: String,
    is_user_owned: bool,
    score: i32,
    fee: u64,
    /// Broad position group (0=GK, 1=DEF, 2=MID, 3=FWD), used to gate buyers
    /// that are already stacked in that area.
    position_group_index: usize,
    /// Reputation of the player's current club, used for reputation-fit gating.
    owner_reputation: u32,
    /// His rating, to tell whether he would improve a buyer's squad.
    ovr: u8,
    /// His club has put him up for sale.
    is_listed: bool,
    /// His age on the day of the sweep.
    age: i32,
    /// Potential minus rating: what a club that builds on young players is looking at.
    potential_gap: i32,
    /// His market value, which every price is a bounded multiple of.
    value: u64,
    /// Days left on his contract, if it has an end.
    contract_days: Option<i64>,
    /// What he is to his club today.
    status: PlayerStatus,
}

impl MarketTarget {
    /// His rating a year on, which is what a club planning its squad is buying.
    fn planned_ovr(&self) -> u8 {
        let potential = (i32::from(self.ovr) + self.potential_gap).clamp(1, 100) as u8;
        crate::squad_outlook::expected_ovr_in_a_year(
            self.ovr,
            potential,
            self.age,
            self.position_group_index,
        )
    }
}

pub(crate) struct LoanMarketCandidate {
    player_id: String,
    wage_contribution_pct: u8,
    buy_option_fee: Option<u64>,
    score: i32,
}

#[cfg(test)]
mod tests;
