import { invoke } from "@tauri-apps/api/core";

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
  ovr: number;
  /** The ceiling: what the development model may let him reach, not a promise. */
  potential: number;
  expected_ovr_next_year: number;
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
  /** A translation key when no senior place is free for him. */
  promotion_blocked: string | null;
}

export interface AcademyReview {
  rows: AcademyRow[];
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
