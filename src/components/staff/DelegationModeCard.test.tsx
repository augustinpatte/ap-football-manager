import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { DelegationLimits } from "../../services/staffService";
import DelegationModeCard from "./DelegationModeCard";

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: () => {} },
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "en" },
  }),
}));

const LIMITS: DelegationLimits = {
  max_staff_wage: 5000,
  max_player_wage: 15000,
  max_commitment: 3_000_000,
  max_transfer_fee: 0,
};

function renderCard(mode: "manual" | "approval" | "full" | "custom", limits: DelegationLimits) {
  return render(
    <DelegationModeCard
      mode={mode}
      limits={limits}
      hasAssistant
      busy={false}
      onModeChange={() => {}}
      onLimitsSave={() => {}}
    />,
  );
}

describe("DelegationModeCard", () => {
  /**
   * Given the Total preset with the authority it carries
   * When the card is shown
   * Then it says the assistant is held to the club's finances and the ceilings are not used.
   */
  it("explains that Total authority replaces the spending limits", () => {
    renderCard("full", { ...LIMITS, finance_bound: true });
    expect(screen.getByText("staff.delegation.mode.totalNote")).toBeInTheDocument();
    expect(screen.queryByText("staff.delegation.mode.limitsOnlyNote")).not.toBeInTheDocument();
  });

  /**
   * Given a Full delegation saved before Total authority existed (no authority flag)
   * When the card is shown
   * Then it says the limits still apply and how to give the authority: nothing is implied.
   */
  it("says an older Full delegation still works within the limits", () => {
    renderCard("full", LIMITS);
    expect(screen.getByText("staff.delegation.mode.limitsOnlyNote")).toBeInTheDocument();
    expect(screen.queryByText("staff.delegation.mode.totalNote")).not.toBeInTheDocument();
  });

  /**
   * Given the Prudent preset
   * When the card is shown
   * Then neither Total note appears: every major decision comes to the manager.
   */
  it("shows no Total note under Prudent", () => {
    renderCard("approval", { ...LIMITS, finance_bound: false });
    expect(screen.queryByText("staff.delegation.mode.totalNote")).not.toBeInTheDocument();
    expect(screen.queryByText("staff.delegation.mode.limitsOnlyNote")).not.toBeInTheDocument();
  });
});
