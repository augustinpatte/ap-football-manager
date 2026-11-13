import { invoke } from "@tauri-apps/api/core";

/**
 * Wire types of the interactive match. The match engine owns every number here; the screen
 * only displays it and sends orders back. A revision identifies the state an order was given
 * against, and an order id makes a retry harmless.
 */

export type MatchSide = "home" | "away";

/** The engine's own positions (`GK`, `CB`, `ST`, ...), shown as the engine spells them. */
export type EnginePosition = string;

export interface MatchTactics {
  mentality: number;
  pressing: number;
  line_height: number;
  width: number;
  tempo: number;
  directness: number;
}

export interface MatchPlayerView {
  id: string;
  name: string;
  shirt_number: number;
  natural_position: EnginePosition;
  /** The role of the slot he occupies; `null` for a bench player. */
  role: EnginePosition | null;
  /** 1 is fresh, 0 is exhausted. */
  condition: number;
  minutes_played: number;
  yellow_cards: number;
  injured: boolean;
}

export interface PendingSubstitution {
  off: string;
  on: string;
}

export interface MatchTeamView {
  side: MatchSide;
  formation: string;
  tactics: MatchTactics;
  on_pitch: MatchPlayerView[];
  bench: MatchPlayerView[];
  used: MatchPlayerView[];
  sent_off: MatchPlayerView[];
  substitutions_used: number;
  substitutions_remaining: number;
  windows_remaining: number;
  pending_substitutions: PendingSubstitution[];
}

export type MatchPhase = "kickoff_pending" | "in_play" | "stoppage" | "half_time" | "full_time";

export interface MatchSessionView {
  session_id: string;
  revision: number;
  clock_ms: number;
  period: "first_half" | "second_half" | "extra_time_first" | "extra_time_second";
  phase: MatchPhase;
  paused: boolean;
  finished: boolean;
  score_home: number;
  score_away: number;
  home: MatchTeamView;
  away: MatchTeamView;
  event_count: number;
}

export type PlayStyleName =
  | "Balanced"
  | "Attacking"
  | "Defensive"
  | "Possession"
  | "Counter"
  | "HighPress";
export type PressingName = "Passive" | "Medium" | "Aggressive";
export type TempoName = "Patient" | "Direct";

export interface PlanView {
  style: PlayStyleName;
  pressing: PressingName;
  tempo: TempoName;
  formation: string;
}

export interface InteractiveView {
  session: MatchSessionView;
  user_side: MatchSide | null;
  assistant_available: boolean;
  assistant_active: boolean;
  supported_formations: string[];
  home_plan: PlanView;
  away_plan: PlanView;
  regulation_minutes: number;
}

/**
 * An event of the canonical stream; `id` is its index there. The rest is the engine's own event
 * JSON, the same shape the 2D replay stores, so both screens read events with the same code.
 */
export interface MatchKeyEvent {
  id: number;
  t_ms: number;
  type: string;
  period?: string;
  side?: MatchSide;
  player?: string;
  scorer?: string;
  assist?: string | null;
  own_goal?: boolean;
  player_off?: string;
  player_on?: string;
  colour?: "yellow" | "second_yellow" | "red";
}

export interface InteractiveSnapshot {
  view: InteractiveView;
  events: MatchKeyEvent[];
  next_event_cursor: number;
}

export type StopReason = "reached" | "period_break" | "key_event" | "paused" | "finished";

export interface AdvanceSnapshot {
  stop: StopReason;
  snapshot: InteractiveSnapshot;
}

export interface TacticalChange {
  style?: PlayStyleName;
  pressing?: PressingName;
  tempo?: TempoName;
  formation?: string;
}

export type UserOrder =
  | { Tactics: TacticalChange }
  | { Substitute: { player_off_id: string; player_on_id: string } }
  | { CancelSubstitution: { player_off_id: string } };

export interface OrderReceipt {
  command_id: string;
  accepted: boolean;
  /** Accepted but not yet in force: a substitution waits for the next stoppage. */
  queued: boolean;
  /** The engine's rejection code, for example `no_substitutions_left`. */
  rejection: string | null;
  revision: number;
  duplicate: boolean;
}

export interface OrderSnapshot {
  receipt: OrderReceipt;
  snapshot: InteractiveSnapshot;
}

export type AdvanceTarget =
  | { kind: "for_minutes"; value: number }
  | { kind: "to_minute"; value: number }
  | { kind: "to_end" };

export function startInteractiveMatch(): Promise<InteractiveSnapshot> {
  return invoke<InteractiveSnapshot>("start_interactive_match");
}

/** The one-time notice (a translation key) that a match saved in progress could not be resumed. */
export function takeMatchRestoreNotice(): Promise<string | null> {
  return invoke<string | null>("take_match_restore_notice");
}

export interface ResumableMatch {
  fixture_index: number;
  competition_id: string;
  fixture_id: string;
}

/** The match restored from the save, if the career was saved in the middle of one. */
export function getResumableMatch(): Promise<ResumableMatch | null> {
  return invoke<ResumableMatch | null>("get_resumable_match");
}

export function getInteractiveMatchState(eventCursor: number): Promise<InteractiveSnapshot> {
  return invoke<InteractiveSnapshot>("get_interactive_match_state", { eventCursor });
}

export function advanceInteractiveMatch(
  target: AdvanceTarget,
  eventCursor: number,
  stopAtKeyEvents = true,
): Promise<AdvanceSnapshot> {
  return invoke<AdvanceSnapshot>("advance_interactive_match", {
    request: { target, stop_at_key_events: stopAtKeyEvents },
    eventCursor,
  });
}

export function submitMatchCommand(
  commandId: string,
  expectedRevision: number,
  order: UserOrder,
  eventCursor: number,
): Promise<OrderSnapshot> {
  return invoke<OrderSnapshot>("submit_match_command", {
    command: { command_id: commandId, expected_revision: expectedRevision, order },
    eventCursor,
  });
}

export function setInteractivePause(
  paused: boolean,
  eventCursor: number,
): Promise<InteractiveSnapshot> {
  return invoke<InteractiveSnapshot>("set_interactive_pause", { paused, eventCursor });
}

export function setInteractiveAssistant(
  active: boolean,
  eventCursor: number,
): Promise<InteractiveSnapshot> {
  return invoke<InteractiveSnapshot>("set_interactive_assistant", { active, eventCursor });
}

/** Applies the finished match to the career; the response is what `finish_live_match` returns. */
export function finishInteractiveMatch<TResponse>(): Promise<TResponse> {
  return invoke<TResponse>("finish_interactive_match");
}
