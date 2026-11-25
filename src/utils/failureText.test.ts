import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "../i18n";
import { failureReason, failureText } from "./failureText";

describe("failureText", () => {
  beforeEach(async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    await i18n.changeLanguage("en");
  });
  afterEach(() => vi.restoreAllMocks());

  /**
   * Given a refusal the game has a sentence for
   * When it is described
   * Then the player reads the sentence and that nothing was changed, never the key
   */
  it("words_a_known_refusal_in_the_players_language", () => {
    const text = failureText("be.error.transactions.requestAbandoned");

    expect(text).not.toContain("be.error");
    expect(text).toContain("cancelled before it could run");
    expect(text).toContain("Nothing was changed.");
  });

  /**
   * Given a key no locale has a sentence for, and Rust prose, and a JavaScript error
   * When each is described
   * Then none of their text reaches the player: one safe message with a reference does
   */
  it("never_shows_an_untranslated_key_raw_prose_or_a_javascript_error", () => {
    for (const failure of [
      "be.error.never.written.anywhere",
      "called `Result::unwrap()` on an `Err` value: Io(NotFound)",
      new TypeError("Cannot read properties of undefined (reading 'x')"),
    ]) {
      const text = failureText(failure);
      expect(text).toMatch(/Reference: [0-9A-Z]{6}/);
      expect(text).not.toMatch(/be\.error|unwrap|TypeError|Cannot read/);
    }
  });

  /**
   * Given the same refusal inside another sentence
   * When the reason is read alone
   * Then it has no trailing "Nothing was changed."
   */
  it("gives_the_reason_alone_for_use_inside_a_sentence", () => {
    expect(failureReason("be.error.transactions.requestAbandoned")).not.toContain(
      "Nothing was changed",
    );
  });

  /**
   * Given a failure while another language is selected
   * When it is described
   * Then the text is in that language
   */
  it("follows_the_selected_language", async () => {
    await i18n.changeLanguage("fr");

    expect(failureText("be.error.transactions.requestAbandoned")).toContain("annulée");
    await i18n.changeLanguage("en");
  });
});
