import { describe, expect, it } from "vitest";

import { LOCALE_FILES, type LocaleTree } from "../../i18n/i18nTestHelpers";
import { marketErrorMessage } from "./globalMarketModel";

/**
 * Every refusal the backend can return on the Global Market and signing paths, as the encoded
 * `be.error.*` key it sends (I2.1 made `squadFull` the one squad-place refusal on every route).
 */
const BACKEND_REFUSALS = [
  "be.error.freeAgents.squadFull?count=29&max=30&release=0",
  "be.error.transfers.insufficientFunds",
  "be.error.transfers.transferBudgetTooLow",
  "be.error.contracts.boardWagePolicy?budget=120000",
  "be.error.transfers.transferWindowClosed",
  "be.error.transfers.playerRefusesMove",
  "be.error.registry.playerAlreadyInWorld",
  "be.error.registry.playerNotAvailable",
  "be.error.registry.playerIncomplete",
  "be.error.registry.offerRejected",
  "be.error.registry.negotiationNotAgreed",
  "be.error.noTeamAssigned",
  "be.error.managedTeamNotFound",
];

function lookup(tree: LocaleTree, key: string): string | undefined {
  let node: unknown = tree;
  for (const part of key.split(".")) {
    if (node === null || typeof node !== "object") return undefined;
    node = (node as LocaleTree)[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** A translator over a real locale file that, like i18next, answers the key when it is missing. */
function translatorFor(tree: LocaleTree) {
  return (key: string, params?: Record<string, string | number>) => {
    const text = lookup(tree, key);
    if (text === undefined) return key;
    return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(params?.[name] ?? ""));
  };
}

describe("the Global Market against the backend's refusals", () => {
  /**
   * Given each refusal the backend returns when a signing or negotiation is not allowed
   * When the market screen turns it into the sentence the player reads, in every shipped locale
   * Then the player gets that refusal's own translated sentence: never a raw key, never the
   * generic fallback that would hide the real reason (a promised squad place, cash, wages...).
   */
  for (const [code, tree] of Object.entries(LOCALE_FILES)) {
    it(`gives every backend refusal its own sentence in ${code}`, () => {
      const t = translatorFor(tree);
      const generic = t("globalMarket.errorGeneric");

      for (const refusal of BACKEND_REFUSALS) {
        const shown = marketErrorMessage(refusal, t);
        expect(shown, refusal).not.toBe(generic);
        expect(shown, refusal).not.toContain("be.error.");
        expect(shown, refusal).not.toContain("{{");
      }
    });
  }

  /**
   * Given the squad-full refusal carrying its numbers
   * When it is shown in English
   * Then the count, the limit and the places to free are in the sentence.
   */
  it("keeps the squad-place numbers in the sentence", () => {
    const t = translatorFor(LOCALE_FILES.en as LocaleTree);

    expect(
      marketErrorMessage("be.error.freeAgents.squadFull?count=29&max=30&release=0", t),
    ).toContain("29 of 30");
  });
});
