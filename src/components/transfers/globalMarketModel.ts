import type {
  ExternalNegotiationOutcome,
  ExternalNegotiationRequest,
  ExternalNegotiationStatus,
  RegistryAvailability,
  RegistryPositionFilter,
  RegistrySearchRequest,
  RegistrySortKey,
} from "../../services/registryService";

/** What the screen edits; the backend owns the search contract it is sent as. */
export interface GlobalMarketQuery {
  text: string;
  nation: string;
  club: string;
  position: RegistryPositionFilter;
  minAge: string;
  maxAge: string;
  availability: RegistryAvailability;
  sort: RegistrySortKey;
  descending: boolean;
  page: number;
}

export const MARKET_PAGE_SIZE = 25;

export const DEFAULT_MARKET_QUERY: GlobalMarketQuery = {
  text: "",
  nation: "",
  club: "",
  position: "any",
  minAge: "",
  maxAge: "",
  availability: "all",
  sort: "ovr",
  descending: true,
  page: 1,
};

/** The terms form as typed. Empty means "the buyer's standard term" for that field. */
export interface NegotiationForm {
  fee: string;
  weeklyWage: string;
  contractYears: string;
  signingBonus: string;
}

export const EMPTY_NEGOTIATION_FORM: NegotiationForm = {
  fee: "",
  weeklyWage: "",
  contractYears: "",
  signingBonus: "",
};

export type NegotiationTone = "positive" | "warning" | "danger" | "neutral";

/** A whole number of euros, or the standard term when the field is empty or not a number. */
export function optionalAmount(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : null;
}

function optionalAge(value: string): number | null {
  const parsed = optionalAmount(value);
  return parsed === null ? null : Math.min(parsed, 99);
}

/** The request the backend judges; nothing here decides an outcome. */
export function requestFromForm(form: NegotiationForm): ExternalNegotiationRequest {
  return {
    fee: optionalAmount(form.fee),
    weeklyWage: optionalAmount(form.weeklyWage),
    contractYears: optionalAmount(form.contractYears),
    signingBonus: optionalAmount(form.signingBonus),
  };
}

/** The search request for one page of the market, from what the screen holds. */
export function searchRequest(query: GlobalMarketQuery): RegistrySearchRequest {
  return {
    text: query.text.trim(),
    nation: query.nation.trim() || null,
    position: query.position,
    minAge: optionalAge(query.minAge),
    maxAge: optionalAge(query.maxAge),
    club: query.club.trim(),
    availability: query.availability,
    sort: query.sort,
    descending: query.descending,
    page: query.page,
    pageSize: MARKET_PAGE_SIZE,
    includeIncomplete: false,
  };
}

/** A changed filter belongs on page one; paging does not reset itself. */
export function withQueryChange(
  query: GlobalMarketQuery,
  patch: Partial<GlobalMarketQuery>,
): GlobalMarketQuery {
  const pagingOnly = Object.keys(patch).every((key) => key === "page");
  return { ...query, ...patch, page: pagingOnly ? (patch.page ?? query.page) : 1 };
}

/** Only an agreement with no blocker may be signed. */
export function canSign(outcome: ExternalNegotiationOutcome | null): boolean {
  return outcome !== null && outcome.status === "agreed" && outcome.blocker === null;
}

/** Whether the last answer asks the manager to accept something before it can be signed. */
export function isCounterOffer(status: ExternalNegotiationStatus | undefined): boolean {
  return status === "club_countered" || status === "player_countered";
}

export function negotiationTone(status: ExternalNegotiationStatus): NegotiationTone {
  switch (status) {
    case "agreed":
      return "positive";
    case "club_countered":
    case "player_countered":
      return "warning";
    case "club_rejected":
    case "player_refused":
    case "blocked":
      return "danger";
    default:
      return "neutral";
  }
}

/** Fill the form from the terms that would end the negotiation, when the manager accepts them. */
export function formFromOutcome(outcome: ExternalNegotiationOutcome): NegotiationForm {
  return {
    fee: outcome.fee > 0 ? String(outcome.fee) : "",
    weeklyWage: outcome.weeklyWage > 0 ? String(outcome.weeklyWage) : "",
    contractYears: outcome.contractYears > 0 ? String(outcome.contractYears) : "",
    signingBonus: outcome.signingBonus > 0 ? String(outcome.signingBonus) : "",
  };
}

export function totalPages(total: number, pageSize: number = MARKET_PAGE_SIZE): number {
  return Math.max(1, Math.ceil(total / pageSize));
}

export function pageRange(
  page: number,
  total: number,
  pageSize: number = MARKET_PAGE_SIZE,
): { from: number; to: number } {
  if (total === 0) return { from: 0, to: 0 };
  const from = (page - 1) * pageSize + 1;
  return { from, to: Math.min(total, from + pageSize - 1) };
}
