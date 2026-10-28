import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ActiveLoanData, TeamData } from "../../store/gameStore";
import PlayerProfileLoanStatusBanner from "./PlayerProfileLoanStatusBanner";

const t = (key: string, options?: Record<string, unknown>) =>
  options && "fee" in options ? `${key}:${String(options.fee)}` : key;

const teams = [
  { id: "parent", name: "Parent FC" },
  { id: "borrower", name: "Borrower FC" },
] as unknown as TeamData[];

describe("PlayerProfileLoanStatusBanner", () => {
  /**
   * Given a loan with a buy option of €1,234,567
   * When the banner shows the loan's terms
   * Then the buy-option fee is exact, as on every other surface where it is negotiated.
   */
  it("shows the buy-option fee exactly", () => {
    const loan = {
      parent_team_id: "parent",
      loan_team_id: "borrower",
      start_date: "2026-08-01",
      end_date: "2027-06-30",
      wage_contribution_pct: 50,
      buy_option_fee: 1234567,
    } as ActiveLoanData;

    render(
      <PlayerProfileLoanStatusBanner
        loan={loan}
        teams={teams}
        managerTeamId="parent"
        language="en"
        t={t}
      />,
    );

    expect(screen.getByText("playerProfile.loanBuyOption:€1,234,567")).toBeInTheDocument();
  });

  /**
   * Given a player lent by the user's club to a club the world does not contain (an FM24 loan)
   * When the banner names the borrower
   * Then the name the import recorded is shown instead of "unknown", and a borrower that is in the
   * world is still named from the world.
   */
  it("names a borrower outside the world from the loan itself", () => {
    const named = (key: string, options?: Record<string, unknown>) =>
      options && "team" in options ? `${key}:${String(options.team)}` : key;
    const base = {
      parent_team_id: "parent",
      loan_team_id: "fm-c-1600",
      start_date: "2026-06-01",
      end_date: "2027-06-30",
      wage_contribution_pct: 0,
    };

    const { rerender } = render(
      <PlayerProfileLoanStatusBanner
        loan={{ ...base, loan_team_name: "Clube Desportivo Nacional" } as ActiveLoanData}
        teams={teams}
        managerTeamId="parent"
        language="en"
        t={named}
      />,
    );
    expect(
      screen.getByText("playerProfile.loanedOutTo:Clube Desportivo Nacional"),
    ).toBeInTheDocument();

    rerender(
      <PlayerProfileLoanStatusBanner
        loan={
          { ...base, loan_team_id: "borrower", loan_team_name: "Ignored Name" } as ActiveLoanData
        }
        teams={teams}
        managerTeamId="parent"
        language="en"
        t={named}
      />,
    );
    expect(screen.getByText("playerProfile.loanedOutTo:Borrower FC")).toBeInTheDocument();
  });
});
