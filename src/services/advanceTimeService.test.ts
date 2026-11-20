import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { advanceTimeWithMode, checkBlockingActions, skipToMatchDay } from "./advanceTimeService";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe("advanceTimeService", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("calls the advance-time backend command with the requested mode", async () => {
    const response = { action: "advanced" };
    mockedInvoke.mockResolvedValueOnce(response);

    await expect(advanceTimeWithMode("delegate")).resolves.toBe(response);

    expect(mockedInvoke).toHaveBeenCalledWith("advance_time_with_mode", {
      mode: "delegate",
    });
  });

  it("returns blocker data when the blocker command succeeds", async () => {
    const blockers = [
      {
        id: "urgent_messages",
        severity: "info",
        text: "1 urgent unread message(s)",
        tab: "Inbox",
      },
    ];
    mockedInvoke.mockResolvedValueOnce(blockers);

    await expect(checkBlockingActions("test")).resolves.toEqual(blockers);
    expect(mockedInvoke).toHaveBeenCalledWith("check_blocking_actions");
  });

  /** Given a blocker check that cannot be answered, then the failure reaches the caller instead of looking like "no blockers". */
  it("rejects when the blocker command fails, so a failed check is not an empty one", async () => {
    const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockedInvoke.mockRejectedValueOnce(new Error("boom"));

    try {
      await expect(checkBlockingActions("test")).rejects.toThrow("boom");
    } finally {
      consoleWarnSpy.mockRestore();
    }
  });

  /** Given a check that succeeds and finds nothing, then the answer is the empty list. */
  it("returns an empty list when the check succeeds and finds nothing", async () => {
    mockedInvoke.mockResolvedValueOnce([]);

    await expect(checkBlockingActions("test")).resolves.toEqual([]);
  });

  /** Given the manager's explicit "Continue anyway", then the backend is told so; otherwise it is not sent. */
  it("sends ignoreBlockers to the advance only when the manager acknowledged", async () => {
    mockedInvoke.mockResolvedValue({ action: "advanced" });

    await advanceTimeWithMode("delegate");
    expect(mockedInvoke).toHaveBeenLastCalledWith("advance_time_with_mode", { mode: "delegate" });
    await advanceTimeWithMode("delegate", true);
    expect(mockedInvoke).toHaveBeenLastCalledWith("advance_time_with_mode", {
      mode: "delegate",
      ignoreBlockers: true,
    });
  });

  it("sends ignoreBlockers to the skip only when the manager acknowledged", async () => {
    mockedInvoke.mockResolvedValue({ action: "arrived" });

    await skipToMatchDay();
    expect(mockedInvoke).toHaveBeenLastCalledWith("skip_to_match_day");
    await skipToMatchDay(true);
    expect(mockedInvoke).toHaveBeenLastCalledWith("skip_to_match_day", { ignoreBlockers: true });
  });

  it("calls the skip-to-match-day backend command", async () => {
    const response = { action: "advanced", days_skipped: 3 };
    mockedInvoke.mockResolvedValueOnce(response);

    await expect(skipToMatchDay()).resolves.toBe(response);
    expect(mockedInvoke).toHaveBeenCalledWith("skip_to_match_day");
  });
});
