import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import type { GameStateData } from "../../store/gameStore";
import { createSeasonContext, createTeam } from "../../test-utils/factories";
import StaffTab from "../staff/StaffTab";
import DashboardAlerts from "./DashboardAlerts";
import { getDashboardAlerts } from "./dashboardHelpers";

/**
 * Given the assistant is holding proposals for the manager, these tests walk the path the manager
 * walks: the dashboard alert, the Staff screen it opens, the answer, and the alert again. The
 * real alert model, alert list and Staff tab are mounted; only the backend is replaced, by a
 * fake that keeps the proposals and hands the changed game back, as `resolve_delegation_proposal`
 * does.
 */

interface FakeProposal {
  id: string;
}

const backend = {
  pending: [] as FakeProposal[],
  resolved: [] as { proposalId: string; accept: boolean }[],
  /** When set, the next answer is refused after the backend has dropped the proposal, as it does. */
  refuseNextAnswer: false,
};

function gameWithPending(): GameStateData {
  return {
    season_context: createSeasonContext(),
    clock: { current_date: "2026-08-10T00:00:00Z", start_date: "2026-07-01T00:00:00Z" },
    manager: {
      display_name: "Jane Doe",
      id: "manager-1",
      first_name: "Jane",
      last_name: "Doe",
      date_of_birth: "1980-01-01",
      nationality: "GB",
      reputation: 50,
      satisfaction: 50,
      fan_approval: 50,
      team_id: "team-1",
      career_stats: {
        matches_managed: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        trophies: 0,
        best_finish: null,
      },
      career_history: [],
    },
    teams: [createTeam()],
    players: [],
    staff: [],
    messages: [],
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
    delegation: { protected_players: [], proposals: backend.pending.map(({ id }) => ({ id })) },
  };
}

function overviewFromBackend() {
  return {
    team_id: "team-1",
    posts: [],
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
      settings: [],
      proposals: backend.pending.map(({ id }) => ({
        id,
        duty: "StaffRecruitment",
        created: "2026-08-01",
        action: { kind: "hireStaff" },
        title_key: "delegation.proposal.hire",
        params: [
          ["name", "Sam Reed"],
          ["post", "Physio"],
          ["wage", "1,200"],
        ],
        reason_key: "delegation.reason.vacancy",
      })),
      log: [],
      limits: {
        max_staff_wage: 5000,
        max_player_wage: 15000,
        max_commitment: 3000000,
        max_transfer_fee: 0,
      },
    },
    delegation_mode: "approval",
    has_assistant: true,
  };
}

const invokeMock = vi.fn(async (command: string, args?: Record<string, unknown>) => {
  switch (command) {
    case "get_staff":
      return {
        team_staff: [],
        available_staff: [],
        scouting_assignments: [],
        youth_scouting_assignments: [],
      };
    case "get_staff_overview":
      return overviewFromBackend();
    case "search_staff_candidates":
    case "get_assistant_recommendations":
      return [];
    case "resolve_delegation_proposal": {
      const proposalId = String(args?.proposalId);
      backend.resolved.push({ proposalId, accept: args?.accept === true });
      backend.pending = backend.pending.filter((proposal) => proposal.id !== proposalId);
      if (backend.refuseNextAnswer) {
        backend.refuseNextAnswer = false;
        throw "be.error.staff.offerRefused";
      }
      return gameWithPending();
    }
    case "get_active_game":
      return gameWithPending();
    default:
      throw new Error(`unexpected command ${command}`);
  }
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => invokeMock(command, args),
}));

/** The part of the dashboard under test: alerts above, the open tab below, one shared game. */
function DashboardHost({ initial }: { initial: GameStateData }) {
  const { t } = useTranslation();
  const [game, setGame] = useState(initial);
  const [tab, setTab] = useState("Home");
  const alerts = getDashboardAlerts(game, false, t);
  return (
    <div>
      <DashboardAlerts alerts={alerts} onNavigate={setTab} />
      {tab === "Staff" ? <StaffTab gameState={game} onGameUpdate={setGame} /> : <p>Home screen</p>}
    </div>
  );
}

function renderDashboard(proposalIds: string[]) {
  backend.pending = proposalIds.map((id) => ({ id }));
  backend.resolved = [];
  backend.refuseNextAnswer = false;
  return render(<DashboardHost initial={gameWithPending()} />);
}

const EN_ALERT = /staff proposal\(s\) waiting for your decision/i;
const FR_ALERT = /proposition\(s\) du staff en attente de votre décision/i;

describe("delegation proposals, from the dashboard alert to the answer", () => {
  beforeEach(async () => {
    await i18nReady;
    await i18n.changeLanguage("en");
    invokeMock.mockClear();
  });

  it("raises a dashboard alert with the number of pending proposals", () => {
    renderDashboard(["prop-1", "prop-2"]);

    expect(screen.getByRole("button", { name: /^2 staff proposal/i })).toBeInTheDocument();
  });

  it("raises no alert when the assistant holds nothing", () => {
    renderDashboard([]);

    expect(screen.queryByRole("button", { name: EN_ALERT })).not.toBeInTheDocument();
  });

  it("opens the Staff screen on the proposals when the alert is clicked", async () => {
    renderDashboard(["prop-1"]);

    fireEvent.click(screen.getByRole("button", { name: EN_ALERT }));

    expect(await screen.findByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Decline" })).toBeInTheDocument();
    expect(screen.queryByText("Home screen")).not.toBeInTheDocument();
  });

  it("answers yes to one proposal and no to another, and the alert count follows", async () => {
    renderDashboard(["prop-1", "prop-2"]);
    fireEvent.click(screen.getByRole("button", { name: EN_ALERT }));

    fireEvent.click((await screen.findAllByRole("button", { name: "Accept" }))[0]);
    await waitFor(() => expect(backend.resolved).toEqual([{ proposalId: "prop-1", accept: true }]));
    expect(await screen.findByRole("button", { name: /^1 staff proposal/i })).toBeInTheDocument();

    fireEvent.click(await screen.findByRole("button", { name: "Decline" }));
    await waitFor(() =>
      expect(backend.resolved).toEqual([
        { proposalId: "prop-1", accept: true },
        { proposalId: "prop-2", accept: false },
      ]),
    );
  });

  it("removes the alert once the last pending proposal is answered", async () => {
    renderDashboard(["prop-1"]);
    fireEvent.click(screen.getByRole("button", { name: EN_ALERT }));

    fireEvent.click(await screen.findByRole("button", { name: "Decline" }));

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: EN_ALERT })).not.toBeInTheDocument(),
    );
    expect(screen.queryByRole("button", { name: "Decline" })).not.toBeInTheDocument();
  });

  /**
   * Given two proposals, and an accept the backend refuses after dropping that proposal,
   * when the manager accepts the first,
   * then the reason shows, the dead proposal leaves the list, and the dashboard alert counts only
   *      the one that is left.
   */
  it("drops a proposal the backend could not carry out, from the list and the alert", async () => {
    renderDashboard(["prop-1", "prop-2"]);
    backend.refuseNextAnswer = true;
    fireEvent.click(screen.getByRole("button", { name: EN_ALERT }));

    fireEvent.click((await screen.findAllByRole("button", { name: "Accept" }))[0]);

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /^1 staff proposal/i })).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Accept" })).toHaveLength(1));
  });

  it("reads in French: the alert, the screen it opens and the two answers", async () => {
    await i18n.changeLanguage("fr");
    renderDashboard(["prop-1", "prop-2"]);

    fireEvent.click(screen.getByRole("button", { name: FR_ALERT }));

    expect(await screen.findAllByRole("button", { name: "Accepter" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Refuser" })).toHaveLength(2);
    expect(
      screen.queryByText(/dashboard\.alerts|staff\.delegation|delegation\.(proposal|reason)/),
    ).not.toBeInTheDocument();
  });
});
