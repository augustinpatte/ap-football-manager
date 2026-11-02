import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { FastForwardState } from "../../hooks/useFastForward";
import type { FastForwardResponse } from "../../services/fastForwardService";
import type { GameStateData } from "../../store/gameStore";
import FastForwardDialog from "./FastForwardDialog";

vi.mock("react-i18next", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-i18next")>()),
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options ? `${key} ${JSON.stringify(options)}` : key,
    i18n: { language: "en", resolvedLanguage: "en" },
  }),
}));

function state(overrides: Partial<FastForwardState> = {}): FastForwardState {
  return {
    phase: "configure",
    today: "2033-07-01",
    target: "2033-07-08",
    policy: "stop_for_decisions",
    progress: null,
    isCancelling: false,
    result: null,
    error: null,
    targetIsValid: true,
    daysAhead: 7,
    open: vi.fn(),
    close: vi.fn(),
    setTarget: vi.fn(),
    setPolicy: vi.fn(),
    start: vi.fn().mockResolvedValue(undefined),
    cancel: vi.fn(),
    resume: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function result(
  stop: FastForwardResponse["stop"],
  extra: Partial<FastForwardResponse> = {},
): FastForwardResponse {
  return {
    progress: {
      start_date: "2033-07-01",
      current_date: "2033-07-05",
      target_date: "2033-07-08",
      days_done: 4,
      days_total: 7,
      user_matches_played: 2,
      seasons_rolled_over: 0,
    },
    stop,
    season_summaries: [],
    passed_decisions: [],
    game: {} as GameStateData,
    results: [],
    ...extra,
  };
}

describe("FastForwardDialog", () => {
  /** Given the dialog closed, then nothing is drawn. */
  it("draws nothing while closed", () => {
    const { container } = render(<FastForwardDialog state={state({ phase: "closed" })} />);
    expect(container).toBeEmptyDOMElement();
  });

  /** Given the form, when a quick pick is chosen, then the target moves by that many days. */
  it("sets the target from the quick picks", () => {
    const current = state();
    render(<FastForwardDialog state={current} />);
    expect(screen.getByRole("dialog", { name: "fastForward.title" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "fastForward.preset.month" }));
    expect(current.setTarget).toHaveBeenCalledWith("2033-07-31");
    fireEvent.click(screen.getByRole("button", { name: "fastForward.preset.threeMonths" }));
    expect(current.setTarget).toHaveBeenLastCalledWith("2033-09-29");
  });

  /** Given each interruption policy, then it is a radio the player can choose by name. */
  it("lets the player choose when to stop", () => {
    const current = state();
    render(<FastForwardDialog state={current} />);
    expect(screen.getByRole("radio", { name: /fastForward.policy.decisions/ })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: /fastForward.policy.nextMatch/ }));
    expect(current.setPolicy).toHaveBeenCalledWith("stop_at_next_match");
    fireEvent.click(screen.getByRole("radio", { name: /fastForward.policy.target/ }));
    expect(current.setPolicy).toHaveBeenLastCalledWith("to_target_date");
  });

  /** Given a target that is not after today, then Start is disabled and the reason is announced. */
  it("refuses to start on an invalid target", () => {
    render(<FastForwardDialog state={state({ targetIsValid: false, daysAhead: 0 })} />);
    expect(screen.getByRole("button", { name: "fastForward.start" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("fastForward.invalidTarget");
  });

  /** Given a valid target, when Start is pressed, then the run starts. */
  it("starts the run", () => {
    const current = state();
    render(<FastForwardDialog state={current} />);
    fireEvent.click(screen.getByRole("button", { name: "fastForward.start" }));
    expect(current.start).toHaveBeenCalledTimes(1);
  });

  /** Given a run in progress, then progress is a labelled progress bar and Stop asks to cancel. */
  it("shows progress and a stop control while running", () => {
    const current = state({
      phase: "running",
      progress: {
        start_date: "2033-07-01",
        current_date: "2033-07-04",
        target_date: "2033-07-08",
        days_done: 3,
        days_total: 6,
        user_matches_played: 1,
        seasons_rolled_over: 0,
      },
    });
    render(<FastForwardDialog state={current} />);
    expect(screen.getByRole("progressbar", { name: "fastForward.title" })).toHaveAttribute(
      "aria-valuenow",
      "50",
    );
    fireEvent.click(screen.getByRole("button", { name: "fastForward.stop" }));
    expect(current.cancel).toHaveBeenCalledTimes(1);
  });

  /** Given a cancel already requested, then Stop cannot be pressed twice. */
  it("disables Stop once a cancel is on its way", () => {
    render(<FastForwardDialog state={state({ phase: "running", isCancelling: true })} />);
    expect(screen.getByRole("button", { name: "fastForward.stopping" })).toBeDisabled();
  });

  /** Given a run that stopped for decisions, then they are listed and the run can be resumed. */
  it("lists what needs the player and offers to carry on", () => {
    const current = state({
      phase: "result",
      result: result({
        kind: "decisions",
        decisions: [
          { kind: "incoming_transfer_offer", id: "o1", params: {} },
          { kind: "transfer_deadline", id: "2033-08-31", params: {} },
        ],
      }),
    });
    render(<FastForwardDialog state={current} />);
    expect(screen.getByRole("heading")).toHaveTextContent("fastForward.result.decisions");
    expect(screen.getByText("fastForward.decision.incomingTransferOffer")).toBeInTheDocument();
    expect(screen.getByText("fastForward.decision.transferDeadline")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "fastForward.resume" }));
    expect(current.resume).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "common.close" }));
    expect(current.close).toHaveBeenCalledTimes(1);
  });

  /** Given a target-date run that passed a deadline, then the report names it with its date. */
  it("reports what a run to the date passed along the way", () => {
    render(
      <FastForwardDialog
        state={state({
          phase: "result",
          result: result(
            { kind: "target_reached" },
            {
              passed_decisions: [
                {
                  date: "2033-08-31",
                  decision: { kind: "transfer_deadline", id: "2033-08-31", params: {} },
                },
              ],
              progress: { ...result({ kind: "target_reached" }).progress, seasons_rolled_over: 1 },
            },
          ),
        })}
      />,
    );
    expect(screen.getByText(/fastForward.decision.transferDeadline/)).toHaveTextContent(
      "2033-08-31",
    );
    expect(screen.getByText(/fastForward.seasonsRolled/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "fastForward.resume" })).toBeNull();
  });

  /** Given a refusal from the backend on the form, then it is announced. */
  it("announces a backend refusal beside the form", () => {
    render(<FastForwardDialog state={state({ error: "be.error.fastForward.alreadyRunning" })} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
