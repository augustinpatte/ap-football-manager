/**
 * Ways of putting a failure on screen that show the player the raw error: its type name and
 * message, a Rust panic, a translation key. Anything a player reads goes through `describeError`
 * or `userErrorText` (`src/utils/errorMessage.ts`).
 */
const RAW_ERROR_DISPLAY: { name: string; pattern: RegExp }[] = [
  { name: "setXError(String(error))", pattern: /\bset\w*Error\(\s*String\(/ },
  { name: "setXError(error.message)", pattern: /\bset\w*Error\(\s*(?:err|error|e)\.message\b/ },
  { name: "{error.message} in a view", pattern: /(?<!\$)\{\s*(?:err|error|e)\.message\s*\}/ },
  {
    name: "{String(error)} in a view",
    pattern: /(?<!\$)\{\s*String\(\s*(?:err|error|e)\s*\)\s*\}/,
  },
];

/** Names of the raw-display patterns a source file uses, one entry per offending line. */
export function rawErrorDisplays(source: string): string[] {
  return source
    .split("\n")
    .flatMap((line, index) =>
      RAW_ERROR_DISPLAY.filter(({ pattern }) => pattern.test(line)).map(
        ({ name }) => `line ${index + 1}: ${name}`,
      ),
    );
}

/**
 * A screen hands a failure to `resolveBackendError`, which returns raw prose and untranslated keys
 * as they are. Screens use `failureText` / `failureReason` (`src/utils/failureText.ts`), which turn
 * anything the game has no sentence for into one safe message. A reference without a call
 * (`userErrorText(error, t, resolveBackendError)`) is how the formatter is passed and is fine.
 */
const DIRECT_RESOLVE_CALL = /\bresolveBackendError\(/;

/** Lines of a screen's source that call `resolveBackendError` directly. */
export function directResolveCalls(source: string): string[] {
  return source
    .split("\n")
    .flatMap((line, index) => (DIRECT_RESOLVE_CALL.test(line) ? [`line ${index + 1}`] : []));
}
