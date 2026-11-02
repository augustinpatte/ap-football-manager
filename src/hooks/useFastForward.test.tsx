import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { JSX } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FastForwardProgress, FastForwardResponse } from "../services/fastForwardService";
import type { GameStateData } from "../store/gameStore";
import { useFastForward } from "./useFastForward";

const service = vi.hoisted(() => ({
  fastForwardToDate: vi.fn(),
  cancelFastForward: vi.fn(),
  onFastForwardProgress: vi.fn(),
}));
vi.mock("../services/fastForwardService", () => service);

const game = { clock: { current_date: "2033-07-08T12:00:00Z" } } as unknown as GameStateData;

function response(stop: FastForwardResponse["stop"], days = 7): FastForwardResponse {
  return {
    progress: {
      start_date: "2033-07-01",
      current_date: "2033-07-08",
      target_date: "2033-07-08",
      days_done: days,
      days_total: 7,
      user_matches_played: 1,
      seasons_rolled_over: 0,
    },
    stop,
    season_summaries: [],
    passed_decisions: [],
    game,
    results: [],
  };
}

function Harness(props: {
  setGameState: (state: GameStateData) => void;
  onFired: () => void;
}): JSX.Element {
  const ff = useFastForward("2033-07-01T12:00:00Z", props.setGameState, props.onFired);
  return (
    <div>
      <button type="button" onClick={ff.open}>
        open
      </button>
      <button type="button" onClick={() => void ff.start()}>
        start
      </button>
      <button type="button" onClick={ff.cancel}>
        cancel
      </button>
      <button type="button" onClick={() => void ff.resume()}>
        resume
      </button>
      <button type="button" onClick={() => ff.setTarget("2033-06-01")}>
        past
      </button>
      <div data-testid="phase">{ff.phase}</div>
      <div data-testid="target">{ff.target}</div>
      <div data-testid="valid">{String(ff.targetIsValid)}</div>
      <div data-testid="days">{ff.daysAhead}</div>
      <div data-testid="progress">{ff.progress?.days_done ?? "none"}</div>
      <div data-testid="error">{ff.error ?? ""}</div>
      <div data-testid="cancelling">{String(ff.isCancelling)}</div>
    </div>
  );
}

function setup() {
  const setGameState = vi.fn();
  const onFired = vi.fn();
  render(<Harness setGameState={setGameState} onFired={onFired} />);
  return { setGameState, onFired };
}

describe("useFastForward", () => {
  beforeEach(() => {
    service.fastForwardToDate.mockReset();
    service.cancelFastForward.mockReset().mockResolvedValue(true);
    service.onFastForwardProgress.mockReset().mockResolvedValue(() => {});
  });

  /** Given the game on 1 July, when the dialog opens, then it proposes a week ahead. */
  it("opens on a target a week from the game's date", () => {
    setup();
    fireEvent.click(screen.getByText("open"));
    expect(screen.getByTestId("phase")).toHaveTextContent("configure");
    expect(screen.getByTestId("target")).toHaveTextContent("2033-07-08");
    expect(screen.getByTestId("days")).toHaveTextContent("7");
  });

  /** Given a target in the past, when checked, then it is not startable. */
  it("rejects a target that is not after today", () => {
    setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("past"));
    expect(screen.getByTestId("valid")).toHaveTextContent("false");
  });

  /** Given a run, when it finishes, then the one world returned is stored and the result shown. */
  it("stores the returned world once and shows the result", async () => {
    service.fastForwardToDate.mockResolvedValue(response({ kind: "target_reached" }));
    const { setGameState } = setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("result"));
    expect(service.fastForwardToDate).toHaveBeenCalledWith({
      target_date: "2033-07-08",
      policy: "stop_for_decisions",
    });
    expect(setGameState).toHaveBeenCalledTimes(1);
    expect(setGameState).toHaveBeenCalledWith(game);
  });

  /** Given progress events during a run, when they arrive, then the hook shows the latest. */
  it("follows the backend's progress while the run is going", async () => {
    let emit: (progress: FastForwardProgress) => void = () => {};
    service.onFastForwardProgress.mockImplementation(async (handler) => {
      emit = handler;
      return () => {};
    });
    let finish: (value: FastForwardResponse) => void = () => {};
    service.fastForwardToDate.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("running"));
    await waitFor(() => expect(service.fastForwardToDate).toHaveBeenCalled());

    act(() => emit({ ...response({ kind: "cancelled" }).progress, days_done: 3 }));
    expect(screen.getByTestId("progress")).toHaveTextContent("3");

    await act(async () => finish(response({ kind: "target_reached" })));
    expect(screen.getByTestId("progress")).toHaveTextContent("7");
  });

  /** Given a double click on Start, when both land before the first commit, then one run starts. */
  it("starts one run for a double click", async () => {
    service.fastForwardToDate.mockResolvedValue(response({ kind: "target_reached" }));
    setup();
    fireEvent.click(screen.getByText("open"));
    const start = screen.getByText("start");
    fireEvent.click(start);
    fireEvent.click(start);
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("result"));
    expect(service.fastForwardToDate).toHaveBeenCalledTimes(1);
  });

  /** Given a run in progress, when the player stops it, then the backend is asked once. */
  it("asks the backend to stop at the next day boundary", async () => {
    service.fastForwardToDate.mockReturnValue(new Promise(() => {}));
    setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(service.fastForwardToDate).toHaveBeenCalled());
    fireEvent.click(screen.getByText("cancel"));
    expect(service.cancelFastForward).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("cancelling")).toHaveTextContent("true");
  });

  /** Given the board dismisses the manager during the run, then the fired flow is opened. */
  it("opens the dismissal flow when the run ends with a firing", async () => {
    service.fastForwardToDate.mockResolvedValue(response({ kind: "manager_fired" }, 4));
    const { onFired } = setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(onFired).toHaveBeenCalledTimes(1));
  });

  /** Given the backend refuses the request, then nothing ran and the form shows why. */
  it("returns to the form with the backend's reason when the request is refused", async () => {
    service.fastForwardToDate.mockRejectedValue("be.error.fastForward.targetTooFar");
    const { setGameState } = setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("configure"));
    expect(screen.getByTestId("error")).toHaveTextContent("be.error.fastForward.targetTooFar");
    expect(setGameState).not.toHaveBeenCalled();
  });

  /** Given a stop for a decision, when the player carries on, then the same target is requested. */
  it("resumes towards the original target after a stop", async () => {
    service.fastForwardToDate
      .mockResolvedValueOnce(
        response({ kind: "decisions", decisions: [{ kind: "blocker", id: "x", params: {} }] }, 2),
      )
      .mockResolvedValueOnce(response({ kind: "target_reached" }));
    setup();
    fireEvent.click(screen.getByText("open"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("result"));
    fireEvent.click(screen.getByText("resume"));
    await waitFor(() => expect(service.fastForwardToDate).toHaveBeenCalledTimes(2));
    expect(service.fastForwardToDate).toHaveBeenLastCalledWith({
      target_date: "2033-07-08",
      policy: "stop_for_decisions",
    });
  });
});
