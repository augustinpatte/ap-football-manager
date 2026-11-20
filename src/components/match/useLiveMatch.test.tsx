import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18nReady } from "../../i18n";
import { useLiveMatch } from "./useLiveMatch";

const service = vi.hoisted(() => ({
  finishCalls: 0,
  release: null as null | ((outcome: unknown) => void),
  outcomes: [] as unknown[],
}));

vi.mock("../../services/interactiveMatchService", () => ({
  // no match to show: only the finish path is under test
  startInteractiveMatch: vi.fn(async () => null),
  getInteractiveMatchState: vi.fn(async () => {
    throw "be.error.noActiveInteractiveMatch";
  }),
  advanceInteractiveMatch: vi.fn(async () => null),
  submitMatchCommand: vi.fn(async () => null),
  setInteractiveAssistant: vi.fn(async () => null),
  finishInteractiveMatch: vi.fn(
    () =>
      new Promise((resolve, reject) => {
        service.finishCalls += 1;
        service.release = (outcome) =>
          outcome instanceof Error ? reject(outcome.message) : resolve(outcome);
      }),
  ),
}));

/** Lets the engine answer the finish that is waiting, once the call has reached it. */
async function answer(outcome: unknown) {
  await vi.waitFor(() => expect(service.release).not.toBeNull());
  const release = service.release;
  service.release = null;
  release?.(outcome);
}

describe("useLiveMatch finishing", () => {
  beforeEach(async () => {
    await i18nReady;
    service.finishCalls = 0;
    service.release = null;
  });

  /** Given a finish request still in flight, When finish is requested twice more, Then the career is asked once and the result is applied once. */
  it("asks the engine to finish once while a finish is in flight", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useLiveMatch<string>(onFinished));
    let first: Promise<void> = Promise.resolve();
    await act(async () => {
      first = result.current.finish();
      void result.current.finish();
      void result.current.finish();
    });
    expect(service.finishCalls).toBe(1);
    await act(async () => {
      await answer("applied");
      await first;
    });
    expect(onFinished).toHaveBeenCalledTimes(1);
    expect(onFinished).toHaveBeenCalledWith("applied");
  });

  /** Given a match already finished and applied, When finish is requested again later, Then nothing is asked of the engine: there is no match left to finalise. */
  it("does not finish again after the result was applied", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useLiveMatch<string>(onFinished));
    await act(async () => {
      const done = result.current.finish();
      await answer("applied");
      await done;
    });
    await act(async () => {
      await result.current.finish();
    });
    expect(service.finishCalls).toBe(1);
    expect(onFinished).toHaveBeenCalledTimes(1);
  });

  /** Given a finish that failed (the match is kept), When the manager tries again, Then the engine is asked again and the result is applied once. */
  it("allows another attempt after a failed finish", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useLiveMatch<string>(onFinished));
    await act(async () => {
      const done = result.current.finish();
      await answer(new Error("be.error.liveMatch.fixtureNotFound"));
      await done;
    });
    expect(onFinished).not.toHaveBeenCalled();
    await act(async () => {
      const done = result.current.finish();
      await answer("applied");
      await done;
    });
    expect(service.finishCalls).toBe(2);
    expect(onFinished).toHaveBeenCalledTimes(1);
  });
});
