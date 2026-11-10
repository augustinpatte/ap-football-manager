import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import type { GameStateData, StaffData } from "../../store/gameStore";
import { createSeasonContext, createTeam } from "../../test-utils/factories";
import StaffTab from "./StaffTab";

/**
 * The Staff screen's keyboard and screen-reader contract, with the real strings: which control says
 * it is on, what each icon-only button is called, and that every control shows where focus is.
 */

const overviewFixture = {
  team_id: "team-1",
  posts: [],
  members: [
    {
      id: "staff-own",
      post: "FirstTeamCoach",
      overall: 70,
      days_left: 400,
      severance: 12345,
      renewal_ask: 900,
    },
  ],
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

function createStaff(overrides: Partial<StaffData> = {}): StaffData {
  return {
    id: "staff-own",
    first_name: "Alex",
    last_name: "Coach",
    display_name: "Alex Coach",
    date_of_birth: "1980-01-01",
    nationality: "GB",
    role: "Coach",
    attributes: { coaching: 70, judgingAbility: 50, judgingPotential: 55, physiotherapy: 30 },
    team_id: "team-1",
    specialization: "Youth",
    wage: 1200,
    contract_end: "2027-06-30",
    ...overrides,
  };
}

const ownCoach = createStaff();
const freeScout = createStaff({
  id: "staff-free",
  first_name: "Sam",
  last_name: "Scout",
  display_name: "Sam Scout",
  role: "Scout",
  team_id: null,
});
const staff = [ownCoach, freeScout];

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
    staff,
    messages: [],
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
  };
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string) => {
    switch (command) {
      case "get_staff":
        return {
          team_staff: [ownCoach],
          available_staff: [freeScout],
          scouting_assignments: [],
          youth_scouting_assignments: [],
        };
      case "get_staff_overview":
        return overviewFixture;
      case "search_staff_candidates":
        return [{ staff: freeScout, asking_wage: 800, blocker: null }];
      case "get_assistant_recommendations":
        return [];
      default:
        throw new Error(`unexpected command ${command}`);
    }
  },
}));

async function renderStaffTab() {
  render(<StaffTab gameState={game()} />);
  await screen.findByText("Alex Coach");
  // The contract line and the renew button wait for the management overview.
  await screen.findByRole("button", { name: "Renew contract" });
}

const FOCUS_RING = "focus-visible:ring-focus";

describe("the Staff screen's keyboard and screen-reader contract", () => {
  beforeEach(async () => {
    await i18nReady;
    await i18n.changeLanguage("en");
  });

  /**
   * Given the Staff tab on My staff,
   * when the manager reads the three view buttons,
   * then exactly the open view says it is pressed, and the state follows a click.
   */
  it("says which of the three views is open", async () => {
    await renderStaffTab();
    const mine = screen.getByRole("button", { name: /^my staff/i });
    const available = screen.getByRole("button", { name: /^available/i });
    const management = screen.getByRole("button", { name: /^management/i });

    expect(mine).toHaveAttribute("aria-pressed", "true");
    expect(available).toHaveAttribute("aria-pressed", "false");
    expect(management).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(available);

    expect(mine).toHaveAttribute("aria-pressed", "false");
    expect(available).toHaveAttribute("aria-pressed", "true");
  });

  /**
   * Given the role filter chips,
   * when none is chosen, then "All" is pressed;
   * when one role is chosen, then only that chip is pressed.
   */
  it("says which role filter is on", async () => {
    await renderStaffTab();
    const all = screen.getByRole("button", { name: "All" });
    const scout = screen.getByRole("button", { name: /^scout$/i });
    expect(all).toHaveAttribute("aria-pressed", "true");
    expect(scout).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(scout);

    expect(all).toHaveAttribute("aria-pressed", "false");
    expect(scout).toHaveAttribute("aria-pressed", "true");
  });

  /**
   * Given the search box,
   * when a screen reader looks for it by name,
   * then it is found without relying on the placeholder, and typing filters the list.
   */
  it("names the search box", async () => {
    await renderStaffTab();
    const search = screen.getByRole("searchbox", { name: "Search staff..." });

    fireEvent.change(search, { target: { value: "zzz" } });

    expect(screen.queryByText("Alex Coach")).not.toBeInTheDocument();
  });

  /**
   * Given a staff card on My staff,
   * when its icon-only buttons are listed,
   * then each one has a name a screen reader can say, not just a hover tooltip.
   */
  it("names every icon-only button on a card", async () => {
    await renderStaffTab();
    const card = screen.getByTestId("staff-card-staff-own");

    expect(within(card).getByRole("button", { name: "Renew contract" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Release staff member" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^available/i }));
    const freeCard = await screen.findByTestId("staff-card-staff-free");
    expect(within(freeCard).getByRole("button", { name: "Hire staff member" })).toBeInTheDocument();
  });

  /**
   * Given every button on the screen in each of its views,
   * when the class list is read,
   * then each carries the project focus ring, so a keyboard user can see where they are.
   */
  it("draws the project focus ring on every button and the search box", async () => {
    await renderStaffTab();
    const missing: string[] = [];
    const check = (label: string, element: HTMLElement) => {
      if (!element.className.includes(FOCUS_RING)) missing.push(label);
    };
    const checkAll = (view: string) => {
      for (const button of screen.getAllByRole("button")) {
        check(`${view}: ${button.textContent || button.getAttribute("aria-label")}`, button);
      }
    };

    checkAll("my staff");
    check("search", screen.getByRole("searchbox", { name: "Search staff..." }));
    fireEvent.click(screen.getByRole("button", { name: /^available/i }));
    await screen.findByTestId("staff-card-staff-free");
    checkAll("available");
    fireEvent.click(screen.getByRole("button", { name: /^management/i }));
    await screen.findByText("Delegation");
    checkAll("management");

    expect(missing).toEqual([]);
  });

  /**
   * Given a status region that is on the page before anything is said,
   * when a search matches nobody,
   * then the empty message arrives inside that same region, which is what a screen reader
   *      announces, and it is gone again when the search is cleared.
   */
  it("announces an empty result through a status region that was already there", async () => {
    await renderStaffTab();
    const region = screen.getByRole("status");
    expect(region).toBeEmptyDOMElement();
    const search = screen.getByRole("searchbox", { name: "Search staff..." });

    fireEvent.change(search, { target: { value: "zzz" } });

    expect(screen.getByRole("status")).toBe(region);
    await waitFor(() => expect(region).toHaveTextContent(/no staff members match/i));

    fireEvent.change(search, { target: { value: "" } });

    expect(region).toBeEmptyDOMElement();
  });
});
