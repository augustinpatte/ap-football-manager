import { vi } from "vitest";
import type {
  InteractiveSnapshot,
  LiveFrame,
  MatchKeyEvent,
  MatchPlayerView,
  MatchTeamView,
  PlanView,
  UserOrder,
} from "../../services/interactiveMatchService";
import { SHAPES } from "./formationPanel.fixtures";

// The engine fake shared by the live-match screen's test files, wired in each with
// `vi.mock(path, async () => (await import("./interactiveMatchTestBackend")).<name>)`.
// It must not import the screen: the screen imports the mocked service, so the mock factory
// waiting on this module while this module waits on the screen would never finish.

export const HALF_MS = 60_000;
export const GOAL_MS = 20_000;

/**
 * A fake of the match engine behind the service. It owns the clock, the frames, the revision and the
 * pending substitutions, produces a frame every 500 ms of the time it plays, breaks at half-time
 * and answers every call from that state, as the real backend does. The screen therefore sees
 * values fed back, never canned replies. Home scores once, at 20 seconds.
 */
export const backend = (() => {
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
    /** While set, an order waits for it before it is applied: a command still in flight. */
    orderGate: null as Promise<void> | null,
    /** While set, an assistant toggle waits for it: a request still in flight. */
    assistantGate: null as Promise<void> | null,
    /** While set, finishing waits for it: the result is still being applied. */
    finishGate: null as Promise<void> | null,
  };
  return { state };
})();

/** The career-level calls the match screen makes: both go through the same queue as the engine's. */
export const career = {
  save: vi.fn<() => Promise<void>>(),
  exit: vi.fn<() => Promise<void>>(),
  closeRequested: null as null | ((event: { preventDefault: () => void }) => Promise<void> | void),
  destroy: vi.fn(async () => undefined),
};

export const sessionMock = {
  saveGame: () => career.save(),
  exitToMenu: () => career.exit(),
};

export const windowMock = {
  getCurrentWindow: () => ({
    onCloseRequested: (handler: typeof career.closeRequested) => {
      career.closeRequested = handler;
      return Promise.resolve(() => undefined);
    },
    destroy: career.destroy,
  }),
};

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
      formation_shapes: SHAPES,
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

export const serviceMock = {
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
    if (s.orderGate) await s.orderGate;
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
    if (backend.state.assistantGate) await backend.state.assistantGate;
    backend.state.assistantActive = active;
    return snapshot();
  }),
  finishInteractiveMatch: vi.fn(async () => {
    backend.state.calls.push("finish");
    if (backend.state.finishGate) await backend.state.finishGate;
    return { game: "applied" };
  }),
};

export function reset() {
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
    orderGate: null,
    assistantGate: null,
    finishGate: null,
  });
}

/** A canvas context that records what is drawn: each player and the ball is an arc, each shirt number a label. */
export const drawn = { arcs: 0, labels: [] as string[], frames: 0 };
export function stubCanvas() {
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
