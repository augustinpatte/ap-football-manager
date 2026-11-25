import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { newRequestId, submitTransaction } from "./transactionService";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

describe("submitTransaction", () => {
  beforeEach(() => invokeMock.mockReset());

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
    invokeMock.mockResolvedValue({ state: "committed" });

    const outcome = await submitTransaction(async () => {
      throw new Error("ipc channel closed");
    });

    expect(outcome).toMatchObject({ kind: "went-through" });
    expect(invokeMock).toHaveBeenCalledWith("get_transaction_status", {
      requestId: expect.any(String),
    });
  });

  /**
   * Given a lost answer for a command the backend refused
   * When it is submitted
   * Then the refusal it recorded is what the caller reads
   */
  it("recovers_the_recorded_refusal_of_a_lost_answer", async () => {
    invokeMock.mockResolvedValue({ state: "refused", error: "be.error.transfers.offerNotPending" });

    const outcome = await submitTransaction(async () => {
      throw new Error("ipc channel closed");
    });

    expect(outcome).toMatchObject({ kind: "refused", error: "be.error.transfers.offerNotPending" });
  });

  /**
   * Given a failure for a command the backend never saw
   * When it is submitted
   * Then it is a failure with nothing changed, safe to try again
   */
  it("reports_a_command_the_backend_never_ran_as_failed", async () => {
    invokeMock.mockResolvedValue({ state: "unknown" });

    const outcome = await submitTransaction(async () => {
      throw new Error("ipc channel closed");
    });

    expect(outcome).toMatchObject({ kind: "failed" });
  });

  /**
   * Given a lost answer and a status lookup that cannot be read
   * When it is submitted
   * Then the result is "unconfirmed": the caller must not invite a blind retry
   */
  it("reports_unconfirmed_when_the_status_cannot_be_read", async () => {
    // A reply that cannot be read stands for "the backend could not be asked".
    invokeMock.mockResolvedValue(undefined);

    const outcome = await submitTransaction(async () => {
      throw new Error("ipc channel closed");
    });

    expect(outcome).toMatchObject({ kind: "unconfirmed" });
  });

  /**
   * Given two submissions
   * When each sends a request id
   * Then the ids differ, and a repeat that reuses the first id is possible by passing it
   */
  it("gives_each_submission_its_own_request_id", async () => {
    const ids: string[] = [];
    await submitTransaction(async (requestId) => ids.push(requestId));
    await submitTransaction(async (requestId) => ids.push(requestId));
    const reused = await submitTransaction(async (requestId) => requestId, ids[0]);

    expect(ids[0]).not.toBe(ids[1]);
    expect(reused).toMatchObject({ value: ids[0] });
    expect(newRequestId()).toMatch(/\S{8,}/);
  });
});
