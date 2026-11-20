import { beforeEach, describe, expect, it, vi } from "vitest";

import { getPlayerRatings } from "./playerRatingService";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("getPlayerRatings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /**
   * Given the ids of two players
   * When their ratings are requested
   * Then the backend command is asked once with camelCase `playerIds`, and its answer comes back
   */
  it("asks the backend once and returns its answer untouched", async () => {
    const answer = { a: { kind: "exact", ovr: 70 }, b: { kind: "standing", standing: "good" } };
    invokeMock.mockResolvedValue(answer);

    await expect(getPlayerRatings(["a", "b"])).resolves.toEqual(answer);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("get_player_ratings", { playerIds: ["a", "b"] });
  });
});
