import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { newRequestId, submitTransaction } from "./transactionService";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const transportFailure = () => {
  throw new Error("ipc channel closed");
};
const noPause = async () => {};

/** The backend answers each look at the request id with the next status in the list (the last one repeats). */
function backendSays(...statuses: unknown[]) {
  let call = 0;
  invokeMock.mockImplementation(async () => statuses[Math.min(call++, statuses.length - 1)]);
}

describe("submitTransaction", () => {
  beforeEach(() => {
    // Braces: vitest runs a function returned from a hook as its teardown, and `mockReset()` returns the mock.
    invokeMock.mockReset();
  });

  /**
   * Given a command that succeeds
   * When it is submitted
   * Then its value comes back and the backend is not asked what became of it
   */
  it("returns_the_value_of_a_command_that_succeeds", async () => {
    const outcome = await submitTransaction(async () => "answer");

    expect(outcome).toMatchObject({ kind: "done", value: "answer" });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  /**
   * Given a command the backend refuses with a translation key
   * When it is submitted
   * Then it is a refusal and nothing is asked: the backend refuses before it writes
   */
  it("treats_a_backend_refusal_as_a_refusal_without_a_status_lookup", async () => {
    const outcome = await submitTransaction(async () => {
      throw "be.error.squadFloor.wouldLeaveShort?group=common.positionGroups.Goalkeeper";
    });

    expect(outcome).toMatchObject({ kind: "refused" });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  /**
   * Given a command whose answer was lost (a transport failure) but which the backend ran
   * When it is submitted
   * Then the backend's record says it went through, and the caller is told so rather than "failed"
   */
  it("reports_a_command_that_ran_but_lost_its_answer_as_gone_through", async () => {
    backendSays({ state: "committed" });

    const outcome = await submitTransaction(transportFailure, { careerToken: 7 });

    expect(outcome).toMatchObject({ kind: "went-through" });
    expect(invokeMock).toHaveBeenCalledWith("settle_transaction", {
      requestId: expect.any(String),
      careerToken: 7,
    });
  });

  /**
   * Given a lost answer for a command the backend refused
   * When it is submitted
   * Then the refusal it recorded is what the caller reads
   */
  it("recovers_the_recorded_refusal_of_a_lost_answer", async () => {
    backendSays({ state: "refused", error: "be.error.transfers.offerNotPending" });

    const outcome = await submitTransaction(transportFailure);

    expect(outcome).toMatchObject({ kind: "refused", error: "be.error.transfers.offerNotPending" });
  });

  /**
   * Given a transport failure after which the backend closes an id it had never seen
   * When it is submitted
   * Then it is a failure with nothing changed: only "abandoned" proves the command did not run
   */
  it("reports_failed_only_when_the_backend_closed_the_id_before_it_ran", async () => {
    backendSays({ state: "abandoned" });

    const outcome = await submitTransaction(transportFailure);

    expect(outcome).toMatchObject({ kind: "failed" });
  });

  /**
   * Given the exact incident: a transport failure while the transaction is still processing, so the
   *       first look says "in progress", and the transaction then commits
   * When the answer is chased
   * Then the caller is told it went through, never that nothing changed, and the id is not resent
   */
  it("waits_for_a_request_that_is_still_executing_and_reports_its_commit", async () => {
    backendSays({ state: "in_progress" }, { state: "in_progress" }, { state: "committed" });
    const pause = vi.fn(noPause);

    const outcome = await submitTransaction(transportFailure, { pause });

    expect(outcome).toMatchObject({ kind: "went-through" });
    expect(pause).toHaveBeenCalledTimes(2);
    const ids = new Set(
      invokeMock.mock.calls.map((call) => (call[1] as { requestId: string }).requestId),
    );
    expect(ids.size).toBe(1);
  });

  /**
   * Given a request the backend still holds after every look
   * When the waiting ends
   * Then the outcome is "in progress" with the original id: not "failed", not "went through", and
   *      the caller is not told it is safe to retry
   */
  it("keeps_the_original_identity_when_the_outcome_cannot_be_established", async () => {
    backendSays({ state: "in_progress" });
    let sentId = "";

    const outcome = await submitTransaction(
      async (request) => {
        sentId = request.id;
        return transportFailure();
      },
      { pause: noPause },
    );

    expect(outcome).toMatchObject({ kind: "in-progress", requestId: sentId });
    expect(outcome.kind).not.toBe("failed");
  });

  /**
   * Given a lost answer and a status lookup that cannot be read
   * When it is submitted
   * Then the result is "unconfirmed": the caller must not invite a blind retry
   */
  it("reports_unconfirmed_when_the_status_cannot_be_read", async () => {
    invokeMock.mockResolvedValue(undefined);

    const outcome = await submitTransaction(transportFailure);

    expect(outcome).toMatchObject({ kind: "unconfirmed" });
  });

  /**
   * Given a lost answer and a backend that is not reachable at all
   * When it is submitted
   * Then the result is "unconfirmed", not "failed"
   */
  it("reports_unconfirmed_when_the_backend_cannot_be_asked", async () => {
    invokeMock.mockImplementation(() => {
      throw new Error("channel closed");
    });

    const outcome = await submitTransaction(transportFailure);

    expect(outcome).toMatchObject({ kind: "unconfirmed" });
  });

  /**
   * Given a lost answer after the career was replaced
   * When the backend says the career changed
   * Then nothing is confirmed about the request: not "went through" and not "failed"
   */
  it("never_confirms_a_request_of_an_earlier_career", async () => {
    backendSays({ state: "career_changed" });

    const outcome = await submitTransaction(transportFailure, { careerToken: 1 });

    expect(outcome).toMatchObject({ kind: "career-changed" });
  });

  /**
   * Given an answer that arrives after another career was installed on screen
   * When the screen's career token no longer matches the one the request was sent for
   * Then the answer is dropped: its game must not be shown in the new career
   */
  it("drops_an_answer_that_belongs_to_a_career_that_is_gone", async () => {
    let onScreen = 1;

    const outcome = await submitTransaction(
      async () => {
        onScreen = 2;
        return { game: "career one" };
      },
      { careerToken: 1, currentCareerToken: () => onScreen },
    );

    expect(outcome).toMatchObject({ kind: "career-changed" });
    expect(outcome).not.toHaveProperty("value");
  });

  /**
   * Given a refusal that arrives after another career was installed on screen
   * When it is submitted
   * Then it is not shown as a refusal about the new career
   */
  it("does_not_present_an_old_careers_refusal_in_the_new_career", async () => {
    let onScreen = 1;

    const outcome = await submitTransaction(
      async () => {
        onScreen = 2;
        throw "be.error.transfers.offerNotPending";
      },
      { careerToken: 1, currentCareerToken: () => onScreen },
    );

    expect(outcome).toMatchObject({ kind: "career-changed" });
  });

  /**
   * Given two submissions
   * When each sends a request stamp
   * Then the ids differ and each carries the career token it was submitted under
   */
  it("gives_each_submission_its_own_request_id_and_the_career_it_was_sent_in", async () => {
    const stamps: { id: string; careerToken?: number }[] = [];
    await submitTransaction(async (request) => stamps.push(request), { careerToken: 3 });
    await submitTransaction(async (request) => stamps.push(request), { careerToken: 3 });

    expect(stamps[0].id).not.toBe(stamps[1].id);
    expect(stamps.map((stamp) => stamp.careerToken)).toEqual([3, 3]);
    expect(newRequestId()).toMatch(/\S{8,}/);
  });
});
