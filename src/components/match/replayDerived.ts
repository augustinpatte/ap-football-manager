// Everything the replay views show, derived from the one stored M12 document at one clock.
// Pure: no view owns a copy of the match, so switching views cannot change the score, the clock or
// the players. Nothing is invented: a statistic is a count of recorded events or frames, and
// an event kind the feed does not name is left out rather than printed as the engine spelled it.

import { homeRosterIndexes, type Replay, type ReplayEvent, type ReplayFrame } from "./m12Replay";

export type Side = "home" | "away";

export type FeedKind =
  | "kickoff"
  | "halfTime"
  | "secondHalfStart"
  | "fullTime"
  | "goal"
  | "ownGoal"
  | "shotSaved"
  | "shotOffTarget"
  | "shotBlocked"
  | "woodwork"
  | "yellowCard"
  | "secondYellow"
  | "redCard"
  | "substitution"
  | "injury"
  | "corner"
  | "foul"
  | "offside";

export type FeedEntry = {
  t_ms: number;
  kind: FeedKind;
  side?: Side;
  /** The player the entry is about (the scorer, the one booked, the player coming on). */
  playerId?: string;
  /** The other player: the one replaced, or the one fouled. */
  otherPlayerId?: string;
};

function asSide(side: string | undefined): Side | undefined {
  return side === "home" || side === "away" ? side : undefined;
}

const SHOT_KINDS: Record<string, FeedKind | undefined> = {
  saved: "shotSaved",
  off_target: "shotOffTarget",
  blocked: "shotBlocked",
  hit_woodwork: "woodwork",
};

const CARD_KINDS: Record<string, FeedKind | undefined> = {
  yellow: "yellowCard",
  second_yellow: "secondYellow",
  red: "redCard",
};

/** One event of the match as a feed entry, or `null` for the many a player does not read. */
export function feedEntryOf(event: ReplayEvent): FeedEntry | null {
  const base = { t_ms: event.t_ms, side: asSide(event.side) };
  switch (event.type) {
    case "goal":
      return {
        ...base,
        kind: event.own_goal ? "ownGoal" : "goal",
        playerId: event.scorer,
      };
    case "shot": {
      // A shot that scored is told by its goal; the others by how they ended.
      const kind = event.outcome ? SHOT_KINDS[event.outcome] : undefined;
      return kind ? { ...base, kind, playerId: event.player } : null;
    }
    case "card": {
      const kind = event.colour ? CARD_KINDS[event.colour] : undefined;
      return kind ? { ...base, kind, playerId: event.player } : null;
    }
    case "substitution":
      return {
        ...base,
        kind: "substitution",
        playerId: event.player_on,
        otherPlayerId: event.player_off,
      };
    case "injury":
      return { ...base, kind: "injury", playerId: event.player };
    case "corner":
      return { ...base, kind: "corner" };
    case "foul":
      return { ...base, kind: "foul", playerId: event.offender, otherPlayerId: event.victim };
    case "offside":
      return { ...base, kind: "offside", playerId: event.player };
    case "period_start":
      return {
        t_ms: event.t_ms,
        kind: event.period === "second_half" ? "secondHalfStart" : "kickoff",
      };
    case "period_end":
      return {
        t_ms: event.t_ms,
        kind: event.period === "second_half" ? "fullTime" : "halfTime",
      };
    default:
      return null;
  }
}

/** The events a player would read, in order. Computed once per replay. */
export function buildFeed(replay: Replay): FeedEntry[] {
  const feed: FeedEntry[] = [];
  for (const event of replay.events) {
    const entry = feedEntryOf(event);
    if (entry) feed.push(entry);
  }
  return feed;
}

/** The last `count` entries at or before `time`, oldest first. */
export function feedAt(feed: FeedEntry[], time: number, count: number): FeedEntry[] {
  const elapsed = feed.filter((entry) => entry.t_ms <= time);
  return elapsed.slice(Math.max(0, elapsed.length - count));
}

export type SideCounts = {
  goals: number;
  shots: number;
  shotsOnTarget: number;
  corners: number;
  fouls: number;
  offsides: number;
  yellowCards: number;
  redCards: number;
  /** Share of the frames so far in which this side had the ball, of those with a holder. */
  possessionPct: number;
};

const EMPTY: SideCounts = {
  goals: 0,
  shots: 0,
  shotsOnTarget: 0,
  corners: 0,
  fouls: 0,
  offsides: 0,
  yellowCards: 0,
  redCards: 0,
  possessionPct: 0,
};

function tally(counts: SideCounts, event: ReplayEvent): void {
  switch (event.type) {
    case "shot":
      counts.shots += 1;
      if (event.outcome === "goal" || event.outcome === "saved") counts.shotsOnTarget += 1;
      break;
    case "corner":
      counts.corners += 1;
      break;
    case "foul":
      counts.fouls += 1;
      break;
    case "offside":
      counts.offsides += 1;
      break;
    case "card":
      if (event.colour === "red" || event.colour === "second_yellow") counts.redCards += 1;
      if (event.colour === "yellow" || event.colour === "second_yellow") counts.yellowCards += 1;
      break;
    default:
  }
}

/**
 * The counts for both sides up to `time`. At full time they equal the engine's own totals (a test
 * against a real M12 document asserts it), so the live numbers and the final ones are one thing.
 */
export function statsAt(
  replay: Replay,
  time: number,
  frame: ReplayFrame,
  homePlayerIds?: ReadonlySet<string>,
): Record<Side, SideCounts> {
  const home = { ...EMPTY };
  const away = { ...EMPTY };
  for (const event of replay.events) {
    if (event.t_ms > time) break;
    const side = asSide(event.side);
    if (side) tally(side === "home" ? home : away, event);
  }
  home.goals = frame.score.home;
  away.goals = frame.score.away;

  const homeIndexes = homeRosterIndexes(replay.roster, homePlayerIds);
  const sideOf = new Map(
    replay.roster.map((id, index): [string, Side] => [
      id,
      homeIndexes.has(index) ? "home" : "away",
    ]),
  );
  let homeFrames = 0;
  let awayFrames = 0;
  for (const f of replay.frames) {
    if (f.t_ms > time) break;
    if (!f.possession) continue;
    if (sideOf.get(f.possession) === "home") homeFrames += 1;
    else if (sideOf.get(f.possession) === "away") awayFrames += 1;
  }
  const held = homeFrames + awayFrames;
  home.possessionPct = held ? (homeFrames / held) * 100 : 0;
  away.possessionPct = held ? (awayFrames / held) * 100 : 0;
  return { home, away };
}

export type ConditionEntry = {
  playerId: string;
  side: Side;
  /** 0-100, 100 fresh. */
  condition: number;
};

/** The condition M12 recorded for each player on the pitch in this frame; empty when not recorded. */
export function conditionsAt(
  replay: Replay,
  frame: ReplayFrame,
  homePlayerIds?: ReadonlySet<string>,
): ConditionEntry[] {
  const agents = frame.extras?.agents;
  if (!agents) return [];
  const homeIndexes = homeRosterIndexes(replay.roster, homePlayerIds);
  const indexOf = new Map(replay.roster.map((id, index): [string, number] => [id, index]));
  return agents.flatMap((agent) => {
    const index = indexOf.get(agent.id);
    if (index === undefined) return [];
    return [
      {
        playerId: agent.id,
        side: homeIndexes.has(index) ? ("home" as const) : ("away" as const),
        condition: agent.cond,
      },
    ];
  });
}

export type LineupSide = {
  onPitch: string[];
  /** Players who have not played, in squad order. */
  bench: string[];
  /** Replaced or sent off, in the order it happened. */
  gone: { playerId: string; reason: "substituted" | "sentOff" }[];
};

/** Who is on the pitch, on the bench, and who has left it, at `time`. */
export function lineupAt(
  replay: Replay,
  frame: ReplayFrame,
  time: number,
  homePlayerIds?: ReadonlySet<string>,
): Record<Side, LineupSide> {
  const homeIndexes = homeRosterIndexes(replay.roster, homePlayerIds);
  const result: Record<Side, LineupSide> = {
    home: { onPitch: [], bench: [], gone: [] },
    away: { onPitch: [], bench: [], gone: [] },
  };
  const left = new Map<string, "substituted" | "sentOff">();
  for (const event of replay.events) {
    if (event.t_ms > time) break;
    if (event.type === "substitution" && event.player_off)
      left.set(event.player_off, "substituted");
    if (event.type === "card" && (event.colour === "red" || event.colour === "second_yellow")) {
      if (event.player) left.set(event.player, "sentOff");
    }
  }
  replay.roster.forEach((playerId, index) => {
    const side = result[homeIndexes.has(index) ? "home" : "away"];
    if (frame.players[index]?.on_pitch) side.onPitch.push(playerId);
    else if (!left.has(playerId)) side.bench.push(playerId);
  });
  for (const [playerId, reason] of left) {
    const index = replay.roster.indexOf(playerId);
    if (index < 0) continue;
    result[homeIndexes.has(index) ? "home" : "away"].gone.push({ playerId, reason });
  }
  return result;
}
