// Pure model of the live 2D match: the frames the engine produced so far, how to read the world at
// any instant between them, which stretches a highlight mode shows, and how fast the picture moves
// through them. Nothing here simulates football: a position is a recorded frame, or a straight
// line between two recorded frames half a second apart so the picture is smooth rather than
// stepping; an event is the engine's. Malformed frames are dropped, never repaired.

import type { ClockView, LiveFrame, MatchKeyEvent } from "../../services/interactiveMatchService";

export type HighlightMode = "key" | "extended" | "full";
export const HIGHLIGHT_MODES: readonly HighlightMode[] = ["key", "extended", "full"];
export const SPEEDS = [1, 2, 4, 8, 16] as const;
export type PlaybackSpeed = (typeof SPEEDS)[number];

export const PITCH_LENGTH_M = 105;
export const PITCH_WIDTH_M = 68;

/** Frames are half a second apart; a player cannot have run further than this in that time. */
const MAX_PLAYER_STEP_M = 12;
/** A kick can carry the ball this far in half a second; more is a restart, not movement. */
const MAX_BALL_STEP_M = 30;
const MAX_FRAME_GAP_MS = 2_000;

// ---------------------------------------------------------------------------------------------
// Frames

function isFiniteNumbers(values: readonly unknown[], length: number): boolean {
  return (
    values.length === length && values.every((v) => typeof v === "number" && Number.isFinite(v))
  );
}

/** A frame the pitch can draw: finite numbers, the right shapes, players inside a sane area. */
export function isValidFrame(frame: unknown): frame is LiveFrame {
  if (typeof frame !== "object" || frame === null) return false;
  const f = frame as Partial<LiveFrame>;
  if (typeof f.t_ms !== "number" || !Number.isFinite(f.t_ms) || f.t_ms < 0) return false;
  if (!Array.isArray(f.ball) || !isFiniteNumbers(f.ball, 3)) return false;
  if (!Array.isArray(f.score) || !isFiniteNumbers(f.score, 2)) return false;
  if (!Array.isArray(f.players)) return false;
  return f.players.every(
    (p) =>
      Array.isArray(p) &&
      isFiniteNumbers(p, 4) &&
      Number.isInteger(p[0]) &&
      p[0] >= 0 &&
      Math.abs(p[1]) < 200 &&
      Math.abs(p[2]) < 200,
  );
}

/** The frames received so far, strictly increasing in time. */
export class FrameStore {
  private readonly list: LiveFrame[] = [];
  /** Frames refused because they were malformed or out of order. */
  rejected = 0;

  get frames(): readonly LiveFrame[] {
    return this.list;
  }

  get startMs(): number {
    return this.list[0]?.t_ms ?? 0;
  }

  get headMs(): number {
    return this.list[this.list.length - 1]?.t_ms ?? 0;
  }

  get length(): number {
    return this.list.length;
  }

  /** Appends the valid, in-order frames of a batch; returns how many were kept. */
  append(batch: readonly unknown[]): number {
    let kept = 0;
    for (const candidate of batch) {
      if (!isValidFrame(candidate)) {
        this.rejected += 1;
        continue;
      }
      const last = this.list[this.list.length - 1];
      if (last && candidate.t_ms <= last.t_ms) {
        this.rejected += 1;
        continue;
      }
      this.list.push(candidate);
      kept += 1;
    }
    return kept;
  }

  /** Index of the last frame at or before `t`, or -1 when `t` precedes them all. */
  indexAtOrBefore(t: number): number {
    let low = 0;
    let high = this.list.length - 1;
    let found = -1;
    while (low <= high) {
      const middle = (low + high) >> 1;
      if (this.list[middle].t_ms <= t) {
        found = middle;
        low = middle + 1;
      } else {
        high = middle - 1;
      }
    }
    return found;
  }
}

export interface SampledPlayer {
  rosterIndex: number;
  x: number;
  y: number;
  heading: number;
}

/** The world at one instant, as drawn. */
export interface SampledFrame {
  t_ms: number;
  period: LiveFrame["period"];
  state: LiveFrame["state"];
  stoppage: string | null;
  ball: { x: number; y: number; z: number };
  holder: number | null;
  possessionSide: LiveFrame["possession_side"];
  score: { home: number; away: number };
  players: SampledPlayer[];
}

function lerp(a: number, b: number, k: number): number {
  return a + (b - a) * k;
}

function lerpAngle(a: number, b: number, k: number): number {
  let delta = (b - a) % (2 * Math.PI);
  if (delta > Math.PI) delta -= 2 * Math.PI;
  if (delta < -Math.PI) delta += 2 * Math.PI;
  return a + delta * k;
}

/**
 * The world at `t`: the last frame at or before it, moved a fraction of the way to the next one.
 * Movement is smoothed only where the two frames agree it is movement; a restart (everybody back
 * to the kickoff spots, the ball to the centre) is a jump and is shown as one.
 */
export function sampleAt(store: FrameStore, t: number): SampledFrame | null {
  if (store.length === 0) return null;
  const index = Math.max(0, store.indexAtOrBefore(t));
  const a = store.frames[index];
  const next = store.frames[index + 1];
  // Frames are half a second apart; a larger gap means the engine was played on without them
  // (skip ahead), and nothing is drawn between the two sides of it.
  const b = next && next.t_ms - a.t_ms <= MAX_FRAME_GAP_MS ? next : undefined;
  const span = b ? b.t_ms - a.t_ms : 0;
  const k = b && span > 0 ? Math.min(1, Math.max(0, (t - a.t_ms) / span)) : 0;

  const ballJump = b
    ? Math.hypot(b.ball[0] - a.ball[0], b.ball[1] - a.ball[1]) > MAX_BALL_STEP_M
    : false;
  const kb = b && !ballJump ? k : 0;
  const following = new Map<number, LiveFrame["players"][number]>();
  if (b) for (const p of b.players) following.set(p[0], p);

  const players = a.players.map(([rosterIndex, x, y, heading]) => {
    const target = following.get(rosterIndex);
    if (!target || Math.hypot(target[1] - x, target[2] - y) > MAX_PLAYER_STEP_M) {
      return { rosterIndex, x, y, heading };
    }
    return {
      rosterIndex,
      x: lerp(x, target[1], k),
      y: lerp(y, target[2], k),
      heading: lerpAngle(heading, target[3], k),
    };
  });
  return {
    t_ms: Math.max(a.t_ms, Math.min(t, b ? b.t_ms : a.t_ms)),
    period: a.period,
    state: a.state,
    stoppage: a.stoppage,
    ball: {
      x: b && !ballJump ? lerp(a.ball[0], b.ball[0], kb) : a.ball[0],
      y: b && !ballJump ? lerp(a.ball[1], b.ball[1], kb) : a.ball[1],
      z: b && !ballJump ? lerp(a.ball[2], b.ball[2], kb) : a.ball[2],
    },
    holder: a.holder,
    possessionSide: a.possession_side,
    score: { home: a.score[0], away: a.score[1] },
    players,
  };
}

// ---------------------------------------------------------------------------------------------
// The match clock

/**
 * The stopwatch reading at `t` for the period `clock` describes: `34'`, or `45+2'` once nominal
 * time has been passed. The engine's clock keeps counting through added time and through the
 * break, so a second-half minute is the minutes played before the half plus the elapsed time in it.
 */
export function clockLabel(t: number, clock: ClockView): string {
  const elapsed = Math.max(0, t - clock.period_start_ms);
  const minute = (ms: number) => Math.floor(ms / 60_000);
  if (elapsed >= clock.period_nominal_ms) {
    const nominalEnd = minute(clock.period_base_ms + clock.period_nominal_ms);
    const added = clock.added_time_ms === null ? null : clock.added_time_ms / 60_000;
    // The break after added time keeps showing the last added minute, not a new one.
    const shown = Math.floor((elapsed - clock.period_nominal_ms) / 60_000) + 1;
    return `${nominalEnd}+${added === null ? shown : Math.min(shown, Math.max(1, added))}'`;
  }
  return `${minute(clock.period_base_ms + elapsed) + 1}'`;
}

/** The clock of whichever period `t` falls in (the one with the latest start not after `t`). */
export function clockAt(clocks: readonly ClockView[], t: number): ClockView | null {
  let found: ClockView | null = null;
  for (const clock of clocks) {
    if (clock.period_start_ms <= t && (!found || clock.period_start_ms >= found.period_start_ms)) {
      found = clock;
    }
  }
  return found;
}

// ---------------------------------------------------------------------------------------------
// Highlights

export interface Span {
  start: number;
  end: number;
}

/**
 * The moments the highlight reel is cut around. Deliberately not the engine's `is_key_event`
 * (ofm_core::interactive_match), which decides what stops a stretch of play and includes
 * substitutions, tactical changes and period boundaries but not shots: a reel wants the football
 * (a shot, a card), the engine's stops want the changes of the match.
 */
const HIGHLIGHT_EVENTS = new Set(["goal", "shot", "card", "injury", "penalty"]);
const EXTENDED_EVENTS = new Set(["corner", "foul", "offside"]);

/** Seconds of run-up and aftermath shown around a moment. */
const PADDING_MS: Record<Exclude<HighlightMode, "full">, { before: number; after: number }> = {
  key: { before: 10_000, after: 5_000 },
  extended: { before: 15_000, after: 8_000 },
};
const GOAL_PADDING = { before: 15_000, after: 10_000 };
const DANGER_MIN_MS = 3_000;
const DANGER_PADDING_MS = 2_000;
/** Spans closer than this are one stretch of play, not two cuts. */
const JOIN_GAP_MS = 2_000;
/** How much run-up a highlight needs, which is how close to the head quiet play is parked. */
export const LEAD_MS = 15_000;

/** Whether home attacks the goal at x = 105 in this period (sides swap at half-time and before extra time). */
export function homeAttacksPositiveGoal(period: LiveFrame["period"]): boolean {
  return period === "first_half" || period === "extra_time_first";
}

/** Which goal a side attacks in this period. */
export function attackedGoalX(side: "home" | "away", period: LiveFrame["period"]): number {
  return (side === "home") === homeAttacksPositiveGoal(period) ? PITCH_LENGTH_M : 0;
}

/** The goal line (x = 0 or 105) a ball at `x` is nearest to: the goal a scorer at that spot has just scored in. */
export function goalLineNearest(x: number): number {
  return x < PITCH_LENGTH_M / 2 ? 0 : PITCH_LENGTH_M;
}

function isDangerous(frame: LiveFrame): boolean {
  if (frame.state !== "in_play" || frame.possession_side === null) return false;
  const goalX = attackedGoalX(frame.possession_side, frame.period);
  return Math.abs(frame.ball[0] - goalX) < 28 && Math.abs(frame.ball[1] - PITCH_WIDTH_M / 2) < 22;
}

function mergeSpans(spans: Span[]): Span[] {
  const sorted = [...spans].sort((a, b) => a.start - b.start);
  const merged: Span[] = [];
  for (const span of sorted) {
    const last = merged[merged.length - 1];
    if (last && span.start <= last.end + JOIN_GAP_MS) last.end = Math.max(last.end, span.end);
    else merged.push({ ...span });
  }
  return merged;
}

/**
 * The stretches of the match a highlight mode shows, from the events and frames received so far.
 * `null` means "all of it" (the full match). Key: every shot, goal, card, injury and penalty with
 * its run-up. Extended: those plus corners, fouls, offsides and every spell of sustained pressure
 * near a goal. A stretch is drawn only from frames the engine produced.
 */
export function highlightSpans(
  mode: HighlightMode,
  events: readonly MatchKeyEvent[],
  store: FrameStore,
): Span[] | null {
  if (mode === "full") return null;
  const padding = PADDING_MS[mode];
  const kinds =
    mode === "extended" ? new Set([...HIGHLIGHT_EVENTS, ...EXTENDED_EVENTS]) : HIGHLIGHT_EVENTS;
  const spans: Span[] = [];
  for (const event of events) {
    if (!kinds.has(event.type)) continue;
    const pad = event.type === "goal" ? GOAL_PADDING : padding;
    spans.push({ start: Math.max(0, event.t_ms - pad.before), end: event.t_ms + pad.after });
  }
  if (mode === "extended") spans.push(...dangerSpans(store));
  return mergeSpans(spans);
}

function dangerSpans(store: FrameStore): Span[] {
  const spans: Span[] = [];
  let runStart: number | null = null;
  let runEnd = 0;
  const close = () => {
    if (runStart !== null && runEnd - runStart >= DANGER_MIN_MS) {
      spans.push({
        start: Math.max(0, runStart - DANGER_PADDING_MS),
        end: runEnd + DANGER_PADDING_MS,
      });
    }
    runStart = null;
  };
  for (const frame of store.frames) {
    if (isDangerous(frame)) {
      if (runStart === null) runStart = frame.t_ms;
      runEnd = frame.t_ms;
    } else {
      close();
    }
  }
  close();
  return spans;
}

// ---------------------------------------------------------------------------------------------
// Pace

export interface PaceInput {
  /** Where the picture is, in match time. */
  t: number;
  /** Wall-clock time that passed since the last step. */
  wallMs: number;
  /** Match seconds per wall second. */
  rate: number;
  /** `null` plays every instant. */
  spans: Span[] | null;
  /** The last instant the engine has produced. */
  headMs: number;
  /** True when the engine will produce nothing more until the manager continues (break, full time). */
  streamEnded: boolean;
}

export interface PaceResult {
  t: number;
  /** The picture has caught up with what the engine has produced and needs more of it. */
  waiting: boolean;
}

/**
 * Moves the picture forward. Inside a highlight it runs at the chosen rate; between highlights it
 * cuts to the next one, which is why a highlight mode shows a match faster than the clock.
 */
export function paceStep(input: PaceInput): PaceResult {
  const { spans, headMs, streamEnded } = input;
  let t = input.t;
  let budget = input.wallMs * input.rate;
  if (spans === null) {
    const next = Math.min(headMs, t + budget);
    return { t: next, waiting: next >= headMs && !streamEnded };
  }
  // Each pass either spends budget inside a span or cuts to the next span, so this ends.
  for (let guard = 0; guard < 10_000; guard += 1) {
    const inside = spans.find((s) => t >= s.start && t < s.end);
    if (inside) {
      const limit = Math.min(inside.end, headMs);
      const room = limit - t;
      if (budget < room) return { t: t + budget, waiting: false };
      budget -= room;
      t = limit;
      if (limit < inside.end) return { t, waiting: !streamEnded };
      if (budget <= 0) return { t, waiting: false };
      continue;
    }
    const next = spans.find((s) => s.start > t);
    if (next && next.start <= headMs) {
      t = next.start;
      continue;
    }
    if (streamEnded) return { t: Math.max(t, headMs), waiting: false };
    // Nothing known to show yet: stay close to the head so a highlight that starts soon keeps its run-up.
    return { t: Math.max(t, headMs - LEAD_MS), waiting: true };
  }
  return { t, waiting: false };
}
