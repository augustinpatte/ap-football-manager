import { useRef, useState } from "react";
import type { GameStateData, PlayerData } from "../../store/gameStore";
import { makeLoanOffer, type LoanOfferResponseData } from "../../services/transfersService";
import { submitTransferCommand } from "./transferSubmission";
import {
  buildLoanPeriodOptions,
  formatTransferFeeInput,
  getDefaultLoanPeriodId,
  getLoanPeriodIdForEndDate,
  type LoanPeriodOptionId,
  parseTransferFeeInput,
} from "./TransfersTab.helpers";
import type { TFunction } from "i18next";
interface Input {
  gameState: GameStateData;
  loanRegistrationDate: string;
  onGameUpdate: ((game: GameStateData) => void) | undefined;
  t: TFunction<"translation", undefined>;
  transferWindowBlocksRegistration: boolean;
}
export function useLoanOfferFlow({
  gameState,
  loanRegistrationDate,
  onGameUpdate,
  t,
  transferWindowBlocksRegistration,
}: Input) {
  // The id, not the player: read from the game state each render, so the dialog sees the player
  // as the game now has him rather than as he was when it opened.
  const [loanTargetId, setLoanTargetId] = useState<string | null>(null);
  const loanTarget =
    loanTargetId === null
      ? null
      : (gameState.players.find((player) => player.id === loanTargetId) ?? null);
  const [loanPeriodId, setLoanPeriodId] = useState<LoanPeriodOptionId | "">(
    getDefaultLoanPeriodId(loanRegistrationDate, null),
  );
  const [loanWageContributionPct, setLoanWageContributionPct] = useState(100);
  const [loanBuyOptionEnabled, setLoanBuyOptionEnabled] = useState(false);
  const [loanBuyOptionFee, setLoanBuyOptionFee] = useState("");
  const [loanLoading, setLoanLoading] = useState(false);
  // A lost answer for an offer that may have been made: Submit stays closed until the dialog is reopened.
  const [loanUnresolved, setLoanUnresolved] = useState(false);
  const [loanError, setLoanError] = useState<string | null>(null);
  const [loanResult, setLoanResult] = useState<LoanOfferResponseData["decision"] | "error" | null>(
    null,
  );
  const [loanSuggestedTerms, setLoanSuggestedTerms] = useState<{
    wageContributionPct: number;
    endDate: string;
    buyOptionFee?: number | null;
  } | null>(null);
  const openLoanOffer = (player: PlayerData) => {
    setLoanTargetId(player.id);
    setLoanPeriodId(getDefaultLoanPeriodId(loanRegistrationDate, player.contract_end));
    setLoanWageContributionPct(100);
    setLoanBuyOptionEnabled(false);
    setLoanBuyOptionFee("");
    setLoanError(null);
    setLoanResult(null);
    setLoanSuggestedTerms(null);
    setLoanUnresolved(false);
  };
  const closeLoanOffer = () => {
    setLoanTargetId(null);
    setLoanPeriodId(getDefaultLoanPeriodId(loanRegistrationDate, null));
    setLoanWageContributionPct(100);
    setLoanBuyOptionEnabled(false);
    setLoanBuyOptionFee("");
    setLoanError(null);
    setLoanResult(null);
    setLoanSuggestedTerms(null);
    setLoanUnresolved(false);
  };
  // One press is one request: a second press while the first is waiting would otherwise send the
  // same offer twice before the button had time to close.
  const submitting = useRef(false);
  const handleMakeLoanOffer = async () => {
    if (!loanTarget || !selectedLoanPeriodOption || submitting.current) return;

    submitting.current = true;
    setLoanLoading(true);
    setLoanError(null);
    setLoanResult(null);
    setLoanSuggestedTerms(null);

    try {
      const submission = await submitTransferCommand(
        (requestId) =>
          makeLoanOffer(
            loanTarget.id,
            selectedLoanPeriodOption.endDate,
            Math.max(0, Math.min(100, Math.round(loanWageContributionPct))),
            loanBuyOptionEnabled ? parseTransferFeeInput(loanBuyOptionFee) : null,
            requestId,
          ),
        { onGameUpdate, t },
      );
      if (submission.kind === "stopped") {
        setLoanResult("error");
        setLoanError(submission.message);
        setLoanUnresolved(submission.unresolved);
        return;
      }
      applyLoanResponse(submission.value);
    } finally {
      submitting.current = false;
      setLoanLoading(false);
    }
  };
  /**
   * The backend has answered, so the offer is on the books whatever happens here. Nothing below can
   * turn that into an error: a local step that throws is logged and the answer stands, because an
   * error shown beside a loan that was made invites a second, duplicate offer.
   */
  const applyLoanResponse = (response: LoanOfferResponseData) => {
    setLoanResult(response.decision);
    try {
      onGameUpdate?.(response.game);
      if (response.decision === "counter_offer" && loanTarget && selectedLoanPeriodOption) {
        applySuggestedLoanTerms(response, loanTarget, selectedLoanPeriodOption.endDate);
      }
    } catch (error) {
      console.error("A step after the loan answer failed; the answer stands", error);
    }
  };
  const applySuggestedLoanTerms = (
    response: LoanOfferResponseData,
    target: PlayerData,
    fallbackEndDate: string,
  ) => {
    setLoanSuggestedTerms({
      wageContributionPct: response.suggested_wage_contribution_pct ?? loanWageContributionPct,
      endDate: response.suggested_end_date ?? fallbackEndDate,
      buyOptionFee: response.suggested_buy_option_fee,
    });
    if (response.suggested_wage_contribution_pct !== null) {
      setLoanWageContributionPct(
        Math.max(0, Math.min(100, Math.round(response.suggested_wage_contribution_pct))),
      );
    }
    if (response.suggested_end_date) {
      setLoanPeriodId(
        getLoanPeriodIdForEndDate(
          loanRegistrationDate,
          target.contract_end,
          response.suggested_end_date,
        ),
      );
    }
    if (response.suggested_buy_option_fee !== null) {
      setLoanBuyOptionEnabled(true);
      setLoanBuyOptionFee(formatTransferFeeInput(response.suggested_buy_option_fee));
    } else {
      setLoanBuyOptionEnabled(false);
      setLoanBuyOptionFee("");
    }
  };
  const parsedLoanBuyOptionFee = loanBuyOptionEnabled
    ? parseTransferFeeInput(loanBuyOptionFee)
    : null;
  const loanPeriodOptions = loanTarget
    ? buildLoanPeriodOptions(loanRegistrationDate, loanTarget.contract_end)
    : [];
  const selectedLoanPeriodOption =
    loanPeriodOptions.find((option) => option.id === loanPeriodId && !option.disabled) ?? null;
  const loanSubmitDisabled =
    loanLoading ||
    loanUnresolved ||
    !selectedLoanPeriodOption ||
    loanResult === "accepted" ||
    transferWindowBlocksRegistration ||
    (loanBuyOptionEnabled && (parsedLoanBuyOptionFee === null || parsedLoanBuyOptionFee <= 0));
  return {
    loanTarget,
    loanPeriodId,
    setLoanPeriodId,
    loanWageContributionPct,
    setLoanWageContributionPct,
    loanBuyOptionEnabled,
    setLoanBuyOptionEnabled,
    loanBuyOptionFee,
    setLoanBuyOptionFee,
    loanLoading,
    loanError,
    loanResult,
    loanSuggestedTerms,
    openLoanOffer,
    closeLoanOffer,
    handleMakeLoanOffer,
    loanPeriodOptions,
    selectedLoanPeriodOption,
    loanSubmitDisabled,
  };
}
