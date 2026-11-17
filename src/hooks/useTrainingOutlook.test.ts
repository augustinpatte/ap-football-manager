import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TrainingOutlookData } from "../services/trainingService";
import { useTrainingOutlook } from "./useTrainingOutlook";

vi.mock("../services/trainingService", () => ({ getTrainingOutlook: vi.fn() }));

const { getTrainingOutlook } = await import("../services/trainingService");
const mockedOutlook = vi.mocked(getTrainingOutlook);

const outlook = (condition: number): TrainingOutlookData => ({
  is_training_day: true,
  taper: null,
  session_intensity: "Medium",
  fixtures_this_week: 1,
  coaching_effect: 1.1,
  recovery_effect: 1.2,
  starters_condition: condition,
  delegated_to: null,
  proposal_waiting: false,
});

beforeEach(() => {
  mockedOutlook.mockReset();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

describe("useTrainingOutlook", () => {
  /**
   * Given the backend's outlook for the club
   * When the hook mounts
   * Then it is null until the answer, then the backend's figures as they are.
   */
  it("reads the backend's outlook", async () => {
    mockedOutlook.mockResolvedValue(outlook(77));
    const { result } = renderHook(() => useTrainingOutlook());
    expect(result.current).toBeNull();
    await waitFor(() => expect(result.current?.starters_condition).toBe(77));
  });

  /**
   * Given a failed read after a good one
   * When the game changes
   * Then the outlook is cleared: the last answer may belong to another career.
   */
  it("drops the last answer when a later read fails", async () => {
    mockedOutlook.mockResolvedValueOnce(outlook(77)).mockRejectedValueOnce(new Error("gone"));
    const { result, rerender } = renderHook(({ revision }) => useTrainingOutlook(revision), {
      initialProps: { revision: 1 },
    });
    await waitFor(() => expect(result.current?.starters_condition).toBe(77));
    rerender({ revision: 2 });
    await waitFor(() => expect(result.current).toBeNull());
  });
});
