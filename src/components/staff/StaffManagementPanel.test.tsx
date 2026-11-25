import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { GameStateData } from "../../store/gameStore";
import type { DutyOwnerView, DutyView, StaffOverview } from "../../services/staffService";
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

const assistant: DutyOwnerView = {
  id: "assist",
  name: "Ada Lovelace",
  post: "AssistantManager",
  overall: 60,
};
const fitnessCoach: DutyOwnerView = {
  id: "fit-1",
  name: "Ana Pires",
  post: "FitnessCoach",
  overall: 80,
};

/** The backend's per-duty view: the assistant runs everything; training can go to a coach. */
function dutyViews(): DutyView[] {
  const duties = [
    "Training",
    "MatchPreparation",
    "Scouting",
    "Youth",
    "StaffRecruitment",
    "StaffRenewals",
    "Lineup",
    "PlayerRenewals",
    "PlayerRecruitment",
    "Tactics",
    "Offers",
  ] as const;
  return duties.map((duty) => ({
    duty,
    supports_approval: !["Lineup", "Tactics", "MatchPreparation", "Scouting"].includes(duty),
    in_charge: assistant,
    named: false,
    candidates: duty === "Training" ? [fitnessCoach, assistant] : [assistant],
  }));
}

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
    duties: dutyViews(),
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

  it("offers the tactical-preparation duty and reports a manager override as left to the manager", () => {
    const base = overview();
    render(
      <StaffManagementPanel
        overview={{
          ...base,
          delegation: {
            ...base.delegation,
            settings: [{ duty: "Tactics", delegated: true, confirm_major: false }],
            log: [
              {
                date: "2026-08-04",
                duty: "Tactics",
                title_key: "delegation.log.managerOverride",
                params: [["until", "2026-08-07"]],
                outcome: "leftToManager",
              },
            ],
          },
        }}
        onGameUpdate={vi.fn()}
      />,
    );

    expect(screen.getByTestId("duty-Tactics")).toBeInTheDocument();
    const entry = screen.getByText(/delegation\.log\.managerOverride/);
    expect(entry).toHaveTextContent('"until":"2026-08-07"');
    expect(entry).toHaveTextContent("staff.delegation.outcome.leftToManager");
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
    expect(screen.getByTestId("duty-PlayerRecruitment")).toBeInTheDocument();
  });

  /**
   * Given the assistant works with the manager's approval
   * When the manager picks full delegation
   * Then nothing is sent yet: a warning says transfers can complete without approval, and only
   *      confirming switches the whole job over
   */
  it("warns before switching the whole job to full delegation, and switches on confirm", async () => {
    const onGameUpdate = vi.fn();
    render(<StaffManagementPanel overview={overview()} onGameUpdate={onGameUpdate} />);

    expect(screen.getByRole("radio", { name: "staff.delegation.mode.approval" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "staff.delegation.mode.full" }));

    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "staff.delegation.mode.fullWarningBody",
    );
    expect(invokeMock).not.toHaveBeenCalledWith("set_delegation_mode", expect.anything());

    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.mode.fullConfirm" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_delegation_mode", { mode: "full" }),
    );
    expect(onGameUpdate).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  /**
   * Given the warning is showing
   * When the manager keeps his approval
   * Then the warning closes and the mode is never sent
   */
  it("does_not_change_the_mode_when_the_warning_is_declined", () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    fireEvent.click(screen.getByRole("radio", { name: "staff.delegation.mode.full" }));
    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.mode.fullCancel" }));

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("set_delegation_mode", expect.anything());
  });

  /**
   * Given the assistant works with approval
   * When the manager returns to manual
   * Then it switches at once: only handing over everything needs the warning
   */
  it("switches_to_manual_without_a_warning", async () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    fireEvent.click(screen.getByRole("radio", { name: "staff.delegation.mode.manual" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_delegation_mode", { mode: "manual" }),
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
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
      <StaffManagementPanel
        overview={overview({ has_assistant: false, duties: [] })}
        onGameUpdate={vi.fn()}
      />,
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

  /**
   * Given a proposal the backend drops when it cannot carry it out (the budget moved, the member
   *       signed elsewhere),
   * when the manager accepts it and the backend refuses,
   * then the reason is shown and the game is read again, so the dead proposal leaves the list and
   *      the dashboard's count instead of waiting there to fail a second time.
   */
  it("reads the game again after a refused answer, because the proposal is gone", async () => {
    const afterRefusal = { delegation: { proposals: [] } } as unknown as GameStateData;
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_assistant_recommendations") return [];
      if (command === "resolve_delegation_proposal") throw "be.error.staff.offerRefused";
      if (command === "get_active_game") return afterRefusal;
      return {} as GameStateData;
    });
    const onGameUpdate = vi.fn();
    render(<StaffManagementPanel overview={overview()} onGameUpdate={onGameUpdate} />);

    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.accept" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await waitFor(() => expect(onGameUpdate).toHaveBeenCalledWith(afterRefusal));
  });

  /**
   * Given a refusal and a game that cannot be read again,
   * when the manager accepts,
   * then the reason still shows: the resync is a courtesy and never hides the error.
   */
  it("still shows the reason when the game cannot be read again", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_assistant_recommendations") return [];
      throw "be.error.staff.offerRefused";
    });
    const onGameUpdate = vi.fn();
    render(<StaffManagementPanel overview={overview()} onGameUpdate={onGameUpdate} />);

    fireEvent.click(screen.getByRole("button", { name: "staff.delegation.accept" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onGameUpdate).not.toHaveBeenCalled();
  });

  /**
   * Given a proposal from the recruitment duty, made on 1 August,
   * when it is listed,
   * then it says which duty it belongs to and when it was made, in the language's own date
   *      format, not as a raw ISO string: the manager sees who is asking.
   */
  it("names the duty and the date a proposal comes from", () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    const proposal = screen.getByTestId("proposal-prop-1");

    expect(proposal).toHaveTextContent("staff.delegation.duties.StaffRecruitment");
    expect(proposal).toHaveTextContent("Aug 1, 2026");
  });

  /**
   * Given several proposals and duties whose buttons all read Accept, Decline, Delegate,
   * when a screen reader lands on one,
   * then its description says which proposal or duty it answers, without changing its name.
   */
  it("describes each repeated button by the proposal or duty it belongs to", () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);

    const proposal = within(screen.getByTestId("proposal-prop-1"));
    const title = /^delegation\.proposal\.hire/;
    expect(
      proposal.getByRole("button", { name: "staff.delegation.accept" }),
    ).toHaveAccessibleDescription(title);
    expect(
      proposal.getByRole("button", { name: "staff.delegation.decline" }),
    ).toHaveAccessibleDescription(title);

    const training = within(screen.getByTestId("duty-Training"));
    expect(
      training.getByRole("button", { name: "staff.delegation.takeBack" }),
    ).toHaveAccessibleDescription("staff.delegation.duties.Training");
    const scouting = within(screen.getByTestId("duty-Scouting"));
    expect(
      scouting.getByRole("button", { name: "staff.delegation.delegate" }),
    ).toHaveAccessibleDescription("staff.delegation.duties.Scouting");
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
  /**
   * Given a club where training can be run by the assistant or a fitness coach
   * When the manager picks the coach, then the assistant again
   * Then each choice is sent to the backend: the coach by id, the assistant as "nobody named".
   */
  it("puts a staff member in charge of a duty, and hands it back to the assistant", async () => {
    render(<StaffManagementPanel overview={overview()} onGameUpdate={vi.fn()} />);
    const select = screen.getByRole("combobox", { name: /staff\.delegation\.owner\.aria/ });

    fireEvent.click(select);
    fireEvent.click(screen.getByRole("option", { name: /Ana Pires/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_duty_owner", {
        duty: "Training",
        staffId: "fit-1",
      }),
    );

    fireEvent.click(screen.getByRole("combobox", { name: /staff\.delegation\.owner\.aria/ }));
    fireEvent.click(screen.getByRole("option", { name: /Ada Lovelace/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_duty_owner", {
        duty: "Training",
        staffId: null,
      }),
    );
  });

  /**
   * Given the lineup and training both handed over
   * When the duty list renders
   * Then "ask me" is offered for training, which has a step to put to the manager, and not for
   *      the lineup, which acts at once in every mode.
   */
  it("offers 'ask me' only where the duty has something to ask", () => {
    const base = overview();
    render(
      <StaffManagementPanel
        overview={{
          ...base,
          delegation: {
            ...base.delegation,
            settings: [
              { duty: "Training", delegated: true, confirm_major: true },
              { duty: "Lineup", delegated: true, confirm_major: true },
            ],
          },
        }}
        onGameUpdate={vi.fn()}
      />,
    );
    expect(within(screen.getByTestId("duty-Training")).getByRole("checkbox")).toBeInTheDocument();
    expect(within(screen.getByTestId("duty-Lineup")).queryByRole("checkbox")).toBeNull();
  });

  /**
   * Given a duty nobody in post can run
   * When the list renders
   * Then it says so and the duty cannot be handed over.
   */
  it("says when nobody can run a duty and refuses to hand it over", () => {
    const base = overview();
    render(
      <StaffManagementPanel
        overview={{
          ...base,
          duties: base.duties.map((d) =>
            d.duty === "Youth" ? { ...d, in_charge: null, candidates: [] } : d,
          ),
        }}
        onGameUpdate={vi.fn()}
      />,
    );
    const row = screen.getByTestId("duty-Youth");
    expect(within(row).getByText("staff.delegation.owner.nobody")).toBeInTheDocument();
    expect(within(row).getByRole("button")).toBeDisabled();
  });

  /**
   * Given a proposal to raise the training load
   * When it renders
   * Then the focus and both intensities are shown by their translated names, not as raw ids.
   */
  it("names the focus and intensities of a training-load proposal", () => {
    const base = overview();
    render(
      <StaffManagementPanel
        overview={{
          ...base,
          delegation: {
            ...base.delegation,
            proposals: [
              {
                id: "prop-9",
                duty: "Training",
                created: "2026-08-04",
                action: { kind: "setTraining" },
                title_key: "delegation.proposal.trainingLoad.title",
                params: [
                  ["focus", "Tactical"],
                  ["intensity", "High"],
                  ["currentIntensity", "Medium"],
                ],
                reason_key: "delegation.proposal.trainingLoad.reason",
              },
            ],
          },
        }}
        onGameUpdate={vi.fn()}
      />,
    );
    const item = screen.getByTestId("proposal-prop-9");
    expect(item).toHaveTextContent("training.intensities.High.label");
    expect(item).toHaveTextContent("training.focuses.Tactical.label");
    expect(item).toHaveTextContent("training.intensities.Medium.label");
  });
});
