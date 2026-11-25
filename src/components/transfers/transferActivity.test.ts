import { describe, expect, it } from "vitest";

import type { LoanOfferData, PlayerData, TransferOfferData } from "../../store/gameStore";
import { buildTransferActivity } from "./transferActivity";

const USER = "team-user";
const OTHER = "team-other";
const TODAY = "2026-08-20";

function offer(overrides: Partial<TransferOfferData>): TransferOfferData {
  return {
    id: "o1",
    from_team_id: OTHER,
    fee: 1_000_000,
    wage_offered: 0,
    last_manager_fee: null,
    negotiation_round: 1,
    suggested_counter_fee: null,
    status: "Pending",
    date: "2026-08-10",
    ...overrides,
  };
}

function loan(overrides: Partial<LoanOfferData>): LoanOfferData {
  return {
    id: "l1",
    from_team_id: OTHER,
    parent_team_id: USER,
    start_date: "2026-08-01",
    end_date: "2027-06-30",
    wage_contribution_pct: 80,
    status: "Pending",
    date: "2026-08-10",
    ...overrides,
  };
}

function player(overrides: Partial<PlayerData>): PlayerData {
  return {
    id: "p1",
    full_name: "A. Player",
    team_id: USER,
    transfer_offers: [],
    loan_offers: [],
    active_loan: null,
    ...overrides,
  } as unknown as PlayerData;
}

const sections = (players: PlayerData[]) => buildTransferActivity(players, USER, TODAY);

describe("buildTransferActivity", () => {
  /**
   * Given a pending offer for one of the manager's players
   * When activity is built
   * Then it is "received", selling, and needs an answer
   */
  it("lists_a_pending_incoming_offer_as_received", () => {
    const result = sections([player({ transfer_offers: [offer({})] })]);

    expect(result.received).toHaveLength(1);
    expect(result.received[0]).toMatchObject({
      direction: "selling",
      kind: "permanent",
      fee: 1_000_000,
    });
    expect(result.negotiating).toHaveLength(0);
  });

  /**
   * Given the manager's own pending bid for another club's player
   * When activity is built
   * Then it is an ongoing negotiation, buying, not "received"
   */
  it("lists_the_managers_pending_bid_as_a_negotiation", () => {
    const result = sections([
      player({ team_id: OTHER, transfer_offers: [offer({ from_team_id: USER })] }),
    ]);

    expect(result.negotiating).toHaveLength(1);
    expect(result.negotiating[0]).toMatchObject({ direction: "buying" });
    expect(result.received).toHaveLength(0);
  });

  /**
   * Given agreed purchase and loan waiting to register
   * When activity is built
   * Then both are "pending registration" with their registration date
   */
  it("separates_agreements_pending_registration", () => {
    const result = sections([
      player({
        id: "p1",
        team_id: OTHER,
        transfer_offers: [
          offer({
            from_team_id: USER,
            status: "PendingRegistration",
            registration_date: "2027-01-01",
          }),
        ],
      }),
      player({
        id: "p2",
        team_id: OTHER,
        loan_offers: [
          loan({ from_team_id: USER, parent_team_id: OTHER, status: "PendingRegistration" }),
        ],
      }),
    ]);

    expect(result.pendingRegistration.map((entry) => entry.kind).sort()).toEqual([
      "loan",
      "permanent",
    ]);
    expect(result.pendingRegistration.find((e) => e.kind === "permanent")?.registrationDate).toBe(
      "2027-01-01",
    );
  });

  /**
   * Given a completed sale and a completed purchase
   * When activity is built
   * Then both are "completed", with the direction each was
   */
  it("lists_completed_transfers_in_both_directions", () => {
    const result = sections([
      player({ id: "sold", team_id: OTHER, transfer_offers: [offer({ status: "Accepted" })] }),
      player({
        id: "bought",
        team_id: USER,
        transfer_offers: [offer({ id: "o2", from_team_id: USER, status: "Accepted" })],
      }),
    ]);

    expect(result.completed.map((entry) => entry.direction).sort()).toEqual(["buying", "selling"]);
  });

  /**
   * Given an active loan out and an accepted loan offer for the same player
   * When activity is built
   * Then the player appears once, under "active loans", and not also as a completed offer
   */
  it("shows_an_active_loan_once", () => {
    const result = sections([
      player({
        team_id: OTHER,
        loan_offers: [loan({ status: "Accepted" })],
        active_loan: {
          parent_team_id: USER,
          loan_team_id: OTHER,
          start_date: "2026-08-01",
          end_date: "2027-06-30",
          wage_contribution_pct: 80,
        },
      }),
    ]);

    expect(result.activeLoans).toHaveLength(1);
    expect(result.activeLoans[0]).toMatchObject({ direction: "selling", kind: "loan" });
    expect(Object.values(result).flat()).toHaveLength(1);
  });

  /**
   * Given old and recent rejected offers, and offers that never touched the manager's club
   * When activity is built
   * Then only the recent closed ones of the manager's own dealings are listed
   */
  it("lists_recent_closed_offers_and_ignores_other_clubs_dealings", () => {
    const result = sections([
      player({
        id: "recent",
        transfer_offers: [offer({ status: "Rejected", closed_on: "2026-08-15" })],
      }),
      player({
        id: "old",
        transfer_offers: [offer({ id: "o3", status: "Rejected", closed_on: "2026-03-01" })],
      }),
      player({
        id: "elsewhere",
        team_id: "team-third",
        transfer_offers: [offer({ id: "o4", from_team_id: OTHER, status: "Accepted" })],
      }),
    ]);

    expect(result.closed.map((entry) => entry.playerId)).toEqual(["recent"]);
    expect(result.completed).toHaveLength(0);
  });

  /**
   * Given a withdrawn offer voided at registration
   * When activity is built
   * Then it is closed and keeps the reason, so the manager can see why the deal fell through
   */
  it("keeps_the_failure_reason_of_a_voided_agreement", () => {
    const result = sections([
      player({
        team_id: OTHER,
        transfer_offers: [
          offer({
            from_team_id: USER,
            status: "Withdrawn",
            closed_on: "2026-08-18",
            registration_failure_reason: "InsufficientFunds",
          }),
        ],
      }),
    ]);

    expect(result.closed[0].failureReason).toBe("InsufficientFunds");
  });

  /**
   * Given nothing at all
   * When activity is built
   * Then every section is empty
   */
  it("is_empty_without_any_dealings", () => {
    expect(Object.values(sections([player({})])).flat()).toEqual([]);
  });
});
