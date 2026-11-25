import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { fetchUserSchedule } from "./scheduleService";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("fetchUserSchedule", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  /**
   * Given a manager with no club, for whom the backend answers null
   * When his schedule is fetched
   * Then it is an empty list, which the view shows as "no matches", not as still loading
   */
  it("answers_an_empty_schedule_for_a_clubless_manager", async () => {
    vi.mocked(invoke).mockResolvedValue(null);

    expect(await fetchUserSchedule()).toEqual({ today: "", fixtures: [] });
  });

  /** Given a schedule from the backend, when fetched, then it is returned as it is. */
  it("returns_the_backends_schedule_unchanged", async () => {
    const schedule = { today: "2026-09-10", fixtures: [] };
    vi.mocked(invoke).mockResolvedValue(schedule);

    expect(await fetchUserSchedule()).toBe(schedule);
  });
});
