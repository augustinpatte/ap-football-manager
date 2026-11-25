import { describe, expect, it } from "vitest";

import { directResolveCalls, rawErrorDisplays } from "./errorDisplayAudit.helpers";

/**
 * No screen prints a raw failure. The audit reads every non-test source file under `src/` and
 * reports each line that hands an error's own text to the UI. A dev-only page may be excused, and
 * only by name, with its reason.
 */

const sources = import.meta.glob(["./**/*.ts", "./**/*.tsx"], {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

/** The two modules that define how a failure is worded may call the resolver themselves. */
const MAY_CALL_THE_RESOLVER = ["./utils/backendI18n.ts", "./utils/failureText.ts"];

/** Developer pages no player reaches. */
const EXCUSED: Record<string, string> = {
  "./pages/SimLab.tsx": "the simulation lab is a developer tool, not part of a career",
  "./dev/benchUi.ts": "the in-app benchmark harness reports to a developer",
};

function isProduction(path: string): boolean {
  return (
    !/\.test\.tsx?$/.test(path) &&
    !path.includes("/test-utils/") &&
    path !== "./errorDisplayAudit.helpers.ts" &&
    !(path in EXCUSED)
  );
}

describe("raw error display", () => {
  /**
   * Given every production source file of the frontend
   * When it is scanned for lines that show an error's own text
   * Then there are none
   */
  it("no_screen_shows_the_raw_text_of_a_failure", () => {
    const paths = Object.keys(sources).filter(isProduction);
    const findings = paths.flatMap((path) =>
      rawErrorDisplays(sources[path]).map((finding) => `${path} ${finding}`),
    );

    expect(paths.length).toBeGreaterThan(300);
    expect(findings).toEqual([]);
  });

  /**
   * Given every production source file of the frontend
   * When it is scanned for a direct `resolveBackendError(...)` call
   * Then only the modules that define the wording call it: a screen showing a failure goes through
   *      `failureText`/`failureReason`, so no raw prose or untranslated key can reach the player
   */
  it("no_screen_hands_a_failure_to_the_raw_resolver", () => {
    const findings = Object.keys(sources)
      .filter(isProduction)
      .filter((path) => !MAY_CALL_THE_RESOLVER.includes(path))
      .flatMap((path) => directResolveCalls(sources[path]).map((line) => `${path} ${line}`));

    expect(findings).toEqual([]);
  });

  /**
   * Given a line that calls the resolver and one that only passes it along
   * When they are scanned
   * Then only the call is reported: the fixture that proves the rule can fail
   */
  it("reports_a_direct_call_and_passes_a_formatter_reference", () => {
    expect(directResolveCalls("setError(resolveBackendError(err));")).toHaveLength(1);
    expect(directResolveCalls("userErrorText(error, t, resolveBackendError)")).toEqual([]);
  });

  /**
   * Given lines that put a raw error on screen and lines that do not
   * When they are scanned
   * Then only the raw ones are reported: the fixture that proves the audit can fail
   */
  it("reports_raw_displays_and_passes_resolved_ones", () => {
    const bad = [
      "setYouthSearchError(String(err));",
      "setError(err.message);",
      "<p>{error.message}</p>",
      "<p>{String(err)}</p>",
    ].join("\n");
    const good = [
      "setYouthSearchError(userErrorText(err, t));",
      "console.error(String(err));",
      "<p>{userErrorText(error, t)}</p>",
    ].join("\n");

    expect(rawErrorDisplays(bad)).toHaveLength(4);
    expect(rawErrorDisplays(good)).toEqual([]);
  });

  /**
   * Given each excuse in the allow-list
   * When the files are listed
   * Then every excused file still exists, so a rename cannot leave a stale excuse behind
   */
  it("every_excused_file_exists", () => {
    for (const path of Object.keys(EXCUSED)) expect(sources[path], path).toBeDefined();
  });
});
