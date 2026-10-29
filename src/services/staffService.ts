import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";
import type { ScoutingAssignment, StaffData, YouthScoutingAssignment } from "../store/types";

export interface StaffSlice {
  team_staff: StaffData[];
  available_staff: StaffData[];
  scouting_assignments: ScoutingAssignment[];
  youth_scouting_assignments: YouthScoutingAssignment[];
}

export async function getStaff(teamId: string): Promise<StaffSlice> {
  return invoke<StaffSlice>("get_staff", { teamId });
}

export async function releaseStaff(staffId: string): Promise<GameStateData> {
  return invoke<GameStateData>("release_staff", { staffId });
}

// --- Staff management (posts, contracts, budget, delegation, assistant) -------------------------

export type StaffPost =
  | "AssistantManager"
  | "FirstTeamCoach"
  | "FitnessCoach"
  | "GoalkeepingCoach"
  | "YouthCoach"
  | "HeadOfYouth"
  | "Scout"
  | "HeadOfScouting"
  | "Analyst"
  | "Physio"
  | "HeadOfMedical";

export type Duty =
  | "Training"
  | "StaffRecruitment"
  | "Scouting"
  | "Youth"
  | "MatchPreparation"
  | "StaffRenewals";

export interface PostStatus {
  post: StaffPost;
  count: number;
  limit: number;
  required: number;
}

export interface MemberView {
  id: string;
  post: StaffPost;
  overall: number;
  /** `null` for an open-ended contract. */
  days_left: number | null;
  severance: number;
  renewal_ask: number;
}

export interface StaffBudgetView {
  budget: number;
  weekly_bill: number;
  headroom: number;
  signing_fee_weeks: number;
  max_years: number;
  severance_pct: number;
}

export interface DutySetting {
  duty: Duty;
  delegated: boolean;
  confirm_major: boolean;
}

export type ProposalAction = { kind: string } & Record<string, unknown>;

export interface DelegationProposal {
  id: string;
  duty: Duty;
  created: string;
  action: ProposalAction;
  title_key: string;
  params: [string, string][];
  reason_key: string;
}

export interface DelegationLogEntry {
  date: string;
  duty: Duty;
  title_key: string;
  params: [string, string][];
}

export interface DelegationState {
  settings: DutySetting[];
  proposals: DelegationProposal[];
  log: DelegationLogEntry[];
}

export interface StaffOverview {
  team_id: string;
  posts: PostStatus[];
  members: MemberView[];
  budget: StaffBudgetView;
  delegation: DelegationState;
  has_assistant: boolean;
}

export interface CandidateView {
  staff: StaffData;
  post: StaffPost;
  overall: number;
  asking_wage: number;
  /** Translation key of what stops the club signing them at their ask, if anything. */
  blocker: string | null;
}

export type OfferResponse =
  | { status: "accepted" }
  | { status: "refused" }
  | { status: "counter"; wage: number };

export interface OfferOutcome {
  response: OfferResponse;
  game: GameStateData;
}

export interface AssistantRecommendation {
  id: string;
  kind: string;
  /** 1 (worth a look) to 3 (act now). */
  severity: number;
  player_id: string | null;
  title_key: string;
  reason_key: string;
  params: [string, string][];
}

export async function getStaffOverview(): Promise<StaffOverview> {
  return invoke<StaffOverview>("get_staff_overview");
}

export async function searchStaffCandidates(
  post: StaffPost | null,
  minOverall = 0,
): Promise<CandidateView[]> {
  return invoke<CandidateView[]>("search_staff_candidates", { post, minOverall });
}

export async function proposeStaffContract(
  staffId: string,
  wage: number,
  years: number,
): Promise<OfferOutcome> {
  return invoke<OfferOutcome>("propose_staff_contract", { staffId, wage, years });
}

export async function renewStaffContract(
  staffId: string,
  wage: number,
  years: number,
): Promise<OfferOutcome> {
  return invoke<OfferOutcome>("renew_staff_contract", { staffId, wage, years });
}

export async function setDutyDelegation(
  duty: Duty,
  delegated: boolean,
  confirmMajor: boolean,
): Promise<GameStateData> {
  return invoke<GameStateData>("set_duty_delegation", { duty, delegated, confirmMajor });
}

export async function resolveDelegationProposal(
  proposalId: string,
  accept: boolean,
): Promise<GameStateData> {
  return invoke<GameStateData>("resolve_delegation_proposal", { proposalId, accept });
}

export async function getAssistantRecommendations(): Promise<AssistantRecommendation[]> {
  return invoke<AssistantRecommendation[]>("get_assistant_recommendations");
}

export async function applyAssistantRecommendation(
  recommendationId: string,
): Promise<GameStateData> {
  return invoke<GameStateData>("apply_assistant_recommendation", { recommendationId });
}
