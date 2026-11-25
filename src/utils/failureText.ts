import i18n from "i18next";
import { resolveBackendError } from "./backendI18n";
import { describeError, userErrorText } from "./errorMessage";

const translate = (key: string, params?: Record<string, string | number>): string =>
  String(i18n.t(key, params ?? {}));

/**
 * What a screen says about a failure it caught, in the language the player chose: the game's own
 * sentence for a refusal (with what it left behind: "Nothing was changed."), and one safe message
 * with a reference for anything else. The raw error goes to the log, never to the screen.
 *
 * For callers without a `t` of their own (error banners, helpers outside a component).
 */
export function failureText(error: unknown): string {
  return userErrorText(error, translate, resolveBackendError);
}

/**
 * The same, as a clause to put inside another sentence ("Failed to load saves: {{error}}"): the
 * reason alone, without the trailing "Nothing was changed.".
 */
export function failureReason(error: unknown): string {
  return describeError(error, translate, resolveBackendError).text;
}
