import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { GameStateData } from "../store/gameStore";
import type { AdvanceMatchResultData } from "./advanceTimeService";
import type { PlayerMovementKind } from "../store/types";
import type { DelegationLogEntry } from "./staffService";

/** How much the fast-forward may decide for the player (Rust: `InterruptPolicy`). */
export type FastForwardPolicy = "stop_at_next_match" | "stop_for_decisions" | "to_target_date";

export interface FastForwardRequest {
  /** `YYYY-MM-DD`; the run ends with the clock reading this date. */
  target_date: string;
  policy: FastForwardPolicy;
}

export type FastForwardDecisionKind =
  | "blocker"
  | "delegation_proposal"
  | "incoming_transfer_offer"
  | "incoming_loan_offer"
  | "free_agent_counter_offer"
  | "transfer_deadline";

export interface FastForwardDecision {
  kind: FastForwardDecisionKind;
  id: string;
  params: Record<string, string>;
}

export type FastForwardStop =
  | { kind: "target_reached" }
  | { kind: "next_match" }
  | { kind: "decisions"; decisions: FastForwardDecision[] }
  | { kind: "season_complete" }
  | { kind: "manager_fired" }
  | { kind: "cancelled" }
  | { kind: "day_limit" }
  | { kind: "error"; error: string };

export interface FastForwardProgress {
  start_date: string;
  current_date: string;
  target_date: string;
  days_done: number;
  days_total: number;
  user_matches_played: number;
  seasons_rolled_over: number;
}

export interface FastForwardPassedDecision {
  date: string;
  decision: FastForwardDecision;
}

/** A list the report keeps short: `total` is the true count, `items` the first of them. */
export interface Listed<T> {
  items: T[];
  total: number;
}

export type StretchEnd = "continues" | "dismissed" | "moved";
export type MatchOutcome = "win" | "draw" | "loss";

export interface ReportMatchLine {
  fixture_id: string;
  competition_id: string;
  competition_name: string;
  competition_name_key: string | null;
  date: string;
  at_home: boolean;
  opponent_id: string;
  opponent_name: string;
  goals_for: number;
  goals_against: number;
  penalties_for: number | null;
  penalties_against: number | null;
  outcome: MatchOutcome;
}

export interface ReportMatches {
  played: number;
  won: number;
  drawn: number;
  lost: number;
  goals_for: number;
  goals_against: number;
  fixtures: Listed<ReportMatchLine>;
}

export type ReportTransferKind =
  | "purchase"
  | "sale"
  | "loan_in"
  | "loan_out"
  | "loan_returned"
  | "loan_ended"
  | "free_signing";

export interface ReportTransferLine {
  date: string;
  kind: ReportTransferKind;
  player_id: string;
  player_name: string;
  fee: number | null;
  counterparty_id: string | null;
  counterparty_name: string | null;
  loan_end_date: string | null;
  via_purchase_option: boolean;
  /** `false` when the ledger records a fee the cash journal never paid: shown as a discrepancy. */
  fee_in_journal: boolean | null;
}

export interface ReportTransfers {
  purchases: number;
  sales: number;
  loans_in: number;
  loans_out: number;
  free_signings: number;
  fees_paid: number;
  fees_received: number;
  moves: Listed<ReportTransferLine>;
}

export type ContractChangeKind = "renewal" | "expired" | "terminated" | "released";

export interface ReportContractLine {
  date: string;
  kind: ContractChangeKind;
  player_id: string;
  player_name: string;
  weekly_wage_before: number | null;
  weekly_wage_after: number | null;
  end_before: string | null;
  end_after: string | null;
}

export interface ReportContracts {
  renewed: number;
  expired: number;
  terminated: number;
  released: number;
  changes: Listed<ReportContractLine>;
}

export type CashKindName =
  | "opening_balance"
  | "player_wages"
  | "staff_wages"
  | "matchday"
  | "sponsorship"
  | "commercial_campaign"
  | "prize_money"
  | "transfer_fee_out"
  | "transfer_fee_in"
  | "signing_bonus"
  | "loan_fee"
  | "contract_termination"
  | "board_support"
  | "facilities"
  | "upkeep"
  | "other";

export interface ReportCashKindTotal {
  kind: CashKindName;
  income: number;
  expense: number;
  net: number;
  posts: number;
}

export interface ReportFinances {
  opening_cash: number;
  closing_cash: number;
  change: number;
  by_kind: ReportCashKindTotal[];
  transfer_fees_out: number;
  transfer_fees_in: number;
  posts: number;
  /** The part of `change` the cash journal does not explain; 0 when the books reconcile. */
  unexplained: number;
  detail_complete: boolean;
}

export interface ReportSquadMove {
  player_id: string;
  player_name: string;
  cause: PlayerMovementKind | null;
  date: string | null;
  counterparty_name: string | null;
  fee: number | null;
}

export interface ReportInjuryLine {
  player_id: string;
  player_name: string;
  injury: string;
  days_remaining_when_seen: number;
  first_seen: string;
  recovered_by: string | null;
  still_at_club: boolean;
}

export interface ReportSquad {
  size_before: number;
  size_after: number;
  arrivals: Listed<ReportSquadMove>;
  departures: Listed<ReportSquadMove>;
  injuries: Listed<ReportInjuryLine>;
}

export interface ReportDelegation {
  executed: number;
  approved: number;
  declined: number;
  left_to_manager: number;
  entries: Listed<DelegationLogEntry>;
}

export type ReportEvent =
  | { kind: "dismissed"; date: string }
  | {
      kind: "season_completed";
      date: string;
      season: number;
      league_name: string;
      position: number;
      total_teams: number;
      points: number;
      division_move: string;
    }
  | {
      kind: "knockout_advanced" | "knockout_eliminated";
      date: string;
      fixture_id: string;
      competition_id: string;
      competition_name: string;
    }
  | {
      kind: "board_message";
      date: string;
      message_id: string;
      subject_key: string | null;
      subject: string;
    };

/** What happened to one club under this manager between `from` and `until` (the first day not played). */
export interface ClubReport {
  club_id: string;
  club_name: string;
  from: string;
  until: string;
  days_played: number;
  ended: StretchEnd;
  matches: ReportMatches;
  transfers: ReportTransfers;
  contracts: ReportContracts;
  finances: ReportFinances;
  squad: ReportSquad;
  delegation: ReportDelegation;
  events: ReportEvent[];
}

export interface SimulationReport {
  clubs: ClubReport[];
}

export interface FastForwardResponse {
  progress: FastForwardProgress;
  stop: FastForwardStop;
  /** One entry per season the run rolled over (target-date policy only). */
  season_summaries: unknown[];
  /** What a target-date run went past that a stopping policy would have stopped for. */
  passed_decisions: FastForwardPassedDecision[];
  /** What happened in the days played; absent from a backend that predates it. */
  report?: SimulationReport;
  game: GameStateData;
  results: AdvanceMatchResultData[];
}

export async function fastForwardToDate(request: FastForwardRequest): Promise<FastForwardResponse> {
  return invoke<FastForwardResponse>("fast_forward_to_date", { request });
}

/** Ask the running fast-forward to stop at the next day boundary. */
export async function cancelFastForward(): Promise<boolean> {
  return invoke<boolean>("cancel_fast_forward");
}

/** Progress while a run is in progress, throttled by the backend. */
export function onFastForwardProgress(
  handler: (progress: FastForwardProgress) => void,
): Promise<UnlistenFn> {
  return listen<FastForwardProgress>("fast-forward-progress", (event) => handler(event.payload));
}
