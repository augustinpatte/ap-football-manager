import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { i18nReady } from "../../i18n";
import InteractiveMatchScreen from "./InteractiveMatchScreen";
import type {
  InteractiveSnapshot,
  MatchKeyEvent,
  MatchPlayerView,
  MatchTeamView,
  UserOrder,
} from "../../services/interactiveMatchService";

/**
 * A fake of the match engine behind the service: it owns the clock, the revision and the pending
 * substitutions and answers every call from that state, as the real backend does. The screen
 * therefore sees values fed back, never canned replies.
 */
const backend = vi.hoisted(() => {
  const state = {
    clockMs: 0,
    revision: 0,
    paused: false,
    finished: false,
    brokeAtHalf: false,
    assistantAvailable: false,
    assistantActive: false,
    pending: [] as { off: string; on: string }[],
    events: [] as unknown[],
    rejectNext: null as string | null,
    failNextAdvance: false,
    calls: [] as string[],
    orders: [] as unknown[],
  };
  return { state };
});

function player(id: string, name: string, role: string | null): MatchPlayerView {
  return {
    id,
    name,
    shirt_number: 1,
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
      player(`${prefix}1`, `${prefix} Keeper`, "GK"),
      player(`${prefix}2`, `${prefix} Back`, "CB"),
    ],
    bench: [player(`${prefix}9`, `${prefix} Spare`, null)],
    used: [],
    sent_off: [],
    substitutions_used: 0,
    substitutions_remaining: 5,
    windows_remaining: 3,
    pending_substitutions: side === "home" ? backend.state.pending : [],
  };
}

function snapshot(): InteractiveSnapshot {
  const s = backend.state;
  const events = s.events as MatchKeyEvent[];
  return {
    view: {
      session: {
        session_id: "s1",
        revision: s.revision,
        clock_ms: s.clockMs,
        period: "first_half",
        phase: s.finished
          ? "full_time"
          : s.brokeAtHalf && s.clockMs === 6 * 60_000
            ? "half_time"
            : "in_play",
        paused: s.paused,
        finished: s.finished,
        score_home: 1,
        score_away: 0,
        home: team("home", "H"),
        away: team("away", "A"),
        event_count: events.length,
      },
      user_side: "home",
      assistant_available: s.assistantAvailable,
      assistant_active: s.assistantActive,
      supported_formations: ["4-4-2", "4-3-3"],
      home_plan: { style: "Balanced", pressing: "Medium", tempo: "Direct", formation: "4-4-2" },
      away_plan: { style: "Balanced", pressing: "Medium", tempo: "Direct", formation: "4-4-2" },
      regulation_minutes: 12,
    },
    events,
    next_event_cursor: events.length,
  };
}

vi.mock("../../services/interactiveMatchService", () => ({
  startInteractiveMatch: vi.fn(async () => {
    backend.state.calls.push("start");
    return snapshot();
  }),
  getInteractiveMatchState: vi.fn(async () => {
    backend.state.calls.push("state");
    return snapshot();
  }),
  advanceInteractiveMatch: vi.fn(async (target: { kind: string; value?: number }) => {
    const s = backend.state;
    s.calls.push(`advance:${target.kind}`);
    if (s.failNextAdvance) {
      s.failNextAdvance = false;
      throw "be.error.interactive.engineFailed";
    }
    if (s.paused) return { stop: "paused", snapshot: snapshot() };
    const minutes = target.kind === "to_end" ? 12 : (target.value ?? 1);
    s.clockMs = Math.min(12 * 60_000, s.clockMs + minutes * 60_000);
    s.revision += 1;
    let stop = "reached";
    if (!s.brokeAtHalf && s.clockMs >= 6 * 60_000) {
      s.clockMs = 6 * 60_000;
      s.brokeAtHalf = true;
      s.events.push({ id: 0, t_ms: 5 * 60_000, type: "goal", side: "home", scorer: "H2" });
      stop = "period_break";
    } else if (s.clockMs >= 12 * 60_000) {
      s.finished = true;
      stop = "finished";
    }
    return { stop, snapshot: snapshot() };
  }),
  submitMatchCommand: vi.fn(async (id: string, revision: number, order: UserOrder) => {
    const s = backend.state;
    s.calls.push(`order:${revision}`);
    s.orders.push(order);
    if (s.rejectNext) {
      const rejection = s.rejectNext;
      s.rejectNext = null;
      return {
        receipt: {
          command_id: id,
          accepted: false,
          queued: false,
          rejection,
          revision: s.revision,
          duplicate: false,
        },
        snapshot: snapshot(),
      };
    }
    if ("Substitute" in order) {
      s.pending.push({ off: order.Substitute.player_off_id, on: order.Substitute.player_on_id });
    }
    s.revision += 1;
    return {
      receipt: {
        command_id: id,
        accepted: true,
        queued: "Substitute" in order,
        rejection: null,
        revision: s.revision,
        duplicate: false,
      },
      snapshot: snapshot(),
    };
  }),
  setInteractivePause: vi.fn(async (paused: boolean) => {
    backend.state.calls.push(`pause:${paused}`);
    backend.state.paused = paused;
    backend.state.revision += 1;
    return snapshot();
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
    revision: 0,
    paused: false,
    finished: false,
    brokeAtHalf: false,
    assistantAvailable: false,
    assistantActive: false,
    pending: [],
    events: [],
    rejectNext: null,
    failNextAdvance: false,
    calls: [],
    orders: [],
  });
}

function renderScreen(onFinished = vi.fn()) {
  render(<InteractiveMatchScreen homeName="Reds" awayName="Blues" onFinished={onFinished} />);
  return onFinished;
}

/** The app's own wording for a key, so a test never restates a translation. */
const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

describe("InteractiveMatchScreen", () => {
  beforeEach(async () => {
    await i18nReady;
    reset();
  });

  /** Given a match under way, when the screen opens, then it shows the engine's clock, score, both lineups and their condition. */
  it("shows what the engine reports", async () => {
    renderScreen();
    expect(await screen.findByText("H Keeper")).toBeInTheDocument();
    expect(screen.getByText("A Back")).toBeInTheDocument();
    expect(screen.getByText(/1 – 0/)).toBeInTheDocument();
    expect(screen.getAllByText("90").length).toBeGreaterThan(0);
    expect(backend.state.calls.filter((c) => c === "start")).toHaveLength(1);
  });

  /** Given the match is played, when the break comes, then play stops there so orders can be given, and the goal is in the feed. */
  it("plays by itself and stops at the half-time break", async () => {
    renderScreen();
    fireEvent.click(await screen.findByRole("button", { name: tr("match.interactive.play") }));
    await waitFor(() => expect(backend.state.brokeAtHalf).toBe(true), { timeout: 4000 });
    expect(await screen.findByRole("button", { name: tr("match.interactive.play") })).toBeEnabled();
    expect(
      screen.getByText(`5' Reds: ${tr("match.replayFeed.goal", { player: "H Back" })}`),
    ).toBeInTheDocument();
    const advances = backend.state.calls.filter((c) => c.startsWith("advance")).length;
    await new Promise((r) => setTimeout(r, 600));
    expect(backend.state.calls.filter((c) => c.startsWith("advance"))).toHaveLength(advances);
  });

  /** Given a request that fails, then the manager is told in words, play stops, and the screen re-reads the engine instead of keeping a stale picture. */
  it("reports a failure and resynchronises with the engine", async () => {
    backend.state.failNextAdvance = true;
    renderScreen();
    fireEvent.click(await screen.findByRole("button", { name: tr("match.interactive.play") }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(tr("be.error.interactive.engineFailed"));
    await waitFor(() => expect(backend.state.calls).toContain("state"));
    expect(screen.getByRole("button", { name: tr("match.interactive.play") })).toBeEnabled();
  });

  /** Given play is running, when the manager pauses and resumes, then the engine is told and play stops and restarts. */
  it("pauses and resumes through the engine", async () => {
    renderScreen();
    fireEvent.click(await screen.findByRole("button", { name: tr("match.interactive.play") }));
    fireEvent.click(await screen.findByRole("button", { name: tr("match.pause") }));
    await waitFor(() => expect(backend.state.paused).toBe(true));
    const resume = await screen.findByRole("button", { name: tr("match.interactive.resume") });
    fireEvent.click(resume);
    await waitFor(() => expect(backend.state.paused).toBe(false));
    expect(backend.state.calls).toContain("pause:true");
    expect(backend.state.calls).toContain("pause:false");
  });

  /** Given a pressing change only, when it is applied, then exactly that field is sent against the revision shown. */
  it("sends only the tactics that changed, with the current revision", async () => {
    renderScreen();
    const apply = await screen.findByRole("button", { name: tr("match.interactive.applyTactics") });
    expect(apply).toBeDisabled();
    fireEvent.click(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    );
    fireEvent.click(
      screen.getByRole("option", {
        name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
      }),
    );
    expect(apply).toBeEnabled();
    fireEvent.click(apply);
    await waitFor(() => expect(backend.state.orders).toHaveLength(1));
    expect(backend.state.orders[0]).toEqual({ Tactics: { pressing: "Aggressive" } });
    expect(backend.state.calls).toContain("order:0");
  });

  /** Given a substitution, when it is confirmed, then the order names both players and it shows as queued; cancelling sends the cancellation. */
  it("queues a substitution and can cancel it", async () => {
    renderScreen();
    await screen.findByText("H Keeper");
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") }));
    fireEvent.click(screen.getByRole("option", { name: "H Back" }));
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectReplacement") }));
    fireEvent.click(screen.getByRole("option", { name: "H Spare" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.confirmSubstitution") }));
    await waitFor(() =>
      expect(backend.state.orders).toContainEqual({
        Substitute: { player_off_id: "H2", player_on_id: "H9" },
      }),
    );
    const queued = await screen.findByText("H Back → H Spare");
    expect(queued).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.removeQueuedSubstitution", { off: "H Back", on: "H Spare" }),
      }),
    );
    await waitFor(() =>
      expect(backend.state.orders).toContainEqual({ CancelSubstitution: { player_off_id: "H2" } }),
    );
  });

  /** Given the engine refuses an order, then the manager is told why, in words, and nothing else changes. */
  it("explains a refusal with the engine's reason", async () => {
    backend.state.rejectNext = "no_substitutions_left";
    renderScreen();
    await screen.findByText("H Keeper");
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") }));
    fireEvent.click(screen.getByRole("option", { name: "H Back" }));
    fireEvent.click(screen.getByRole("combobox", { name: tr("match.selectReplacement") }));
    fireEvent.click(screen.getByRole("option", { name: "H Spare" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.confirmSubstitution") }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(tr("match.interactive.rejection.no_substitutions_left"));
  });

  /** Given the match is not over, then it cannot be finished; at full time finishing applies it and hands the career's response on. */
  it("finishes only at full time and returns the career's answer", async () => {
    const onFinished = renderScreen();
    const finish = await screen.findByRole("button", { name: tr("match.interactive.finish") });
    expect(finish).toBeDisabled();
    const toEnd = () => screen.getByRole("button", { name: tr("match.interactive.toEnd") });
    // the first period ends in the half-time break, where play stops for orders
    fireEvent.click(toEnd());
    await waitFor(() => expect(backend.state.brokeAtHalf).toBe(true));
    await waitFor(() => expect(toEnd()).toBeEnabled());
    fireEvent.click(toEnd());
    await waitFor(() =>
      expect(screen.getByRole("button", { name: tr("match.interactive.finish") })).toBeEnabled(),
    );
    expect(screen.getByText(tr("match.interactive.fullTimeHint"))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.finish") }));
    await waitFor(() => expect(onFinished).toHaveBeenCalledWith({ game: "applied" }));
    expect(backend.state.calls.filter((c) => c === "finish")).toHaveLength(1);
  });

  /** Given match-day tactics are not delegated, then the assistant switch is off and explained; once delegated it can be turned on. */
  it("offers the assistant only when it has been delegated", async () => {
    renderScreen();
    const box = await screen.findByRole("checkbox", { name: tr("match.interactive.assistant") });
    expect(box).toBeDisabled();
    expect(screen.getByText(tr("match.interactive.assistantUnavailable"))).toBeInTheDocument();
  });

  it("turns the assistant on when it is available", async () => {
    backend.state.assistantAvailable = true;
    renderScreen();
    const box = await screen.findByRole("checkbox", { name: tr("match.interactive.assistant") });
    expect(box).toBeEnabled();
    fireEvent.click(box);
    await waitFor(() => expect(backend.state.assistantActive).toBe(true));
    const hint = screen.getByText(tr("match.interactive.assistantHint"));
    expect(within(hint.parentElement as HTMLElement).getByRole("checkbox")).toBeChecked();
  });
});
