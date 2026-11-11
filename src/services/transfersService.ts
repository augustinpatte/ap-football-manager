import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";

export interface TransferNegotiationFeedbackData {
  mood: "calm" | "firm" | "tense" | "positive" | "guarded";
  headline_key: string;
  detail_key?: string | null;
  tension: number;
  patience: number;
  round: number;
  params?: Record<string, string>;
}

export interface TransferNegotiationResponseData {
  decision: "accepted" | "rejected" | "counter_offer" | "player_talks";
  suggested_fee: number | null;
  is_terminal: boolean;
  registration_date?: string | null;
  feedback: TransferNegotiationFeedbackData;
  /** The wage and length the player asks for, when he is the one holding the deal up. */
  player_demand?: PlayerDemandData | null;
  game: GameStateData;
}

/** The contract a player asks for in place of the one offered. */
export interface PlayerDemandData {
  weekly_wage: number;
  contract_years: number;
}

/** How the player takes a move on the terms offered; mirrors `transfers::PlayerOutlook`. */
export interface PlayerOutlookData {
  interest: "keen" | "hesitant" | "unlikely";
  /** The standard terms he starts from. */
  expected_wage: number;
  expected_years: number;
  /** What he would ask for if the offer fell short, when he would ask at all. */
  asks_wage?: number | null;
  asks_years?: number | null;
}

export interface TransferBidProjectionData {
  projection: {
    transfer_budget_before: number;
    transfer_budget_after: number;
    finance_before: number;
    finance_after: number;
    annual_wage_bill_before: number;
    annual_wage_bill_after: number;
    annual_wage_budget: number;
    current_weekly_wage_spend: number;
    projected_weekly_wage_spend: number;
    weekly_wage_budget: number;
    incoming_player_weekly_wage: number;
    projected_wage_budget_usage_pct: number;
    exceeds_transfer_budget: boolean;
    exceeds_finance: boolean;
    /** The board would refuse his standard wage at this club, the same verdict the bid is held to. */
    blocked_by_wage_policy: boolean;
    /** Debit fires on this date (window closed → PendingRegistration). Absent when the deal would execute immediately. */
    pending_registration_date?: string | null;
    /** Cash that leaves on completion: the first instalment and any signing bonus. */
    cash_due_on_completion?: number;
    signing_bonus?: number;
    /** Payments still to come after completion, a year apart. */
    later_instalments?: number;
    later_instalment_amount?: number;
    /** The player would value a stepping-stone promise; the backend refuses it for anyone else. */
    stepping_stone_available?: boolean;
    /** The player's own view of the move on these terms. */
    player_outlook?: PlayerOutlookData | null;
  };
}

/** What a buyer attaches to a fee; mirrors `domain::transfer_terms::TransferTerms`. */
export interface TransferTermsData {
  instalments: number;
  sell_on_pct: number;
  signing_bonus: number;
  promise_stepping_stone: boolean;
  /** The weekly wage offered to the player; omitted or 0 means the buyer's standard terms. */
  player_weekly_wage?: number;
  /** The contract length offered to the player, in years; omitted or 0 means the standard. */
  player_contract_years?: number;
}

export interface LoanOfferResponseData {
  decision: "accepted" | "rejected" | "counter_offer";
  offer_id: string;
  suggested_wage_contribution_pct: number | null;
  suggested_end_date: string | null;
  suggested_buy_option_fee: number | null;
  is_terminal: boolean;
  game: GameStateData;
}

export async function makeTransferBid(
  playerId: string,
  fee: number,
  terms?: TransferTermsData,
): Promise<TransferNegotiationResponseData> {
  return invoke<TransferNegotiationResponseData>("make_transfer_bid", {
    playerId,
    fee,
    terms,
  });
}

export async function makeLoanOffer(
  playerId: string,
  endDate: string,
  wageContributionPct: number,
  buyOptionFee: number | null = null,
): Promise<LoanOfferResponseData> {
  return invoke<LoanOfferResponseData>("make_loan_offer", {
    playerId,
    endDate,
    wageContributionPct,
    buyOptionFee,
  });
}

export async function exerciseLoanBuyOption(playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("exercise_loan_buy_option", {
    playerId,
  });
}

export async function respondToOffer(
  playerId: string,
  offerId: string,
  accept: boolean,
): Promise<GameStateData> {
  return invoke<GameStateData>("respond_to_offer", {
    playerId,
    offerId,
    accept,
  });
}

export async function respondToLoanOffer(
  playerId: string,
  offerId: string,
  accept: boolean,
): Promise<GameStateData> {
  return invoke<GameStateData>("respond_to_loan_offer", {
    playerId,
    offerId,
    accept,
  });
}

export async function counterLoanOffer(
  playerId: string,
  offerId: string,
  endDate: string,
  wageContributionPct: number,
  buyOptionFee: number | null = null,
): Promise<LoanOfferResponseData> {
  return invoke<LoanOfferResponseData>("counter_loan_offer", {
    playerId,
    offerId,
    endDate,
    wageContributionPct,
    buyOptionFee,
  });
}

export async function counterOffer(
  playerId: string,
  offerId: string,
  requestedFee: number,
): Promise<TransferNegotiationResponseData> {
  return invoke<TransferNegotiationResponseData>("counter_offer", {
    playerId,
    offerId,
    requestedFee,
  });
}

/** The user's negotiation over a player as the backend's own rule reads it. */
export interface BidNegotiationStatusData {
  /** The offer the next bid continues, or the broken-off talks; null when the next bid starts over. */
  offer_id: string | null;
  continues_talks: boolean;
  /** For broken-off talks: the day the owner will talk again (YYYY-MM-DD). */
  talks_resume_on?: string | null;
}

export async function getBidNegotiationStatus(playerId: string): Promise<BidNegotiationStatusData> {
  return invoke<BidNegotiationStatusData>("get_bid_negotiation_status", { playerId });
}

export async function previewTransferBidFinancialImpact(
  playerId: string,
  fee: number,
  terms?: TransferTermsData,
): Promise<TransferBidProjectionData> {
  return invoke<TransferBidProjectionData>("preview_transfer_bid_financial_impact", {
    playerId,
    fee,
    terms,
  });
}

export async function toggleTransferList(playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("toggle_transfer_list", {
    playerId,
  });
}

export async function toggleLoanList(playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("toggle_loan_list", {
    playerId,
  });
}

export type DeadlineItemState =
  | "incoming_offer"
  | "counter_received"
  | "awaiting_club"
  | "scheduled_registration";

export interface DeadlineItemData {
  kind: "transfer" | "loan";
  state: DeadlineItemState;
  player_id: string;
  player_name: string;
  counterparty_team_id: string;
  counterparty_team_name: string;
  offer_id: string;
  /** Absent for a loan, which has no fee. */
  fee: number | null;
  /** The user must act, or the negotiation lapses with the window. */
  needs_response: boolean;
  registration_date?: string;
}

export interface DeadlineDealData {
  player_id: string;
  player_name: string;
  from_team_name: string;
  to_team_name: string;
  fee: number;
  date: string;
}

/** The user's side of the market as the window closes, as the backend reads it. */
export interface TransferDeadlineSummaryData {
  window_open: boolean;
  phase: "early" | "mid" | "last_week" | "last_72h" | "deadline_day";
  days_remaining: number | null;
  closes_on: string | null;
  urgent: boolean;
  items: DeadlineItemData[];
  urgent_response_count: number;
  recent_deals: DeadlineDealData[];
}

export async function getTransferDeadlineSummary(): Promise<TransferDeadlineSummaryData> {
  return invoke<TransferDeadlineSummaryData>("get_transfer_deadline_summary");
}
