import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { GameStateData } from "../store/gameStore";
import type { AdvanceMatchResultData } from "./advanceTimeService";

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

export interface FastForwardResponse {
  progress: FastForwardProgress;
  stop: FastForwardStop;
  /** One entry per season the run rolled over (target-date policy only). */
  season_summaries: unknown[];
  /** What a target-date run went past that a stopping policy would have stopped for. */
  passed_decisions: FastForwardPassedDecision[];
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
