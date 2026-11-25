import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { useGameStore, type GameStateData } from "../../store/gameStore";
import { submitTransferCommand } from "./transferSubmission";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const t = (key: string) => key;
const noPause = async () => {};
const transportFailure = () => {
  throw new Error("ipc channel closed");
};

function showCareer(careerToken: number) {
  useGameStore.setState({ gameState: { career_token: careerToken } as unknown as GameStateData });
}

function backendSays(...statuses: unknown[]) {
  let call = 0;
  invokeMock.mockImplementation(async (command) =>
    command === "settle_transaction"
      ? statuses[Math.min(call++, statuses.length - 1)]
      : ({ career_token: 1 } as unknown),
  );
}

describe("submitTransferCommand", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    showCareer(1);
  });

  /**
   * Given a command whose answer was lost while the backend was still executing it
   * When the backend commits during the wait
   * Then the dialog is told the request went through (and stays closed), never that nothing changed
   */
  it("reports_a_request_that_commits_after_the_answer_was_lost_as_gone_through", async () => {
    backendSays({ state: "in_progress" }, { state: "committed" });
    const onGameUpdate = vi.fn();

    const submission = await submitTransferCommand(transportFailure, {
      onGameUpdate,
      t,
      pause: noPause,
    });

    expect(submission).toEqual({
      kind: "stopped",
      message: "transfers.requestWentThrough",
      unresolved: true,
    });
    expect(onGameUpdate).toHaveBeenCalledWith({ career_token: 1 });
  });

  /**
   * Given a request the backend still holds after the whole wait
   * When the dialog reads the result
   * Then it says the request is still being processed and stays closed to a second send
   */
  it("says_the_request_is_still_running_and_does_not_invite_a_resend", async () => {
    backendSays({ state: "in_progress" });

    const submission = await submitTransferCommand(transportFailure, { t, pause: noPause });

    expect(submission).toEqual({
      kind: "stopped",
      message: "transfers.requestStillRunning",
      unresolved: true,
    });
  });

  /**
   * Given a lost answer for a request the backend closed before it ran
   * When the dialog reads the result
   * Then it says nothing was changed and Submit may open again
   */
  it("allows_a_new_attempt_only_when_the_backend_proved_the_request_never_ran", async () => {
    backendSays({ state: "abandoned" });

    const submission = await submitTransferCommand(transportFailure, { t, pause: noPause });

    expect(submission).toEqual({
      kind: "stopped",
      message: "transfers.requestNeverRan",
      unresolved: false,
    });
  });

  /**
   * Given an answer that arrives after a different career was put on screen
   * When the dialog reads the result
   * Then the answer is not delivered to the new career
   */
  it("does_not_deliver_an_answer_to_a_career_that_replaced_the_one_it_was_sent_in", async () => {
    const submission = await submitTransferCommand(
      async () => {
        showCareer(2);
        return { game: "career one" };
      },
      { t, pause: noPause },
    );

    expect(submission).toEqual({
      kind: "stopped",
      message: "transfers.requestCareerChanged",
      unresolved: true,
    });
  });

  /**
   * Given the career on screen
   * When a command is sent
   * Then the request carries that career's token
   */
  it("stamps_the_request_with_the_career_on_screen", async () => {
    showCareer(9);
    const run = vi.fn(async () => "answer");

    await submitTransferCommand(run, { t, pause: noPause });

    expect(run).toHaveBeenCalledWith({ id: expect.any(String), careerToken: 9 });
  });
});
