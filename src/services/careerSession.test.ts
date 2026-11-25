import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import {
  beginCareerOperation,
  getCareerPhase,
  lookUpActiveCareer,
  resetCareerSessionForTests,
} from "./careerSession";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const NO_SESSION = "be.error.noActiveGameSession";

/** A promise the test settles by hand, so responses can arrive in the order the test chooses. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function activeGame(name: string) {
  return { manager: { display_name: name }, extra_translations: {} };
}

describe("lookUpActiveCareer", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    resetCareerSessionForTests();
  });

  /**
   * Given the backend holds a career that already has a club
   * When the lookup runs
   * Then it reports the game and the dashboard as the route
   */
  it("reports_an_employed_career_with_the_dashboard_route", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return activeGame("A");
      if (command === "get_career_stage") return "employed";
      return null;
    });

    const lookup = await lookUpActiveCareer();

    expect(lookup).toMatchObject({ kind: "found", route: "/dashboard" });
    expect(getCareerPhase()).toBe("active");
  });

  /**
   * Given a career that has not taken its first club
   * When the lookup runs
   * Then the route is team selection and the phase says the career awaits a club
   */
  it("reports_a_clubless_career_as_awaiting_a_club", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return activeGame("A");
      if (command === "get_career_stage") return "awaitingClub";
      return null;
    });

    const lookup = await lookUpActiveCareer();

    expect(lookup).toMatchObject({ kind: "found", route: "/select-team" });
    expect(getCareerPhase()).toBe("awaitingClub");
  });

  /**
   * Given the backend holds no career
   * When the lookup runs
   * Then the answer is "none", which is not an error
   */
  it("reports_no_career_when_the_backend_has_no_session", async () => {
    invokeMock.mockRejectedValue(NO_SESSION);

    const lookup = await lookUpActiveCareer();

    expect(lookup).toEqual({ kind: "none" });
    expect(getCareerPhase()).toBe("none");
  });

  /**
   * Given the backend fails for a reason other than "no session"
   * When the lookup runs
   * Then the answer is "failed" and carries the error, so no caller can mistake it for "no career"
   */
  it("reports_an_unexpected_failure_as_failed_not_as_none", async () => {
    const boom = new Error("database is locked");
    invokeMock.mockRejectedValue(boom);

    const lookup = await lookUpActiveCareer();

    expect(lookup).toEqual({ kind: "failed", error: boom });
    expect(getCareerPhase()).toBe("failed");
  });

  /**
   * Given a lookup is waiting on the backend (the mount probe)
   * When a second lookup is asked for (the `game-state-changed` event) before the first returns
   * Then one reading is shared, the later one is awaited too, and both callers get the same answer
   */
  it("shares_one_lookup_between_the_mount_probe_and_an_event", async () => {
    const firstGame = deferred<unknown>();
    const secondGame = deferred<unknown>();
    const games = [firstGame, secondGame];
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return games.shift()?.promise;
      if (command === "get_career_stage") return "employed";
      return null;
    });

    const probe = lookUpActiveCareer();
    const event = lookUpActiveCareer();
    firstGame.resolve(activeGame("stale"));
    await Promise.resolve();
    secondGame.resolve(activeGame("fresh"));
    const [probeResult, eventResult] = await Promise.all([probe, event]);

    expect(probeResult).toBe(eventResult);
    expect(probeResult).toMatchObject({
      kind: "found",
      state: { manager: { display_name: "fresh" } },
    });
    expect(invokeMock.mock.calls.filter(([c]) => c === "get_active_game")).toHaveLength(2);
  });

  /**
   * Given a lookup whose game read returns after the career has changed underneath it
   * When a later request arrives while it is still pending
   * Then the earlier, stale read is never the answer a caller navigates on
   */
  it("never_answers_with_a_read_older_than_the_latest_request", async () => {
    const stale = deferred<unknown>();
    const fresh = deferred<unknown>();
    const stages = ["awaitingClub", "employed"];
    const games = [stale, fresh];
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return games.shift()?.promise;
      if (command === "get_career_stage") return stages.shift();
      return null;
    });

    const first = lookUpActiveCareer();
    const second = lookUpActiveCareer();
    // The newer read finishes first; the older one limps in afterwards.
    stale.resolve(activeGame("stale"));
    fresh.resolve(activeGame("fresh"));

    expect(await first).toMatchObject({ route: "/dashboard" });
    expect(await second).toMatchObject({ route: "/dashboard" });
  });

  /**
   * Given a lookup that has finished
   * When another is asked for later
   * Then it reads the backend again rather than replaying a remembered answer
   */
  it("reads_the_backend_again_for_a_lookup_asked_after_the_first_finished", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return activeGame("A");
      if (command === "get_career_stage") return "employed";
      return null;
    });

    await lookUpActiveCareer();
    await lookUpActiveCareer();

    expect(invokeMock.mock.calls.filter(([c]) => c === "get_active_game")).toHaveLength(2);
  });
});

describe("beginCareerOperation", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    resetCareerSessionForTests();
  });

  /**
   * Given a career is being created
   * When a second creation or load is begun
   * Then it is refused, so a double click cannot start two careers
   */
  it("refuses_a_second_operation_while_one_is_running", () => {
    const first = beginCareerOperation("creating");
    const second = beginCareerOperation("loading");

    expect(first).not.toBeNull();
    expect(second).toBeNull();
    expect(getCareerPhase()).toBe("creating");
  });

  /**
   * Given an operation that finished
   * When it is released
   * Then the phase leaves "creating" and a new operation may begin
   */
  it("allows_a_new_operation_after_the_first_is_released", () => {
    const first = beginCareerOperation("loading");
    first?.release();

    expect(getCareerPhase()).not.toBe("loading");
    expect(beginCareerOperation("creating")).not.toBeNull();
  });

  /**
   * Given a career is loading
   * When the backend's `game-state-changed` event triggers a lookup
   * Then the lookup answers "busy" without touching the backend, because the operation navigates itself
   */
  it("holds_back_event_lookups_while_a_career_is_being_loaded", async () => {
    beginCareerOperation("loading");

    const lookup = await lookUpActiveCareer({ skipWhileBusy: true });

    expect(lookup).toEqual({ kind: "busy" });
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
