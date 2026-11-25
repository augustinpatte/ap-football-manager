import { describe, expect, it } from "vitest";
import en from "../../../i18n/locales/en.json";
import type {
  CashKindName,
  ContractChangeKind,
  MatchOutcome,
  ReportTransferKind,
} from "../../../services/fastForwardService";
import type { DecisionOutcome } from "../../../services/staffService";
import type { PlayerMovementKind } from "../../../store/types";

const report = en.fastForward.report as unknown as Record<string, Record<string, unknown>>;
const has = (group: string, key: string) => typeof report[group]?.[key] === "string";

// Each list is every value the backend can send for the type; a `Record` keyed by the union makes
// the compiler reject a list that misses one when the backend adds a value.
const CASH_KINDS: Record<CashKindName, true> = {
  opening_balance: true,
  player_wages: true,
  staff_wages: true,
  matchday: true,
  sponsorship: true,
  commercial_campaign: true,
  prize_money: true,
  transfer_fee_out: true,
  transfer_fee_in: true,
  signing_bonus: true,
  loan_fee: true,
  contract_termination: true,
  board_support: true,
  facilities: true,
  upkeep: true,
  other: true,
};
const TRANSFER_KINDS: Record<ReportTransferKind, true> = {
  purchase: true,
  sale: true,
  loan_in: true,
  loan_out: true,
  loan_returned: true,
  loan_ended: true,
  free_signing: true,
};
const CONTRACT_KINDS: Record<ContractChangeKind, true> = {
  renewal: true,
  expired: true,
  terminated: true,
  released: true,
};
const OUTCOMES: Record<MatchOutcome, true> = { win: true, draw: true, loss: true };
const DELEGATION: Record<DecisionOutcome, true> = {
  executed: true,
  approved: true,
  declined: true,
  leftToManager: true,
};
const CAUSES: Record<PlayerMovementKind, true> = {
  permanent_transfer: true,
  loan_start: true,
  loan_return: true,
  loan_to_buy: true,
  free_agent_signing: true,
  released: true,
  renewal: true,
  retired: true,
  initial_contract: true,
};

describe("the report's words for every value the backend can send", () => {
  /**
   * Given each enum the report shows (cash kinds, transfer and contract kinds, outcomes, causes)
   * When its values are looked up in the English file
   * Then every one has its own sentence: a missing key would otherwise render as the key itself, and
   * a test that translates the same key would still pass
   */
  it.each([
    ["cashKind", CASH_KINDS],
    ["transfer", TRANSFER_KINDS],
    ["contract", CONTRACT_KINDS],
    ["outcome", OUTCOMES],
    ["delegationOutcome", DELEGATION],
    ["cause", CAUSES],
  ] as const)("has_a_sentence_for_every_%s", (group, values) => {
    for (const value of Object.keys(values))
      expect(has(group, value), `${group}.${value}`).toBe(true);
  });

  /** Given a purchase or a sale through a purchase option, then it has its own sentence too. */
  it("has_the_purchase_option_sentences", () => {
    expect(has("transfer", "purchaseOption")).toBe(true);
    expect(has("transfer", "saleOption")).toBe(true);
  });
});
