import { invoke } from "@tauri-apps/api/core";

/** What the backend knows of a request id: see `get_transaction_status`. */
type TransactionStatus =
  | { state: "unknown" }
  | { state: "committed" }
  | { state: "refused"; error: string };

/**
 * How a submitted command ended.
 *
 * - `done`        it ran and answered.
 * - `refused`     the backend said no, in a rule's words, having changed nothing.
 * - `went-through` the answer was lost on the way, but the backend recorded that it ran.
 * - `failed`      the answer was lost and the backend never ran it: nothing changed, retry is safe.
 * - `unconfirmed` the answer was lost and the backend could not be asked: do not invite a retry.
 */
export type TransactionOutcome<T> =
  | { kind: "done"; value: T; requestId: string }
  | { kind: "refused"; error: unknown; requestId: string }
  | { kind: "went-through"; requestId: string }
  | { kind: "failed"; error: unknown; requestId: string }
  | { kind: "unconfirmed"; error: unknown; requestId: string };

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

async function getTransactionStatus(requestId: string): Promise<TransactionStatus> {
  return await invoke<TransactionStatus>("get_transaction_status", { requestId });
}

/**
 * Runs a command that moves money or players under a request id, and never lets a lost answer turn
 * into a guess. The id goes to the backend with the command; if the command then fails in a way
 * that is not one of the backend's own refusals (a transport error), the backend is asked what
 * became of that id before anything is reported, so a loan that was signed is never shown as an
 * error and a failed one is never silently retried into a duplicate.
 *
 * Pass `requestId` again to repeat the same user action; omit it for a new one.
 */
export async function submitTransaction<T>(
  run: (requestId: string) => Promise<T>,
  requestId: string = newRequestId(),
): Promise<TransactionOutcome<T>> {
  try {
    return { kind: "done", value: await run(requestId), requestId };
  } catch (error) {
    if (isBackendRefusal(error)) return { kind: "refused", error, requestId };
    try {
      const status = await getTransactionStatus(requestId);
      if (status.state === "committed") return { kind: "went-through", requestId };
      if (status.state === "refused") return { kind: "refused", error: status.error, requestId };
      return { kind: "failed", error, requestId };
    } catch {
      return { kind: "unconfirmed", error, requestId };
    }
  }
}
