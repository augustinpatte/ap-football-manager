import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { JSX } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { emptyClub } from "../components/dashboard/simulationReport/reportFixtures";
import type { FastForwardResponse } from "../services/fastForwardService";
import type { GameStateData } from "../store/gameStore";
import { useFastForward } from "./useFastForward";

const service = vi.hoisted(() => ({
  fastForwardToDate: vi.fn(),
  cancelFastForward: vi.fn(),
  onFastForwardProgress: vi.fn(),
}));
vi.mock("../services/fastForwardService", () => service);

function finished(careerToken = 7): FastForwardResponse {
  return {
    progress: {
      start_date: "2033-07-01",
      current_date: "2033-07-08",
      target_date: "2033-07-08",
      days_done: 7,
      days_total: 7,
      user_matches_played: 1,
      seasons_rolled_over: 0,
    },
    stop: { kind: "target_reached" },
    season_summaries: [],
    passed_decisions: [],
    report: { clubs: [emptyClub({ days_played: 7 })] },
    game: { career_token: careerToken } as unknown as GameStateData,
    results: [],
  };
}

function Harness({ careerToken }: { careerToken: number }): JSX.Element {
  const ff = useFastForward("2033-07-01T12:00:00Z", vi.fn(), vi.fn(), careerToken);
  return (
    <div>
      <button type="button" onClick={() => ff.open({ days: 7, policy: "to_target_date" })}>
        open
      </button>
      <button type="button" onClick={() => void ff.start()}>
        start
      </button>
      <button type="button" onClick={ff.close}>
        close
      </button>
      <button type="button" onClick={ff.openReport}>
        open-report
      </button>
      <button type="button" onClick={ff.leaveReport}>
        leave-report
      </button>
      <div data-testid="phase">{ff.phase}</div>
      <div data-testid="last-run">{ff.lastRun ? "kept" : "none"}</div>
    </div>
  );
}

const phase = () => screen.getByTestId("phase").textContent;

async function runOnce() {
  fireEvent.click(screen.getByText("open"));
  fireEvent.click(screen.getByText("start"));
  await waitFor(() => expect(phase()).toBe("result"));
}

describe("the fast-forward report across the dialog's navigation", () => {
  beforeEach(() => {
    service.fastForwardToDate.mockReset();
    service.onFastForwardProgress.mockResolvedValue(() => undefined);
    service.fastForwardToDate.mockResolvedValue(finished());
  });

  /**
   * Given a finished run whose result is on screen
   * When the report is opened and left
   * Then the report is shown and leaving it returns to the result: the navigation is a stack, not a reset
   */
  it("opens_the_report_from_the_result_and_returns_to_it", async () => {
    render(<Harness careerToken={7} />);
    await runOnce();

    fireEvent.click(screen.getByText("open-report"));
    expect(phase()).toBe("report");
    fireEvent.click(screen.getByText("leave-report"));

    expect(phase()).toBe("result");
  });

  /**
   * Given a run whose dialog was closed
   * When the report is opened from outside the dialog and left
   * Then the report of that run is still there, and leaving it closes the dialog
   */
  it("keeps_the_last_report_after_the_dialog_is_closed", async () => {
    render(<Harness careerToken={7} />);
    await runOnce();
    fireEvent.click(screen.getByText("close"));
    expect(phase()).toBe("closed");
    expect(screen.getByTestId("last-run")).toHaveTextContent("kept");

    fireEvent.click(screen.getByText("open-report"));
    expect(phase()).toBe("report");
    fireEvent.click(screen.getByText("leave-report"));

    expect(phase()).toBe("closed");
    expect(service.fastForwardToDate).toHaveBeenCalledTimes(1);
  });

  /**
   * Given a report opened and left several times
   * When the backend is asked
   * Then it was asked only for the run itself: opening a report reads what the run handed back
   */
  it("never_asks_the_backend_when_a_report_is_opened_again", async () => {
    render(<Harness careerToken={7} />);
    await runOnce();
    const calls = service.fastForwardToDate.mock.calls.length;

    for (let i = 0; i < 3; i++) {
      fireEvent.click(screen.getByText("open-report"));
      fireEvent.click(screen.getByText("leave-report"));
    }

    expect(service.fastForwardToDate).toHaveBeenCalledTimes(calls);
    expect(service.cancelFastForward).not.toHaveBeenCalled();
  });

  /**
   * Given a report of a career that is no longer on screen
   * When another career is shown
   * Then its report is not offered: a report never describes the wrong career
   */
  it("does_not_offer_a_report_of_another_career", async () => {
    const view = render(<Harness careerToken={7} />);
    await runOnce();
    expect(screen.getByTestId("last-run")).toHaveTextContent("kept");

    act(() => view.rerender(<Harness careerToken={8} />));

    expect(screen.getByTestId("last-run")).toHaveTextContent("none");
  });
});
