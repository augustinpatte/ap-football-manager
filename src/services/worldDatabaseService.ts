import { invoke } from "@tauri-apps/api/core";

/** A world the game can start a career in, as `list_world_databases` describes it. */
export interface WorldDatabaseInfo {
  id: string;
  name: string;
  description: string;
  team_count: number;
  player_count: number;
  history_mode: string;
  base_year: number | null;
  snapshot_date: string | null;
  /** "builtin" | "user" */
  source: string;
  /** Filesystem path; empty for the built-in random world. */
  path: string;
  world_id: string;
  competitions: string[];
  /** RFC 3339 date a career opens on, when the world fixes it. */
  start_date: string | null;
}

export interface Fm24WorldInstalled {
  worldId: string;
  path: string;
  sha256: string;
  bytes: number;
  checksTotal: number;
}

export async function listWorldDatabases(): Promise<WorldDatabaseInfo[]> {
  return (await invoke<WorldDatabaseInfo[] | null>("list_world_databases")) ?? [];
}

/** The `world.db` the game found where it looks, or `null` when the player must point at one. */
export function findFm24WorldSource(): Promise<string | null> {
  return invoke<string | null>("find_fm24_world_source");
}

/** Builds the FM24 world from the player's own `world.db`; rejects with a translation key. */
export function installFm24World(worldDb?: string): Promise<Fm24WorldInstalled> {
  return invoke<Fm24WorldInstalled>("install_fm24_world", { worldDb: worldDb ?? null });
}

/** Whether the FM24 world is installed and whether it predates the current economy fixes. */
export interface Fm24InstallStatus {
  installed: boolean;
  outdated: boolean;
  revision: number | null;
  currentRevision: number;
}

export function getFm24InstallStatus(): Promise<Fm24InstallStatus> {
  return invoke<Fm24InstallStatus>("get_fm24_install_status");
}
