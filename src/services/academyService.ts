import { invoke } from "@tauri-apps/api/core";

import type { RatingConfidence, RatingView } from "./playerRatingService";

export type AcademyVerdict =
  | "ready_for_senior"
  | "keep_developing"
  | "loan_candidate"
  | "unlikely_to_progress";

export interface AcademyRow {
  player_id: string;
  full_name: string;
  age: number;
  natural_position: string;
  /** His rating, exact: he is the club's own player. */
  ovr: number;
  /**
   * What the youth staff judge his ceiling to be: always a band (`kind: "estimate"`), never the
   * number the development model holds. A ceiling is not a promise.
   */
  ceiling: RatingView;
  verdict: AcademyVerdict;
  /** Suffixes of `academy.reason.*` keys. */
  reasons: string[];
  /** Rating of the senior squad's cover in his position group: the bar he is measured on. */
  squad_cover: number;
  seasons_left_in_academy: number;
  injured: boolean;
  on_loan: boolean;
  loan_listed: boolean;
  contract_end: string | null;
  weekly_wage: number;
  /** His individual training focus; `null` when he follows his group's or the team's. */
  training_focus: string | null;
  /** A translation key when no senior place is free for him. */
  promotion_blocked: string | null;
  advice: RowAdvice;
}

export type AdviceState = "recommended" | "available" | "blocked";

/** Whether the review recommends an action, merely allows it, or the game would refuse it, and why. */
export interface ActionAdvice {
  state: AdviceState;
  /** Suffixes of `academy.advice.*` keys, like `promote.onLoan`. */
  reasons: string[];
}

export interface RowAdvice {
  promote: ActionAdvice;
  loan: ActionAdvice;
  /** Staying is what happens when the manager does nothing: this says whether that is the advice. */
  keep: ActionAdvice;
  /** Present only when his contract ends within the renewal horizon. */
  renew: ActionAdvice | null;
}

/** What the club's youth department is and does, in the terms the screen explains it. */
export interface YouthCoaching {
  rating: number;
  has_staff: boolean;
  confidence: RatingConfidence;
  /** Width, in rating points, of the band the department reads a ceiling to. */
  band_width: number;
  /** Extra progression the academy gets from the youth coaches, in percent. */
  progression_bonus_pct: number;
}

export interface AcademySummary {
  coaching: YouthCoaching;
  /** The middle of the assessed ceilings, averaged; `null` with an empty academy. */
  average_ceiling: number | null;
  /** Youngsters whose ceiling is assessed to reach the A tier even on the low reading. */
  high_ceiling_count: number;
}

export interface AcademyReview {
  rows: AcademyRow[];
  summary: AcademySummary | null;
  seniors: number;
  senior_limit: number;
  /** Academy players per position group `[GK, DEF, MID, FWD]` against the target. */
  by_group: [number, number, number, number];
  target_by_group: [number, number, number, number];
  next_intake: number;
}

/** `null` when the manager has no club. */
export async function getAcademyReview(): Promise<AcademyReview | null> {
  return invoke<AcademyReview | null>("get_academy_review");
}
