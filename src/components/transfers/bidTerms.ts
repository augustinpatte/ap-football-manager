import type { TransferTermsData } from "../../services/transfersService";

/** The terms form as the manager types it: strings, so a half-typed value is not lost. */
export interface BidTermsInput {
  instalments: string;
  sellOnPct: string;
  /** In millions, like the fee field next to it. */
  signingBonusMillions: string;
  promise: boolean;
  /** The player's own weekly wage and contract length; empty means the standard terms. */
  playerWage: string;
  playerYears: string;
}

export const DEFAULT_BID_TERMS: BidTermsInput = {
  instalments: "1",
  sellOnPct: "0",
  signingBonusMillions: "0",
  promise: false,
  playerWage: "",
  playerYears: "",
};

// Mirror the backend's limits (`domain::transfer_terms`) only to keep the form from sending what
// it would refuse outright; the backend stays the authority and answers with its own error.
export const MAX_INSTALMENTS = 4;
export const MAX_SELL_ON_PCT = 30;
export const MAX_PLAYER_CONTRACT_YEARS = 5;

export type ParsedBidTerms =
  | { valid: true; terms: TransferTermsData | undefined }
  | { valid: false };

/**
 * What to send: `undefined` for a plain fee (so an untouched form sends nothing new), the terms
 * when any were set, and `valid: false` for a value that is not a number in range.
 */
export function parseBidTerms(input: BidTermsInput): ParsedBidTerms {
  const instalments = Number(input.instalments);
  const sellOnPct = Number(input.sellOnPct);
  const bonusMillions = Number(input.signingBonusMillions);
  const playerWage = input.playerWage.trim() === "" ? 0 : Number(input.playerWage);
  const playerYears = input.playerYears.trim() === "" ? 0 : Number(input.playerYears);
  const valid =
    Number.isInteger(instalments) &&
    instalments >= 1 &&
    instalments <= MAX_INSTALMENTS &&
    Number.isInteger(sellOnPct) &&
    sellOnPct >= 0 &&
    sellOnPct <= MAX_SELL_ON_PCT &&
    Number.isFinite(bonusMillions) &&
    bonusMillions >= 0 &&
    Number.isInteger(playerWage) &&
    playerWage >= 0 &&
    Number.isInteger(playerYears) &&
    playerYears >= 0 &&
    playerYears <= MAX_PLAYER_CONTRACT_YEARS;
  if (!valid) {
    return { valid: false };
  }
  const signing_bonus = Math.round(bonusMillions * 1_000_000);
  const plain =
    instalments === 1 &&
    sellOnPct === 0 &&
    signing_bonus === 0 &&
    !input.promise &&
    playerWage === 0 &&
    playerYears === 0;
  if (plain) {
    return { valid: true, terms: undefined };
  }
  return {
    valid: true,
    terms: {
      instalments,
      sell_on_pct: sellOnPct,
      signing_bonus,
      promise_stepping_stone: input.promise,
      ...(playerWage > 0 ? { player_weekly_wage: playerWage } : {}),
      ...(playerYears > 0 ? { player_contract_years: playerYears } : {}),
    },
  };
}
