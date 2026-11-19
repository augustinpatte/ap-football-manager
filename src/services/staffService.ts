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
  | "StaffRenewals"
  | "Lineup"
  | "PlayerRenewals"
  | "PlayerRecruitment"
  | "Tactics"
  | "Offers";

/**
 * The presets the duty settings amount to. `custom` is derived, never chosen. `approval` is shown
 * as Prudent (every major decision is a proposal) and `full` as Total (the assistant conducts the
 * whole strategy within the club's finances); the identifiers are kept so saves and callers do not
 * change.
 */
export type DelegationMode = "manual" | "approval" | "full" | "custom";

export type DecisionOutcome = "executed" | "approved" | "declined" | "leftToManager";

/** What the assistant may commit without asking; each figure caps one decision. */
export interface DelegationLimits {
  /** Highest weekly wage for a staff hire or renewal. */
  max_staff_wage: number;
  /** Highest weekly wage for a player renewal. */
  max_player_wage: number;
  /** Highest weekly wage x 52 x years of a single contract. */
  max_commitment: number;
  /** Highest transfer fee; 0 sends every transfer to the manager. */
  max_transfer_fee: number;
  /**
   * Total authority: the assistant is held to the club's cash, wage room and the board's rules
   * instead of the ceilings above, and may sell and list players. Set only by choosing the Total
   * mode; the backend ignores any value sent with the limits.
   */
  finance_bound?: boolean;
}

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
  /** The staff member the manager put in charge; absent or null means the assistant manager. */
  owner?: string | null;
}

/** A member of staff who runs, or could run, a duty. */
export interface DutyOwnerView {
  id: string;
  name: string;
  post: StaffPost;
  overall: number;
}

/** Who is responsible for one duty and what the manager can change about it (the backend's view). */
export interface DutyView {
  duty: Duty;
  /** Whether "ask me" changes anything for this duty. */
  supports_approval: boolean;
  /** Who runs it today; null when nobody in post can. */
  in_charge: DutyOwnerView | null;
  /** Whether the manager named him, rather than the assistant being the default. */
  named: boolean;
  /** Everyone who could be put in charge, best first; one entry means there is no choice. */
  candidates: DutyOwnerView[];
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
  outcome: DecisionOutcome;
}

export interface DelegationState {
  settings: DutySetting[];
  proposals: DelegationProposal[];
  log: DelegationLogEntry[];
  limits: DelegationLimits;
  /** Players the manager has said are not to be sold. */
  protected_players?: string[];
}

export interface StaffOverview {
  team_id: string;
  posts: PostStatus[];
  members: MemberView[];
  budget: StaffBudgetView;
  delegation: DelegationState;
  delegation_mode: DelegationMode;
  has_assistant: boolean;
  duties: DutyView[];
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

/** Puts a member of staff in charge of a duty; `null` hands it back to the assistant manager. */
export async function setDutyOwner(duty: Duty, staffId: string | null): Promise<GameStateData> {
  return invoke<GameStateData>("set_duty_owner", { duty, staffId });
}

export async function setDelegationMode(mode: DelegationMode): Promise<GameStateData> {
  return invoke<GameStateData>("set_delegation_mode", { mode });
}

export async function setDelegationLimits(limits: DelegationLimits): Promise<GameStateData> {
  return invoke<GameStateData>("set_delegation_limits", { limits });
}

/** Tells the assistant a player is not to be sold (or that he may be again). */
export async function setPlayerProtected(
  playerId: string,
  protectedFromSale: boolean,
): Promise<GameStateData> {
  return invoke<GameStateData>("set_player_protected", {
    playerId,
    protected: protectedFromSale,
  });
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
