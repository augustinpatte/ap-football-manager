import { invoke } from "@tauri-apps/api/core";

/**
 * What a command carries so the backend can tell one user action from another and one career from
 * another: the id of the action and the `career_token` of the game the screen was showing.
 */
export interface RequestStamp {
  id: string;
  careerToken?: number;
}

/** What the backend knows of a request id: see `settle_transaction`. */
type TransactionStatus =
  | { state: "unknown" }
  | { state: "in_progress" }
  | { state: "committed" }
  | { state: "refused"; error: string }
  | { state: "abandoned" }
  | { state: "career_changed" };

/**
 * How a submitted command ended.
 *
 * - `done`          it ran and answered.
 * - `refused`       the backend said no, in a rule's words, having changed nothing.
 * - `went-through`  the answer was lost on the way, but the backend recorded that it ran.
 * - `failed`        the answer was lost and the backend closed the id before it ran: nothing changed,
 *                   a retry is safe but needs a NEW request id (this one is closed).
 * - `in-progress`   the backend still had the request when we stopped waiting: it may yet commit.
 *                   Neither "nothing happened" nor "done"; do not retry, check later with this id.
 * - `unconfirmed`   the answer was lost and the backend could not be asked: do not invite a retry.
 * - `career-changed` the career was replaced meanwhile; nothing can be confirmed about the old one
 *                   and nothing from it may be shown in the new one.
 */
export type TransactionOutcome<T> =
  | { kind: "done"; value: T; requestId: string }
  | { kind: "refused"; error: unknown; requestId: string }
  | { kind: "went-through"; requestId: string }
  | { kind: "failed"; error: unknown; requestId: string }
  | { kind: "in-progress"; error: unknown; requestId: string }
  | { kind: "unconfirmed"; error: unknown; requestId: string }
  | { kind: "career-changed"; requestId: string };

export interface SubmitOptions {
  /** `career_token` of the game on screen when the user pressed the button. */
  careerToken?: number;
  /**
   * Reads the token of the game on screen now. When it differs from `careerToken` by the time an
   * answer arrives, the answer belongs to a career that is gone and is dropped, not delivered.
   */
  currentCareerToken?: () => number | undefined;
  /** Pauses between two looks at an in-progress request. Injectable so tests need no real time. */
  pause?: (milliseconds: number) => Promise<void>;
}

/** How long a lost answer is chased while the backend still holds the request (about 5 seconds). */
const RECONCILE_PAUSES_MS = [100, 200, 400, 800, 1000, 1000, 1000, 1000];

export function newRequestId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ?? `req-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
}

/** A rule's refusal: the backend sends these as translation keys, and sends them before it writes. */
function isBackendRefusal(error: unknown): boolean {
  const text = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return text.startsWith("be.error.");
}

const defaultPause = (milliseconds: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, milliseconds));

async function settleTransaction(stamp: RequestStamp): Promise<TransactionStatus> {
  const status = await invoke<TransactionStatus | undefined>("settle_transaction", {
    requestId: stamp.id,
    careerToken: stamp.careerToken,
  });
  // An unreadable reply stands for "the backend could not be asked".
  if (!status || typeof status.state !== "string") throw new Error("unreadable transaction status");
  return status;
}

/**
 * Asks the backend what became of a request whose answer was lost, until it can say. A request the
 * backend has never seen is closed by the question (so a late delivery cannot run behind our back);
 * one it is still executing is asked about again after a short pause.
 */
async function reconcile<T>(
  stamp: RequestStamp,
  error: unknown,
  pause: (milliseconds: number) => Promise<void>,
): Promise<TransactionOutcome<T>> {
  const requestId = stamp.id;
  try {
    for (const wait of [...RECONCILE_PAUSES_MS, null]) {
      const status = await settleTransaction(stamp);
      switch (status.state) {
        case "committed":
          return { kind: "went-through", requestId };
        case "refused":
          return { kind: "refused", error: status.error, requestId };
        case "abandoned":
          return { kind: "failed", error, requestId };
        case "career_changed":
          return { kind: "career-changed", requestId };
        case "unknown":
          // Settling never answers "unknown"; if it does, nothing can be concluded.
          return { kind: "unconfirmed", error, requestId };
        case "in_progress":
          if (wait === null) return { kind: "in-progress", error, requestId };
          await pause(wait);
      }
    }
  } catch {
    return { kind: "unconfirmed", error, requestId };
  }
  return { kind: "in-progress", error, requestId };
}

/**
 * Runs a command that moves money or players under a request stamp, and never lets a lost answer
 * turn into a guess. The stamp goes to the backend with the command; if the command then fails in a
 * way that is not one of the backend's own refusals (a transport error), the backend is asked what
 * became of that id before anything is reported, so a loan that was signed is never shown as an
 * error, a request still executing is never reported as "nothing happened", and a failed one is
 * never silently retried into a duplicate.
 *
 * Each call is a new user action with a new id. A retry after `failed` is a new action.
 */
export async function submitTransaction<T>(
  run: (request: RequestStamp) => Promise<T>,
  options: SubmitOptions = {},
): Promise<TransactionOutcome<T>> {
  const stamp: RequestStamp = { id: newRequestId(), careerToken: options.careerToken };
  const careerChanged = () =>
    options.currentCareerToken !== undefined &&
    options.currentCareerToken() !== options.careerToken;
  try {
    const value = await run(stamp);
    // An answer from a career that has since been replaced must not reach the new one.
    if (careerChanged()) return { kind: "career-changed", requestId: stamp.id };
    return { kind: "done", value, requestId: stamp.id };
  } catch (error) {
    if (isBackendRefusal(error)) {
      return careerChanged()
        ? { kind: "career-changed", requestId: stamp.id }
        : { kind: "refused", error, requestId: stamp.id };
    }
    return reconcile<T>(stamp, error, options.pause ?? defaultPause);
  }
}
