import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "../../i18n";
import type { GameStateData, PlayerData } from "../../store/gameStore";
import { makeLoanOffer, type LoanOfferResponseData } from "../../services/transfersService";
import { getActiveGame } from "../../services/sessionService";
import { useLoanOfferFlow } from "./useLoanOfferFlow";

vi.mock("../../services/transfersService", () => ({ makeLoanOffer: vi.fn() }));
vi.mock("../../services/sessionService", () => ({ getActiveGame: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";

const makeLoanOfferMock = vi.mocked(makeLoanOffer);
const invokeMock = vi.mocked(invoke);
const t = i18n.t.bind(i18n);

const player = { id: "p1", contract_end: "2029-06-30" } as unknown as PlayerData;
const game = { players: [player] } as unknown as GameStateData;

function response(decision: LoanOfferResponseData["decision"]): LoanOfferResponseData {
  return {
    decision,
    offer_id: "o1",
    suggested_wage_contribution_pct: null,
    suggested_end_date: null,
    suggested_buy_option_fee: null,
    is_terminal: true,
    game: { players: [] } as unknown as GameStateData,
  };
}

function mountFlow(onGameUpdate: (game: GameStateData) => void) {
  const hook = renderHook(() =>
    useLoanOfferFlow({
      gameState: game,
      loanRegistrationDate: "2026-08-01",
      onGameUpdate,
      t: t as never,
      transferWindowBlocksRegistration: false,
    }),
  );
  act(() => hook.result.current.openLoanOffer(player));
  return hook;
}

describe("useLoanOfferFlow", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    makeLoanOfferMock.mockReset();
    invokeMock.mockReset();
    vi.mocked(getActiveGame).mockReset();
  });

  /**
   * Given the backend has agreed and signed the loan
   * When a local step after the reply fails (the store update throws)
   * Then the dialog still shows the loan as accepted and no error: it never reports a failure
   *      beside a loan that was made, and the submit button stays closed
   */
  it("keeps_an_accepted_loan_accepted_when_the_local_update_throws", async () => {
    makeLoanOfferMock.mockResolvedValue(response("accepted"));
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const hook = mountFlow(() => {
      throw new Error("store exploded");
    });

    await act(async () => hook.result.current.handleMakeLoanOffer());

    expect(hook.result.current.loanResult).toBe("accepted");
    expect(hook.result.current.loanError).toBeNull();
    expect(hook.result.current.loanSubmitDisabled).toBe(true);
    consoleError.mockRestore();
  });

  /**
   * Given the backend refuses with a rule's key that names a position group
   * When the offer is sent
   * Then the dialog shows the rule in words, says nothing was changed, and shows no key
   */
  it("shows_a_readable_refusal_for_a_squad_floor_rule", async () => {
    makeLoanOfferMock.mockRejectedValue(
      "be.error.squadFloor.wouldLeaveShort?group=common.positionGroups.Goalkeeper",
    );
    const hook = mountFlow(vi.fn());

    await act(async () => hook.result.current.handleMakeLoanOffer());

    expect(hook.result.current.loanResult).toBe("error");
    expect(hook.result.current.loanError).toContain("Goalkeeper");
    expect(hook.result.current.loanError).toContain("Nothing was changed");
    expect(hook.result.current.loanError).not.toContain("be.error");
  });

  /**
   * Given the answer is lost in transit but the backend recorded the loan
   * When the offer is sent
   * Then the dialog says it went through and refreshes the game, instead of showing a failure
   */
  it("reports_a_lost_answer_for_a_loan_that_was_made_as_made", async () => {
    makeLoanOfferMock.mockRejectedValue(new Error("ipc channel closed"));
    invokeMock.mockResolvedValue({ state: "committed" });
    const refreshed = { players: [] } as unknown as GameStateData;
    vi.mocked(getActiveGame).mockResolvedValue(refreshed);
    const onGameUpdate = vi.fn();
    const hook = mountFlow(onGameUpdate);

    await act(async () => hook.result.current.handleMakeLoanOffer());

    expect(onGameUpdate).toHaveBeenCalledWith(refreshed);
    expect(hook.result.current.loanError).toBe(t("transfers.requestWentThrough"));
    // The offer may have been made: Submit stays closed, so the answer is read, not repeated.
    expect(hook.result.current.loanSubmitDisabled).toBe(true);
  });

  /**
   * Given the same button pressed twice before the first answer arrives
   * When both presses are handled
   * Then one loan offer is sent
   */
  it("sends_one_offer_for_a_double_press", async () => {
    let resolveFirst!: (value: LoanOfferResponseData) => void;
    makeLoanOfferMock.mockImplementation(
      () => new Promise<LoanOfferResponseData>((resolve) => (resolveFirst = resolve)),
    );
    const hook = mountFlow(vi.fn());

    await act(async () => {
      void hook.result.current.handleMakeLoanOffer();
      void hook.result.current.handleMakeLoanOffer();
    });
    await act(async () => resolveFirst(response("accepted")));

    expect(makeLoanOfferMock).toHaveBeenCalledTimes(1);
  });

  /**
   * Given each press is a user action of its own
   * When two offers are sent one after the other
   * Then each carries a different request id, so the backend treats them as different requests
   */
  it("sends_a_fresh_request_id_for_each_user_action", async () => {
    makeLoanOfferMock.mockResolvedValue(response("rejected"));
    const hook = mountFlow(vi.fn());

    await act(async () => hook.result.current.handleMakeLoanOffer());
    await act(async () => hook.result.current.handleMakeLoanOffer());

    const ids = makeLoanOfferMock.mock.calls.map((call) => call[4]?.id);
    expect(ids).toHaveLength(2);
    expect(ids[0]).toBeTruthy();
    expect(ids[0]).not.toBe(ids[1]);
  });
});
