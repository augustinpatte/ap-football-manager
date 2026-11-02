import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { GameStateData } from "../../store/gameStore";
import type { StaffOverview } from "../../services/staffService";
import OfferDialog from "./OfferDialog";
import StaffManagementPanel from "./StaffManagementPanel";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: () => {} },
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params && Object.keys(params).length > 0 ? `${key} ${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

function overview(overrides: Partial<StaffOverview> = {}): StaffOverview {
  return {
    team_id: "team-1",
    posts: [
      { post: "AssistantManager", count: 0, limit: 1, required: 1 },
      { post: "Physio", count: 1, limit: 2, required: 1 },
    ],
    members: [],
    budget: {
      budget: 20000,
      weekly_bill: 5000,
      headroom: 15000,
      signing_fee_weeks: 1,
      max_years: 3,
      severance_pct: 50,
    },
    delegation: {
      settings: [
        { duty: "Training", delegated: true, confirm_major: true },
        { duty: "Scouting", delegated: false, confirm_major: true },
      ],
      proposals: [
        {
          id: "prop-1",
          duty: "StaffRecruitment",
          created: "2026-08-01",
          action: { kind: "hireStaff" },
          title_key: "delegation.proposal.hire",
          params: [["post", "Physio"]],
          reason_key: "delegation.reason.vacancy",
        },
      ],
      log: [
        {
          date: "2026-08-02",
          duty: "Lineup",
          title_key: "delegation.log.lineupChosen",
          params: [["changes", "2"]],
          outcome: "executed",
        },
        {
          date: "2026-08-03",
          duty: "PlayerRenewals",
          title_key: "delegation.log.playerRenewalLeft",
          params: [["player", "Ace"]],
          outcome: "leftToManager",
        },
      ],
      limits: {
        max_staff_wage: 5000,
        max_player_wage: 15000,
        max_commitment: 3000000,
        max_transfer_fee: 0,
      },
    },
    delegation_mode: "approval",
    has_assistant: true,
    ...overrides,
  };
}

describe("StaffManagementPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_assistant_recommendations") {
        return [
          {
            id: "rec-1",
            kind: "RestPlayer",
            severity: 3,
            player_id: "p1",
            title_key: "assistant.rec.tiredStarter.title",
            reason_key: "assistant.rec.tiredStarter.reason",
            params: [["player", "Ace"]],
          },
        ];
      }
      return {} as GameStateData;
    });
  });

  it("shows the budget, a vacant required seat, a proposal and a recommendation", async () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    expect(screen.getByText("staff.posts.AssistantManager")).toBeInTheDocument();
    expect(screen.getByText("staff.mgmt.vacant")).toBeInTheDocument();
    expect(screen.getByTestId("proposal-prop-1")).toBeInTheDocument();
    expect(await screen.findByTestId("rec-rec-1")).toBeInTheDocument();
  });

  it("hands a duty over and takes it back", async () => {
    const onGameUpdate = vi.fn();
    render(<StaffManagementPanel overview={overview()} onGameUpdate={onGameUpdate} />);

    const scouting = screen.getByTestId("duty-Scouting");
    fireEvent.click(scouting.querySelector("button") as HTMLButtonElement);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_duty_delegation", {
        duty: "Scouting",
        delegated: true,
        confirmMajor: true,
      }),
    );

    const training = screen.getByTestId("duty-Training");
    const buttons = training.querySelectorAll("button");
    fireEvent.click(buttons[buttons.length - 1]);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_duty_delegation", {
        duty: "Training",
        delegated: false,
        confirmMajor: true,
      }),
    );
    expect(onGameUpdate).toHaveBeenCalledTimes(2);
  });

  it("lists the lineup and player-renewal duties among those that can be handed over", () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);
    expect(screen.getByTestId("duty-Lineup")).toBeInTheDocument();
    expect(screen.getByTestId("duty-PlayerRenewals")).toBeInTheDocument();
  });

  it("switches the whole job to full delegation in one click", async () => {
    const onGameUpdate = vi.fn();
    render(<StaffManagementPanel overview={overview()} onGameUpdate={onGameUpdate} />);

    expect(screen.getByRole("radio", { name: "staff.delegation.mode.approval" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "staff.delegation.mode.full" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_delegation_mode", { mode: "full" }),
    );
    expect(onGameUpdate).toHaveBeenCalledTimes(1);
  });

  it("saves spending limits only once they differ from the saved ones", async () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    const save = screen.getByRole("button", { name: "staff.delegation.limits.save" });
    expect(save).toBeDisabled();
    fireEvent.change(screen.getByLabelText("staff.delegation.limits.max_transfer_fee"), {
      target: { value: "250000" },
    });
    expect(save).toBeEnabled();
    fireEvent.click(save);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_delegation_limits", {
        limits: {
          max_staff_wage: 5000,
          max_player_wage: 15000,
          max_commitment: 3000000,
          max_transfer_fee: 250000,
        },
      }),
    );
  });

  it("shows what the assistant did, and marks what he left to the manager", () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);
    expect(screen.getByText(/delegation\.log\.lineupChosen/)).toBeInTheDocument();
    const left = screen.getByText(/delegation\.log\.playerRenewalLeft/);
    expect(left).toHaveTextContent("staff.delegation.outcome.leftToManager");
  });

  it("cannot delegate without an assistant", () => {
    render(
      <StaffManagementPanel overview={overview({ has_assistant: false })} onGameUpdate={vi.fn()} />,
    );
    const button = screen.getByTestId("duty-Scouting").querySelector("button");
    expect(button).toBeDisabled();
  });

  it("answers a proposal and applies a recommendation", async () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.accept" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_delegation_proposal", {
        proposalId: "prop-1",
        accept: true,
      }),
    );

    await screen.findByTestId("rec-rec-1");
    fireEvent.click(screen.getByRole("button", { name: "staff.assistant.apply" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("apply_assistant_recommendation", {
        recommendationId: "rec-1",
      }),
    );
  });

  it("shows the backend's error when an action fails", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_assistant_recommendations") return [];
      throw "be.error.delegation.proposalNotFound";
    });
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.decline" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });
});

describe("OfferDialog", () => {
  it("offers a counter-offer wage that can be accepted in one click", async () => {
    const onSubmit = vi
      .fn()
      .mockResolvedValueOnce({ status: "counter", wage: 2100 })
      .mockResolvedValueOnce({ status: "accepted" });
    const onClose = vi.fn();
    render(
      <OfferDialog
        mode="hire"
        name="Sam"
        askingWage={2000}
        maxYears={3}
        signingFeeWeeks={1}
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "staff.offer.submit" }));
    fireEvent.click(await screen.findByRole("button", { name: /staff.offer.acceptCounter/ }));

    await waitFor(() => expect(onSubmit).toHaveBeenLastCalledWith(2100, 2));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("says so when the offer is refused and keeps the dialog open", async () => {
    const onSubmit = vi.fn().mockResolvedValue({ status: "refused" });
    const onClose = vi.fn();
    render(
      <OfferDialog
        mode="renew"
        name="Sam"
        askingWage={2000}
        maxYears={3}
        signingFeeWeeks={1}
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "staff.offer.submit" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("staff.offer.refused");
    expect(onClose).not.toHaveBeenCalled();
  });
});
