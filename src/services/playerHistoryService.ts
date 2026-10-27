import { invoke } from "@tauri-apps/api/core";

import type { CareerEntry, PlayerMovementEntry } from "../store/types";

/** What the game state leaves off every player: one profile loads its own. */
export interface PlayerHistory {
  player_id: string;
  career: CareerEntry[];
  movement_history: PlayerMovementEntry[];
}

export async function getPlayerHistory(playerId: string): Promise<PlayerHistory | null> {
  return invoke<PlayerHistory | null>("get_player_history", { playerId });
}
