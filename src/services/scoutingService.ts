import { invoke } from "@tauri-apps/api/core";

import type { GameStateData, ScoutedPlayerView } from "../store/gameStore";
import type { RatingView } from "./playerRatingService";

export interface StartYouthScoutingInput {
  scoutId: string;
  region?: string | null;
  objective?: string | null;
  targetPosition?: string | null;
}

export async function getScoutedReport(playerId: string): Promise<ScoutedPlayerView | null> {
  return invoke<ScoutedPlayerView | null>("get_scouted_report", { playerId });
}

export async function sendScout(scoutId: string, playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("send_scout", {
    scoutId,
    playerId,
  });
}

export async function startYouthScouting(input: StartYouthScoutingInput): Promise<GameStateData> {
  return invoke<GameStateData>("start_youth_scouting", {
    scoutId: input.scoutId,
    region: input.region ?? null,
    objective: input.objective ?? null,
    targetPosition: input.targetPosition ?? null,
  });
}

export async function cancelYouthScouting(assignmentId: string): Promise<GameStateData> {
  return invoke<GameStateData>("cancel_youth_scouting", {
    assignmentId,
  });
}

export async function reassignYouthScouting(
  assignmentId: string,
  scoutId: string,
): Promise<GameStateData> {
  return invoke<GameStateData>("reassign_youth_scouting", {
    assignmentId,
    scoutId,
  });
}

export type ReportStatus = "unscouted" | "current" | "stale";
export type ReportConfidence = "low" | "moderate" | "high";
export type SquadImpact = "starter" | "cover" | "no_gain";
export type DevelopmentOutlook = "high_growth" | "growing" | "near_peak" | "declining" | "unclear";

/** A closed range of ratings: the truth lies inside it, the middle is not the truth. */
export interface RatingRange {
  low: number;
  high: number;
}

export interface ScoutReportView {
  scouted_on: string;
  scout_name: string;
  /** Reports narrowed together; a higher number is a firmer estimate. */
  reports: number;
  ability: RatingRange;
  /** `null` when the scout could not judge potential. */
  potential: RatingRange | null;
  confidence: ReportConfidence;
  /** Attribute names, as in `common.attributes.*`. */
  strengths: string[];
  weaknesses: string[];
}

/** A player as the manager's club knows him: public facts, plus the club's own report if any. */
export interface ScoutingTargetView {
  player_id: string;
  full_name: string;
  age: number;
  natural_position: string;
  alternate_positions: string[];
  nationality: string;
  team_id: string | null;
  team_name: string | null;
  /** The overall rating as the manager may read it; the true rating is not on this view. */
  rating: RatingView;
  market_value: number;
  weekly_wage: number;
  contract_end: string | null;
  is_free_agent: boolean;
  transfer_listed: boolean;
  loan_listed: boolean;
  injured: boolean;
  status: ReportStatus;
  shortlisted: boolean;
  being_scouted: boolean;
  report: ScoutReportView | null;
  outlook: DevelopmentOutlook;
  squad_impact: SquadImpact | null;
  /** `null` for a free agent: there is no fee. */
  fee_affordable: boolean | null;
  wage_permitted: boolean;
  /** Suffixes of `scouting.risk.*` keys. */
  risks: string[];
}

export interface ScoutingDossierView {
  reports: ScoutingTargetView[];
  shortlist: ScoutingTargetView[];
}

export async function getScoutingDossier(): Promise<ScoutingDossierView> {
  return invoke<ScoutingDossierView>("get_scouting_dossier");
}

export async function shortlistPlayer(playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("shortlist_player", { playerId });
}

export async function unshortlistPlayer(playerId: string): Promise<GameStateData> {
  return invoke<GameStateData>("unshortlist_player", { playerId });
}

export async function cancelScouting(assignmentId: string): Promise<GameStateData> {
  return invoke<GameStateData>("cancel_scouting", { assignmentId });
}

export async function reassignScouting(
  assignmentId: string,
  scoutId: string,
): Promise<GameStateData> {
  return invoke<GameStateData>("reassign_scouting", { assignmentId, scoutId });
}
