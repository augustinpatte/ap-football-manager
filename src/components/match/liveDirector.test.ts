import { describe, expect, it } from "vitest";
import type {
  AdvanceSnapshot,
  InteractiveView,
  LiveFrame,
  MatchKeyEvent,
  StopReason,
} from "../../services/interactiveMatchService";
import { LiveDirector } from "./liveDirector";

const HALF_MS = 60_000;

/**
 * A fake of the engine behind `advance`: it owns its clock, produces a frame every 500 ms of what
 * it plays, ends the first half in a break and the second at full time, and answers every call
 * from that state. The director therefore sees values fed back, never canned replies.
 */
function fakeEngine(scheduled: { t_ms: number; type: string }[] = []) {
  const state = { clock: 0, lastFrame: -1, period: 0, nextEvent: 0, calls: [] as number[] };
  const frameAt = (t: number): LiveFrame => ({
    t_ms: t,
    period: state.period === 0 ? "first_half" : "second_half",
    state: "in_play",
    stoppage: null,
    ball: [52.5, 34, 0],
    holder: t >= 2_000 ? 0 : null,
    possession_side: t >= 2_000 ? "home" : null,
    score: [0, 0],
    players: [[0, 10 + (t % 10_000) / 1000, 20, 0]],
  });
  async function advance(seconds: number): Promise<AdvanceSnapshot> {
    state.calls.push(seconds);
    const periodEnd = (state.period + 1) * HALF_MS;
    const target = Math.min(state.clock + seconds * 1000, periodEnd);
    const frames: LiveFrame[] = [];
    for (let t = state.lastFrame + 500; t <= target; t += 500) {
      if (state.lastFrame >= t) continue;
      frames.push(frameAt(t));
      state.lastFrame = t;
    }
    if (state.lastFrame < 0) {
      frames.push(frameAt(0));
      state.lastFrame = 0;
    }
    state.clock = target;
    const due = scheduled.slice(state.nextEvent).filter((e) => e.t_ms <= target);
    const events: MatchKeyEvent[] = due.map((e, i) => ({ ...e, id: state.nextEvent + i }));
    state.nextEvent += due.length;
    let stop: StopReason = "reached";
    if (target >= periodEnd) {
      stop = state.period === 0 ? "period_break" : "finished";
      if (state.period === 0) state.period = 1;
    }
    const view = {
      user_side: "home",
      roster: [{ id: "p0", side: "home" }],
      regulation_minutes: 90,
      session: { finished: stop === "finished" },
      clock: {
        period_start_ms: state.period === 0 ? 0 : HALF_MS,
        period_base_ms: state.period === 0 ? 0 : 45 * 60_000,
        period_nominal_ms: HALF_MS,
        nominal_end_ms: (state.period + 1) * HALF_MS,
        added_time_ms: null,
      },
    } as unknown as InteractiveView;
    return {
      stop,
      frames,
      snapshot: { view, events, next_event_cursor: state.nextEvent },
    };
  }
  return { state, advance };
}

/** Runs the director for `wallMs`, pumping the engine when it asks, in 16 ms animation frames. */
async function run(
  director: LiveDirector,
  advance: (s: number) => Promise<AdvanceSnapshot | null>,
  wallMs: number,
) {
  const seen: number[] = [];
  for (let spent = 0; spent < wallMs; spent += 16) {
    director.tick(16);
    await director.pump(advance);
    seen.push(director.getSnapshot().playheadMs);
    // the picture never gets ahead of what the engine produced
    expect(director.getSnapshot().playheadMs).toBeLessThanOrEqual(director.store.headMs);
  }
  return seen;
}

async function started(scheduled: { t_ms: number; type: string }[] = []) {
  const engine = fakeEngine(scheduled);
  const director = new LiveDirector();
  director.ingestAdvance(await engine.advance(0));
  return { engine, director };
}

describe("LiveDirector", () => {
  /** Given a match that has produced only the kickoff frame, Then the picture is paused at kickoff and orders are allowed. */
  it("starts paused at the kickoff the engine produced", async () => {
    const director = new LiveDirector();
    expect(director.getSnapshot().status).toBe("loading");
    const { director: ready } = await started();
    const snapshot = ready.getSnapshot();
    expect(snapshot.status).toBe("paused");
    expect(snapshot.playheadMs).toBe(0);
    expect(snapshot.canOrder).toBe(true);
  });

  /** Given play, Then the picture follows the engine at the chosen speed and never runs ahead of it. */
  it("plays at the chosen speed and never overtakes the engine", async () => {
    const { engine, director } = await started();
    director.setSpeed(4);
    director.play();
    await run(director, engine.advance, 4_000);
    const played = director.getSnapshot().playheadMs;
    // 4 s of wall time at 4x is about 16 s of match, give or take the first request round trip
    expect(played).toBeGreaterThan(12_000);
    expect(played).toBeLessThan(20_000);
    expect(director.getSnapshot().status).toBe("playing");
    expect(director.getSnapshot().canOrder).toBe(false);
  });

  /** Given a running match, When the manager pauses, Then the picture runs on to the engine's head, and only there are orders allowed. */
  it("lets the picture catch up with the engine before an order can be given", async () => {
    const { engine, director } = await started();
    director.play();
    await run(director, engine.advance, 3_000);
    const callsBefore = engine.state.calls.length;
    director.pause();
    expect(director.getSnapshot().status).toBe("pausing");
    expect(director.getSnapshot().canOrder).toBe(false);
    await run(director, engine.advance, 2_000);
    const snapshot = director.getSnapshot();
    expect(snapshot.status).toBe("paused");
    expect(snapshot.canOrder).toBe(true);
    expect(snapshot.playheadMs).toBe(director.store.headMs);
    // pausing asks the engine for nothing more
    expect(engine.state.calls.length).toBe(callsBefore);
  });

  /** Given a pause pressed while a stretch is still in flight, When the reply extends the engine's head, Then no order is allowed while it is in flight, nor until the picture has caught up with it. */
  it("does not allow an order when a late reply puts the engine ahead of a paused picture", async () => {
    const { engine, director } = await started();
    director.play();
    director.tick(16);
    let release: () => void = () => undefined;
    const gated = (seconds: number) =>
      new Promise<AdvanceSnapshot | null>((resolve) => {
        release = () => resolve(engine.advance(seconds));
      });
    const pending = director.pump(gated);
    director.pause();
    // at the head, but the engine is about to move: an order now would meet a moment not yet shown
    expect(director.getSnapshot().canOrder).toBe(false);
    expect(director.getSnapshot().status).toBe("pausing");
    release();
    await pending;
    expect(director.store.headMs).toBeGreaterThan(director.getSnapshot().playheadMs);
    expect(director.getSnapshot().canOrder).toBe(false);
    await run(director, engine.advance, 4_000);
    const settled = director.getSnapshot();
    expect(settled.playheadMs).toBe(director.store.headMs);
    expect(settled.canOrder).toBe(true);
  });

  /** Given a request already in flight, Then a second pump does not ask the engine again. */
  it("keeps one request in flight", async () => {
    const { engine, director } = await started();
    director.play();
    await Promise.all([director.pump(engine.advance), director.pump(engine.advance)]);
    expect(engine.state.calls.filter((s) => s > 0)).toHaveLength(1);
  });

  /** Given the end of the first half, Then the picture stops at the break and the second half starts only when asked, with no frame repeated or missing. */
  it("stops at the break and continues into the second half", async () => {
    const { engine, director } = await started();
    director.setSpeed(16);
    director.play();
    await run(director, engine.advance, 20_000);
    let snapshot = director.getSnapshot();
    expect(snapshot.status).toBe("break");
    expect(snapshot.playheadMs).toBe(HALF_MS);
    expect(snapshot.canOrder).toBe(true);
    expect(engine.state.clock).toBe(HALF_MS);

    director.play();
    await run(director, engine.advance, 30_000);
    snapshot = director.getSnapshot();
    expect(snapshot.status).toBe("finished");
    expect(snapshot.playheadMs).toBe(2 * HALF_MS);
    expect(snapshot.canOrder).toBe(false);
    const times = director.store.frames.map((f) => f.t_ms);
    expect(times).toEqual([...new Set(times)].sort((a, b) => a - b));
    expect(times[times.length - 1]).toBe(2 * HALF_MS);
    // every 500 ms exactly once: nothing missing, nothing duplicated
    expect(times).toHaveLength((2 * HALF_MS) / 500 + 1);
    expect(director.store.rejected).toBe(0);
  });

  /** Given frames in which a player holds the ball, Then the statistics source counts possession by player id up to the picture's instant, from the engine's frames. */
  it("hands the statistics the engine's possession by player id", async () => {
    const { engine, director } = await started();
    director.ingestAdvance(await engine.advance(5));
    const { source, homeIds } = director.statsSource();
    expect(homeIds.has("p0")).toBe(true);
    expect(source.roster).toEqual(["p0"]);
    expect(source.frames[source.frames.length - 1]).toEqual({ t_ms: 5_000, possession: "p0" });
    expect(source.frames.find((f) => f.t_ms === 1_000)?.possession).toBeNull();
    expect(source.frames).toHaveLength(director.store.length);
  });

  /** Given a match at full time, Then play does nothing. */
  it("cannot be played past full time", async () => {
    const { engine, director } = await started();
    director.setSpeed(16);
    director.play();
    await run(director, engine.advance, 20_000);
    director.play();
    await run(director, engine.advance, 20_000);
    const callsAtEnd = engine.state.calls.length;
    director.play();
    await run(director, engine.advance, 1_000);
    expect(director.getSnapshot().status).toBe("finished");
    expect(engine.state.calls.length).toBe(callsAtEnd);
  });

  /** Given the half-time break not yet reached, Then play cannot skip it. */
  it("will not start the second half before the break has been shown", async () => {
    const { engine, director } = await started();
    director.setSpeed(16);
    director.play();
    // run until the engine has produced the break but the picture is still on its way
    for (let i = 0; i < 2_000 && engine.state.clock < HALF_MS; i += 1) {
      director.tick(16);
      await director.pump(engine.advance);
    }
    director.pause();
    director.play();
    expect(director.store.headMs).toBeLessThanOrEqual(HALF_MS);
    expect(director.getSnapshot().playheadMs).toBeLessThanOrEqual(HALF_MS);
  });

  /** Given the engine failing, Then the director stops asking and the picture settles. */
  it("halts when the engine call fails", async () => {
    const { director } = await started();
    director.play();
    await director.pump(async () => null);
    expect(director.getSnapshot().status).not.toBe("playing");
    expect(director.needsMore()).toBe(false);
  });

  /** Given frames the engine should never send, Then they are counted and dropped rather than drawn. */
  it("drops malformed frames and reports them", async () => {
    const { engine, director } = await started();
    const reply = await engine.advance(5);
    reply.frames.push({ ...reply.frames[0], t_ms: 99_999, ball: [Number.NaN, 0, 0] });
    director.ingestAdvance(reply);
    expect(director.getSnapshot().rejectedFrames).toBe(1);
    expect(director.store.headMs).toBe(5_000);
  });

  /** Given events from the engine, Then the feed shows only what the picture has reached, and each once. */
  it("reveals events as the picture reaches them", async () => {
    const { engine, director } = await started([
      { t_ms: 3_000, type: "shot" },
      { t_ms: 9_000, type: "goal" },
    ]);
    director.setSpeed(16);
    director.play();
    await run(director, engine.advance, 100);
    expect(
      director.getSnapshot().events.every((e) => e.t_ms <= director.getSnapshot().playheadMs),
    ).toBe(true);
    await run(director, engine.advance, 3_000);
    const ids = director.getSnapshot().events.map((e) => e.id);
    expect(ids).toEqual([0, 1]);
    expect(
      director.getSnapshot().events.every((e) => e.t_ms <= director.getSnapshot().playheadMs),
    ).toBe(true);
  });

  /** Given the key highlights, Then quiet stretches are cut and the half takes far less wall time than the full match. */
  it("shows a half faster in key highlights, and never the quiet stretch", async () => {
    const events = [{ t_ms: 12_000, type: "shot" }];
    const play = async (mode: "full" | "key") => {
      const { engine, director } = await started(events);
      director.setMode(mode);
      director.setSpeed(4);
      director.play();
      const seen = await run(director, engine.advance, 40_000);
      return { director, seen, wall: seen.findIndex((t) => t >= HALF_MS) * 16 };
    };
    const full = await play("full");
    const key = await play("key");
    expect(key.director.getSnapshot().status).toBe("break");
    expect(key.wall).toBeLessThan(full.wall / 2);
    // nothing between the end of the shot's window and the end of the half was shown, except the run to the break
    const shown = key.seen.filter((t) => t > 20_000 && t < HALF_MS - 1);
    expect(shown).toHaveLength(0);
  });

  /** Given a mode change in the middle of play, Then the picture continues from where it is. */
  it("changes highlight mode without moving the picture", async () => {
    const { engine, director } = await started([{ t_ms: 30_000, type: "goal" }]);
    director.setSpeed(8);
    director.play();
    await run(director, engine.advance, 1_000);
    const before = director.positionMs;
    expect(before).toBeGreaterThan(0);
    director.setMode("extended");
    expect(director.getSnapshot().playheadMs).toBe(before);
    director.setMode("full");
    expect(director.getSnapshot().playheadMs).toBe(before);
  });

  /** Given a screen whose first view is already in the second half (a saved match reloaded), When the stopwatch is asked for an instant of the first half, Then it reads that half's minute instead of nothing. */
  it("keeps a stopwatch for the first half when it only ever saw the second", async () => {
    const engine = fakeEngine([{ t_ms: 10_000, type: "goal" }]);
    engine.state.period = 1;
    engine.state.clock = HALF_MS + 20_000;
    engine.state.lastFrame = HALF_MS + 19_500;
    const director = new LiveDirector();
    director.ingestAdvance(await engine.advance(0));
    expect(director.labelAt(10_000)).toBe("1'");
    expect(director.labelAt(HALF_MS + 20_000)).toBe("46'");
    // the feed's first-half goal is now in the director's events with a readable minute
    expect(director.getSnapshot().events.map((e) => director.labelAt(e.t_ms))).toEqual(["1'"]);
  });

  /** Given a match taken over (a saved match reloaded) at 30 s in a highlight mode, with a goal already scored before that point, When it plays on, Then the picture starts at the engine's head, the earlier goal is in the feed at once, and the later goal appears only when the picture reaches it. */
  it("plays a taken-over match on from the head in a highlight mode", async () => {
    const engine = fakeEngine([
      { t_ms: 5_000, type: "goal" },
      { t_ms: 50_000, type: "goal" },
    ]);
    engine.state.clock = 30_000;
    engine.state.lastFrame = 29_500;
    const director = new LiveDirector();
    director.setMode("key");
    director.ingestAdvance(await engine.advance(0));
    expect(director.getSnapshot().playheadMs).toBe(30_000);
    expect(director.getSnapshot().events.map((e) => e.t_ms)).toEqual([5_000]);
    director.setSpeed(8);
    director.play();
    const seen = await run(director, engine.advance, 6_000);
    expect(seen.every((t, i) => i === 0 || t >= seen[i - 1])).toBe(true);
    expect(seen[0]).toBeGreaterThanOrEqual(30_000);
    const snapshot = director.getSnapshot();
    expect(snapshot.playheadMs).toBeGreaterThan(50_000);
    expect(snapshot.events.map((e) => e.t_ms)).toEqual([5_000, 50_000]);
  });
});
