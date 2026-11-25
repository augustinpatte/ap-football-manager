import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { GameStateData } from "../../store/gameStore";
import {
  exerciseLoanBuyOption,
  respondToOffer,
  respondToLoanOffer,
} from "../../services/transfersService";
import type { RequestStamp } from "../../services/transactionService";
import { submitTransferCommand } from "./transferSubmission";
interface Input {
  onGameUpdate: ((game: GameStateData) => void) | undefined;
}
export function useTransferResponses({ onGameUpdate }: Input) {
  const { t } = useTranslation();
  const [responseError, setResponseError] = useState<string | null>(null);
  // One answer at a time: a second press on Accept before the first returns would send a second
  // request for an offer that the first is already settling.
  const answering = useRef(false);
  const respond = async (
    failureLog: string,
    action: (request: RequestStamp) => Promise<GameStateData>,
  ): Promise<void> => {
    if (answering.current) return;
    answering.current = true;
    setResponseError(null);
    try {
      const submission = await submitTransferCommand(action, { onGameUpdate, t });
      if (submission.kind === "stopped") {
        console.error(failureLog, submission.message);
        setResponseError(submission.message);
        return;
      }
      try {
        onGameUpdate?.(submission.value);
      } catch (error) {
        console.error("A step after the answer failed; the answer stands", error);
      }
    } finally {
      answering.current = false;
    }
  };
  const handleRespondOffer = (playerId: string, offerId: string, accept: boolean) =>
    respond("Failed to respond to offer:", (request) =>
      respondToOffer(playerId, offerId, accept, request),
    );
  const handleRespondLoanOffer = (playerId: string, offerId: string, accept: boolean) =>
    respond("Failed to respond to loan offer:", (request) =>
      respondToLoanOffer(playerId, offerId, accept, request),
    );
  const handleExerciseLoanBuyOption = (playerId: string) =>
    respond("Failed to exercise loan buy option:", (request) =>
      exerciseLoanBuyOption(playerId, request),
    );
  return {
    responseError,
    handleRespondOffer,
    handleRespondLoanOffer,
    handleExerciseLoanBuyOption,
  };
}
