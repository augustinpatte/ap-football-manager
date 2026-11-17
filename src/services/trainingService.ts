import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";
import type { TrainingFocusAttributesData, TrainingGroupData } from "../store/types";

// Lives in `store/types.ts` with the rest of the wire shape — `TeamData.training_groups` needs
// it, and a type the store depends on cannot sit in a service the store does not import.
// Re-exported here so existing imports keep working.
export type { TrainingGroupData } from "../store/types";

export async function setTraining(focus: string, intensity: string): Promise<GameStateData> {
  return invoke<GameStateData>("set_training", {
    focus,
    intensity,
  });
}

export async function setTrainingSchedule(schedule: string): Promise<GameStateData> {
  return invoke<GameStateData>("set_training_schedule", {
    schedule,
  });
}

export async function setTrainingGroups(groups: TrainingGroupData[]): Promise<GameStateData> {
  return invoke<GameStateData>("set_training_groups", {
    groups,
  });
}

export async function setPlayerTrainingFocus(
  playerId: string,
  focus: string | null,
): Promise<GameStateData> {
  return invoke<GameStateData>("set_player_training_focus", {
    playerId,
    focus,
  });
}

export async function getTrainingFocusAttributes(): Promise<TrainingFocusAttributesData[]> {
  return invoke<TrainingFocusAttributesData[]>("get_training_focus_attributes");
}

/** What the club's training will really do today and this week, as the backend works it out. */
export interface TrainingOutlookData {
  is_training_day: boolean;
  /** Why today's session is lighter than the stored plan. */
  taper: "nearMatch" | "congested" | null;
  /** The intensity today's session actually runs at. */
  session_intensity: string;
  fixtures_this_week: number;
  /** Teaching quality of the coaching staff for the stored focus; 1.0 is average. */
  coaching_effect: number;
  /** Recovery from the medical department and fitness coaches; 1.0 is none. */
  recovery_effect: number;
  starters_condition: number;
  /** Who sets the plan when training is handed over; null when the manager keeps it. */
  delegated_to: { name: string; post: string; overall: number } | null;
  proposal_waiting: boolean;
}

/** The outlook for the user's club; null when the user has no club. */
export async function getTrainingOutlook(): Promise<TrainingOutlookData | null> {
  return invoke<TrainingOutlookData | null>("get_training_outlook");
}
