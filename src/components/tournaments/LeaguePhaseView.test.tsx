import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import LeaguePhaseOverview from "./LeaguePhaseOverview";
import LeaguePhaseView from "./LeaguePhaseView";
import type { TournamentsTeamLookup } from "./teamLookup";
import type {
  FixtureData,
  KnockoutTieData,
  LeagueData,
  LeaguePhaseData,
  LeaguePhaseRowData,
} from "../../store/types";

vi.mock("react-i18next", async () => {
  const { useRef } = await import("react");
  return {
    useTranslation: () => {
      useRef(null);
      return {
        t: (key: string, options?: Record<string, unknown>) =>
          options ? `${key} ${JSON.stringify(options)}` : key,
      };
    },
  };
});

vi.mock("./TournamentsTopScorers", () => ({
  default: () => <div data-testid="top-scorers" />,
}));

const teams: TournamentsTeamLookup = {
  userTeamId: "club-03",
  isClubTeam: () => true,
  resolveTeamName: (id) => `Name of ${id}`,
  onSelectTeam: vi.fn(),
};

function row(index: number): LeaguePhaseRowData {
  return {
    team_id: `club-${String(index).padStart(2, "0")}`,
    position: index + 1,
    played: 8,
    won: 4,
    drawn: 2,
    lost: 2,
    goals_for: 12,
    goals_against: 8,
    away_goals_for: 5,
    away_wins: 2,
    points: 14,
    opponents_points: 0,
    opponents_goal_difference: 0,
    opponents_goals_for: 0,
    disciplinary_points: 0,
    coefficient: null,
  };
}

function fixture(id: string, home: string, away: string, score?: [number, number]): FixtureData {
  return {
    id,
    matchday: 9,
    date: "2027-02-16",
    home_team_id: home,
    away_team_id: away,
    competition: "ContinentalClub",
    competitive: true,
    advancing_team_id: null,
    status: score ? "Completed" : "Scheduled",
    result: score
      ? { home_goals: score[0], away_goals: score[1], home_scorers: [], away_scorers: [] }
      : null,
  };
}

function state(overrides: Partial<LeaguePhaseData> = {}): LeaguePhaseData {
  return {
    stage: "LeaguePhaseInProgress",
    calendar_year: 2026,
    seeding: [],
    pots: [],
    table: Array.from({ length: 36 }, (_, i) => row(i)),
    outcome: null,
    ties: [],
    champion_id: null,
    draw_error: null,
    ...overrides,
  };
}

function competition(
  phase: LeaguePhaseData,
  fixtures: FixtureData[] = [],
  participants = ["club-03"],
): LeagueData {
  return {
    id: "cl",
    name: "Champions",
    season: 2026,
    fixtures,
    standings: [],
    participant_ids: participants,
    rules: {
      format: "LeaguePhaseAndKnockout",
      counts_in_season_flow: true,
      league_phase: { pot_count: 6, opponents_per_pot: 1, direct_places: 4, playoff_places: 8 },
    },
    league_phase: phase,
  } as unknown as LeagueData;
}

function render_(phase: LeaguePhaseData, fixtures: FixtureData[] = []) {
  return render(
    <LeaguePhaseView competition={competition(phase, fixtures)} state={phase} teams={teams} />,
  );
}

describe("LeaguePhaseView table", () => {
  /**
   * Given a 36-club league phase in progress
   * When the table is shown
   * Then every club has a row, in the backend's order, with the three zones sized by the rules
   *      named in text for assistive technology, and the table says it is provisional.
   */
  it("shows 36 rows in zones of 8, 16 and 12, and says the table is provisional", () => {
    render_(state());

    const rows = screen.getAllByTestId(/^leaguephase-standing-club-/);
    expect(rows).toHaveLength(36);
    const zones = rows.map((r) => r.getAttribute("data-zone"));
    // The places come from the competition's own rules (4 and 8 here), not from a constant.
    expect(zones.filter((z) => z === "direct")).toHaveLength(4);
    expect(zones.filter((z) => z === "playoff")).toHaveLength(8);
    expect(zones.filter((z) => z === "eliminated")).toHaveLength(24);
    expect(within(rows[0]).getByText("tournaments.leaguePhase.zones.direct")).toBeInTheDocument();
    expect(within(rows[4]).getByText("tournaments.leaguePhase.zones.playoff")).toBeInTheDocument();
    expect(
      within(rows[12]).getByText("tournaments.leaguePhase.zones.eliminated"),
    ).toBeInTheDocument();
    expect(screen.getByText("tournaments.leaguePhase.provisional")).toBeInTheDocument();
    expect(screen.getByRole("table")).toBeInTheDocument();
  });

  /**
   * Given a finished league phase whose ranking used the seeding order for a tie
   * When the table is shown
   * Then it is no longer provisional, and the note about missing coefficients appears.
   */
  it("drops the provisional note when the outcome is fixed and flags a coefficient fallback", () => {
    render_(
      state({
        stage: "KnockoutPlayoffs",
        outcome: {
          ranking: [],
          direct: [],
          playoff: [],
          eliminated: [],
          coefficient_fallback_used: true,
        },
      }),
    );

    expect(screen.queryByText("tournaments.leaguePhase.provisional")).not.toBeInTheDocument();
    expect(screen.getByText("tournaments.leaguePhase.coefficientFallback")).toBeInTheDocument();
  });

  /**
   * Given the managed club is in the field
   * When the view is shown
   * Then its next European match is named, and without one the view says so.
   */
  it("names the managed club's next European match", () => {
    render_(state(), [fixture("m1", "club-03", "club-07")]);
    expect(screen.getByTestId("leaguephase-next").textContent).toContain(
      "tournaments.leaguePhase.nextMatch",
    );
    expect(screen.getByTestId("leaguephase-next").textContent).toContain("Name of club-07");
  });

  it("says when the managed club has no European match left", () => {
    render_(state(), []);
    expect(screen.getByTestId("leaguephase-next").textContent).toBe(
      "tournaments.leaguePhase.noNextMatch",
    );
  });
});

describe("LeaguePhaseView manager information", () => {
  /**
   * Given the managed club sits fifth in a league phase still being played
   * When the view is shown
   * Then it says where the club stands and how many points the last play-off place has now.
   */
  it("shows the managed club's position and the play-off cut-off while the table is open", () => {
    render_(state());

    const text = screen.getByTestId("leaguephase-standing").textContent ?? "";
    expect(text).toContain("tournaments.leaguePhase.yourPosition");
    expect(text).toContain('"position":4');
    expect(text).toContain('"total":36');
    // The rules of this fixture put the last play-off place at 4 + 8 = 12.
    expect(text).toContain("tournaments.leaguePhase.cutoffPoints");
    expect(text).toContain('"position":12');
  });

  /**
   * Given a league phase whose outcome is fixed
   * When the view is shown
   * Then the cut-off is no longer offered: the table is final and the zones say it.
   */
  it("drops the cut-off once the outcome is fixed", () => {
    render_(
      state({
        stage: "KnockoutPlayoffs",
        outcome: {
          ranking: [],
          direct: [],
          playoff: [],
          eliminated: [],
          coefficient_fallback_used: false,
        },
      }),
    );
    expect(screen.getByTestId("leaguephase-standing").textContent).not.toContain("cutoffPoints");
  });

  /**
   * Given an edition seeded by reputation, then one seeded by coefficients, then one with a pot trade
   * When each is shown
   * Then the game's own draw is always declared, the seeding basis is the true one, and the trade
   *      names both clubs.
   */
  it("declares the draw as the game's, the seeding basis and any pot trade", () => {
    const { unmount } = render_(state());
    let notes = screen.getByTestId("leaguephase-notes").textContent ?? "";
    expect(notes).toContain("tournaments.leaguePhase.drawByGame");
    expect(notes).toContain("tournaments.leaguePhase.seededByReputation");
    expect(notes).not.toContain("seededByCoefficient");
    expect(notes).toContain("tournaments.leaguePhase.prizeNote");
    unmount();

    render_(
      state({
        coefficients: { "club-01": 4000 },
        pot_adjustments: [
          { moved_club_id: "club-04", exchanged_club_id: "club-09", from_pot: 0, to_pot: 1 },
        ],
      }),
    );
    notes = screen.getByTestId("leaguephase-notes").textContent ?? "";
    expect(notes).toContain("tournaments.leaguePhase.seededByCoefficient");
    expect(notes).not.toContain("seededByReputation");
    expect(notes).toContain("Name of club-04");
    expect(notes).toContain("Name of club-09");
  });

  /**
   * Given a final that has been drawn
   * When the bracket is shown
   * Then the final is marked as played at a neutral venue, in words, and a two-legged round is not.
   */
  it("marks only the final as a neutral venue", () => {
    const finalTie: KnockoutTieData = {
      id: "final",
      stage: "Final",
      slot: 0,
      path: 1,
      seeded_team_id: "club-01",
      unseeded_team_id: "club-02",
      fixture_ids: ["f"],
      resolution: null,
    };
    const semi: KnockoutTieData = {
      ...finalTie,
      id: "semi",
      stage: "SemiFinals",
      fixture_ids: ["s1", "s2"],
    };
    render_(state({ stage: "Final", ties: [semi, finalTie] }), [
      fixture("f", "club-01", "club-02"),
      fixture("s1", "club-02", "club-01"),
      fixture("s2", "club-01", "club-02"),
    ]);

    expect(
      within(screen.getByTestId("leaguephase-tie-final")).getByText(
        "tournaments.leaguePhase.neutralVenue",
      ),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("leaguephase-tie-semi")).queryByText(
        "tournaments.leaguePhase.neutralVenue",
      ),
    ).not.toBeInTheDocument();
  });
});

describe("LeaguePhaseView before and after", () => {
  /**
   * Given a competition whose draw could not be made
   * When the view is shown
   * Then it explains why in an alert instead of showing an empty table.
   */
  it("explains a failed draw instead of showing an empty table", () => {
    render_(
      state({ stage: "Created", table: [], draw_error: "be.error.leaguePhase.draw.infeasible" }),
    );

    expect(screen.getByRole("alert").textContent).toContain("be.error.leaguePhase.draw.infeasible");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.queryByTestId("leaguephase-next")).not.toBeNull();
  });

  /**
   * Given a competition whose draw has not been made and has no error
   * When the view is shown
   * Then it says the draw is pending, without an alert.
   */
  it("says a draw that has not been made yet is pending", () => {
    render_(state({ stage: "Created", table: [] }));

    expect(screen.getByText("tournaments.leaguePhase.drawNotMade")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  /**
   * Given a completed edition
   * When the view is shown
   * Then the champion is named and no next match is offered.
   */
  it("names the champion of a completed edition", () => {
    render_(
      state({
        stage: "Completed",
        champion_id: "club-01",
        outcome: {
          ranking: [],
          direct: [],
          playoff: [],
          eliminated: [],
          coefficient_fallback_used: false,
        },
      }),
    );

    expect(screen.getByTestId("leaguephase-champion").textContent).toContain("Name of club-01");
    expect(screen.queryByTestId("leaguephase-next")).not.toBeInTheDocument();
  });
});

describe("LeaguePhaseView bracket", () => {
  function tie(overrides: Partial<KnockoutTieData>): KnockoutTieData {
    return {
      id: "t1",
      stage: "Playoffs",
      slot: 0,
      path: 0,
      seeded_team_id: "club-09",
      unseeded_team_id: "club-23",
      fixture_ids: ["leg1", "leg2"],
      resolution: null,
      ...overrides,
    };
  }

  /**
   * Given play-offs drawn and unplayed
   * When the bracket is shown
   * Then the drawn round holds its tie and every later round says it is not drawn yet.
   */
  it("shows the drawn round and names the rest as not drawn", () => {
    render_(state({ stage: "KnockoutPlayoffs", ties: [tie({})] }), [
      fixture("leg1", "club-23", "club-09"),
      fixture("leg2", "club-09", "club-23"),
    ]);

    expect(screen.getByTestId("leaguephase-tie-t1")).toBeInTheDocument();
    expect(screen.getAllByText("tournaments.leaguePhase.notDrawn")).toHaveLength(4);
  });

  /**
   * Given a tie level on aggregate that a shootout decided for the unseeded club
   * When the bracket is shown
   * Then both legs and the aggregate show, the winner is named in words for screen readers, and
   *      the shootout is labelled.
   */
  it("shows legs, aggregate, the winner in words and the shootout", () => {
    const resolved = tie({
      resolution: {
        winner_id: "club-23",
        seeded_goals: 2,
        unseeded_goals: 2,
        penalties: [3, 4],
        decision: "Penalties",
      },
    });
    render_(state({ stage: "RoundOf16", ties: [resolved] }), [
      fixture("leg1", "club-23", "club-09", [2, 1]),
      fixture("leg2", "club-09", "club-23", [1, 0]),
    ]);

    const card = screen.getByTestId("leaguephase-tie-t1");
    expect(within(card).getByText("Name of club-23").textContent).toContain(
      "tournaments.leaguePhase.advances",
    );
    expect(within(card).getByText("Name of club-09").textContent).not.toContain(
      "tournaments.leaguePhase.advances",
    );
    expect(card.textContent).toContain("tournaments.leaguePhase.onPenalties");
    expect(card.textContent).toContain("(4)");
    // The unseeded club scored 2 and 0 over its two legs (aggregate 2), the seeded club 1 and 1
    // (aggregate 2): each leg and each aggregate is announced, and so is each side's shootout.
    expect(within(card).getAllByText(/leaguePhase\.legScore/)).toHaveLength(4);
    expect(within(card).getAllByText(/leaguePhase\.aggregateScore.*"goals":2/)).toHaveLength(2);
    expect(within(card).getByText(/leaguePhase\.penaltiesScore.*"goals":3/)).toBeInTheDocument();
    expect(within(card).getByText(/leaguePhase\.penaltiesScore.*"goals":4/)).toBeInTheDocument();
  });
});

describe("LeaguePhaseOverview", () => {
  /**
   * Given a league phase in progress
   * When the overview is shown
   * Then it holds the compact table and the scorers, and its status line.
   */
  it("shows the compact table, the status and the scorers", () => {
    const phase = state();
    render(
      <LeaguePhaseOverview
        competition={competition(phase)}
        state={phase}
        topScorers={[]}
        teams={teams}
      />,
    );

    expect(screen.getAllByTestId(/^leaguephase-overview-standing-club-/)).toHaveLength(36);
    expect(screen.getByTestId("leaguephase-status")).toBeInTheDocument();
    expect(screen.getByTestId("top-scorers")).toBeInTheDocument();
  });
});
