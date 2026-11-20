// The live screen through the real service module on the engine's real payloads: only Tauri's
// `invoke` is replaced, by a host that answers with the replies the backend test
// `write_live_fixture` recorded from a real M12 match. A screen that asked for the wrong command,
// the wrong argument names or read the wrong field would fail here and pass against a hand-built fake.

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { i18nReady } from "../../i18n";
import fixture from "./liveMatch.real.generated.json";
import InteractiveMatchScreen from "./InteractiveMatchScreen";

const host = vi.hoisted(() => ({
  calls: [] as { command: string; args: Record<string, unknown> }[],
  next: 0,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string, args: Record<string, unknown> = {}) => {
    host.calls.push({ command, args });
    const data = fixture as unknown as {
      start: unknown;
      replies: { stop: string; frames: unknown[]; snapshot: unknown }[];
    };
    if (command === "start_interactive_match") return data.start;
    if (command === "advance_interactive_match") {
      // The first request is the kickoff frame; the next ones are the recorded stretches in order.
      const reply = data.replies[Math.min(host.next, data.replies.length - 1)];
      host.next += 1;
      return host.next > data.replies.length ? { ...reply, frames: [] } : reply;
    }
    throw new Error(`unexpected command ${command}`);
  }),
}));

const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

async function wall(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe("the live screen on real engine payloads", () => {
  beforeEach(async () => {
    await i18nReady;
    host.calls = [];
    host.next = 0;
    const context = new Proxy({}, { get: () => () => undefined, set: () => true });
    HTMLCanvasElement.prototype.getContext = vi.fn(
      () => context,
    ) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  });
  afterEach(() => vi.useRealTimers());

  /** Given the engine's real replies, When the manager plays at 16x, Then the screen asks for frames by seconds with the argument names the backend reads, and shows the real match's clock. */
  it("plays a real match's first minutes through the real service", async () => {
    render(
      <InteractiveMatchScreen
        homeName="Reds"
        awayName="Blues"
        onFinished={vi.fn()}
        onExited={vi.fn()}
      />,
    );
    await screen.findByRole("img", {
      name: tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" }),
    });
    await wall(50);
    expect(screen.getByTestId("live-clock")).toHaveTextContent("1'");

    fireEvent.click(screen.getByRole("radio", { name: "16×" }));
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(9_000);
    // 16 match seconds per wall second: nine seconds of wall time is past the second minute
    expect(screen.getByTestId("live-clock").textContent).toMatch(/^[23]'$/);

    const advances = host.calls.filter((c) => c.command === "advance_interactive_match");
    expect(advances.length).toBeGreaterThan(1);
    for (const call of advances) {
      expect(call.args.captureFrames).toBe(true);
      expect(call.args.request).toMatchObject({ target: { kind: "for_seconds" } });
      expect(typeof call.args.eventCursor).toBe("number");
    }
    expect(host.calls.map((c) => c.command)).not.toContain("set_interactive_pause");
  });

  /** Given the real lineups, Then the screen shows the engine's own players with their conditions, and no raw id. */
  it("shows the engine's lineups from the real view", async () => {
    render(
      <InteractiveMatchScreen
        homeName="Reds"
        awayName="Blues"
        onFinished={vi.fn()}
        onExited={vi.fn()}
      />,
    );
    await screen.findByRole("img");
    await wall(50);
    const start = (
      fixture as unknown as {
        start: { view: { session: { home: { on_pitch: { name: string }[] } } } };
      }
    ).start;
    const keeper = start.view.session.home.on_pitch[0].name;
    expect(screen.getAllByText(keeper).length).toBeGreaterThan(0);
    expect(document.body.textContent).not.toMatch(/\bghost\b/);
  });
});
