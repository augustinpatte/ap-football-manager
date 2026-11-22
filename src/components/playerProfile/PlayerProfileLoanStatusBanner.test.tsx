import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ActiveLoanData, TeamData } from "../../store/gameStore";
import PlayerProfileLoanStatusBanner from "./PlayerProfileLoanStatusBanner";

const t = (key: string, options?: Record<string, unknown>) =>
  options && "fee" in options ? `${key}:${String(options.fee)}` : key;

const teams = [
  { id: "parent", name: "Parent FC" },
  { id: "borrower", name: "Borrower FC" },
] as unknown as TeamData[];

const baseLoan = {
  parent_team_id: "parent",
  loan_team_id: "borrower",
  start_date: "2026-08-01",
  end_date: "2027-06-30",
  wage_contribution_pct: 50,
} satisfies ActiveLoanData;

function renderBanner(loan: ActiveLoanData, translate = t) {
  return render(
    <PlayerProfileLoanStatusBanner loan={loan} teams={teams} language="en" t={translate} />,
  );
}

describe("PlayerProfileLoanStatusBanner", () => {
  /**
   * Given a loan with a buy option of €1,234,567
   * When the banner shows the loan's terms
   * Then the buy-option fee is exact, as on every other surface where it is negotiated.
   */
  it("shows the buy-option fee exactly", () => {
    renderBanner({ ...baseLoan, buy_option_fee: 1234567 });

    expect(screen.getByText("playerProfile.loanBuyOption:€1,234,567")).toBeInTheDocument();
  });

  /**
   * Given a loan between two clubs of the world, seen by someone who manages neither or either
   * When the banner renders
   * Then the contract owner and the club the player plays for are both named, each under its label.
   */
  it("names the contract owner and the club the player plays for, each under its label", () => {
    renderBanner({ ...baseLoan, terms_source: "agreed" });

    const owner = screen.getByText("playerProfile.loanOwnerLabel").parentElement as HTMLElement;
    const playing = screen.getByText("playerProfile.loanPlayingForLabel")
      .parentElement as HTMLElement;
    expect(within(owner).getByText("Parent FC")).toBeInTheDocument();
    expect(within(playing).getByText("Borrower FC")).toBeInTheDocument();
  });

  /**
   * Given a player lent to a club the world does not contain (an imported loan)
   * When the banner names the borrower
   * Then the name the import recorded is shown instead of "unknown", and a borrower that is in the
   * world is still named from the world.
   */
  it("names a borrower outside the world from the loan itself", () => {
    const outside = { ...baseLoan, loan_team_id: "fm-c-1600" };

    const { unmount } = renderBanner({ ...outside, loan_team_name: "Clube Desportivo Nacional" });
    expect(screen.getByText("Clube Desportivo Nacional")).toBeInTheDocument();
    unmount();

    renderBanner({ ...baseLoan, loan_team_name: "Ignored Name" });
    expect(screen.getByText("Borrower FC")).toBeInTheDocument();
    expect(screen.queryByText("Ignored Name")).not.toBeInTheDocument();
  });

  /**
   * Given a loan whose terms the importer estimated (the save records the loan, not its terms)
   * When the banner renders
   * Then the end date and wage share are labelled as estimates and a note says so; the plain
   * wording that would present them as a contract is not used.
   */
  it("does not present estimated terms as contract facts", () => {
    renderBanner({ ...baseLoan, terms_source: "estimated" });

    expect(screen.getByText("playerProfile.loanUntilEstimate")).toBeInTheDocument();
    expect(screen.getByText("playerProfile.loanWageSplitEstimate")).toBeInTheDocument();
    expect(screen.getByText("playerProfile.loanTermsEstimatedNote")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanUntil")).not.toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanWageSplit")).not.toBeInTheDocument();
  });

  /**
   * Given a loan agreed inside the game
   * When the banner renders
   * Then the terms are stated plainly with no note.
   */
  it("states agreed terms plainly", () => {
    renderBanner({ ...baseLoan, terms_source: "agreed" });

    expect(screen.getByText("playerProfile.loanUntil")).toBeInTheDocument();
    expect(screen.getByText("playerProfile.loanWageSplit")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanTermsEstimatedNote")).not.toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanTermsUnrecordedNote")).not.toBeInTheDocument();
  });

  /**
   * Given a loan whose terms a data source carries
   * When the banner renders
   * Then they are stated plainly with no note, like agreed terms.
   */
  it("states sourced terms plainly", () => {
    renderBanner({ ...baseLoan, terms_source: "sourced" });

    expect(screen.getByText("playerProfile.loanUntil")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanTermsEstimatedNote")).not.toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanTermsUnrecordedNote")).not.toBeInTheDocument();
  });

  /**
   * Given a loan from a save written before terms had a source (the field is absent)
   * When the banner renders
   * Then it is neither called agreed nor estimated: a note says where its terms came from was not
   * recorded.
   */
  it("says so when a loan's terms source was never recorded", () => {
    renderBanner({ ...baseLoan });

    expect(screen.getByText("playerProfile.loanTermsUnrecordedNote")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.loanTermsEstimatedNote")).not.toBeInTheDocument();
  });
});
