import { invoke } from "@tauri-apps/api/core";

export interface StoredReplayResponse {
  replay_json: string;
}

export interface SpatialMatchResponse<TGame, TEvent> {
  game: TGame;
  home_goals: number;
  away_goals: number;
  events: TEvent[];
}

export function simulateSpatialMatch<TGame, TEvent>(): Promise<
  SpatialMatchResponse<TGame, TEvent>
> {
  return invoke<SpatialMatchResponse<TGame, TEvent>>("simulate_spatial_match");
}

export function prepareSpatialReplay(): Promise<StoredReplayResponse> {
  return invoke<StoredReplayResponse>("prepare_spatial_replay");
}

export function finishSpatialReplay<TGame, TEvent>(): Promise<SpatialMatchResponse<TGame, TEvent>> {
  return invoke<SpatialMatchResponse<TGame, TEvent>>("finish_spatial_replay");
}

/** Reads the persisted document only; the backend deliberately never simulates here. */
export function loadMatchReplay(
  competitionId: string,
  fixtureId: string,
): Promise<StoredReplayResponse | null> {
  return invoke<StoredReplayResponse | null>("load_spatial_replay", { competitionId, fixtureId });
}
