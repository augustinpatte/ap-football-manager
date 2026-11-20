import { invoke } from "@tauri-apps/api/core";

/** A broad public reading of a player, from weakest to strongest: never a rating. */
export type PublicStanding = "limited" | "average" | "good" | "strong" | "elite";

export type RatingConfidence = "low" | "moderate" | "high";

/**
 * How the manager may read a player's overall rating, as the backend decides
 * (`ofm_core::rating_view`): exact for his own player and one in his division, a scout's range for a
 * reported player, a public standing for anyone else. A screen shows this and never a raw `ovr`.
 */
export type RatingView =
  | { kind: "exact"; ovr: number }
  | {
      kind: "estimate";
      low: number;
      high: number;
      confidence: RatingConfidence;
      /** `false` when the report predates this season: the player may have changed since. */
      current: boolean;
      scouted_on: string;
    }
  | { kind: "standing"; standing: PublicStanding };

/** The reading of each requested player, by id; an id no player has is not in the answer. */
export async function getPlayerRatings(playerIds: string[]): Promise<Record<string, RatingView>> {
  return await invoke<Record<string, RatingView>>("get_player_ratings", { playerIds });
}
