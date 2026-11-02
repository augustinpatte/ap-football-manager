import { describe, expect, it } from "vitest";
import { buildEventsOnlyReplay, buildReplayFixture, MINUTE_MS } from "./m12Replay.fixtures";
import {
  frameAtOrBefore,
  homeRosterIndexes,
  parseM12Replay,
  playersOnPitch,
  type ReplayParseResult,
} from "./m12Replay";

function parsedReplay(result: ReplayParseResult) {
  if (!result.ok) throw new Error(`expected a playable replay, got ${result.reason}`);
  return result.replay;
}

describe("parseM12Replay", () => {
  /** Given a full document with benches, when parsed, then every roster entry and frame is kept. */
  it("accepts a complete replay with benches", () => {
    const fixture = buildReplayFixture();
    const replay = parsedReplay(parseM12Replay(fixture.json));
    expect(replay.roster).toHaveLength(28);
    expect(replay.frames).toHaveLength(4);
  });

  /** Given a result with events but no frames, when parsed, then it is "no frames", never a fabricated animation. */
  it("reports a replay without frames as having no spatial data", () => {
    expect(parseM12Replay(buildEventsOnlyReplay())).toEqual({ ok: false, reason: "noFrames" });
  });

  /** Given a single frame, when parsed, then there is nothing to animate between. */
  it("treats one frame as no spatial data", () => {
    const { document } = buildReplayFixture();
    const frames = (document.frames as unknown[]).slice(0, 1);
    expect(parseM12Replay(JSON.stringify({ ...document, frames }))).toEqual({
      ok: false,
      reason: "noFrames",
    });
  });

  /** Given a document from another schema version, when parsed, then it is refused as unsupported. */
  it("refuses an unsupported schema version", () => {
    const { document } = buildReplayFixture();
    expect(parseM12Replay(JSON.stringify({ ...document, schema_version: 2 }))).toEqual({
      ok: false,
      reason: "unsupportedSchema",
    });
  });

  /** Given text that is not JSON, when parsed, then it is unreadable. */
  it("refuses text that is not JSON", () => {
    expect(parseM12Replay("{")).toEqual({ ok: false, reason: "unreadable" });
    expect(parseM12Replay("null")).toEqual({ ok: false, reason: "unreadable" });
  });

  /** Given a frame whose player count disagrees with the roster, when parsed, then it is inconsistent. */
  it("refuses frames that do not match the roster", () => {
    const { document } = buildReplayFixture();
    const frames = (document.frames as { players: unknown[] }[]).map((frame, index) =>
      index === 2 ? { ...frame, players: frame.players.slice(1) } : frame,
    );
    expect(parseM12Replay(JSON.stringify({ ...document, frames }))).toEqual({
      ok: false,
      reason: "inconsistentFrames",
    });
  });

  /** Given frames out of chronological order, when parsed, then it is inconsistent. */
  it("refuses frames that go back in time", () => {
    const { document } = buildReplayFixture();
    const frames = [...(document.frames as unknown[])].reverse();
    expect(parseM12Replay(JSON.stringify({ ...document, frames })).ok).toBe(false);
  });

  /** Given a player without a position, when parsed, then the document is malformed. */
  it("refuses a frame player without coordinates", () => {
    const { document } = buildReplayFixture();
    const frames = (document.frames as { players: object[] }[]).map((frame) => ({
      ...frame,
      players: frame.players.map(() => ({ on_pitch: true })),
    }));
    expect(parseM12Replay(JSON.stringify({ ...document, frames }))).toEqual({
      ok: false,
      reason: "malformed",
    });
  });
});

describe("frameAtOrBefore", () => {
  /** Given sorted frames, when seeking between two of them, then the earlier one is shown. */
  it("returns the last frame at or before the requested time", () => {
    const replay = parsedReplay(parseM12Replay(buildReplayFixture().json));
    expect(frameAtOrBefore(replay.frames, 0).t_ms).toBe(0);
    expect(frameAtOrBefore(replay.frames, 29 * MINUTE_MS).t_ms).toBe(10 * MINUTE_MS);
    expect(frameAtOrBefore(replay.frames, 30 * MINUTE_MS).t_ms).toBe(30 * MINUTE_MS);
    expect(frameAtOrBefore(replay.frames, 10 * 60 * MINUTE_MS).t_ms).toBe(60 * MINUTE_MS);
  });
});

describe("playersOnPitch", () => {
  const fixture = buildReplayFixture();
  const replay = parsedReplay(parseM12Replay(fixture.json));

  /** Given kick-off with benches, when listing the pitch, then exactly 22 players (11 per side) are on it. */
  it("shows 22 players at kick-off and none from the bench", () => {
    const players = playersOnPitch(replay.frames[0], replay.roster);
    expect(players).toHaveLength(22);
    expect(players.filter((player) => player.side === "home")).toHaveLength(11);
    expect(players.some((player) => player.playerId === "home-12")).toBe(false);
  });

  /** Given a red card, when seeking past it, then the sent-off player leaves the pitch (21 left). */
  it("drops a sent-off player", () => {
    const players = playersOnPitch(frameAtOrBefore(replay.frames, 31 * MINUTE_MS), replay.roster);
    expect(players).toHaveLength(21);
    expect(players.some((player) => player.playerId === "away-1")).toBe(false);
  });

  /** Given a substitution, when seeking past it, then the replacement is on and the player off. */
  it("swaps a substituted player for the one from the bench", () => {
    const players = playersOnPitch(frameAtOrBefore(replay.frames, 61 * MINUTE_MS), replay.roster);
    const onPitchIds = players.map((player) => player.playerId);
    expect(onPitchIds).toContain("home-12");
    expect(onPitchIds).not.toContain("home-2");
    expect(players.filter((player) => player.side === "home")).toHaveLength(11);
  });

  /** Given benches of different sizes, when no home ids are given, then the half-split assumption misfiles players. */
  it("needs explicit home ids when the benches are uneven", () => {
    const uneven = buildReplayFixture({ homeBench: 0, awayBench: 8 });
    const unevenReplay = parsedReplay(parseM12Replay(uneven.json));
    const guessed = playersOnPitch(unevenReplay.frames[0], unevenReplay.roster);
    const explicit = playersOnPitch(
      unevenReplay.frames[0],
      unevenReplay.roster,
      new Set(uneven.homeIds),
    );
    expect(explicit.filter((player) => player.side === "home")).toHaveLength(11);
    expect(explicit.filter((player) => player.side === "away")).toHaveLength(11);
    expect(guessed.filter((player) => player.side === "home")).not.toHaveLength(11);
  });

  /** Given explicit home ids, when splitting the roster, then they override the roster order. */
  it("honours explicit home ids over roster order", () => {
    const indexes = homeRosterIndexes(["a", "b", "c", "d"], new Set(["b", "d"]));
    expect([...indexes]).toEqual([1, 3]);
  });
});
