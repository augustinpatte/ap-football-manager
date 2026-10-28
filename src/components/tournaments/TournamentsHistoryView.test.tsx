import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { EditionSummaryData, LeagueData } from "../../store/types";
import TournamentsHistoryView, { seasonSpan } from "./TournamentsHistoryView";
import type { TournamentsTeamLookup } from "./teamLookup";

vi.mock("react-i18next", async () => {
  const { useRef } = await import("react");
  return {
    useTranslation: () => {
      useRef(null);
      return {
        t: (key: string, params?: Record<string, unknown>) =>
          params ? `${key}:${JSON.stringify(params)}` : key,
      };
    },
  };
});

const teams: TournamentsTeamLookup = {
  userTeamId: null,
  isClubTeam: () => true,
  resolveTeamName: (id) => id.toUpperCase(),
  onSelectTeam: vi.fn(),
};

const competition = {
  id: "cup",
  name: "Cup",
  season: 2027,
  fixtures: [],
  standings: [],
} as LeagueData;

const edition = (overrides: Partial<EditionSummaryData>): EditionSummaryData => ({
  competition_id: "cup",
  season: 2026,
  completed_on: "2027-05-22",
  champion_id: "lens",
  runner_up_id: "psg",
  final_result: null,
  movements: null,
  ...overrides,
});

describe("seasonSpan", () => {
  it("writes the season that starts in 2026 as 2026/27, across the century", () => {
    expect(seasonSpan(2026)).toBe("2026/27");
    expect(seasonSpan(2099)).toBe("2099/00");
  });
});

describe("TournamentsHistoryView", () => {
  it("says so when no season has finished", () => {
    render(<TournamentsHistoryView competition={competition} history={[]} teams={teams} />);
    expect(screen.getByTestId("tournaments-history-empty")).toBeTruthy();
  });

  it("lists the winner and the final's score, penalties included", () => {
    render(
      <TournamentsHistoryView
        competition={competition}
        history={[
          edition({
            final_result: {
              home_team_id: "lens",
              away_team_id: "psg",
              home_goals: 1,
              away_goals: 1,
              home_penalties: 4,
              away_penalties: 3,
            },
          }),
          edition({ competition_id: "other", champion_id: "elsewhere" }),
        ]}
        teams={teams}
      />,
    );
    expect(screen.getByTestId("tournaments-history-champion-2026").textContent).toBe("LENS");
    expect(screen.getByTestId("tournaments-history-final-2026").textContent).toContain(
      "LENS 1–1 PSG",
    );
    expect(screen.getByTestId("tournaments-history-final-2026").textContent).toContain("4");
    expect(screen.queryByText("ELSEWHERE")).toBeNull();
  });

  it("shows the clubs a division sent down and the lower division's promoted ones", () => {
    const movements = {
      upper_competition_id: "top",
      lower_competition_id: "second",
      season: 2026,
      promoted_direct: ["a", "b"],
      promoted_playoff: ["c"],
      relegated_direct: ["x", "y"],
      relegated_barrage: ["z"],
      ties: [],
      reserves_blocked: ["castilla"],
      reserves_forced_down: [],
      reserves_readmitted: [],
    };
    const upper = { ...competition, id: "top" } as LeagueData;
    const lower = { ...competition, id: "second" } as LeagueData;
    const history = [
      edition({ competition_id: "top", movements }),
      edition({ competition_id: "second", movements }),
    ];
    const { unmount } = render(
      <TournamentsHistoryView competition={upper} history={history} teams={teams} />,
    );
    expect(screen.getByTestId("tournaments-history-moves-2026").textContent).toBe("X, Y, Z");
    unmount();
    render(<TournamentsHistoryView competition={lower} history={history} teams={teams} />);
    expect(screen.getByTestId("tournaments-history-moves-2026").textContent).toBe("A, B, C");
    expect(screen.getByText("CASTILLA")).toBeTruthy();
  });
});
