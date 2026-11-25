import type { GameStateData } from "../../store/gameStore";
import { getActiveGame } from "../../services/sessionService";
import { submitTransaction } from "../../services/transactionService";
import { resolveBackendError } from "../../utils/backendI18n";
import { userErrorText } from "../../utils/errorMessage";

type Translate = (key: string, params?: Record<string, string | number>) => string;

/** What a transfer dialog does next: carry on with the answer, or show a message and stop. */
export type TransferSubmission<T> =
  | { kind: "done"; value: T }
  /** `unresolved`: the action may have happened (a lost answer); the dialog must not invite a resend. */
  | { kind: "stopped"; message: string; unresolved: boolean };

/**
 * Sends one user action that moves money or players and turns every way it can end into either its
 * answer or one readable message. The dialog never decides "failed" from a mere rejection: a
 * refusal is worded by the backend's own key (and says nothing was changed), a lost answer is
 * checked against the backend's record first, and one that did go through refreshes the game and
 * says so instead of showing an error beside a transfer that happened.
 */
export async function submitTransferCommand<T>(
  run: (requestId: string) => Promise<T>,
  options: { onGameUpdate?: (game: GameStateData) => void; t: Translate },
): Promise<TransferSubmission<T>> {
  const outcome = await submitTransaction(run);
  switch (outcome.kind) {
    case "done":
      return { kind: "done", value: outcome.value };
    case "went-through": {
      try {
        options.onGameUpdate?.(await getActiveGame());
      } catch (error) {
        console.error("Could not refresh the game after a lost answer", error);
      }
      return {
        kind: "stopped",
        message: options.t("transfers.requestWentThrough"),
        unresolved: true,
      };
    }
    case "refused":
    case "failed":
    case "unconfirmed":
      return {
        kind: "stopped",
        message: userErrorText(outcome.error, options.t, resolveBackendError),
        unresolved: outcome.kind === "unconfirmed",
      };
  }
}
