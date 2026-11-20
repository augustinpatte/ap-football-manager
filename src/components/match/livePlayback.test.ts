import { describe, expect, it } from "vitest";
import type { ClockView, LiveFrame, MatchKeyEvent } from "../../services/interactiveMatchService";
import {
  FrameStore,
  attackedGoalX,
  goalLineNearest,
  homeAttacksPositiveGoal,
  clockAt,
  clockLabel,
  highlightSpans,
  isValidFrame,
  paceStep,
  sampleAt,
} from "./livePlayback";

function frame(t_ms: number, overrides: Partial<LiveFrame> = {}): LiveFrame {
  return {
    t_ms,
    period: "first_half",
    state: "in_play",
    stoppage: null,
    ball: [52.5, 34, 0],
    holder: null,
    possession_side: null,
    score: [0, 0],
    players: [
      [0, 10, 10, 0],
      [1, 80, 40, 1],
    ],
    ...overrides,
  };
}

function storeOf(...frames: LiveFrame[]): FrameStore {
  const store = new FrameStore();
  store.append(frames);
  return store;
}

const ev = (t_ms: number, type: string): MatchKeyEvent => ({ id: t_ms, t_ms, type });

describe("FrameStore", () => {
  /** Given malformed and out-of-order frames, Then only valid in-order ones are kept and the rest counted. */
  it("keeps valid frames in order and counts what it refuses", () => {
    const store = new FrameStore();
    const kept = store.append([
      frame(0),
      frame(500),
      frame(500), // duplicate instant
      frame(300), // goes back in time
      { ...frame(1000), ball: [Number.NaN, 0, 0] },
      { ...frame(1000), players: [[0, 1, 2]] },
      null,
      frame(1000),
    ]);
    expect(kept).toBe(3);
    expect(store.frames.map((f) => f.t_ms)).toEqual([0, 500, 1000]);
    expect(store.rejected).toBe(5);
  });

  /** Given an empty timeline, Then nothing can be sampled and the head is zero. */
  it("handles an empty timeline", () => {
    const store = new FrameStore();
    expect(sampleAt(store, 1000)).toBeNull();
    expect(store.headMs).toBe(0);
    expect(store.indexAtOrBefore(5)).toBe(-1);
  });

  /** Given a frame with an absurd coordinate, Then it is not drawable. */
  it("refuses coordinates far outside any stadium", () => {
    expect(isValidFrame({ ...frame(0), players: [[0, 5000, 1, 0]] })).toBe(false);
    expect(isValidFrame(frame(0))).toBe(true);
  });
});

describe("sampleAt", () => {
  /** Given two frames half a second apart, Then a time between them is the straight line between them. */
  it("moves players and the ball a fraction of the way to the next frame", () => {
    const store = storeOf(
      frame(0, { ball: [10, 10, 0], players: [[0, 10, 10, 0]] }),
      frame(500, { ball: [20, 10, 2], players: [[0, 12, 14, 0]] }),
    );
    const mid = sampleAt(store, 250);
    expect(mid?.ball).toEqual({ x: 15, y: 10, z: 1 });
    expect(mid?.players[0]).toMatchObject({ x: 11, y: 12 });
    expect(sampleAt(store, 500)?.players[0]).toMatchObject({ x: 12, y: 14 });
  });

  /** Given a restart where everyone is placed back at the kickoff spots, Then the picture jumps instead of sliding across the pitch. */
  it("does not smooth a restart", () => {
    const store = storeOf(
      frame(0, { ball: [100, 30, 0], players: [[0, 95, 30, 0]] }),
      frame(500, { ball: [52.5, 34, 0], players: [[0, 40, 34, 0]] }),
    );
    const mid = sampleAt(store, 250);
    expect(mid?.ball.x).toBe(100);
    expect(mid?.players[0].x).toBe(95);
  });

  /** Given a player who is not in the next frame (sent off), Then he is shown where he last was and never invented. */
  it("keeps a player who vanishes in place until his last frame ends", () => {
    const store = storeOf(
      frame(0, {
        players: [
          [0, 10, 10, 0],
          [1, 20, 20, 0],
        ],
      }),
      frame(500, { players: [[0, 11, 10, 0]] }),
    );
    const mid = sampleAt(store, 250);
    expect(mid?.players).toHaveLength(2);
    expect(sampleAt(store, 500)?.players).toHaveLength(1);
  });

  /** Given a time past the last frame, Then the last frame is shown and nothing is extrapolated. */
  it("never extrapolates beyond the head", () => {
    const store = storeOf(frame(0), frame(500, { score: [1, 0] }));
    const late = sampleAt(store, 99_000);
    expect(late?.t_ms).toBe(500);
    expect(late?.score).toEqual({ home: 1, away: 0 });
  });

  /** Given a heading that crosses the +/-pi seam, Then it turns the short way. */
  it("turns players the short way round", () => {
    const store = storeOf(
      frame(0, { players: [[0, 1, 1, 3.0]] }),
      frame(500, { players: [[0, 1, 1, -3.0]] }),
    );
    const heading = sampleAt(store, 250)?.players[0].heading ?? 0;
    expect(Math.abs(Math.abs(heading) - Math.PI)).toBeLessThan(0.2);
  });
});

describe("clock", () => {
  const first: ClockView = {
    period_start_ms: 0,
    period_base_ms: 0,
    period_nominal_ms: 45 * 60_000,
    nominal_end_ms: 45 * 60_000,
    added_time_ms: 3 * 60_000,
  };
  const second: ClockView = {
    period_start_ms: 48 * 60_000 + 20_000,
    period_base_ms: 45 * 60_000,
    period_nominal_ms: 45 * 60_000,
    nominal_end_ms: 93 * 60_000 + 20_000,
    added_time_ms: null,
  };

  /** Given the engine's clock, Then the stopwatch shows minutes played, added time as 45+n, and the second half from 46'. */
  it("labels minutes, added time and the second half", () => {
    expect(clockLabel(0, first)).toBe("1'");
    expect(clockLabel(34 * 60_000 + 10_000, first)).toBe("35'");
    expect(clockLabel(45 * 60_000, first)).toBe("45+1'");
    expect(clockLabel(47 * 60_000 + 59_000, first)).toBe("45+3'");
    // the break after three added minutes does not invent a fourth
    expect(clockLabel(48 * 60_000 + 10_000, first)).toBe("45+3'");
    expect(clockLabel(second.period_start_ms, second)).toBe("46'");
    expect(clockLabel(second.period_start_ms + 10 * 60_000, second)).toBe("56'");
  });

  /** Given both periods' clocks, Then an instant is read against the period it falls in. */
  it("reads an instant against its own period", () => {
    expect(clockAt([first, second], 10 * 60_000)).toBe(first);
    expect(clockAt([first, second], second.period_start_ms + 1)).toBe(second);
    expect(clockAt([], 5)).toBeNull();
  });
});

describe("highlights", () => {
  const store = storeOf(frame(0), frame(500));

  /** Given the full mode, Then nothing is cut. */
  it("shows every instant in full mode", () => {
    expect(highlightSpans("full", [ev(100_000, "goal")], store)).toBeNull();
  });

  /** Given a shot and a corner, Then key shows the shot with its run-up and extended adds the corner. */
  it("selects more of the match in extended than in key", () => {
    const events = [ev(100_000, "shot"), ev(300_000, "corner"), ev(500_000, "pass")];
    const key = highlightSpans("key", events, store);
    const extended = highlightSpans("extended", events, store);
    expect(key).toEqual([{ start: 90_000, end: 105_000 }]);
    expect(extended).toHaveLength(2);
    expect(extended?.[0]).toEqual({ start: 85_000, end: 108_000 });
    // a pass is football detail, never a highlight
    expect(extended?.some((s) => s.start <= 500_000 && s.end >= 500_000)).toBe(false);
  });

  /** Given two moments seconds apart, Then they are one stretch, not two cuts. */
  it("joins neighbouring moments", () => {
    const spans = highlightSpans("key", [ev(100_000, "shot"), ev(108_000, "card")], store);
    expect(spans).toHaveLength(1);
  });

  /** Given a goal, Then it keeps a longer run-up and aftermath than other moments. */
  it("gives a goal the longest window", () => {
    expect(highlightSpans("key", [ev(100_000, "goal")], store)).toEqual([
      { start: 85_000, end: 110_000 },
    ]);
  });

  /** Given sustained possession near a goal, Then extended mode shows it even without a shot. */
  it("finds spells of pressure near a goal", () => {
    const frames: LiveFrame[] = [];
    for (let t = 0; t <= 6_000; t += 500) {
      frames.push(frame(t, { ball: [95, 34, 0], possession_side: "home" }));
    }
    const spans = highlightSpans("extended", [], storeOf(...frames));
    expect(spans).toEqual([{ start: 0, end: 8_000 }]);
    expect(highlightSpans("key", [], storeOf(...frames))).toEqual([]);
  });

  /** Given the second half, Then the side that attacked +x now attacks -x. */
  it("swaps the goals at half-time", () => {
    expect(attackedGoalX("home", "first_half")).toBe(105);
    expect(attackedGoalX("home", "second_half")).toBe(0);
    expect(attackedGoalX("away", "second_half")).toBe(105);
    expect(attackedGoalX("home", "extra_time_first")).toBe(105);
  });

  /** Given every period, Then one rule says which way home attacks, and the goal a side attacks follows it. */
  it("has one rule for the way home attacks in every period", () => {
    const periods = ["first_half", "second_half", "extra_time_first", "extra_time_second"] as const;
    expect(periods.map(homeAttacksPositiveGoal)).toEqual([true, false, true, false]);
    for (const period of periods) {
      expect(attackedGoalX("home", period)).toBe(homeAttacksPositiveGoal(period) ? 105 : 0);
      expect(attackedGoalX("away", period)).toBe(homeAttacksPositiveGoal(period) ? 0 : 105);
    }
  });

  /** Given where a goal was scored, Then the goal line lit is the nearer one, and the halfway line goes to the far goal. */
  it("picks the goal line nearest to a goal's position", () => {
    expect(goalLineNearest(0.5)).toBe(0);
    expect(goalLineNearest(104.5)).toBe(105);
    expect(goalLineNearest(52.49)).toBe(0);
    expect(goalLineNearest(52.5)).toBe(105);
  });
});

describe("paceStep", () => {
  const base = { rate: 1, headMs: 60_000, streamEnded: false };

  /** Given the full match, Then the picture moves at the chosen rate and waits at the head. */
  it("plays at the chosen rate and waits for the engine at the head", () => {
    expect(paceStep({ ...base, t: 1_000, wallMs: 100, spans: null, rate: 8 })).toEqual({
      t: 1_800,
      waiting: false,
    });
    expect(paceStep({ ...base, t: 59_900, wallMs: 1_000, spans: null })).toEqual({
      t: 60_000,
      waiting: true,
    });
    // at a break there is nothing more to wait for
    expect(
      paceStep({ ...base, t: 59_900, wallMs: 1_000, spans: null, streamEnded: true }).waiting,
    ).toBe(false);
  });

  /** Given highlights, Then the picture cuts from the end of one to the start of the next without playing the quiet part. */
  it("cuts between highlights", () => {
    const spans = [
      { start: 10_000, end: 20_000 },
      { start: 40_000, end: 50_000 },
    ];
    const step = paceStep({ ...base, t: 19_900, wallMs: 1_000, spans, rate: 1 });
    // 100 ms left in the first span, then the cut, then the remaining 900 ms of the second
    expect(step.t).toBe(40_900);
    expect(step.waiting).toBe(false);
  });

  /** Given highlights and nothing known ahead, Then it parks near the head to keep the run-up of what comes next. */
  it("parks near the head when no highlight is known ahead", () => {
    const step = paceStep({
      ...base,
      t: 21_000,
      wallMs: 100,
      spans: [{ start: 10_000, end: 20_000 }],
    });
    expect(step.waiting).toBe(true);
    expect(step.t).toBe(45_000);
  });

  /** Given the end of a half and no more highlights, Then the picture ends at the head instead of waiting forever. */
  it("ends at the head when the stream is over", () => {
    expect(
      paceStep({
        ...base,
        streamEnded: true,
        t: 21_000,
        wallMs: 100,
        spans: [{ start: 10_000, end: 20_000 }],
      }),
    ).toEqual({ t: 60_000, waiting: false });
  });

  /** Given a highlight that runs past what the engine has produced, Then the picture waits at the head inside it. */
  it("waits inside a highlight that is not fully produced yet", () => {
    const step = paceStep({
      ...base,
      t: 59_000,
      wallMs: 5_000,
      spans: [{ start: 50_000, end: 70_000 }],
    });
    expect(step).toEqual({ t: 60_000, waiting: true });
  });
});
