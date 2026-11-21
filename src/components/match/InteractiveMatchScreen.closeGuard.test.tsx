import { act, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18nReady } from "../../i18n";
import { backend, career, reset, stubCanvas } from "./interactiveMatchTestBackend";
import { opened, tr, wall } from "./interactiveMatchTestScreen";

vi.mock(
  "../../services/sessionService",
  async () => (await import("./interactiveMatchTestBackend")).sessionMock,
);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("./interactiveMatchTestBackend")).windowMock,
);
vi.mock(
  "../../services/interactiveMatchService",
  async () => (await import("./interactiveMatchTestBackend")).serviceMock,
);

describe("InteractiveMatchScreen window-close guard", () => {
  const saveName = () => tr("match.interactive.saveGame");

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

  /** Plays on, pauses and saves: the checkpoint is current and nothing is dirty. */
  async function savedCheckpoint() {
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.play") }));
    await wall(1500);
    fireEvent.click(screen.getByRole("button", { name: tr("match.pause") }));
    await wall(3000);
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(100);
  }

  /** Holds the engine's next answer, as a request still in flight; returns the release. */
  function hold(gate: "advanceGate" | "orderGate" | "assistantGate" | "finishGate") {
    let release = () => {};
    backend.state[gate] = new Promise<void>((resolve) => {
      release = resolve;
    });
    return () => {
      backend.state[gate] = null;
      release();
    };
  }

  /** The engine's most recent call: proof the request under test really reached it. */
  const lastCall = () => backend.state.calls[backend.state.calls.length - 1];

  async function requestClose() {
    const preventDefault = vi.fn();
    await act(async () => {
      await career.closeRequested?.({ preventDefault });
    });
    return preventDefault;
  }

  /** Given a saved checkpoint and play resumed, when the window is closed while the engine is still answering the first advance, then the close is held and the player is asked. */
  it("holds a window close while an advance is pending after the last save", async () => {
    await opened();
    await savedCheckpoint();
    const release = hold("advanceGate");
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.resume") }));
    await wall(300);
    expect(lastCall()).toMatch(/^advance:/); // the engine is mid-answer
    expect(await requestClose()).toHaveBeenCalled();
    expect(career.destroy).not.toHaveBeenCalled();
    release();
  });

  /** Given an order still in flight after a save, when the window is closed, then the close is held. */
  it("holds a window close while an order is pending after the last save", async () => {
    await opened();
    await savedCheckpoint();
    const release = hold("orderGate");
    fireEvent.click(
      screen.getByRole("combobox", { name: tr("tactics.phaseSettings.pressingIntensity") }),
    );
    fireEvent.click(
      screen.getByRole("option", {
        name: tr("tactics.phaseSettings.pressingIntensity_Aggressive"),
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.applyTactics") }));
    await wall(50);
    expect(backend.state.calls.some((call) => call.startsWith("order:"))).toBe(true);
    expect(await requestClose()).toHaveBeenCalled();
    release();
  });

  /** Given a saved checkpoint and a skip to the break still being produced, when the window is closed, then the close is held. */
  it("holds a window close while a skip ahead is pending after the last save", async () => {
    await opened();
    await savedCheckpoint();
    const release = hold("advanceGate");
    fireEvent.click(screen.getByRole("button", { name: tr("match.liveView.skip") }));
    await wall(50);
    expect(lastCall()).toBe("advance:to_end:frames");
    expect(await requestClose()).toHaveBeenCalled();
    release();
  });

  /** Given a saved checkpoint and an assistant toggle still in flight, when the window is closed, then the close is held. */
  it("holds a window close while the assistant is being switched after the last save", async () => {
    backend.state.assistantAvailable = true;
    await opened();
    await savedCheckpoint();
    const release = hold("assistantGate");
    fireEvent.click(screen.getByRole("checkbox", { name: tr("match.interactive.assistant") }));
    await wall(50);
    expect(lastCall()).toBe("assistant:true");
    expect(await requestClose()).toHaveBeenCalled();
    release();
  });

  /** Given a saved match at full time and the result still being applied, when the window is closed, then the close is held: the result is not in any checkpoint yet. */
  it("holds a window close while the finished match is being applied", async () => {
    await opened();
    fireEvent.click(screen.getByRole("button", { name: tr("match.liveView.skip") }));
    await wall(200);
    fireEvent.click(screen.getByRole("button", { name: tr("match.liveView.startSecondHalf") }));
    await wall(300);
    fireEvent.click(screen.getByRole("button", { name: tr("match.liveView.skip") }));
    await wall(300);
    fireEvent.click(screen.getByRole("button", { name: saveName() }));
    await wall(100);
    const release = hold("finishGate");
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.finish") }));
    await wall(50);
    expect(lastCall()).toBe("finish");
    expect(await requestClose()).toHaveBeenCalled();
    release();
  });

  /** Given a match just opened whose kickoff fetch (it asks for no time) is still in flight, when the window is closed, then it closes without a prompt: nothing has been played. */
  it("does not hold a window close over the kickoff fetch", async () => {
    const release = hold("advanceGate");
    await opened();
    expect(lastCall()).toBe("advance:for_seconds:frames");
    expect(await requestClose()).not.toHaveBeenCalled();
    release();
  });

  /** Given a pending advance and a close request, when the player chooses Save & Quit, then the save runs after the advance, so the checkpoint carries it, and the window closes once. */
  it("saves after the pending advance and then closes on Save & Quit", async () => {
    await opened();
    await savedCheckpoint();
    const release = hold("advanceGate");
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.resume") }));
    await wall(300);
    await requestClose();
    fireEvent.click(screen.getByRole("button", { name: tr("closeConfirm.saveQuit") }));
    await wall(50);
    expect(career.save).toHaveBeenCalledTimes(1); // the checkpoint above; the quit save is queued
    release();
    await wall(200);
    expect(career.save).toHaveBeenCalledTimes(2);
    expect(career.destroy).toHaveBeenCalledTimes(1);
  });

  /** Given a pending advance and a close request, when the save before quitting fails, then the window stays open and the reason is shown. */
  it("keeps the window open when the save for a pending advance fails", async () => {
    await opened();
    await savedCheckpoint();
    const release = hold("advanceGate");
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.resume") }));
    await wall(300);
    await requestClose();
    career.save.mockRejectedValueOnce("be.error.noActiveSaveSession");
    fireEvent.click(screen.getByRole("button", { name: tr("closeConfirm.saveQuit") }));
    release();
    await wall(200);
    expect(career.destroy).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(tr("be.error.noActiveSaveSession"));
  });

  /** Given an advance that failed after a save, when the window is closed, then the close is still held: the engine may have moved before it failed. */
  it("holds a window close after a failed advance, whose effect is unknown", async () => {
    await opened();
    await savedCheckpoint();
    backend.state.failNextAdvance = true;
    fireEvent.click(screen.getByRole("button", { name: tr("match.interactive.resume") }));
    await wall(300);
    expect(await requestClose()).toHaveBeenCalled();
  });
});
