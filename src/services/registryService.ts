import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";

/** Which source position group a search accepts. */
export type RegistryPositionFilter = "any" | "goalkeeper" | "defender" | "midfielder" | "forward";

/** Whether an external player can be signed now. */
export type RegistryAvailability = "all" | "free_agent" | "contracted";

export type RegistrySortKey = "name" | "age" | "ovr" | "value" | "club";

/**
 * One page request. The clock, roster exclusions and paging clamp are decided by the backend, so
 * this never carries a date or an exclusion list.
 */
export interface RegistrySearchRequest {
  text: string;
  nation: string | null;
  position: RegistryPositionFilter;
  minAge: number | null;
  maxAge: number | null;
  club: string;
  availability: RegistryAvailability;
  sort: RegistrySortKey;
  descending: boolean;
  page: number;
  pageSize: number;
  /** Also search records the game classified unusable, to show why they were excluded. */
  includeIncomplete: boolean;
}

export interface RegistrySummary {
  id: string;
  name: string;
  age: number | null;
  nationality: string | null;
  position: string | null;
  positionGroup: string | null;
  ovr: number;
  potential: number;
  club: string | null;
  /** "free_agent" | "contracted", projected from the contract end against the career clock. */
  ownership: string;
  contractEnd: string | null;
  marketValue: number;
  weeklyWage: number;
  /** What an external club asks, in euros; zero for a free agent. */
  askingPrice: number;
  status: string;
}

export interface RegistryCounts {
  all: number;
  freeAgent: number;
  contracted: number;
  incomplete: number;
}

export interface RegistryPage {
  items: RegistrySummary[];
  total: number;
  page: number;
  pageSize: number;
  counts: RegistryCounts;
}

export interface RegistryPlayerDetail {
  id: string;
  name: string;
  firstName: string;
  lastName: string;
  commonName: string | null;
  age: number | null;
  birthDate: string | null;
  nation: string | null;
  nationality: string | null;
  position: string | null;
  positionGroup: string | null;
  naturalPosition: string | null;
  alternatePositions: string[];
  footedness: string;
  weakFoot: number;
  attributes: Record<string, number> | null;
  ovr: number;
  potential: number;
  clubId: number | null;
  clubName: string | null;
  clubNation: string | null;
  clubReputation: number;
  ownership: string;
  contractEnd: string | null;
  marketValue: number;
  weeklyWage: number;
  valueSource: string;
  wageSource: string;
  askingPrice: number;
  status: string;
  exclusionReason: string | null;
  fmUid: number | null;
  fmUidStatus: string;
  sourceRowHash: string;
}

/** What a recruitment would cost and the one key blocking it, if any. */
export interface ExternalRecruitPreview {
  playerId: string;
  freeAgent: boolean;
  alreadyInWorld: boolean;
  fee: number;
  /** The contracted club's own floor for this player. `fee` is never below it. */
  minimumFee: number;
  /** "accepted", "countered" or "rejected" in the backend's deterministic club model. */
  clubDecision: "accepted" | "countered" | "rejected";
  weeklyWage: number;
  contractYears: number;
  contractEnd: string;
  canAffordFee: boolean;
  wagePolicyAllows: boolean;
  windowOpen: boolean;
  blocker: string | null;
}

export interface ExternalRecruitOutcome {
  playerId: string;
  freeAgent: boolean;
  fee: number;
  weeklyWage: number;
  contractEnd: string;
  fromClub: string | null;
}

export interface RecruitExternalResponse {
  /** The mutated career, exactly as every other command returns it. */
  game: GameStateData;
  outcome: ExternalRecruitOutcome;
}

/** What the manager puts on the table. `null`/omitted keeps the buyer's standard term. */
export interface ExternalNegotiationRequest {
  fee?: number | null;
  weeklyWage?: number | null;
  contractYears?: number | null;
  signingBonus?: number | null;
}

/** Where one negotiation stands; the screen shows it instead of inferring it from the numbers. */
export type ExternalNegotiationStatus =
  | "agreed"
  | "club_countered"
  | "club_rejected"
  | "player_countered"
  | "player_refused"
  | "blocked";

/** One complete answer: both parties' decisions and every check the move still has to pass. */
export interface ExternalNegotiationOutcome {
  playerId: string;
  freeAgent: boolean;
  status: ExternalNegotiationStatus;
  fee: number;
  minimumFee: number;
  clubDecision: "accepted" | "countered" | "rejected";
  weeklyWage: number;
  contractYears: number;
  signingBonus: number;
  contractEnd: string;
  playerVerdict: "accepts" | "counters" | "refuses";
  playerScore: number;
  mainObstacle: string | null;
  canAffordFee: boolean;
  wagePolicyAllows: boolean;
  canAffordBonus: boolean;
  windowOpen: boolean;
  blocker: string | null;
  fromClub: string | null;
}

export interface ExternalRegistryStatus {
  installed: boolean;
  schemaVersion: number | null;
  sourceSha256: string | null;
  playersSeen: number;
  eligible: number;
  freeAgents: number;
  activeWorld: number;
  missingIdentity: number;
  missingNation: number;
  noPosition: number;
  outsideAgeRange: number;
  bytes: number;
}

export function searchExternalPlayers(request: RegistrySearchRequest): Promise<RegistryPage> {
  return invoke<RegistryPage>("search_external_players", { request });
}

export function getExternalPlayer(playerId: string): Promise<RegistryPlayerDetail | null> {
  return invoke<RegistryPlayerDetail | null>("get_external_player", { playerId });
}

export function previewExternalRecruitment(playerId: string): Promise<ExternalRecruitPreview> {
  return invoke<ExternalRecruitPreview>("preview_external_recruitment", { playerId });
}

export function recruitExternalPlayer(playerId: string): Promise<RecruitExternalResponse> {
  return invoke<RecruitExternalResponse>("recruit_external_player", { playerId });
}

/** Ask the selling club and the player what they make of these terms. Pure: nothing moves. */
export function negotiateExternalTransfer(
  playerId: string,
  request: ExternalNegotiationRequest,
): Promise<ExternalNegotiationOutcome> {
  return invoke<ExternalNegotiationOutcome>("negotiate_external_transfer", { playerId, request });
}

/** Complete an agreed negotiation. A counter is refused: it is not a signature. */
export function finalizeExternalTransfer(
  playerId: string,
  request: ExternalNegotiationRequest,
): Promise<RecruitExternalResponse> {
  return invoke<RecruitExternalResponse>("finalize_external_transfer", { playerId, request });
}

export function getExternalRegistryStatus(): Promise<ExternalRegistryStatus> {
  return invoke<ExternalRegistryStatus>("get_external_registry_status");
}
