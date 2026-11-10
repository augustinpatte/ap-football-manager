import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import type { GameStateData } from "../../store/gameStore";
import { createSeasonContext, createTeam } from "../../test-utils/factories";
import StaffTab from "./StaffTab";

/**
 * What the Management view shows while the backend has not answered, when it cannot, and after
 * the manager asks again. It used to draw nothing in all three cases.
 */

const overviewFixture = {
  team_id: "team-1",
  posts: [],
  members: [],
  budget: {
    budget: 50000,
    weekly_bill: 10000,
    headroom: 40000,
    signing_fee_weeks: 1,
    max_years: 3,
    severance_pct: 50,
  },
  delegation: {
    settings: [],
    proposals: [],
    log: [],
    limits: { max_staff_wage: 0, max_player_wage: 0, max_commitment: 0, max_transfer_fee: 0 },
  },
  delegation_mode: "manual",
  has_assistant: false,
};

/** How the next get_staff_overview calls answer; a test sets it and the fake reads it each time. */
const backend = {
  overview: "ok" as "ok" | "pending" | "fail",
  overviewCalls: 0,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    switch (command) {
      case "get_staff":
        return Promise.resolve({
          team_staff: [],
          available_staff: [],
          scouting_assignments: [],
          youth_scouting_assignments: [],
        });
      case "get_staff_overview":
        backend.overviewCalls += 1;
        if (backend.overview === "pending") return new Promise(() => {});
        if (backend.overview === "fail") return Promise.reject("be.error.noActiveGameSession");
        return Promise.resolve(overviewFixture);
      case "search_staff_candidates":
      case "get_assistant_recommendations":
        return Promise.resolve([]);
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  },
}));

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

async function openManagement() {
  render(<StaffTab gameState={game()} />);
  fireEvent.click(await screen.findByRole("button", { name: /^management/i }));
}

describe("the Staff Management view while the backend answers, fails and is asked again", () => {
  beforeEach(async () => {
    await i18nReady;
    await i18n.changeLanguage("en");
    backend.overview = "ok";
    backend.overviewCalls = 0;
  });

  /** Given an answer that has not arrived, when Management is open, then it says it is loading. */
  it("says it is loading while the overview has not arrived", async () => {
    backend.overview = "pending";

    await openManagement();

    expect(await screen.findByRole("status")).toHaveTextContent("Loading...");
  });

  /**
   * Given the backend refuses the overview,
   * when Management is open,
   * then the manager is told why in their language, with a way to ask again.
   */
  it("says why it could not load and offers a retry", async () => {
    backend.overview = "fail";

    await openManagement();

    expect(await screen.findByRole("alert")).toHaveTextContent("No active game session");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  /**
   * Given a failed load,
   * when the backend recovers and the manager presses Retry,
   * then the overview is asked for again and the panel replaces the error.
   */
  it("loads the panel after Retry once the backend answers", async () => {
    backend.overview = "fail";
    await openManagement();
    await screen.findByRole("alert");
    const callsBeforeRecovery = backend.overviewCalls;
    backend.overview = "ok";

    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));

    expect(await screen.findByText("Delegation")).toBeInTheDocument();
    expect(backend.overviewCalls).toBeGreaterThan(callsBeforeRecovery);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
