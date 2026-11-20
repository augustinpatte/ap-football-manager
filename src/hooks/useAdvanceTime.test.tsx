import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState, type JSX } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";
import { useAdvanceTime } from "./useAdvanceTime";

const navigateMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateMock,
}));

const mockedInvoke = vi.mocked(invoke);

function HookHarness(props: {
  defaultMatchMode?: "live" | "spectator" | "delegate";
  hasMatchToday: boolean;
  continueToNextEvent?: boolean;
}): JSX.Element {
  const [, setGameState] = useState<GameStateData | null>(null);
  const {
    advanceError,
    blockerModal,
    handleConfirmMatch,
    handleContinue,
    handleSkipToMatchDay,
    showMatchConfirm,
  } = useAdvanceTime(
    (state) => setGameState(state),
    props.hasMatchToday,
    props.defaultMatchMode,
    true,
    false,
    props.continueToNextEvent ?? false,
  );

  return (
    <div>
      <button type="button" onClick={() => void handleContinue()}>
        Continue
      </button>
      <button type="button" onClick={handleConfirmMatch}>
        Confirm Match
      </button>
      <button type="button" onClick={() => void handleSkipToMatchDay()}>
        Skip
      </button>
      <button type="button" onClick={() => blockerModal?.pendingAction?.()}>
        Continue anyway
      </button>
      {advanceError ? <p role="alert">{advanceError}</p> : null}
      <div data-testid="show-match-confirm">{String(showMatchConfirm)}</div>
      <div data-testid="blocker-count">{blockerModal?.blockers.length ?? 0}</div>
    </div>
  );
}

describe("useAdvanceTime", (): void => {
  beforeEach(function resetMocks(): void {
    mockedInvoke.mockReset();
    navigateMock.mockReset();
  });

  it("shows match confirmation before advancing on match day", async (): Promise<void> => {
    render(<HookHarness hasMatchToday defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    await waitFor((): void => {
      expect(screen.getByTestId("show-match-confirm")).toHaveTextContent("true");
    });

    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("navigates to the live match with snapshot and fixture state after confirmation", async (): Promise<void> => {
    const snapshot = {
      phase: "PreKickOff",
      current_minute: 0,
      home_score: 0,
      away_score: 0,
      possession: "Home",
      ball_zone: "Midfield",
    };

    mockedInvoke.mockResolvedValueOnce({
      action: "live_match",
      fixture_index: 7,
      competition_id: "cup1",
      fixture_id: "cup-fix7",
      mode: "live",
      snapshot,
    });

    render(<HookHarness hasMatchToday defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm Match" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("advance_time_with_mode", {
        mode: "live",
      });
    });

    expect(navigateMock).toHaveBeenCalledWith("/match", {
      state: {
        fixtureIndex: 7,
        competitionId: "cup1",
        fixtureId: "cup-fix7",
        mode: "live",
        snapshot,
      },
    });
  });

  it("checks blocking actions before a normal continue and stops when blockers exist", async (): Promise<void> => {
    mockedInvoke.mockResolvedValueOnce([
      {
        id: "urgent_messages",
        severity: "info",
        tab: "Inbox",
        text: "1 urgent unread message(s)",
      },
    ]);

    render(<HookHarness hasMatchToday={false} defaultMatchMode="spectator" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("check_blocking_actions");
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    expect(mockedInvoke).toHaveBeenCalledTimes(1);
    expect(navigateMock).not.toHaveBeenCalled();
  });

  it("advances time on a normal day when blocker checks return empty", async (): Promise<void> => {
    const advancedGame = {
      clock: {
        current_date: "2026-07-02",
        start_date: "2026-07-01",
      },
    } as GameStateData;

    mockedInvoke.mockResolvedValueOnce([]).mockResolvedValueOnce({
      action: "advanced",
      game: advancedGame,
    });

    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenNthCalledWith(1, "check_blocking_actions");
      expect(mockedInvoke).toHaveBeenNthCalledWith(2, "advance_time_with_mode", {
        mode: "live",
      });
    });

    expect(navigateMock).not.toHaveBeenCalled();
  });

  it("checks blocking actions before skipping to match day and stops when blockers exist", async (): Promise<void> => {
    mockedInvoke.mockResolvedValueOnce([
      {
        id: "contract_expiry",
        severity: "warning",
        tab: "Finances",
        text: "A contract decision is still pending",
      },
    ]);

    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Skip" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("check_blocking_actions");
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    expect(mockedInvoke).toHaveBeenCalledTimes(1);
  });

  it("fires a single backend skip when Skip is double-clicked", async (): Promise<void> => {
    // Regression: the re-entrancy guard is a ref, not `isAdvancing` — two
    // rapid clicks both read the stale pre-commit state, so without the ref
    // two concurrent skip_to_match_day calls raced to setGameState.
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") return [];
      if (command === "skip_to_match_day") {
        return {
          action: "arrived",
          days_skipped: 3,
          results: [],
          game: { clock: { current_date: "2026-07-05T00:00:00Z" } },
        };
      }
      throw new Error(`unexpected command ${command}`);
    });

    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    const skip = screen.getByRole("button", { name: "Skip" });
    fireEvent.click(skip);
    fireEvent.click(skip);

    await waitFor((): void => {
      expect(
        mockedInvoke.mock.calls.filter(([command]) => command === "skip_to_match_day"),
      ).toHaveLength(1);
    });
  });

  /**
   * Given a contract warning that blocks skipping to the match day
   * When the manager chooses "continue anyway"
   * Then the warning is recorded as dealt with before the skip runs, so the skip is not stopped
   * by the same warning on its first day.
   */
  it("records the contract warnings as dealt with before continuing a skip anyway", async (): Promise<void> => {
    const calls: string[] = [];
    mockedInvoke.mockImplementation(async (command: string) => {
      calls.push(command);
      if (command === "check_blocking_actions") {
        return [{ id: "key_contract_risk", severity: "warn", tab: "Squad", text: "x" }];
      }
      if (command === "acknowledge_contract_warnings") return 2;
      if (command === "skip_to_match_day") {
        return {
          action: "arrived",
          days_skipped: 3,
          results: [],
          game: { clock: { current_date: "2026-07-05T00:00:00Z" } },
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await waitFor((): void => {
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    fireEvent.click(screen.getByRole("button", { name: "Continue anyway" }));

    await waitFor((): void => {
      expect(calls).toContain("skip_to_match_day");
    });
    expect(calls.indexOf("acknowledge_contract_warnings")).toBeGreaterThan(-1);
    expect(calls.indexOf("acknowledge_contract_warnings")).toBeLessThan(
      calls.indexOf("skip_to_match_day"),
    );
  });

  /**
   * Given the day-by-day run is blocked on its first day
   * When the manager chooses "continue anyway"
   * Then the backend is told to play that day past the blockers.
   */
  it("tells the backend to play the first day when a day-by-day run continues anyway", async (): Promise<void> => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") {
        return [{ id: "key_contract_risk", severity: "warn", tab: "Squad", text: "x" }];
      }
      if (command === "advance_one_day")
        return { action: "match_day", date: "2026-07-05", results: [] };
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" continueToNextEvent />);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor((): void => {
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    fireEvent.click(screen.getByRole("button", { name: "Continue anyway" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("advance_one_day", { ignoreBlockers: true });
    });
  });

  /**
   * Given a blocker check that fails,
   * When the manager clicks Continue,
   * Then nothing is advanced, a usable error is shown, and no blocker dialog pretends the list was empty.
   */
  it("does not advance when the blocker check fails and says why", async (): Promise<void> => {
    const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") throw "be.error.noActiveGameSession";
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(mockedInvoke).not.toHaveBeenCalledWith("advance_time_with_mode", expect.anything());
    expect(screen.getByTestId("blocker-count")).toHaveTextContent("0");
    consoleWarnSpy.mockRestore();
  });

  /** Given a failed blocker check on Skip, then no multi-day skip is started. */
  it("does not skip when the blocker check fails", async (): Promise<void> => {
    const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") throw new Error("ipc down");
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Skip" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(mockedInvoke).not.toHaveBeenCalledWith("skip_to_match_day");
    expect(mockedInvoke).not.toHaveBeenCalledWith("skip_to_match_day", expect.anything());
    consoleWarnSpy.mockRestore();
  });

  /**
   * Given a clear blocker check but a backend failure while playing the day,
   * When the manager presses Continue,
   * Then an error is shown (never a silent stop) and the manager can press Continue again.
   */
  it("says so when the backend fails while advancing a day", async (): Promise<void> => {
    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") return [];
      if (command === "advance_time_with_mode") throw "be.error.noActiveGameSession";
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    consoleErrorSpy.mockRestore();
  });

  /** Given a clear check but a backend failure during the multi-day skip, then an error is shown. */
  it("says so when the backend fails while skipping to match day", async (): Promise<void> => {
    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") return [];
      if (command === "skip_to_match_day") throw new Error("database is locked");
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Skip" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    consoleErrorSpy.mockRestore();
  });

  /** Given a failed check followed by a working one, when the manager tries again, then the day advances and the error is gone. */
  it("lets the manager retry after a failed check", async (): Promise<void> => {
    const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    let checks = 0;
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") {
        checks += 1;
        if (checks === 1) throw new Error("transient");
        return [];
      }
      if (command === "advance_time_with_mode") {
        return { action: "advanced", game: { clock: { current_date: "2026-07-02" } } };
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("advance_time_with_mode", { mode: "live" });
    });
    await waitFor((): void => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
    consoleWarnSpy.mockRestore();
  });

  /**
   * Given blockers shown to the manager,
   * When they choose "continue anyway" on a normal Continue,
   * Then the backend is told to play the day past them, exactly once, and no new check loops.
   */
  it("tells the backend to play the day when a Continue goes ahead anyway", async (): Promise<void> => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") {
        return [{ id: "key_contract_risk", severity: "warn", tab: "Squad", text: "x" }];
      }
      if (command === "acknowledge_contract_warnings") return 1;
      if (command === "advance_time_with_mode") {
        return { action: "advanced", game: { clock: { current_date: "2026-07-02" } } };
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor((): void => {
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    fireEvent.click(screen.getByRole("button", { name: "Continue anyway" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("advance_time_with_mode", {
        mode: "live",
        ignoreBlockers: true,
      });
    });
    const advances = mockedInvoke.mock.calls.filter(([c]) => c === "advance_time_with_mode");
    const checks = mockedInvoke.mock.calls.filter(([c]) => c === "check_blocking_actions");
    expect(advances).toHaveLength(1);
    expect(checks).toHaveLength(1);
  });

  /**
   * Given a blocker that appeared after the frontend's check passed,
   * When the backend answers `blocked`,
   * Then the manager sees the blockers and can continue anyway, which asks the backend to ignore them.
   */
  it("shows the blockers when the backend itself refuses the day", async (): Promise<void> => {
    let advances = 0;
    mockedInvoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "check_blocking_actions") return [];
      if (command === "acknowledge_contract_warnings") return 0;
      if (command === "advance_time_with_mode") {
        advances += 1;
        if ((args as { ignoreBlockers?: boolean }).ignoreBlockers) {
          return { action: "advanced", game: { clock: { current_date: "2026-07-02" } } };
        }
        return {
          action: "blocked",
          blockers: [{ id: "urgent_messages", severity: "info", tab: "Inbox", text: "1" }],
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor((): void => {
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });
    expect(navigateMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Continue anyway" }));
    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("advance_time_with_mode", {
        mode: "live",
        ignoreBlockers: true,
      });
    });
    expect(advances).toBe(2);
  });

  /** Given "continue anyway" on a skip, then the backend is told to skip past the blockers. */
  it("tells the backend to skip past the blockers when the manager continues anyway", async (): Promise<void> => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "check_blocking_actions") {
        return [{ id: "key_contract_risk", severity: "warn", tab: "Squad", text: "x" }];
      }
      if (command === "acknowledge_contract_warnings") return 2;
      if (command === "skip_to_match_day") {
        return { action: "arrived", days_skipped: 1, results: [], game: { clock: {} } };
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<HookHarness hasMatchToday={false} defaultMatchMode="live" />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await waitFor((): void => {
      expect(screen.getByTestId("blocker-count")).toHaveTextContent("1");
    });

    fireEvent.click(screen.getByRole("button", { name: "Continue anyway" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("skip_to_match_day", { ignoreBlockers: true });
    });
  });
});
