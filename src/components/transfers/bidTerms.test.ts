import { describe, expect, it } from "vitest";

import { DEFAULT_BID_TERMS, parseBidTerms, type BidTermsInput } from "./bidTerms";

const input = (overrides: Partial<BidTermsInput>): BidTermsInput => ({
  ...DEFAULT_BID_TERMS,
  ...overrides,
});

describe("parseBidTerms", () => {
  /**
   * Given an untouched terms form
   * When it is parsed
   * Then nothing is sent, so a plain bid is the bid it always was.
   */
  it("sends nothing for a plain fee", () => {
    expect(parseBidTerms(DEFAULT_BID_TERMS)).toEqual({ valid: true, terms: undefined });
  });

  /**
   * Given instalments, a clause, a bonus in millions and a promise
   * When the form is parsed
   * Then the backend's units come out: a whole-euro bonus and the promise flag.
   */
  it("converts the form to the backend's terms", () => {
    expect(
      parseBidTerms(
        input({ instalments: "3", sellOnPct: "15", signingBonusMillions: "0.25", promise: true }),
      ),
    ).toEqual({
      valid: true,
      terms: {
        instalments: 3,
        sell_on_pct: 15,
        signing_bonus: 250000,
        promise_stepping_stone: true,
      },
    });
  });

  /**
   * Given the wage and length the player was offered
   * When the form is parsed
   * Then they ride in the terms; left empty they are not sent, so the standard terms apply.
   */
  it("carries the player's own wage and length when they are set", () => {
    expect(parseBidTerms(input({ playerWage: "45000", playerYears: "4" }))).toEqual({
      valid: true,
      terms: {
        instalments: 1,
        sell_on_pct: 0,
        signing_bonus: 0,
        promise_stepping_stone: false,
        player_weekly_wage: 45000,
        player_contract_years: 4,
      },
    });
    expect(parseBidTerms(input({ playerWage: "", playerYears: "" }))).toEqual({
      valid: true,
      terms: undefined,
    });
  });

  /**
   * Given a value out of range or not a number
   * When the form is parsed
   * Then it is invalid, so the offer cannot be submitted with it.
   */
  it.each([
    ["no instalments", { instalments: "0" }],
    ["too many instalments", { instalments: "5" }],
    ["a fractional instalment count", { instalments: "2.5" }],
    ["a clause over the limit", { sellOnPct: "31" }],
    ["a negative bonus", { signingBonusMillions: "-1" }],
    ["a bonus that is not a number", { signingBonusMillions: "abc" }],
    ["a contract longer than five years", { playerYears: "6" }],
    ["a negative wage", { playerWage: "-5" }],
    ["a fractional wage", { playerWage: "100.5" }],
  ])("rejects %s", (_label, overrides) => {
    expect(parseBidTerms(input(overrides))).toEqual({ valid: false });
  });
});
