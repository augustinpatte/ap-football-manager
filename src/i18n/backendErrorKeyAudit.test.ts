import { readdirSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { backendErrorKeysIn, collectBackendErrorKeys } from "./backendErrorAudit.helpers";
import { hasLocaleKey } from "./i18nTestHelpers";
import en from "./locales/en.json";

/**
 * Every refusal the backend can send to the screen has text the player can read. The locale files
 * are checked against English elsewhere (`localeCoverage`), so English is the gate here: a key with
 * no English sentence would reach the player as the key itself or as a generic message.
 */

const RUST_ROOTS = [
  "src-tauri/src",
  ...readdirSync("src-tauri/crates").map((crate) => `src-tauri/crates/${crate}/src`),
].filter((root) => {
  try {
    readdirSync(root);
    return true;
  } catch {
    return false;
  }
});

describe("backend error keys", () => {
  /**
   * Given every player-facing Rust source file
   * When its `be.error.*` literals are collected
   * Then each has an English sentence in en.json
   */
  it("every_key_the_backend_can_send_has_text_in_english", () => {
    const uses = collectBackendErrorKeys(RUST_ROOTS);
    const untranslated = uses.filter(({ key }) => !hasLocaleKey(en, key));

    // The scan must actually have looked at the code: a path typo would otherwise pass vacuously.
    expect(uses.length).toBeGreaterThan(200);
    expect(untranslated.map(({ key, file }) => `${key}  (${file})`)).toEqual([]);
  });

  /**
   * Given a source file with a key that has no text, a prefix and a test-only key
   * When it is scanned
   * Then only the production key is found: this is the fixture that proves the audit can fail
   */
  it("finds_production_keys_and_ignores_prefixes_and_test_modules", () => {
    const source = [
      'const A: &str = "be.error.audit.missingEverywhere";',
      'const PREFIX: &str = "be.error.audit.family.";',
      "#[cfg(test)]",
      "mod tests {",
      '    const T: &str = "be.error.audit.onlyInTests";',
      "}",
    ].join("\n");

    expect(backendErrorKeysIn(source)).toEqual(["be.error.audit.missingEverywhere"]);
    expect(hasLocaleKey(en, "be.error.audit.missingEverywhere")).toBe(false);
  });
});
