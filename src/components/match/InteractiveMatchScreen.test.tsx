import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { i18nReady } from "../../i18n";
import InteractiveMatchScreen from "./InteractiveMatchScreen";
import type {
  InteractiveSnapshot,
  LiveFrame,
  MatchKeyEvent,
  MatchPlayerView,
  MatchTeamView,
  PlanView,
  UserOrder,
} from "../../services/interactiveMatchService";

const HALF_MS = 60_000;
const GOAL_MS = 20_000;

/**
 * A fake of the match engine behind the service. It owns the clock, the frames, the revision and the
 * pending substitutions, produces a frame every 500 ms of the time it plays, breaks at half-time
 * and answers every call from that state, as the real backend does. The screen therefore sees
 * values fed back, never canned replies. Home scores once, at 20 seconds.
 */
const backend = vi.hoisted(() => {
  const state = {
    clockMs: 0,
    lastFrameMs: -1,
    // not 0: an order quoting a hard-coded or stale revision must fail to match
    revision: 7,
    plan: {
      style: "Balanced",
      pressing: "Medium",
      tempo: "Direct",
      formation: "4-4-2",
    } as PlanView,
    finished: false,
    brokeAtHalf: false,
    assistantAvailable: false,
    assistantActive: false,
    pending: [] as { off: string; on: string }[],
    emittedEvents: 0,
    stoppage: null as string | null,
    rejectNext: null as string | null,
    failNextAdvance: false,
    calls: [] as string[],
    orders: [] as unknown[],
    /** Receipts by command id: the real engine answers a repeated id with the first answer and applies nothing. */
    receipts: new Map<string, unknown>(),
    /** While set, an advance waits for it before answering: a request still in flight. */
    advanceGate: null as Promise<void> | null,
  };
  return { state };
});

/** The career-level calls the match screen makes: both go through the same queue as the engine's. */
const career = vi.hoisted(() => ({
  save: vi.fn<() => Promise<void>>(),
  exit: vi.fn<() => Promise<void>>(),
  closeRequested: null as null | ((event: { preventDefault: () => void }) => Promise<void> | void),
  destroy: vi.fn(async () => undefined),
}));

vi.mock("../../services/sessionService", () => ({
  saveGame: () => career.save(),
  exitToMenu: () => career.exit(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (handler: typeof career.closeRequested) => {
      career.closeRequested = handler;
      return Promise.resolve(() => undefined);
    },
    destroy: career.destroy,
  }),
}));

const SCHEDULED: MatchKeyEvent[] = [
  { id: 0, t_ms: 0, type: "period_start", period: "first_half" },
  { id: 1, t_ms: GOAL_MS, type: "goal", side: "home", scorer: "H2", position: { x: 104.5, y: 34 } },
];

function player(id: string, name: string, role: string | null, shirt: number): MatchPlayerView {
  return {
    id,
    name,
    shirt_number: shirt,
    natural_position: role ?? "CM",
    role,
    condition: 0.9,
    minutes_played: 0,
    yellow_cards: 0,
    injured: false,
  };
}

function team(side: "home" | "away", prefix: string): MatchTeamView {
  return {
    side,
    formation: "4-4-2",
    tactics: { mentality: 50, pressing: 55, line_height: 60, width: 50, tempo: 70, directness: 50 },
    on_pitch: [
      player(`${prefix}1`, `${prefix} Keeper`, "GK", 1),
      player(`${prefix}2`, `${prefix} Back`, "CB", 4),
    ],
    bench: [player(`${prefix}9`, `${prefix} Spare`, null, 14)],
    used: [],
    sent_off: [],
    substitutions_used: 0,
    substitutions_remaining: 5,
    windows_remaining: 3,
    pending_substitutions: side === "home" ? backend.state.pending : [],
  };
}

function frameAt(t: number): LiveFrame {
  const second = t >= HALF_MS;
  return {
    t_ms: t,
    period: second ? "second_half" : "first_half",
    state: backend.state.stoppage ? "stoppage" : "in_play",
    stoppage: backend.state.stoppage,
    ball: [30 + t / 1000, 34, 0],
    holder: 0,
    possession_side: "home",
    score: [t >= GOAL_MS ? 1 : 0, 0],
    players: [
      [0, 10 + t / 1000, 30, 0],
      [1, 20 + t / 1000, 40, 0],
      [2, 80 - t / 1000, 30, 3],
      [3, 70 - t / 1000, 40, 3],
    ],
  };
}

function snapshot(): InteractiveSnapshot {
  const s = backend.state;
  const events = SCHEDULED.filter((e) => e.t_ms <= s.clockMs);
  const second = s.brokeAtHalf;
  return {
    view: {
      session: {
        session_id: "s1",
        revision: s.revision,
        clock_ms: s.clockMs,
        period: second ? "second_half" : "first_half",
        phase: s.finished
          ? "full_time"
          : s.clockMs === HALF_MS && !second
            ? "half_time"
            : "in_play",
        paused: false,
        finished: s.finished,
        score_home: s.clockMs >= GOAL_MS ? 1 : 0,
        score_away: 0,
        home: team("home", "H"),
        away: team("away", "A"),
        event_count: events.length,
      },
      user_side: "home",
      assistant_available: s.assistantAvailable,
      assistant_active: s.assistantActive,
      supported_formations: ["4-4-2", "4-3-3"],
      home_plan: { ...s.plan },
      away_plan: { style: "Balanced", pressing: "Medium", tempo: "Direct", formation: "4-4-2" },
      regulation_minutes: 2,
      roster: [
        { id: "H1", side: "home" },
        { id: "H2", side: "home" },
        { id: "A1", side: "away" },
        { id: "A2", side: "away" },
      ],
      clock: {
        period_start_ms: second ? HALF_MS : 0,
        period_base_ms: second ? 45 * 60_000 : 0,
        period_nominal_ms: HALF_MS,
        nominal_end_ms: second ? 2 * HALF_MS : HALF_MS,
        added_time_ms: null,
      },
    },
    events: events.slice(s.emittedEvents),
    next_event_cursor: events.length,
  };
}

vi.mock("../../services/interactiveMatchService", () => ({
  startInteractiveMatch: vi.fn(async () => {
    backend.state.calls.push("start");
    // taking over a match under way: the next advance hands out the current frame again
    backend.state.lastFrameMs = -1;
    return snapshot();
  }),
  getInteractiveMatchState: vi.fn(async () => {
    backend.state.calls.push("state");
    return snapshot();
  }),
  advanceInteractiveMatch: vi.fn(
    async (
      target: { kind: string; value?: number },
      _cursor: number,
      _stopAtKeyEvents: boolean,
      capture: boolean,
    ) => {
      const s = backend.state;
      s.calls.push(`advance:${target.kind}:${capture ? "frames" : "plain"}`);
      if (s.advanceGate) await s.advanceGate;
      if (s.failNextAdvance) {
        s.failNextAdvance = false;
        throw "be.error.interactive.engineFailed";
      }
      const limit = s.brokeAtHalf ? 2 * HALF_MS : HALF_MS;
      const wanted =
        target.kind === "to_end"
          ? limit
          : s.clockMs + (target.value ?? 0) * (target.kind === "for_minutes" ? 60_000 : 1_000);
      const reached = Math.min(limit, wanted);
      const frames: LiveFrame[] = [];
      if (capture) {
        const from = s.lastFrameMs < 0 ? reached : s.lastFrameMs + 500;
        for (let t = from; t <= reached; t += 500) frames.push(frameAt(t));
        if (s.lastFrameMs < 0 && frames.length === 0) frames.push(frameAt(0));
        s.lastFrameMs = Math.max(s.lastFrameMs, reached);
      }
      // The real engine's revision counts its steps. Asked for frames, as the screen always does,
      // it steps once per 500 ms frame, so it moves about twice per simulated second (measured on
      // the real backend: +2 a second with frames, +1 per stretch without); an accepted order adds one.
      s.revision += Math.round(((reached - s.clockMs) / 1000) * 2);
      s.clockMs = reached;
      let stop = "reached";
      if (reached >= limit) {
        if (s.brokeAtHalf) {
          s.finished = true;
          stop = "finished";
        } else {
          stop = "period_break";
          s.brokeAtHalf = true;
        }
      }
      const result = snapshot();
      s.emittedEvents = result.next_event_cursor;
      // The clock label of the second half is computed by the view the engine reports after the break.
      return { stop, snapshot: result, frames };
    },
  ),
  submitMatchCommand: vi.fn(async (id: string, revision: number, order: UserOrder) => {
    const s = backend.state;
    s.calls.push(`order:${revision}`);
    const answered = s.receipts.get(id);
    if (answered) {
      return { receipt: { ...(answered as object), duplicate: true }, snapshot: snapshot() };
    }
    if (revision !== s.revision) {
      const stale = {
        command_id: id,
        accepted: false,
        queued: false,
        rejection: "stale_revision",
        revision: s.revision,
        duplicate: false,
      };
      s.receipts.set(id, stale);
      return { receipt: stale, snapshot: snapshot() };
    }
    s.orders.push(order);
    if (s.rejectNext) {
      const rejection = s.rejectNext;
      s.rejectNext = null;
      const refused = {
        command_id: id,
        accepted: false,
        queued: false,
        rejection,
        revision: s.revision,
        duplicate: false,
      };
      s.receipts.set(id, refused);
      return { receipt: refused, snapshot: snapshot() };
    }
    if ("Tactics" in order) Object.assign(s.plan, order.Tactics);
    if ("Substitute" in order) {
      s.pending.push({ off: order.Substitute.player_off_id, on: order.Substitute.player_on_id });
    }
    if ("CancelSubstitution" in order) {
      s.pending = s.pending.filter((p) => p.off !== order.CancelSubstitution.player_off_id);
    }
    s.revision += 1;
    const accepted = {
      command_id: id,
      accepted: true,
      queued: "Substitute" in order,
      rejection: null,
      revision: s.revision,
      duplicate: false,
    };
    s.receipts.set(id, accepted);
    return { receipt: accepted, snapshot: snapshot() };
  }),
  setInteractiveAssistant: vi.fn(async (active: boolean) => {
    backend.state.calls.push(`assistant:${active}`);
    backend.state.assistantActive = active;
    return snapshot();
  }),
  finishInteractiveMatch: vi.fn(async () => {
    backend.state.calls.push("finish");
    return { game: "applied" };
  }),
}));

function reset() {
  Object.assign(backend.state, {
    clockMs: 0,
    lastFrameMs: -1,
    revision: 7,
    plan: { style: "Balanced", pressing: "Medium", tempo: "Direct", formation: "4-4-2" },
    finished: false,
    brokeAtHalf: false,
    assistantAvailable: false,
    assistantActive: false,
    pending: [],
    emittedEvents: 0,
    stoppage: null,
    rejectNext: null,
    failNextAdvance: false,
    calls: [],
    orders: [],
    receipts: new Map(),
    advanceGate: null,
  });
}

/** A canvas context that records what is drawn: each player and the ball is an arc, each shirt number a label. */
const drawn = { arcs: 0, labels: [] as string[], frames: 0 };
function stubCanvas() {
  const context = new Proxy(
    {},
    {
      get(_target, name) {
        if (name === "arc") return () => (drawn.arcs += 1);
        if (name === "fillText") return (text: string) => drawn.labels.push(text);
        if (name === "fillRect") return () => undefined;
        return () => undefined;
      },
      set: () => true,
    },
  );
  HTMLCanvasElement.prototype.getContext = vi.fn(
    () => context,
  ) as unknown as typeof HTMLCanvasElement.prototype.getContext;
}

/** Lets `ms` of wall-clock time pass: animation frames run, and the engine fake answers between them. */
async function wall(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

function renderScreen(onFinished = vi.fn(), onExited = vi.fn()) {
  render(
    <InteractiveMatchScreen
      homeName="Reds"
      awayName="Blues"
      onFinished={onFinished}
      onExited={onExited}
    />,
  );
  return onFinished;
}

/** The app's own wording for a key, so a test never restates a translation. */
const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

async function opened(onFinished = vi.fn()) {
  renderScreen(onFinished);
  await screen.findByRole("img", {
    name: tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" }),
  });
  await wall(50);
  return onFinished;
}

describe("InteractiveMatchScreen (live 2D)", () => {
  beforeEach(async () => {
    await i18nReady;
    reset();
    drawn.arcs = 0;
    drawn.labels = [];
    stubCanvas();
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  /** Given a match about to start, when the screen opens, then the pitch shows the kickoff the engine produced: every player and the ball, the shirt numbers, the teams and the engine's lineups. */
  it("shows the kickoff the engine produced", async () => {
    await opened();
    expect(screen.getByTestId("live-score")).toHaveTextContent("0 – 0");
    expect(screen.getByTestId("live-clock")).toHaveTextContent("1'");
    expect(screen.getByText("H Keeper")).toBeInTheDocument();
    // four players and the ball (a shadow ellipse is not an arc)
    expect(drawn.arcs).toBeGreaterThanOrEqual(5);
    expect(drawn.labels).toEqual(expect.arrayContaining(["1", "4"]));
    expect(backend.state.calls.filter((c) => c === "start")).toHaveLength(1);
    expect(backend.state.calls).toContain("advance:for_seconds:frames");
    // nothing animates while paused: no further engine calls, no further draws
    const calls = backend.state.calls.length;
    const arcs = drawn.arcs;
    await wall(1000);
    expect(backend.state.calls.length).toBe(calls);
    expect(drawn.arcs).toBe(arcs);
  });

  /** Given play, then the picture and clock follow the engine, orders are not possible while it runs, and the score changes only when the picture reaches the goal. */
  it("plays the engine's frames and shows the goal when the picture reaches it", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(500);
    expect(screen.getByTestId("live-score")).toHaveTextContent("0 – 0");
    expect(
      screen.getByRole("button", { name: tr("match.interactive.applyTactics") }),
    ).toBeDisabled();
    expect(screen.getByText(tr("match.liveView.pauseToAct"))).toBeInTheDocument();
    expect(drawn.arcs).toBeGreaterThan(10);
    // 2x by default: the goal at 20 s of match time is reached after about 10 s of wall time
    await wall(11_000);
    expect(screen.getByTestId("live-score")).toHaveTextContent("1 – 0");
    // the goal is announced, with the scorer's name and the stopwatch minute, and is in the feed
    expect(
      screen.getByText(tr("match.liveView.goalBanner", { player: "H Back", minute: "1'" })),
    ).toBeInTheDocument();
    expect(screen.getByText(tr("match.replayFeed.goal", { player: "H Back" }))).toBeInTheDocument();
  });

  /** Given play is running, when the manager pauses, then the picture first reaches the engine's moment, and only then can an order be given, against the revision shown. */
  it("pauses at a moment the manager can act on and sends orders against its revision", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    fireEvent.click(screen.getByRole("button", { name: tr("match.pause") }));
    await wall(3000);
    const apply = screen.getByRole("button", { name: tr("match.interactive.applyTactics") });
    expect(apply).toBeDisabled(); // nothing chosen yet, but no longer because the match is running
    expect(screen.queryByText(tr("match.liveView.pauseToAct"))).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    );
    fireEvent.click(
      screen.getByRole("option", {
        name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
      }),
    );
    expect(apply).toBeEnabled();
    const revision = backend.state.revision;
    fireEvent.click(apply);
    await wall(100);
    expect(backend.state.orders).toEqual([{ Tactics: { pressing: "Aggressive" } }]);
    expect(backend.state.calls).toContain(`order:${revision}`);
    // the accepted plan comes back from the engine, so there is nothing left to apply
    expect(apply).toBeDisabled();
  });

  /** Given an accepted order, then the frames that follow it are produced after it, and none already shown is changed. */
  it("never changes the part of the match already shown when an order is given", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    fireEvent.click(screen.getByRole("button", { name: tr("match.pause") }));
    await wall(3000);
    const clockAtOrder = backend.state.clockMs;
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") }));
    fireEvent.click(screen.getByRole("option", { name: "H Back" }));
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectReplacement") }));
    fireEvent.click(screen.getByRole("option", { name: "H Spare" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.confirmSubstitution") }));
    await wall(100);
    expect(backend.state.orders).toContainEqual({
      Substitute: { player_off_id: "H2", player_on_id: "H9" },
    });
    // the engine did not move because of the order
    expect(backend.state.clockMs).toBe(clockAtOrder);
    expect(await screen.findByText("H Back → H Spare")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.removeQueuedSubstitution", { off: "H Back", on: "H Spare" }),
      }),
    );
    await wall(100);
    expect(backend.state.orders).toContainEqual({ CancelSubstitution: { player_off_id: "H2" } });
  });

  /** Given the engine refuses an order, then the manager is told why, in words. */
  it("explains a refusal with the engine's reason", async () => {
    backend.state.rejectNext = "no_substitutions_left";
    await opened();
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") }));
    fireEvent.click(screen.getByRole("option", { name: "H Back" }));
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectReplacement") }));
    fireEvent.click(screen.getByRole("option", { name: "H Spare" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.confirmSubstitution") }));
    await wall(100);
    expect(screen.getByRole("alert")).toHaveTextContent(
      tr("match.interactive.rejection.no_substitutions_left"),
    );
  });

  /** Given the first half played at speed, then play stops at half-time for orders, the second half starts only on request, and full time enables Finish, which applies the result once even when clicked twice. */
  it("stops at half-time, plays the second half and finishes the match exactly once", async () => {
    const onFinished = await opened();
    fireEvent.click(screen.getByRole("radio", { name: "16×" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(8000);
    expect(screen.getByText(tr("match.halfTime"))).toBeInTheDocument();
    expect(backend.state.clockMs).toBe(HALF_MS);
    const startSecond = screen.getByRole("button", { name: tr("match.liveView.startSecondHalf") });
    const callsAtBreak = backend.state.calls.length;
    await wall(1000);
    expect(backend.state.calls.length).toBe(callsAtBreak); // nothing runs by itself past the break

    fireEvent.click(startSecond);
    await wall(10_000);
    expect(screen.getByText(tr("match.fullTime"))).toBeInTheDocument();
    expect(screen.getByText(tr("match.interactive.fullTimeHint"))).toBeInTheDocument();
    const finish = screen.getByRole("button", { name: tr("match.interactive.finish") });
    expect(finish).toBeEnabled();
    fireEvent.click(finish);
    fireEvent.click(finish);
    await wall(100);
    expect(onFinished).toHaveBeenCalledTimes(1);
    expect(onFinished).toHaveBeenCalledWith({ game: "applied" });
    expect(backend.state.calls.filter((c) => c === "finish")).toHaveLength(1);
  });

  /** Given the match is not over, then it cannot be finished. */
  it("cannot be finished before full time", async () => {
    await opened();
    expect(screen.getByRole("button", { name: tr("match.interactive.finish") })).toBeDisabled();
  });

  /** Given skip ahead, then the engine is played on to the break in one call, with the frames, and the picture shows where it got to. */
  it("skips ahead to the break and shows it", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.liveView.skip") }));
    await wall(200);
    expect(backend.state.calls.filter((c) => c === "advance:to_end:frames")).toHaveLength(1);
    expect(screen.getByText(tr("match.halfTime"))).toBeInTheDocument();
    expect(screen.getByTestId("live-score")).toHaveTextContent("1 – 0");
    expect(
      screen.getByRole("button", { name: tr("match.liveView.startSecondHalf") }),
    ).toBeEnabled();
  });

  /** Given a request that fails, then the manager is told in words, play stops, and the screen re-reads the engine instead of keeping a stale picture. */
  it("reports a failure and resynchronises with the engine", async () => {
    await opened();
    backend.state.failNextAdvance = true;
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(3000);
    expect(screen.getByRole("alert")).toHaveTextContent(tr("be.error.interactive.engineFailed"));
    expect(backend.state.calls).toContain("state");
    expect(screen.queryByRole("button", { name: tr("match.pause") })).not.toBeInTheDocument();
  });

  /** Given a match under way, then one polite status line carries the score and phase (not the ticking clock). */
  it("announces the score and phase once, not every slice", async () => {
    await opened();
    expect(screen.getByRole("status")).toHaveTextContent(
      `Reds 0, Blues 0, ${tr("match.liveView.kickoffPending")}`,
    );
    expect(document.querySelectorAll("[aria-live]").length).toBeLessThanOrEqual(1);
  });

  /** Given a queued substitution whose players are not in the engine's lists, then the screen says an unknown player, never a raw id. */
  it("never shows a raw player id", async () => {
    backend.state.pending.push({ off: "ghost-1", on: "ghost-2" });
    await opened();
    const unknown = tr("match.replayUnknownPlayer");
    expect(screen.getByText(`${unknown} → ${unknown}`)).toBeInTheDocument();
    expect(screen.queryByText(/ghost/)).not.toBeInTheDocument();
  });

  /** Given the engine stopping play for a reason this version has wording for, or one a newer engine adds, Then the caption says it in words, and an unknown reason is "play stopped", never a raw key. */
  it("names why play stopped, and never shows a raw key for a reason it does not know", async () => {
    for (const [reason, key] of [
      ["foul", "match.liveView.stoppage.foul"],
      ["var_check", "match.liveView.stoppage.other"],
    ]) {
      cleanup();
      reset();
      backend.state.stoppage = reason;
      await opened();
      expect(screen.getByText(tr(key))).toBeInTheDocument();
      expect(document.body.textContent).not.toContain("match.liveView");
    }
  });

  /** Given the highlight modes, then all three are offered, the full match is the default, and the choice is exposed as a radio group. */
  it("offers the highlight modes", async () => {
    await opened();
    const group = screen.getByRole("radiogroup", { name: tr("match.liveView.mode") });
    expect(
      within(group).getByRole("radio", { name: tr("match.liveView.modes.full") }),
    ).toBeChecked();
    fireEvent.click(within(group).getByRole("radio", { name: tr("match.liveView.modes.key") }));
    expect(
      within(group).getByRole("radio", { name: tr("match.liveView.modes.key") }),
    ).toBeChecked();
  });

  /** Given match-day tactics are not delegated, then the assistant switch is off and explained; once delegated it can be turned on. */
  it("offers the assistant only when it has been delegated", async () => {
    await opened();
    expect(
      screen.getByRole("checkbox", { name: tr("match.interactive.assistant") }),
    ).toBeDisabled();
    expect(screen.getByText(tr("match.interactive.assistantUnavailable"))).toBeInTheDocument();
  });

  it("turns the assistant on when it is available", async () => {
    backend.state.assistantAvailable = true;
    await opened();
    const box = screen.getByRole("checkbox", { name: tr("match.interactive.assistant") });
    expect(box).toBeEnabled();
    fireEvent.click(box);
    await waitFor(() => expect(backend.state.assistantActive).toBe(true));
  });
  /** Given a match under way, when the manager changes the language, then the match is not started again, the picture stays where it was and no data warning appears. */
  it("keeps the match as it is when the language changes", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(4000);
    const callsBefore = [...backend.state.calls];
    expect(callsBefore.filter((c) => c === "start")).toHaveLength(1);
    const clockBefore = screen.getByTestId("live-clock").textContent;
    try {
      await act(async () => {
        await i18n.changeLanguage("fr");
      });
      await wall(100);
      expect(backend.state.calls.filter((c) => c === "start")).toHaveLength(1);
      expect(backend.state.calls.filter((c) => c === "advance:for_seconds:frames").length).toBe(
        callsBefore.filter((c) => c === "advance:for_seconds:frames").length,
      );
      expect(screen.getByTestId("live-clock").textContent).toBe(clockBefore);
      expect(screen.queryByText(i18n.t("match.liveView.incompleteData"))).not.toBeInTheDocument();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
  });

  /** Given the screen is closed and opened again on the same match (or the career is reloaded), when an order is given, then it is a new order: the engine has not answered its id before. */
  it("gives an order on a reopened screen an id the engine has not used", async () => {
    await opened();
    const give = async () => {
      fireEvent.click(
        screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
      );
      fireEvent.click(
        screen.getByRole("option", {
          name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
        }),
      );
      fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.applyTactics") }));
      await wall(100);
    };
    await give();
    expect(backend.state.orders).toEqual([{ Tactics: { pressing: "Aggressive" } }]);
    cleanup();
    backend.state.plan = { ...backend.state.plan, pressing: "Medium" };
    await opened();
    await give();
    // the second order really changed the plan, rather than being answered as a repeat of the first
    expect(backend.state.orders).toHaveLength(2);
    expect(backend.state.plan.pressing).toBe("Aggressive");
  });

  /** Given the manager pauses while the engine is still producing a stretch, when the stretch lands, then no order was sent for a moment the picture had not reached: orders stay off until the picture is at the head again. */
  it("does not take an order while a stretch of play is still being produced", async () => {
    await opened();
    let release = () => {};
    backend.state.advanceGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(300); // the first stretch is requested and held
    fireEvent.click(screen.getByRole("button", { name: tr("match.pause") }));
    await wall(50);
    // the match is paused, but the engine is still producing play: nothing can be ordered yet
    expect(screen.getByText(tr("match.liveView.status.pausing"))).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    ).toBeDisabled();
    expect(backend.state.calls.filter((c) => c.startsWith("order:"))).toHaveLength(0);
    backend.state.advanceGate = null;
    release();
    await wall(5000);
    // the picture caught up with what the engine produced, and only then are orders possible
    fireEvent.click(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    );
    fireEvent.click(
      screen.getByRole("option", {
        name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.applyTactics") }));
    await wall(100);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(backend.state.plan.pressing).toBe("Aggressive");
  });
});

describe("InteractiveMatchScreen save controls", () => {
  const saveName = () => tr("match.interactive.saveGame");
  const exitName = () => tr("match.interactive.saveExit");

  beforeEach(async () => {
    await i18nReady;
    reset();
    stubCanvas();
    career.save.mockReset().mockImplementation(async () => {
      backend.state.calls.push("save");
    });
    career.exit.mockReset().mockImplementation(async () => {
      backend.state.calls.push("exit");
    });
    career.destroy.mockClear();
    career.closeRequested = null;
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  /** Given a live managed match, when the player clicks Save game, then the career is saved once and the screen says so, and the match carries on. */
  it("saves the game from the match screen and confirms it", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(50);
    expect(career.save).toHaveBeenCalledTimes(1);
    expect(screen.getByText(tr("match.interactive.saved"))).toHaveAttribute("role", "status");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: saveName() })).toBeEnabled();
    expect(screen.getByRole("button", { name: tr("match.interactive.play") })).toBeEnabled();
  });

  /** Given a save in flight, when the player clicks Save, Save and return, Play or Finish again, then nothing duplicates or interleaves and the buttons say they are busy. */
  it("allows one save at a time and blocks conflicting actions while it runs", async () => {
    await opened();
    let finishSave = () => {};
    career.save.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishSave = resolve;
        }),
    );
    const save = screen.getByRole("button", { name: saveName() });
    fireEvent.click(save);
    await wall(10);
    expect(save).toBeDisabled();
    expect(save).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: exitName() })).toBeDisabled();
    expect(screen.getByRole("button", { name: tr("match.interactive.play") })).toBeDisabled();
    fireEvent.click(save);
    fireEvent.click(screen.getByRole("button", { name: exitName() }));
    expect(career.save).toHaveBeenCalledTimes(1);
    expect(career.exit).not.toHaveBeenCalled();
    finishSave();
    await wall(50);
    expect(save).toBeEnabled();
  });

  /** Given an accepted order, when the player saves straight after it, then the order reached the engine before the save, so the checkpoint carries it. */
  it("saves after the orders already accepted, never before them", async () => {
    await opened();
    fireEvent.click(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    );
    fireEvent.click(
      screen.getByRole("option", {
        name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.applyTactics") }));
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(100);
    const calls = backend.state.calls;
    const orderAt = calls.findIndex((c) => c.startsWith("order:"));
    expect(orderAt).toBeGreaterThanOrEqual(0);
    expect(calls.indexOf("save")).toBeGreaterThan(orderAt);
    expect(backend.state.plan.pressing).toBe("Aggressive");
  });

  /** Given the backend refuses the save, when the player clicks Save, then the translated reason is shown, the match is untouched and the save can be retried. */
  it("explains a failed save and keeps the match", async () => {
    await opened();
    career.save.mockRejectedValueOnce("be.error.noActiveSaveSession");
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(50);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(tr("be.error.noActiveSaveSession"));
    expect(alert).not.toHaveTextContent("be.error.");
    expect(screen.queryByText(tr("match.interactive.saved"))).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: tr("match.interactive.play") })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(50);
    expect(career.save).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  /** Given a save that failed with an unknown error, when the screen reports it, then the player still gets an actionable sentence and no raw key. */
  it("falls back to an actionable sentence for an unknown save error", async () => {
    await opened();
    career.save.mockRejectedValueOnce(new Error("disk exploded"));
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(50);
    expect(screen.getByRole("alert")).toHaveTextContent(tr("match.interactive.saveFailed"));
  });

  /** Given a match under way, when the player chooses Save and return to menu, then the existing exit path runs once and the screen hands over to the menu. */
  it("saves and returns to the menu through the existing exit path", async () => {
    const onExited = vi.fn();
    renderScreen(vi.fn(), onExited);
    await screen.findByRole("img", {
      name: tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" }),
    });
    await wall(50);
    const exit = screen.getByRole("button", { name: exitName() });
    fireEvent.click(exit);
    fireEvent.click(exit);
    await wall(50);
    expect(career.exit).toHaveBeenCalledTimes(1);
    expect(career.save).not.toHaveBeenCalled();
    expect(onExited).toHaveBeenCalledTimes(1);
  });

  /** Given the exit save fails, when the player chooses Save and return, then they stay in the match with the reason shown and can try again. */
  it("stays in the match when the exit save fails", async () => {
    const onExited = vi.fn();
    renderScreen(vi.fn(), onExited);
    await screen.findByRole("img", {
      name: tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" }),
    });
    await wall(50);
    career.exit.mockRejectedValueOnce("be.error.liveMatch.inProgress");
    fireEvent.click(screen.getByRole("button", { name: exitName() }));
    await wall(50);
    expect(onExited).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(tr("be.error.liveMatch.inProgress"));
    expect(screen.getByRole("button", { name: exitName() })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: exitName() }));
    await wall(50);
    expect(onExited).toHaveBeenCalledTimes(1);
  });

  /** Given a match played on since the last save, when the window is closed, then the close is held and the player is asked, and Save & Quit saves before the window goes. */
  it("never closes the window silently over unsaved match progress", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    const preventDefault = vi.fn();
    await act(async () => {
      await career.closeRequested?.({ preventDefault });
    });
    expect(preventDefault).toHaveBeenCalled();
    expect(career.destroy).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: tr("closeConfirm.saveQuit") }));
    await wall(100);
    expect(career.save).toHaveBeenCalledTimes(1);
    expect(career.destroy).toHaveBeenCalledTimes(1);
  });

  /** Given Save & Quit fails, when the window close was requested, then the window stays open and the reason is shown. */
  it("keeps the window open when the save before closing fails", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    career.save.mockRejectedValueOnce("be.error.noActiveSaveSession");
    await act(async () => {
      await career.closeRequested?.({ preventDefault: vi.fn() });
    });
    fireEvent.click(screen.getByRole("button", { name: tr("closeConfirm.saveQuit") }));
    await wall(100);
    expect(career.destroy).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(tr("be.error.noActiveSaveSession"));
  });

  /** Given the match was saved and nothing has happened since, when the window is closed, then it closes without a prompt. */
  it("lets the window close once the match is saved and unchanged", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    fireEvent.click(screen.getByRole("button", { name: tr("match.pause") }));
    await wall(3000);
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(100);
    const preventDefault = vi.fn();
    await act(async () => {
      await career.closeRequested?.({ preventDefault });
    });
    expect(preventDefault).not.toHaveBeenCalled();
  });
});
