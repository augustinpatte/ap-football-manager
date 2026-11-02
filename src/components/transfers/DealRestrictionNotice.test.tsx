import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PlayerData, TeamData } from "../../store/gameStore";
import DealRestrictionNotice from "./DealRestrictionNotice";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string>) =>
      params
        ? `${key}|${Object.entries(params)
            .map(([k, v]) => `${k}=${v}`)
            .join(",")}`
        : key,
    i18n: { language: "en" },
  }),
}));

const teams = [{ id: "borrower", name: "Borrow FC", short_name: "BOR" }] as unknown as TeamData[];

function player(overrides: Partial<PlayerData>): PlayerData {
  return { id: "p1", transfer_offers: [], loan_offers: [], ...overrides } as unknown as PlayerData;
}

describe("DealRestrictionNotice", () => {
  /**
   * Given a player nobody is blocked from
   * When the notice is rendered
   * Then nothing is shown.
   */
  it("renders nothing for a free player", () => {
    const { container } = render(<DealRestrictionNotice player={player({})} teams={teams} />);
    expect(container).toBeEmptyDOMElement();
  });

  /**
   * Given a player on loan until a stored date
   * When the notice is rendered
   * Then it names the club and says until when, as a status the player can read.
   */
  it("says where the player is on loan and until when", () => {
    render(
      <DealRestrictionNotice
        player={player({
          active_loan: {
            parent_team_id: "owner",
            loan_team_id: "borrower",
            start_date: "2026-07-01",
            end_date: "2027-01-15",
            wage_contribution_pct: 50,
          },
        })}
        teams={teams}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("transfers.restriction.onLoanDetail");
    expect(screen.getByRole("status")).toHaveTextContent("club=Borrow FC");
    expect(screen.getByRole("status")).toHaveTextContent("date=January 15, 2027");
  });

  /**
   * Given an agreed move that stores no date
   * When the notice is rendered
   * Then the no-date wording is used.
   */
  it("uses the wording without a date when none is known", () => {
    render(
      <DealRestrictionNotice
        player={player({
          transfer_offers: [
            { id: "o1", from_team_id: "x", status: "PendingRegistration" },
          ] as unknown as PlayerData["transfer_offers"],
        })}
        teams={teams}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "transfers.restriction.pendingDetailNoDate",
    );
  });
});
