import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import type { GameStateData } from "../../store/gameStore";
import { createSeasonContext, createTeam } from "../../test-utils/factories";
import DashboardAlerts from "./DashboardAlerts";
import type { DashboardAlert } from "./dashboardHelpers";
import {
  createDashboardProfileNavigationState,
  navigateDashboardProfiles,
} from "./dashboardProfileNavigation";
import { createDashboardTabContentModel } from "./dashboardTabContentModel";
import DashboardWorkspaceContent from "./DashboardWorkspaceContent";

vi.mock("./DashboardTabContent", () => ({
  default: ({ viewModel }: { viewModel: { activeTab: string } }) => (
    <button type="button">Content of {viewModel.activeTab}</button>
  ),
}));

const SQUAD_ALERT: DashboardAlert = {
  id: "xi",
  text: "Your starting XI is incomplete",
  tab: "Squad",
  severity: "warn",
};

function game(): GameStateData {
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
  };
}

/** The workspace as the dashboard drives it: one navigation state, the alerts handing it a tab. */
function Workspace({ alerts }: { alerts: DashboardAlert[] }) {
  const gameState = game();
  const [navigation, setNavigation] = useState(createDashboardProfileNavigationState("Home"));
  return (
    <DashboardWorkspaceContent
      dashboardAlerts={alerts}
      gameState={gameState}
      profileNavigation={navigation}
      contentLabel={`Screen ${navigation.activeTab}`}
      dashboardTabContentModel={createDashboardTabContentModel({
        activeTab: navigation.activeTab,
        gameState,
        seasonComplete: false,
        visitedOnboardingTabs: new Set<string>(),
        initialMessageId: null,
        handlers: {
          onSelectPlayer: vi.fn(),
          onSelectTeam: vi.fn(),
          onGameUpdate: vi.fn(),
          onNavigate: vi.fn(),
        },
      })}
      onNavigate={(tab) => setNavigation((current) => navigateDashboardProfiles(current, tab))}
      onSelectPlayer={vi.fn()}
      onSelectTeam={vi.fn()}
      onGameUpdate={vi.fn()}
      isUnemployed={false}
    />
  );
}

describe("dashboard alerts for keyboard and screen-reader users", () => {
  /**
   * Given a dashboard with no alert,
   * when an alert appears later,
   * then it lands inside a polite status region that was already on the page, which is the only
   * way a screen reader announces it.
   */
  it("keeps a polite status region on the page while there is nothing to say", () => {
    const { rerender } = render(<DashboardAlerts alerts={[]} onNavigate={vi.fn()} />);
    const region = screen.getByRole("status");
    expect(region).toBeEmptyDOMElement();

    rerender(<DashboardAlerts alerts={[SQUAD_ALERT]} onNavigate={vi.fn()} />);

    expect(screen.getByRole("status")).toBe(region);
    expect(region).toHaveTextContent("Your starting XI is incomplete");
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  /**
   * Given an alert that sends the manager to the Squad tab,
   * when it is activated from the keyboard,
   * then focus moves to the screen it opened, labelled with that screen's name, rather than
   * staying on the alert above it.
   */
  it("moves focus to the opened screen when an alert is activated", () => {
    render(<Workspace alerts={[SQUAD_ALERT]} />);
    const alert = screen.getByRole("button", { name: /starting XI is incomplete/i });
    alert.focus();

    fireEvent.click(alert);

    const screenRegion = screen.getByRole("region", { name: "Screen Squad" });
    expect(screenRegion).toHaveFocus();
    expect(screenRegion).toHaveTextContent("Content of Squad");
  });

  /**
   * Given the manager never used an alert,
   * when the dashboard first renders,
   * then focus is left where it was: the screen does not steal it on load.
   */
  it("does not take focus when nothing was activated", () => {
    render(<Workspace alerts={[SQUAD_ALERT]} />);

    expect(screen.getByRole("region", { name: "Screen Home" })).not.toHaveFocus();
  });

  /**
   * Given focus moved to the screen after one alert,
   * when a second alert for another tab is activated,
   * then focus moves again to the new screen.
   */
  it("moves focus again on a second alert", async () => {
    const second: DashboardAlert = {
      ...SQUAD_ALERT,
      id: "fin",
      text: "Money is tight",
      tab: "Finances",
    };
    render(<Workspace alerts={[SQUAD_ALERT, second]} />);

    fireEvent.click(screen.getByRole("button", { name: /starting XI/i }));
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: /money is tight/i }));

    expect(screen.getByRole("region", { name: "Screen Finances" })).toHaveFocus();
  });
});
