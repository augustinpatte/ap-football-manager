import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { createPlayer, createTeam } from "../../test-utils/factories";
import LoanOfferModal from "./LoanOfferModal";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "en" },
  }),
}));

describe("LoanOfferModal", () => {
  /**
   * Given a loan target whose `position` is the legacy bucket and whose natural position is granular
   * When the loan modal opens
   * Then the badge reads the granular position.
   */
  it("badges the player by their granular natural position", () => {
    render(
      <LoanOfferModal
        loanTarget={createPlayer({ position: "Defender", natural_position: "LeftBack" })}
        teams={[createTeam()]}
        periodId=""
        periodOptions={[]}
        selectedEndDate="2027-01-01"
        onPeriodChange={vi.fn()}
        wageContributionPct={50}
        onWageContributionChange={vi.fn()}
        buyOptionEnabled={false}
        buyOptionFee=""
        onBuyOptionEnabledChange={vi.fn()}
        onBuyOptionFeeChange={vi.fn()}
        result={null}
        error={null}
        loading={false}
        submitDisabled={false}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("common.posAbbr.LeftBack")).toBeInTheDocument();
    expect(screen.queryByText("common.posAbbr.Defender")).not.toBeInTheDocument();
  });

  /**
   * Given a target who is already out on loan
   * When the loan form opens
   * Then it says where and until when, and the offer cannot be submitted.
   */
  it("explains a player already on loan and does not let the offer be sent", () => {
    render(
      <LoanOfferModal
        loanTarget={createPlayer({
          active_loan: {
            parent_team_id: "owner",
            loan_team_id: "team-1",
            start_date: "2026-07-01",
            end_date: "2027-01-15",
            wage_contribution_pct: 50,
          },
        })}
        teams={[createTeam()]}
        periodId=""
        periodOptions={[]}
        selectedEndDate="2027-01-01"
        onPeriodChange={vi.fn()}
        wageContributionPct={50}
        onWageContributionChange={vi.fn()}
        buyOptionEnabled={false}
        buyOptionFee=""
        onBuyOptionEnabledChange={vi.fn()}
        onBuyOptionFeeChange={vi.fn()}
        result={null}
        error={null}
        loading={false}
        submitDisabled={false}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent("transfers.restriction.onLoanDetail");
    expect(screen.getByRole("button", { name: "transfers.submitLoanOffer" })).toBeDisabled();
  });

  /**
   * Given the dialog is open over a button that had focus
   * When it opens, Tab is pressed on its last control, and then Escape
   * Then focus starts inside it, Tab wraps instead of leaving it, and Escape asks to close.
   */
  it("takes focus, traps Tab and closes on Escape", () => {
    const onClose = vi.fn();
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    render(
      <LoanOfferModal
        loanTarget={createPlayer()}
        teams={[createTeam()]}
        periodId=""
        periodOptions={[]}
        selectedEndDate="2027-01-01"
        onPeriodChange={vi.fn()}
        wageContributionPct={50}
        onWageContributionChange={vi.fn()}
        buyOptionEnabled={false}
        buyOptionFee=""
        onBuyOptionEnabledChange={vi.fn()}
        onBuyOptionFeeChange={vi.fn()}
        result={null}
        error={null}
        loading={false}
        submitDisabled={false}
        onSubmit={vi.fn()}
        onClose={onClose}
      />,
    );

    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveFocus();
    const controls = dialog.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), select:not([disabled])",
    );
    const last = controls[controls.length - 1];
    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).toBe(controls[0]);

    fireEvent.keyDown(window, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
    opener.remove();
  });
});
