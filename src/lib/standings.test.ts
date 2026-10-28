import { describe, expect, it } from "vitest";

import { orderStandings } from "./standings";

const row = (team_id: string, points: number, goals_for: number, goals_against: number) => ({
  team_id,
  played: 0,
  won: 0,
  drawn: 0,
  lost: 0,
  goals_for,
  goals_against,
  points,
});

describe("orderStandings", () => {
  const rows = [row("a", 70, 80, 30), row("b", 70, 60, 40), row("c", 71, 10, 40)];

  it("ranks a competition with no authored tiebreakers by points, goal difference and goals scored", () => {
    const order = orderStandings({ standings: rows, rules: undefined }).map((r) => r.team_id);
    expect(order).toEqual(["c", "a", "b"]);
  });

  // The backend has the results head-to-head is read from; the screen must not undo its order.
  it("keeps the backend's order when the competition authors its tiebreakers", () => {
    const order = orderStandings({
      standings: [row("b", 70, 60, 40), row("a", 70, 80, 30)],
      rules: {
        format: "LeagueTable",
        counts_in_season_flow: true,
        tiebreakers: ["Points", "HeadToHeadPoints"],
      },
    }).map((r) => r.team_id);
    expect(order).toEqual(["b", "a"]);
  });
});
