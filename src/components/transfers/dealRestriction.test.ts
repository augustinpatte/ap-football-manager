import { describe, expect, it } from "vitest";

import type { PlayerData } from "../../store/gameStore";
import { dealRestrictionOf } from "./dealRestriction";

function player(overrides: Partial<PlayerData> = {}): PlayerData {
  return {
    id: "p1",
    transfer_offers: [],
    loan_offers: [],
    active_loan: null,
    ...overrides,
  } as unknown as PlayerData;
}

describe("dealRestrictionOf", () => {
  /**
   * Given a player who is free to approach
   * When his restriction is read
   * Then there is none.
   */
  it("reports nothing for a player nobody is blocked from", () => {
    expect(dealRestrictionOf(player())).toBeNull();
  });

  /**
   * Given a player out on loan with an end date on his record
   * When his restriction is read
   * Then it names the loan and its end date, taken from the record.
   */
  it("reports the loan and the date he comes back from his own record", () => {
    const restriction = dealRestrictionOf(
      player({
        active_loan: {
          parent_team_id: "owner",
          loan_team_id: "borrower",
          start_date: "2026-07-01",
          end_date: "2027-01-15",
          wage_contribution_pct: 50,
        },
      }),
    );
    expect(restriction).toEqual({ kind: "onLoan", clubId: "borrower", until: "2027-01-15" });
  });

  /**
   * Given a player with a transfer agreed and waiting for its registration date
   * When his restriction is read
   * Then it carries that date.
   */
  it("reports a transfer waiting for registration with its date", () => {
    const restriction = dealRestrictionOf(
      player({
        transfer_offers: [
          {
            id: "o1",
            from_team_id: "buyer",
            status: "PendingRegistration",
            registration_date: "2026-08-04",
          },
        ] as unknown as PlayerData["transfer_offers"],
      }),
    );
    expect(restriction).toEqual({ kind: "pendingMove", date: "2026-08-04" });
  });

  /**
   * Given a loan agreed and waiting to start
   * When his restriction is read
   * Then it carries the loan's start date.
   */
  it("reports a loan waiting to start with its start date", () => {
    const restriction = dealRestrictionOf(
      player({
        loan_offers: [
          {
            id: "l1",
            from_team_id: "borrower",
            status: "PendingRegistration",
            start_date: "2026-08-03",
          },
        ] as unknown as PlayerData["loan_offers"],
      }),
    );
    expect(restriction).toEqual({ kind: "pendingMove", date: "2026-08-03" });
  });

  /**
   * Given an agreed move that records no date
   * When his restriction is read
   * Then the date is absent: none is invented.
   */
  it("never invents a date the record does not hold", () => {
    const restriction = dealRestrictionOf(
      player({
        transfer_offers: [
          { id: "o1", from_team_id: "buyer", status: "PendingRegistration" },
        ] as unknown as PlayerData["transfer_offers"],
      }),
    );
    expect(restriction).toEqual({ kind: "pendingMove", date: null });
  });
});
