import { describe, expect, it } from "vitest";

import {
  DEFAULT_MARKET_QUERY,
  MARKET_PAGE_SIZE,
  canSign,
  formFromOutcome,
  isCounterOffer,
  negotiationTone,
  optionalAmount,
  pageRange,
  requestFromForm,
  searchRequest,
  totalPages,
  withQueryChange,
} from "./globalMarketModel";
import type { ExternalNegotiationOutcome } from "../../services/registryService";

const outcome: ExternalNegotiationOutcome = {
  playerId: "fm-p-1",
  freeAgent: false,
  status: "agreed",
  fee: 5_000_000,
  minimumFee: 4_000_000,
  clubDecision: "accepted",
  weeklyWage: 40_000,
  contractYears: 3,
  signingBonus: 250_000,
  contractEnd: "2029-06-30",
  playerVerdict: "accepts",
  playerScore: 62,
  mainObstacle: null,
  canAffordFee: true,
  wagePolicyAllows: true,
  canAffordBonus: true,
  windowOpen: true,
  blocker: null,
  fromClub: "fm-c-900",
};

describe("globalMarketModel", () => {
  describe("optionalAmount", () => {
    it("keeps a positive whole number and treats a blank or unusable field as no offer", () => {
      expect(optionalAmount(" 4000000 ")).toBe(4_000_000);
      expect(optionalAmount("12.7")).toBe(12);
      expect(optionalAmount("")).toBeNull();
      expect(optionalAmount("0")).toBeNull();
      expect(optionalAmount("-3")).toBeNull();
      expect(optionalAmount("abc")).toBeNull();
    });
  });

  describe("requestFromForm", () => {
    it("sends only the fields the manager filled in", () => {
      expect(
        requestFromForm({ fee: "5000000", weeklyWage: "", contractYears: "3", signingBonus: "" }),
      ).toEqual({
        fee: 5_000_000,
        weeklyWage: null,
        contractYears: 3,
        signingBonus: null,
      });
    });
  });

  describe("searchRequest", () => {
    it("maps every sort key and filter to the backend contract", () => {
      for (const sort of ["name", "age", "ovr", "value", "club"] as const) {
        const request = searchRequest({ ...DEFAULT_MARKET_QUERY, sort, descending: false });
        expect(request.sort).toBe(sort);
        expect(request.descending).toBe(false);
      }
      for (const availability of ["all", "free_agent", "contracted"] as const) {
        expect(searchRequest({ ...DEFAULT_MARKET_QUERY, availability }).availability).toBe(
          availability,
        );
      }
      for (const position of ["any", "goalkeeper", "defender", "midfielder", "forward"] as const) {
        expect(searchRequest({ ...DEFAULT_MARKET_QUERY, position }).position).toBe(position);
      }

      const narrowed = searchRequest({
        ...DEFAULT_MARKET_QUERY,
        text: "  martin ",
        nation: " fr ",
        club: " Lyon ",
        minAge: "18",
        maxAge: "99",
        page: 4,
      });
      expect(narrowed).toMatchObject({
        text: "martin",
        nation: "fr",
        club: "Lyon",
        minAge: 18,
        maxAge: 99,
        page: 4,
        pageSize: MARKET_PAGE_SIZE,
        includeIncomplete: false,
      });
    });

    it("treats a blank nation and age as no restriction", () => {
      const request = searchRequest(DEFAULT_MARKET_QUERY);
      expect(request.nation).toBeNull();
      expect(request.minAge).toBeNull();
      expect(request.maxAge).toBeNull();
    });
  });

  describe("withQueryChange", () => {
    it("returns to the first page when a filter changes and stays put when only paging", () => {
      const paged = { ...DEFAULT_MARKET_QUERY, page: 5 };
      expect(withQueryChange(paged, { text: "silva" }).page).toBe(1);
      expect(withQueryChange(paged, { sort: "value" }).page).toBe(1);
      expect(withQueryChange(paged, { availability: "free_agent" }).page).toBe(1);
      expect(withQueryChange(paged, { page: 6 }).page).toBe(6);
    });
  });

  describe("canSign", () => {
    it("signs only an agreement no rule blocks", () => {
      expect(canSign(outcome)).toBe(true);
      expect(canSign({ ...outcome, status: "club_countered" })).toBe(false);
      expect(canSign({ ...outcome, blocker: "be.error.transfers.playerRefusesMove" })).toBe(false);
      expect(canSign(null)).toBe(false);
    });
  });

  describe("counter offers", () => {
    it("recognises both counters and gives each status its tone", () => {
      expect(isCounterOffer("club_countered")).toBe(true);
      expect(isCounterOffer("player_countered")).toBe(true);
      expect(isCounterOffer("agreed")).toBe(false);
      expect(negotiationTone("agreed")).toBe("positive");
      expect(negotiationTone("club_countered")).toBe("warning");
      expect(negotiationTone("player_refused")).toBe("danger");
    });

    it("fills the form from the terms that would close the deal", () => {
      expect(formFromOutcome(outcome)).toEqual({
        fee: "5000000",
        weeklyWage: "40000",
        contractYears: "3",
        signingBonus: "250000",
      });
      expect(formFromOutcome({ ...outcome, fee: 0, signingBonus: 0 })).toMatchObject({
        fee: "",
        signingBonus: "",
      });
    });
  });

  describe("pagination", () => {
    it("covers an empty, a partial and a full last page", () => {
      expect(totalPages(0)).toBe(1);
      expect(totalPages(1)).toBe(1);
      expect(totalPages(26)).toBe(2);
      expect(pageRange(1, 0)).toEqual({ from: 0, to: 0 });
      expect(pageRange(1, 26)).toEqual({ from: 1, to: 25 });
      expect(pageRange(2, 26)).toEqual({ from: 26, to: 26 });
    });
  });
});
