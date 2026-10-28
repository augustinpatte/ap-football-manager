import { invoke } from "@tauri-apps/api/core";
import type {
  EditionSummaryData,
  LeagueData,
  SeasonAwardsData,
  WorldCupChampionData,
} from "../store/types";

export interface PlayerNameEntry {
  match_name: string;
  full_name: string;
  team_id: string | null;
  team_name: string | null;
}

export interface CompetitionsView {
  competitions: LeagueData[];
  /** Finished editions, most recent first. */
  history?: EditionSummaryData[];
  team_names: Record<string, string>;
  national_team_names: Record<string, string>;
  national_team_name_keys: Record<string, string>;
  player_names: Record<string, PlayerNameEntry>;
  world_cup_champions: WorldCupChampionData[];
  manager_team_id: string | null;
  active_competition_ids: string[];
}

export async function fetchCompetitionsView(): Promise<CompetitionsView> {
  return invoke<CompetitionsView>("get_competitions_view", { query: {} });
}

export async function fetchSeasonAwards(): Promise<SeasonAwardsData> {
  return invoke<SeasonAwardsData>("get_season_awards");
}
