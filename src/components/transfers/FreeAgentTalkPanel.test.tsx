import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { FreeAgentTalkData } from "../../store/gameStore";
import FreeAgentTalkPanel from "./FreeAgentTalkPanel";
import type { FreeAgentTalkPanelData } from "./useFreeAgentContractFlow";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key} ${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

function talk(overrides: Partial<FreeAgentTalkData> = {}): FreeAgentTalkData {
  return {
    id: "fat:1",
    club_id: "team-1",
    status: "awaiting_response",
    weekly_wage: 4000,
    contract_years: 3,
    signing_bonus: 0,
    promise_stepping_stone: false,
    round: 1,
    opened_on: "2026-08-01",
    updated_on: "2026-08-01",
    respond_on: "2026-08-03",
    counter_wage: null,
    counter_years: null,
    counter_expires_on: null,
    closed_on: null,
    reason: null,
    made_by_manager: true,
    ...overrides,
  };
}

function panel(overrides: Partial<FreeAgentTalkPanelData> = {}): FreeAgentTalkPanelData {
  return {
    interest: null,
    talk: null,
    rivals: [],
    busy: false,
    onAcceptCounter: vi.fn(),
    onWithdraw: vi.fn(),
    ...overrides,
  };
}

describe("FreeAgentTalkPanel", () => {
  /**
   * Given an elite player who is unlikely to accept a modest club
   * When the panel renders his interest
   * Then it says so, names the obstacle, and says how long an answer takes.
   */
  it("explains why an elite free agent is unlikely to accept", () => {
    render(
      <FreeAgentTalkPanel
        panel={panel({
          interest: {
            verdict: "unlikely",
            main_obstacle: "club_below_his_level",
            expected_wage: 130000,
            expected_years: 3,
            place_in_side: "key",
            days_unattached: 5,
            min_response_days: 1,
            max_response_days: 3,
            rival_clubs: [],
            open_talk: null,
          },
        })}
      />,
    );

    expect(screen.getByText("freeAgents.talk.interest.unlikely")).toBeInTheDocument();
    expect(screen.getByText("freeAgents.reason.club_below_his_level")).toBeInTheDocument();
    expect(screen.getByText("freeAgents.talk.place.key")).toBeInTheDocument();
    expect(screen.getByText(/freeAgents\.talk\.responseTime/)).toHaveTextContent('"max":3');
  });

  /**
   * Given an offer awaiting an answer
   * When the manager withdraws it
   * Then the withdraw handler runs, and while busy the button cannot be pressed twice.
   */
  it("lets a pending offer be withdrawn, once", () => {
    const onWithdraw = vi.fn();
    const { rerender } = render(<FreeAgentTalkPanel panel={panel({ talk: talk(), onWithdraw })} />);

    expect(screen.getByRole("status")).toHaveTextContent("freeAgents.talk.awaiting");
    fireEvent.click(screen.getByRole("button", { name: "freeAgents.talk.withdraw" }));
    expect(onWithdraw).toHaveBeenCalledTimes(1);

    rerender(<FreeAgentTalkPanel panel={panel({ talk: talk(), onWithdraw, busy: true })} />);
    expect(screen.getByRole("button", { name: "freeAgents.talk.withdraw" })).toBeDisabled();
  });

  /**
   * Given a counter-offer from the player
   * When the panel renders
   * Then his terms are shown and accepting them is one button.
   */
  it("shows a counter-offer with its terms and lets the manager accept it", () => {
    const onAcceptCounter = vi.fn();
    render(
      <FreeAgentTalkPanel
        panel={panel({
          talk: talk({
            status: "counter_offered",
            respond_on: null,
            counter_wage: 5200,
            counter_years: 3,
            counter_expires_on: "2026-08-06",
            reason: "wage_too_low",
          }),
          onAcceptCounter,
        })}
      />,
    );

    expect(screen.getByText(/freeAgents\.talk\.counter /)).toHaveTextContent('"years":3');
    expect(screen.getByText("freeAgents.reason.wage_too_low")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "freeAgents.talk.acceptCounter" }));
    expect(onAcceptCounter).toHaveBeenCalledTimes(1);
  });

  /**
   * Given a talk that ended because a rival signed him, and another club still bidding
   * When the panel renders
   * Then the outcome and the rival are both stated.
   */
  it("reports a lost talk and the clubs still bidding", () => {
    render(
      <FreeAgentTalkPanel
        panel={panel({
          talk: talk({
            status: "lost_to_rival",
            respond_on: null,
            reason: "better_offer_elsewhere",
          }),
          rivals: [{ club_id: "big", club_name: "Big FC" }],
        })}
      />,
    );

    expect(screen.getByText(/freeAgents\.talk\.ended\.lost_to_rival/)).toBeInTheDocument();
    expect(screen.getByText(/freeAgents\.talk\.rivals/)).toHaveTextContent("Big FC");
  });
});
