import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady, SUPPORTED_LANGUAGES } from "../i18n";
import { describeError, resolveTranslatedErrorMessage } from "./errorMessage";

/**
 * What a player is shown when a command fails. Real translations, every shipped language: the
 * message has to read as a sentence, name the cause, and never leak a translation key, a query
 * string, a placeholder, a stack trace or an `undefined`.
 */

const FLOOR = "be.error.squadFloor.wouldLeaveShort?group=common.positionGroups.Goalkeeper";
const LEAKS = ["be.error", "common.positionGroups", "group=", "?", "{{", "TypeError", " at "];
/** Whole words only: "nulla" (Italian for "nothing") is not a leaked null. */
const LEAKED_WORDS = /\b(undefined|null|NaN)\b/;
const expectNoLeaks = (text: string) => {
  for (const leak of LEAKS) expect(text).not.toContain(leak);
  expect(text).not.toMatch(LEAKED_WORDS);
};

beforeAll(async () => {
  await i18nReady;
});

afterAll(async () => {
  await i18n.changeLanguage("en");
});

/** The translator of the language the app is currently showing, as `useTranslation` gives it. */
const currentT = () => i18n.t.bind(i18n);

describe("describeError", () => {
  for (const { code } of SUPPORTED_LANGUAGES) {
    const t = () => currentT();

    /**
     * Given a squad-floor refusal whose parameter is itself a translation key
     * When it is described in this language
     * Then the position is named in words and nothing of the key or query string is left
     */
    it(`names_the_position_group_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      const described = describeError(FLOOR, t());

      expect(described.text).toContain(t()("common.positionGroups.Goalkeeper"));
      expectNoLeaks(described.text);
      expect(described.kind).toBe("refusal");
    });

    /**
     * Given a refusal that carries a club name with a space, URL-encoded
     * When it is described
     * Then the name is decoded and the figures are shown
     */
    it(`decodes_url_encoded_parameters_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      const described = describeError(
        "be.error.transfers.buyerSquadFull?club=Seller%20FC&count=30&max=30",
        t(),
      );

      expect(described.text).toContain("Seller FC");
      expect(described.text).toContain("30");
      expectNoLeaks(described.text);
    });

    /**
     * Given a key that needs a parameter, sent without it
     * When it is described
     * Then no placeholder or the word undefined reaches the player
     */
    it(`survives_a_missing_parameter_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      const described = describeError("be.error.squadFloor.wouldLeaveSeniorsShort", t());

      expect(described.text.length).toBeGreaterThan(10);
      expectNoLeaks(described.text);
    });

    /**
     * Given a code the game has no text for, a raw Rust message and a JavaScript error with a stack
     * When each is described
     * Then the player gets the localized "could not confirm" message with a reference, and the raw
     *      text is not shown
     */
    it(`falls_back_safely_for_unknown_and_unexpected_failures_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      vi.spyOn(console, "error").mockImplementation(() => {});
      vi.spyOn(console, "warn").mockImplementation(() => {});
      const jsError = new TypeError("Cannot read properties of undefined (reading 'x')");
      jsError.stack = "TypeError: boom\n    at foo (bar.ts:1:1)";
      for (const failure of [
        "be.error.notARealCode.xyz",
        "Offer not found or not pending",
        "thread 'main' panicked at src/lib.rs:10:5",
        jsError,
      ]) {
        const described = describeError(failure, t());

        expect(described.kind).toBe("technical");
        expect(described.reference).toMatch(/^[A-Z0-9]{6}$/);
        expect(described.text).toContain(described.reference ?? "?");
        for (const leak of ["be.error", "panicked", "Offer not found", "TypeError", "bar.ts"]) {
          expect(described.text).not.toContain(leak);
        }
      }
      vi.restoreAllMocks();
    });
  }

  /**
   * Given a parameter value that looks like a key the game has no text for
   * When it is described
   * Then the key path is not shown
   */
  it("does_not_show_an_unknown_nested_key", async () => {
    await i18n.changeLanguage("en");
    const described = describeError(
      "be.error.squadFloor.wouldLeaveShort?group=common.positionGroups.Mystery",
      currentT(),
    );

    expect(described.text).not.toContain("common.positionGroups");
  });

  /**
   * Given the same failure twice and a different one
   * When references are made
   * Then equal failures share a reference and different ones do not, so a log line can be matched
   */
  it("makes_stable_distinct_references", async () => {
    await i18n.changeLanguage("en");
    vi.spyOn(console, "error").mockImplementation(() => {});
    const t = currentT();
    const first = describeError("Offer not found", t).reference;
    const again = describeError("Offer not found", t).reference;
    const other = describeError("Something else entirely", t).reference;
    vi.restoreAllMocks();

    expect(first).toBe(again);
    expect(first).not.toBe(other);
  });

  /**
   * Given a business-rule refusal
   * When it is described
   * Then it says nothing was changed, which is true because the backend refuses before it writes
   */
  it("says_a_refusal_changed_nothing", async () => {
    await i18n.changeLanguage("en");
    const t = currentT();

    const described = describeError(FLOOR, t);

    expect(described.followUp).toBe(t("errors.nothingChanged"));
  });
});

describe("resolveTranslatedErrorMessage", () => {
  /**
   * Given the same encoded refusal through the helper the flows call
   * When it is resolved
   * Then it is the readable text, not the key (the counteroffer dialog showed the key)
   */
  it("resolves_nested_keys_for_the_flows", async () => {
    await i18n.changeLanguage("en");
    const text = resolveTranslatedErrorMessage(FLOOR, currentT());

    expect(text).toContain("Goalkeeper");
    expect(text).not.toContain("be.error");
  });
});
