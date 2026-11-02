import { invoke } from "@tauri-apps/api/core";

import type { FreeAgentTalkData, FreeAgentTalkReason, GameStateData } from "../store/gameStore";

export interface FreeAgentContractProjection {
  current_annual_wage_bill: number;
  projected_annual_wage_bill: number;
  annual_wage_budget: number;
  annual_soft_cap: number;
  current_weekly_wage_spend: number;
  projected_weekly_wage_spend: number;
  current_cash_runway_weeks: number | null;
  projected_cash_runway_weeks: number | null;
  currently_over_budget: boolean;
  policy_allows: boolean;
}

/**
 * What an offer, an answer to a counter or a withdrawal came to. An offer is never signed on the
 * spot: `pending` means the player has it and answers on `respond_on`.
 */
export interface FreeAgentContractResponseData {
  outcome: "pending" | "counter_offer" | "accepted" | "rejected";
  game: GameStateData;
  talk: FreeAgentTalkData | null;
  respond_on: string | null;
  reason: FreeAgentTalkReason | null;
  suggested_wage: number | null;
  suggested_years: number | null;
}

export type FreeAgentInterestLevel = "keen" | "hesitant" | "unlikely";
export type FreeAgentPlaceInSide = "key" | "starter" | "rotation" | "backup";

/** How the player would take the terms now, and who else is bidding. Not a promise. */
export interface FreeAgentInterestData {
  verdict: FreeAgentInterestLevel;
  main_obstacle: FreeAgentTalkReason | null;
  expected_wage: number;
  expected_years: number;
  place_in_side: FreeAgentPlaceInSide;
  days_unattached: number;
  min_response_days: number;
  max_response_days: number;
  rival_clubs: { club_id: string; club_name: string }[];
  open_talk: FreeAgentTalkData | null;
}

/**
 * Where the manager's senior squad list stands. The limit is an administrative registration
 * limit of the club, not the player's decision: `blocks_signing` says only that a place must be
 * freed first (by releasing, selling, loaning out or moving a player to the youth squad).
 */
export interface SquadCapacityData {
  registered_seniors: number;
  senior_limit: number;
  youth: number;
  loaned_in: number;
  loaned_out: number;
  free_places: number;
  /** Places to free before one more player can be signed (0 when there is room). */
  releases_needed: number;
  blocks_signing: boolean;
}

export interface FreeAgentContractProjectionResponseData {
  projection: FreeAgentContractProjection;
  /** The player would value a stepping-stone promise; the backend refuses it for anyone else. */
  stepping_stone_available: boolean;
  /** A translation key: why a signing bonus of the size asked about cannot be paid, if it cannot. */
  signing_bonus_error?: string | null;
  interest: FreeAgentInterestData;
  /** The club's senior squad list now, so a full list is explained with its numbers. */
  squad_capacity?: SquadCapacityData | null;
}

export async function offerFreeAgentContract(
  playerId: string,
  weeklyWage: number,
  contractYears: number,
  promiseSteppingStone = false,
  signingBonus = 0,
): Promise<FreeAgentContractResponseData> {
  return invoke<FreeAgentContractResponseData>("offer_free_agent_contract", {
    playerId,
    weeklyWage,
    contractYears,
    promiseSteppingStone,
    signingBonus,
  });
}

/** The manager takes the terms the player asked for in his counter-offer; he signs at once. */
export async function acceptFreeAgentCounter(
  playerId: string,
): Promise<FreeAgentContractResponseData> {
  return invoke<FreeAgentContractResponseData>("accept_free_agent_counter", { playerId });
}

/** The manager takes an offer back, or lets a counter go. */
export async function withdrawFreeAgentOffer(
  playerId: string,
): Promise<FreeAgentContractResponseData> {
  return invoke<FreeAgentContractResponseData>("withdraw_free_agent_offer", { playerId });
}

export async function previewFreeAgentContractImpact(
  playerId: string,
  weeklyWage: number,
  signingBonus = 0,
  contractYears?: number,
  promiseSteppingStone = false,
): Promise<FreeAgentContractProjectionResponseData> {
  return invoke<FreeAgentContractProjectionResponseData>("preview_free_agent_contract_impact", {
    playerId,
    weeklyWage,
    signingBonus,
    contractYears,
    promiseSteppingStone,
  });
}
