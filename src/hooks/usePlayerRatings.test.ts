import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RatingView } from "../services/playerRatingService";
import { usePlayerRatings } from "./usePlayerRatings";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const GOOD: RatingView = { kind: "standing", standing: "good" };
const SEEN: RatingView = {
  kind: "estimate",
  low: 60,
  high: 66,
  confidence: "high",
  current: true,
  scouted_on: "2026-09-01",
};

describe("usePlayerRatings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /**
   * Given two player ids
   * When the hook mounts
   * Then it asks the backend once for both and returns each reading by id, none before it arrives
   */
  it("asks once for the ids and returns each reading", async () => {
    invokeMock.mockResolvedValue({ a: GOOD, b: SEEN });

    const { result } = renderHook(() => usePlayerRatings(["a", "b"], 1));

    expect(result.current).toEqual({});
    await waitFor(() => expect(result.current).toEqual({ a: GOOD, b: SEEN }));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("get_player_ratings", { playerIds: ["a", "b"] });
  });

  /**
   * Given a reading on screen
   * When the game changes (a scout finished) and the backend now knows more
   * Then the hook asks again and shows the new reading, never keeping the old one silently
   */
  it("asks again when the game changes", async () => {
    invokeMock.mockResolvedValueOnce({ a: GOOD }).mockResolvedValueOnce({ a: SEEN });

    const { result, rerender } = renderHook(({ key }) => usePlayerRatings(["a"], key), {
      initialProps: { key: 1 },
    });
    await waitFor(() => expect(result.current.a).toEqual(GOOD));

    rerender({ key: 2 });

    await waitFor(() => expect(result.current.a).toEqual(SEEN));
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  /**
   * Given a slow answer for one set of ids and a quicker one for the next
   * When the slow one arrives last
   * Then it is dropped, so a screen never shows the reading of a player it has moved away from
   */
  it("drops an answer that was overtaken", async () => {
    let releaseFirst: (value: Record<string, RatingView>) => void = () => {};
    invokeMock
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseFirst = resolve;
          }),
      )
      .mockResolvedValueOnce({ b: SEEN });

    const { result, rerender } = renderHook(({ ids }) => usePlayerRatings(ids, 1), {
      initialProps: { ids: ["a"] },
    });
    rerender({ ids: ["b"] });
    await waitFor(() => expect(result.current).toEqual({ b: SEEN }));

    await act(async () => releaseFirst({ a: GOOD }));

    expect(result.current).toEqual({ b: SEEN });
  });

  /**
   * Given a backend that fails
   * When the hook asks
   * Then it returns no readings (the screen shows a dash) and does not throw
   */
  it("returns nothing when the backend fails", async () => {
    invokeMock.mockRejectedValue(new Error("boom"));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const { result } = renderHook(() => usePlayerRatings(["a"], 1));

    await waitFor(() => expect(errorSpy).toHaveBeenCalled());
    expect(result.current).toEqual({});
    errorSpy.mockRestore();
  });
});
