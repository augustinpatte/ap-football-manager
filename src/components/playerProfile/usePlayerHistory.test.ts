import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getPlayerHistory, type PlayerHistory } from "../../services/playerHistoryService";
import type { PlayerData } from "../../store/gameStore";
import { usePlayerHistory } from "./usePlayerHistory";

vi.mock("../../services/playerHistoryService", () => ({
  getPlayerHistory: vi.fn(),
}));

function playerNamed(id: string, teamId = "t1"): PlayerData {
  return { id, team_id: teamId, career: [], movement_history: undefined } as unknown as PlayerData;
}

function historyOf(id: string, season: number): PlayerHistory {
  return {
    player_id: id,
    career: [
      { season, team_id: "t1", team_name: "T1", appearances: 3, goals: 1, assists: 0 },
    ] as PlayerHistory["career"],
    movement_history: [],
  };
}

describe("usePlayerHistory", () => {
  beforeEach(() => {
    vi.mocked(getPlayerHistory).mockReset();
  });

  /**
   * Given the light game state, which carries no career for a player
   * When a profile opens
   * Then the career is asked of the backend once and shown
   */
  it("loads the career when a profile opens", async () => {
    vi.mocked(getPlayerHistory).mockResolvedValue(historyOf("a", 2025));

    const { result } = renderHook(() => usePlayerHistory(playerNamed("a"), "2026-08-01"));

    expect(result.current.career).toEqual([]);
    await waitFor(() => expect(result.current.career[0]?.season).toBe(2025));
    expect(getPlayerHistory).toHaveBeenCalledTimes(1);
    expect(getPlayerHistory).toHaveBeenCalledWith("a");
  });

  /**
   * Given a profile showing player A
   * When the same mounted profile is handed player B, whose history is still loading
   * Then not even the first render for B shows A's career
   */
  it("never shows the previous player's history for the next player", async () => {
    vi.mocked(getPlayerHistory).mockImplementation((id) =>
      id === "a" ? Promise.resolve(historyOf("a", 2025)) : new Promise(() => {}),
    );
    const seen: PlayerHistory[] = [];
    const { rerender } = renderHook(
      ({ id }) => {
        const history = usePlayerHistory(playerNamed(id), "2026-08-01");
        seen.push(history);
        return history;
      },
      { initialProps: { id: "a" } },
    );
    await waitFor(() => expect(seen[seen.length - 1]?.career).toHaveLength(1));
    const before = seen.length;

    rerender({ id: "b" });

    expect(seen.slice(before).every((h) => h.player_id === "b" && h.career.length === 0)).toBe(
      true,
    );
  });

  /**
   * Given an opened profile
   * When the game date moves (a transfer or a new season can have happened)
   * Then the history is fetched again, so it is never out of date
   */
  it("refetches when the game date or the club changes", async () => {
    vi.mocked(getPlayerHistory)
      .mockResolvedValueOnce(historyOf("a", 2025))
      .mockResolvedValueOnce(historyOf("a", 2026))
      .mockResolvedValueOnce(historyOf("a", 2027));
    const { result, rerender } = renderHook(
      ({ date, team }) => usePlayerHistory(playerNamed("a", team), date),
      { initialProps: { date: "2026-08-01", team: "t1" } },
    );
    await waitFor(() => expect(result.current.career[0]?.season).toBe(2025));

    rerender({ date: "2026-08-02", team: "t1" });
    await waitFor(() => expect(result.current.career[0]?.season).toBe(2026));
    rerender({ date: "2026-08-02", team: "t2" });
    await waitFor(() => expect(result.current.career[0]?.season).toBe(2027));
    expect(getPlayerHistory).toHaveBeenCalledTimes(3);
  });

  /**
   * Given a player the game no longer holds (a retiree), or a failing backend
   * When the profile opens
   * Then it falls back to what the player object carries instead of erroring
   */
  it("falls back to the player's own fields without an answer", async () => {
    vi.mocked(getPlayerHistory).mockResolvedValueOnce(null).mockRejectedValueOnce(new Error("x"));
    const retiree = {
      ...playerNamed("r"),
      career: historyOf("r", 2019).career,
    } as PlayerData;

    const { result, rerender } = renderHook(({ d }) => usePlayerHistory(retiree, d), {
      initialProps: { d: "1" },
    });
    await waitFor(() => expect(getPlayerHistory).toHaveBeenCalledTimes(1));
    expect(result.current.career[0]?.season).toBe(2019);

    rerender({ d: "2" });
    await waitFor(() => expect(getPlayerHistory).toHaveBeenCalledTimes(2));
    expect(result.current.career[0]?.season).toBe(2019);
  });
});
