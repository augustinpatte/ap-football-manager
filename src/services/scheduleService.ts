import { invoke } from "@tauri-apps/api/core";
import type { FixtureCompetition } from "../store/types";

export interface ScheduleQuery {
  competition_id: string;
}

export interface FixtureResult {
  home_goals: number;
  away_goals: number;
}

export interface FixtureSummary {
  id: string;
  matchday: number;
  date: string;
  home_team_id: string;
  home_team_name: string;
  away_team_id: string;
  away_team_name: string;
  competition: FixtureCompetition;
  competition_id: string;
  /** "Scheduled" | "InProgress" | "Completed" */
  status: string;
  result: FixtureResult | null;
  /** The date the match was first scheduled for, when it has been moved. */
  rescheduled_from?: string | null;
}

export interface MatchdayGroup {
  key: string;
  date: string;
  matchday: number;
  competition: FixtureCompetition;
  is_next_user_match: boolean;
  fixtures: FixtureSummary[];
}

export interface ScheduleSlice {
  competition_id: string;
  competition_name: string;
  today: string;
  past_groups: MatchdayGroup[];
  upcoming_groups: MatchdayGroup[];
  next_user_match_date: string | null;
}

export function fetchSchedule(query: ScheduleQuery): Promise<ScheduleSlice> {
  return invoke<ScheduleSlice>("get_schedule", { query });
}

/** One of the manager's matches, from whichever competition holds it (see `get_user_schedule`). */
export interface UserFixture {
  fixture: FixtureSummary;
  competition_name: string;
  is_home: boolean;
  is_next: boolean;
}

export interface UserSchedule {
  today: string;
  fixtures: UserFixture[];
}

/** The manager's matches across every competition his club plays in, in date order. */
export async function fetchUserSchedule(): Promise<UserSchedule | null> {
  return (await invoke<UserSchedule | null>("get_user_schedule")) ?? null;
}
