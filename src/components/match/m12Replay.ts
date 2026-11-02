// Pure model of the M12 replay document (schema_version 1) shown by `M12ReplayViewer`.
// Nothing here simulates or invents data: a document that does not carry spatial frames is
// reported as such, never padded with made-up positions.

export type FramePlayer = {
  x: number;
  y: number;
  heading: number;
  on_pitch: boolean;
};

export type ReplayFrame = {
  t_ms: number;
  period: string;
  ball: { x: number; y: number; z: number };
  score: { home: number; away: number };
  players: FramePlayer[];
};

export type ReplayEvent = {
  t_ms: number;
  type: string;
  side?: string;
  scorer?: string;
  player?: string;
  player_off?: string;
  player_on?: string;
  colour?: string;
};

export type Replay = {
  schema_version: number;
  home_team: string;
  away_team: string;
  roster: string[];
  frames: ReplayFrame[];
  events: ReplayEvent[];
};

/** Why a stored document cannot be played. Each reason has its own player-facing message. */
export type ReplayRejection =
  | "unreadable"
  | "unsupportedSchema"
  | "noFrames"
  | "inconsistentFrames"
  | "malformed";

export type ReplayParseResult =
  | { ok: true; replay: Replay }
  | { ok: false; reason: ReplayRejection };

export const SUPPORTED_REPLAY_SCHEMA_VERSION = 1;

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isFramePlayer(value: unknown): value is FramePlayer {
  if (!value || typeof value !== "object") return false;
  const player = value as Partial<FramePlayer>;
  return (
    isNumber(player.x) &&
    isNumber(player.y) &&
    isNumber(player.heading) &&
    typeof player.on_pitch === "boolean"
  );
}

function isReplayFrame(value: unknown): value is ReplayFrame {
  if (!value || typeof value !== "object") return false;
  const frame = value as Partial<ReplayFrame>;
  return (
    isNumber(frame.t_ms) &&
    typeof frame.period === "string" &&
    !!frame.ball &&
    isNumber(frame.ball.x) &&
    isNumber(frame.ball.y) &&
    isNumber(frame.ball.z) &&
    !!frame.score &&
    isNumber(frame.score.home) &&
    isNumber(frame.score.away) &&
    Array.isArray(frame.players) &&
    frame.players.every(isFramePlayer)
  );
}

function isReplayEvent(value: unknown): value is ReplayEvent {
  if (!value || typeof value !== "object") return false;
  const event = value as Partial<ReplayEvent>;
  return isNumber(event.t_ms) && typeof event.type === "string";
}

function framesAreConsistent(frames: ReplayFrame[], rosterSize: number): boolean {
  return frames.every(
    (frame, index) =>
      frame.players.length === rosterSize && (index === 0 || frame.t_ms >= frames[index - 1].t_ms),
  );
}

export function parseM12Replay(source: string): ReplayParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch {
    return { ok: false, reason: "unreadable" };
  }
  if (!parsed || typeof parsed !== "object") return { ok: false, reason: "unreadable" };
  const replay = parsed as Partial<Replay>;
  if (replay.schema_version !== SUPPORTED_REPLAY_SCHEMA_VERSION) {
    return { ok: false, reason: "unsupportedSchema" };
  }
  const { roster, frames, events } = replay;
  if (
    typeof replay.home_team !== "string" ||
    typeof replay.away_team !== "string" ||
    !Array.isArray(roster) ||
    !roster.every((player) => typeof player === "string") ||
    !Array.isArray(frames) ||
    !Array.isArray(events) ||
    !events.every(isReplayEvent)
  ) {
    return { ok: false, reason: "malformed" };
  }
  // A result with events but no recorded movement is a legitimate document (frame recording
  // can be off): say so instead of pretending the viewer failed.
  if (frames.length < 2) return { ok: false, reason: "noFrames" };
  if (!frames.every(isReplayFrame)) return { ok: false, reason: "malformed" };
  if (!framesAreConsistent(frames, roster.length)) {
    return { ok: false, reason: "inconsistentFrames" };
  }
  return {
    ok: true,
    replay: {
      schema_version: replay.schema_version,
      home_team: replay.home_team,
      away_team: replay.away_team,
      roster,
      frames,
      events,
    },
  };
}

/** The last frame recorded at or before `time` (frames are sorted by `t_ms`). */
export function frameAtOrBefore(frames: ReplayFrame[], time: number): ReplayFrame {
  let low = 0;
  let high = frames.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (frames[middle].t_ms <= time) low = middle;
    else high = middle - 1;
  }
  return frames[low];
}

export type PitchPlayer = {
  rosterIndex: number;
  playerId: string;
  side: "home" | "away";
  /** 1-based position within its own side's roster; a label, not the shirt number. */
  label: number;
  x: number;
  y: number;
  heading: number;
};

/**
 * Which roster entries are the home side's. The replay document does not say, so an explicit set
 * of ids (from the game state) wins; otherwise the roster is assumed home-first and split in
 * half, which only holds when both sides list the same number of players.
 */
export function homeRosterIndexes(
  roster: string[],
  homePlayerIds?: ReadonlySet<string>,
): Set<number> {
  const home = new Set<number>();
  roster.forEach((playerId, index) => {
    const isHome = homePlayerIds ? homePlayerIds.has(playerId) : index < roster.length / 2;
    if (isHome) home.add(index);
  });
  return home;
}

/** Players actually on the pitch in this frame, bench and sent-off players excluded. */
export function playersOnPitch(
  frame: ReplayFrame,
  roster: string[],
  homePlayerIds?: ReadonlySet<string>,
): PitchPlayer[] {
  const home = homeRosterIndexes(roster, homePlayerIds);
  const counters = { home: 0, away: 0 };
  const result: PitchPlayer[] = [];
  frame.players.forEach((player, rosterIndex) => {
    const side = home.has(rosterIndex) ? "home" : "away";
    counters[side] += 1;
    if (!player.on_pitch) return;
    result.push({
      rosterIndex,
      playerId: roster[rosterIndex],
      side,
      label: counters[side],
      x: player.x,
      y: player.y,
      heading: player.heading,
    });
  });
  return result;
}

// Only the event types the M12 output contract names (spatial_match_output.rs tests) are
// translated; any other type is shown as the engine spelled it rather than guessed at.
export const REPLAY_EVENT_TYPES = ["goal", "card", "substitution", "injury"] as const;

export type KnownReplayEventType = (typeof REPLAY_EVENT_TYPES)[number];

export function isKnownReplayEventType(type: string): type is KnownReplayEventType {
  return (REPLAY_EVENT_TYPES as readonly string[]).includes(type);
}

/** The player an event is mainly about, for the name shown next to it. */
export function eventPlayerId(event: ReplayEvent): string | undefined {
  return event.scorer ?? event.player ?? event.player_on;
}
