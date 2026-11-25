type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

/** How the player should read a failure: a rule that said no, or something that went wrong. */
export type ErrorKind = "refusal" | "technical";

export interface ErrorDescription {
  /** What happened, in the player's language. Never a key, a query string or a stack trace. */
  text: string;
  /** What the player can rely on afterwards ("Nothing was changed."); `null` when it is not known. */
  followUp: string | null;
  kind: ErrorKind;
  /**
   * For a failure the game has no text for: a short code, shown to the player and written to the
   * log next to the raw error, so the two can be matched. `null` for a known refusal.
   */
  reference: string | null;
}

function extractErrorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }

  if (error instanceof Error) {
    return error.message;
  }

  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof (error as { message?: unknown }).message === "string"
  ) {
    return (error as { message: string }).message;
  }

  return String(error);
}

/** A dotted translation key: no spaces, starts with a letter, at least two segments. */
const KEY_SHAPE = /^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z0-9_]+)+$/;

function parseEncodedErrorMessage(message: string): {
  key: string;
  params?: Record<string, string>;
} | null {
  const trimmed = message.trim();
  const separatorIndex = trimmed.indexOf("?");
  const key = separatorIndex === -1 ? trimmed : trimmed.slice(0, separatorIndex);

  if (!KEY_SHAPE.test(key)) {
    return null;
  }

  return {
    key,
    params:
      separatorIndex === -1
        ? undefined
        : Object.fromEntries(new URLSearchParams(trimmed.slice(separatorIndex + 1)).entries()),
  };
}

/**
 * Parameter values that are themselves translation keys (a position group, a competition) are
 * translated; one the game has no text for is reduced to its last word rather than shown as a path.
 */
function readableParams(
  params: Record<string, string> | undefined,
  t: TranslateFn,
): Record<string, string> | undefined {
  if (!params) return params;
  return Object.fromEntries(
    Object.entries(params).map(([name, value]) => {
      if (!KEY_SHAPE.test(value) || value.split(".").length < 3) return [name, value];
      const text = t(value);
      return [name, text !== value ? text : (value.split(".").pop() ?? value)];
    }),
  );
}

/** A short stable code for a raw failure, so the screen and the log can be matched. */
function referenceFor(raw: string): string {
  let hash = 5381;
  for (let index = 0; index < raw.length; index += 1) {
    hash = ((hash << 5) + hash + raw.charCodeAt(index)) >>> 0;
  }
  return hash.toString(36).toUpperCase().padStart(6, "0").slice(-6);
}

/**
 * Backend keys that say an action DID happen (or may have): their text is the player's cue to look
 * and not to retry, so "Nothing was changed." is never added to them.
 */
const COMMITTED_OR_UNKNOWN_KEYS = new Set(["be.error.transactions.outcomeUnreadable"]);

/** A placeholder the message needed and the failure did not carry. */
const UNFILLED_PLACEHOLDER = /\{\{[^}]*\}\}/g;

/**
 * Describes any failure for the player. A backend refusal the game has text for reads as that
 * text, with its parameters made readable; everything else (an unknown code, English prose from
 * Rust, a JavaScript error) gets one safe message that says the outcome is unconfirmed, with a
 * reference, and the raw error goes to the log instead of the screen.
 */
export function describeError(
  error: unknown,
  t: TranslateFn,
  format?: (error: unknown) => string,
): ErrorDescription {
  const raw = extractErrorMessage(error);
  const parsed = parseEncodedErrorMessage(raw);

  if (parsed) {
    const translated = t(parsed.key, readableParams(parsed.params, t));
    if (translated !== parsed.key) {
      // `format` is the app's fuller formatter (money, dates, countries); its answer is used only
      // when it is a real rewrite and not the key or message handed back.
      const formatted = format?.(error);
      const usable = formatted && formatted !== raw && !formatted.includes("be.error.");
      return {
        text: (usable ? formatted : translated).replace(UNFILLED_PLACEHOLDER, "—"),
        followUp: COMMITTED_OR_UNKNOWN_KEYS.has(parsed.key) ? null : t("errors.nothingChanged"),
        kind: "refusal",
        reference: null,
      };
    }
  }

  const reference = referenceFor(raw);
  console.error(`[error ${reference}]`, error);
  return {
    text: t("errors.unexpected", { reference }),
    followUp: null,
    kind: "technical",
    reference,
  };
}

export function getErrorMessage(error: unknown): string {
  return extractErrorMessage(error);
}

/**
 * The text of a failure for a line of UI: [`describeError`] without its follow-up. Prefer
 * `describeError` where the screen can also say what the failure changed.
 */
export function resolveTranslatedErrorMessage(error: unknown, t: TranslateFn): string {
  return describeError(error, t).text;
}

/**
 * What a line of UI says about a failure: its text and, when known, what the failure left behind
 * ("Nothing was changed."), as one sentence run.
 */
export function userErrorText(
  error: unknown,
  t: TranslateFn,
  format?: (error: unknown) => string,
): string {
  const described = describeError(error, t, format);
  return described.followUp ? `${described.text} ${described.followUp}` : described.text;
}
