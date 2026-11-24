import { describe, expect, it } from "vitest";

import {
  aggregateSoFar,
  clubStanding,
  drawnStages,
  isLeaguePhaseCompetition,
  isTableProvisional,
  leaguePhaseOf,
  leaguePhasePlaces,
  stageKey,
  zoneOfPosition,
} from "./leaguePhase";
import type { FixtureData, KnockoutTieData, LeagueData, LeaguePhaseData } from "../store/types";

const places = { direct: 8, playoff: 16 };

function fixture(overrides: Partial<FixtureData>): FixtureData {
  return {
    id: "f",
    matchday: 1,
    date: "2026-09-08",
    home_team_id: "a",
    away_team_id: "b",
    competition: "ContinentalClub",
    competitive: true,
    advancing_team_id: null,
    status: "Scheduled",
    result: null,
    ...overrides,
  };
}

function result(home: number, away: number): FixtureData["result"] {
  return { home_goals: home, away_goals: away, home_scorers: [], away_scorers: [] };
}

function league(overrides: Partial<LeagueData> = {}): LeagueData {
  return {
    id: "cl",
    name: "Champions",
    season: 2026,
    fixtures: [],
    standings: [],
    rules: { format: "LeaguePhaseAndKnockout", counts_in_season_flow: true },
    ...overrides,
  } as LeagueData;
}

describe("league-phase zones", () => {
  /**
   * Given the regulation's 8 direct and 16 play-off places
   * When a position is classified
   * Then 1-8 go direct, 9-24 to the play-offs and 25 and below are out.
   */
  it("splits 36 positions into 8, 16 and 12", () => {
    const zones = Array.from({ length: 36 }, (_, i) => zoneOfPosition(i + 1, places));

    expect(zones.filter((z) => z === "direct")).toHaveLength(8);
    expect(zones.filter((z) => z === "playoff")).toHaveLength(16);
    expect(zones.filter((z) => z === "eliminated")).toHaveLength(12);
    expect(zoneOfPosition(8, places)).toBe("direct");
    expect(zoneOfPosition(9, places)).toBe("playoff");
    expect(zoneOfPosition(24, places)).toBe("playoff");
    expect(zoneOfPosition(25, places)).toBe("eliminated");
  });

  /**
   * Given rules that state their places, and rules that do not
   * When the places are read
   * Then the authored ones are used, and the regulation's are the default.
   */
  it("reads the places from the rules and defaults to 8 and 16", () => {
    const authored = league({
      rules: {
        format: "LeaguePhaseAndKnockout",
        counts_in_season_flow: true,
        league_phase: { pot_count: 6, opponents_per_pot: 1, direct_places: 4, playoff_places: 8 },
      },
    });

    expect(leaguePhasePlaces(authored)).toEqual({ direct: 4, playoff: 8 });
    expect(leaguePhasePlaces(league())).toEqual(places);
  });
});

describe("league-phase state", () => {
  /**
   * Given a competition of another format that carries stray state
   * When its league phase is read
   * Then it has none: the format decides, not the presence of a field.
   */
  it("reads state only from a league-phase competition", () => {
    const state = { stage: "Created" } as LeaguePhaseData;
    const cup = league({
      rules: { format: "Knockout", counts_in_season_flow: true },
      league_phase: state,
    });

    expect(leaguePhaseOf(cup)).toBeNull();
    expect(isLeaguePhaseCompetition(cup)).toBe(false);
    expect(leaguePhaseOf(league({ league_phase: state }))).toBe(state);
    expect(leaguePhaseOf(null)).toBeNull();
  });

  /**
   * Given a table before and after the outcome is fixed
   * When asked whether it is provisional
   * Then it is provisional exactly until the outcome exists.
   */
  it("is provisional until the outcome is fixed", () => {
    expect(isTableProvisional({ outcome: null } as LeaguePhaseData)).toBe(true);
    expect(
      isTableProvisional({
        outcome: { ranking: [], direct: [], playoff: [], eliminated: [] },
      } as unknown as LeaguePhaseData),
    ).toBe(false);
  });

  it("names stages by their translation keys", () => {
    expect(stageKey("LeaguePhaseInProgress")).toBe("leaguePhaseInProgress");
    expect(stageKey("RoundOf16")).toBe("roundOf16");
    expect(stageKey("Completed")).toBe("completed");
  });

  /**
   * Given ties of two rounds
   * When the drawn stages are listed
   * Then they come in playing order, and a round with no tie is absent.
   */
  it("lists the drawn knockout stages in order", () => {
    const tie = (stage: KnockoutTieData["stage"]) => ({ stage }) as KnockoutTieData;
    const state = { ties: [tie("RoundOf16"), tie("Playoffs")] } as LeaguePhaseData;

    expect(drawnStages(state)).toEqual(["Playoffs", "RoundOf16"]);
  });
});

describe("a tie's aggregate", () => {
  const tie = {
    id: "t",
    fixture_ids: ["leg1", "leg2"],
    seeded_team_id: "seed",
    unseeded_team_id: "underdog",
  } as KnockoutTieData;

  /**
   * Given a first leg won 2-1 by the unseeded host and an unplayed return leg
   * When the aggregate so far is read
   * Then it counts the leg played, by club, not by home and away.
   */
  it("counts goals by club across the legs played so far", () => {
    const legs = new Map([
      [
        "leg1",
        fixture({
          id: "leg1",
          home_team_id: "underdog",
          away_team_id: "seed",
          result: result(2, 1),
        }),
      ],
      ["leg2", fixture({ id: "leg2", home_team_id: "seed", away_team_id: "underdog" })],
    ]);

    expect(aggregateSoFar(tie, legs)).toEqual([1, 2]);

    legs.set(
      "leg2",
      fixture({ id: "leg2", home_team_id: "seed", away_team_id: "underdog", result: result(3, 0) }),
    );

    expect(aggregateSoFar(tie, legs)).toEqual([4, 2]);
  });
});

describe("clubStanding", () => {
  const competition = {
    rules: { format: "LeaguePhaseAndKnockout", league_phase: { direct_places: 8, playoff_places: 16 } },
  } as unknown as LeagueData;
  const table = Array.from({ length: 36 }, (_, index) => ({
    team_id: `c${index + 1}`,
    position: index + 1,
    points: 20 - index / 2,
  }));
  const state = { table } as unknown as LeaguePhaseData;

  /**
   * Given a 36-club table with the last play-off place at 24th
   * When the standing of the 30th club is read
   * Then it carries that club's own place and points and the points of the 24th, never its own as
   *      the cut-off.
   */
  it("reads the club's own row and the points of the last play-off place", () => {
    expect(clubStanding(competition, state, "c30")).toEqual({
      position: 30,
      points: 5.5,
      total: 36,
      cutoff: { position: 24, points: 8.5 },
    });
  });

  it("is null for a club that is not in the table", () => {
    expect(clubStanding(competition, state, "absent")).toBeNull();
  });
});
