// The live screen against the engine's real output: `liveMatch.real.generated.json` was written by
// the backend test `write_live_fixture` from a real M12 match (the first three minutes, 30 seconds
// per reply, serialised exactly as the IPC does). Hand-built frames can agree with the code that
// reads them by construction; these cannot.

import { describe, expect, it, vi } from "vitest";
import type { AdvanceSnapshot, InteractiveSnapshot } from "../../services/interactiveMatchService";
import fixture from "./liveMatch.real.generated.json";
import { LiveDirector } from "./liveDirector";
import { PITCH_LENGTH_M, PITCH_WIDTH_M, sampleAt } from "./livePlayback";
import { statsAt } from "./replayDerived";
import { drawPitch } from "./replayPitch";

const start = fixture.start as unknown as InteractiveSnapshot;
const replies = fixture.replies as unknown as AdvanceSnapshot[];

function loaded(): LiveDirector {
  const director = new LiveDirector();
  director.ingestSnapshot(start);
  for (const reply of replies) director.ingestAdvance(reply);
  return director;
}

describe("the live screen on the engine's real frames", () => {
  /** Given three real minutes of a real match, Then every frame is accepted, strictly ordered and half a second apart. */
  it("accepts every real frame, in order, on the engine's grid", () => {
    const director = loaded();
    expect(director.store.rejected).toBe(0);
    const times = director.store.frames.map((f) => f.t_ms);
    expect(times[0]).toBe(0);
    expect(times[times.length - 1]).toBe(180_000);
    expect(times).toHaveLength(361);
    expect(times.every((t, i) => i === 0 || t - times[i - 1] === 500)).toBe(true);
  });

  /** Given the real frames, Then every player is on the pitch's plane, 11 a side, and the roster gives each a side. */
  it("has eleven a side inside the stadium in every real frame", () => {
    const director = loaded();
    const roster = start.view.roster;
    expect(roster.length).toBeGreaterThanOrEqual(22);
    for (const frame of director.store.frames) {
      const perSide = { home: 0, away: 0 };
      for (const [index, x, y] of frame.players) {
        perSide[roster[index].side] += 1;
        expect(x).toBeGreaterThan(-3);
        expect(x).toBeLessThan(PITCH_LENGTH_M + 3);
        expect(y).toBeGreaterThan(-3);
        expect(y).toBeLessThan(PITCH_WIDTH_M + 3);
      }
      expect(perSide).toEqual({ home: 11, away: 11 });
      expect(frame.ball.every(Number.isFinite)).toBe(true);
    }
  });

  /** Given real frames, Then nobody moves faster than a player can between two frames, so interpolation is a straight line over a real movement. */
  it("moves players at human speeds between frames", () => {
    const frames = loaded().store.frames;
    let fastest = 0;
    for (let i = 1; i < frames.length; i += 1) {
      const before = new Map(frames[i - 1].players.map((p) => [p[0], p]));
      for (const [index, x, y] of frames[i].players) {
        const previous = before.get(index);
        if (!previous) continue;
        fastest = Math.max(
          fastest,
          Math.hypot(x - previous[1], y - previous[2]) /
            ((frames[i].t_ms - frames[i - 1].t_ms) / 1000),
        );
      }
    }
    // the kickoff layout is not a run: the fastest real sprint in three minutes is well under 12 m/s
    expect(fastest).toBeLessThan(12);
  });

  /** Given real frames, Then the clock and the statistics read from them are those of the match. */
  it("reads the clock and possession from the real data", () => {
    const director = loaded();
    expect(director.labelAt(0)).toBe("1'");
    expect(director.labelAt(179_000)).toBe("3'");
    const { source, homeIds } = director.statsSource();
    const stats = statsAt(source, 180_000, { score: { home: 0, away: 0 } }, homeIds);
    expect(stats.home.possessionPct + stats.away.possessionPct).toBeCloseTo(100, 5);
    expect(stats.home.possessionPct).toBeGreaterThan(5);
    expect(stats.away.possessionPct).toBeGreaterThan(5);
    expect(homeIds.size).toBeGreaterThanOrEqual(11);
  });

  /** Given a real sampled frame, Then the pitch draws all 22 players and the ball without error. */
  it("draws a real frame", () => {
    const director = loaded();
    const sample = sampleAt(director.store, 91_250);
    expect(sample?.players).toHaveLength(22);
    const arcs = vi.fn();
    const context = new Proxy(
      {},
      { get: (_t, name) => (name === "arc" ? arcs : () => undefined), set: () => true },
    );
    const roster = start.view.roster;
    drawPitch(context as unknown as CanvasRenderingContext2D, 960, 640, {
      frame: {
        ball: sample?.ball ?? { x: 0, y: 0, z: 0 },
        possession: sample?.holder === null || !sample ? null : roster[sample.holder as number].id,
      },
      onPitch: (sample?.players ?? []).map((p) => ({
        rosterIndex: p.rosterIndex,
        playerId: roster[p.rosterIndex].id,
        side: roster[p.rosterIndex].side,
        label: p.rosterIndex,
        x: p.x,
        y: p.y,
        heading: p.heading,
      })),
      homeAttacksPositiveX: true,
    });
    expect(arcs.mock.calls.length).toBeGreaterThanOrEqual(22 + 1);
  });

  /** Given the real replies fed to the director one by one as a screen would ask for them, Then it plays the three minutes through without ever overtaking the data. */
  it("plays through real replies without overtaking them", async () => {
    const director = new LiveDirector();
    director.ingestSnapshot(start);
    director.ingestAdvance(replies[0]);
    let next = 1;
    director.setSpeed(16);
    director.play();
    let guard = 0;
    while (director.positionMs < 180_000 && guard < 5_000) {
      guard += 1;
      director.tick(16);
      await director.pump(async () => replies[next++] ?? null);
      expect(director.positionMs).toBeLessThanOrEqual(director.store.headMs);
    }
    expect(director.positionMs).toBe(180_000);
    expect(director.store.rejected).toBe(0);
  });
});
