import { describe, expect, it } from "vitest";

import type { FixtureData } from "../../store/gameStore";
import { settleSnapshot } from "./finalResult";
import type { MatchEvent, MatchSnapshot } from "./types";

function goal(side: "Home" | "Away", minute: number, playerId: string): MatchEvent {
  return {
    minute,
    event_type: "Goal",
    side,
    zone: "Box",
    player_id: playerId,
    secondary_player_id: null,
  };
}

function snapshot(overrides: Partial<MatchSnapshot> = {}): MatchSnapshot {
  return {
    phase: "PreKickOff",
    current_minute: 0,
    home_score: 0,
    away_score: 0,
    events: [],
    home_team: { id: "home1", name: "Home FC", players: [] },
    away_team: { id: "away1", name: "Away FC", players: [] },
    ...overrides,
  } as unknown as MatchSnapshot;
}

function fixtureWith(result: NonNullable<FixtureData["result"]> | null): FixtureData {
  return { id: "fix1", status: "Completed", result } as unknown as FixtureData;
}

const storedFourTwo: NonNullable<FixtureData["result"]> = {
  home_goals: 4,
  away_goals: 2,
  home_scorers: [
    { player_id: "h1", minute: 5 },
    { player_id: "h1", minute: 30 },
    { player_id: "h2", minute: 61 },
    { player_id: "h3", minute: 88 },
  ],
  away_scorers: [
    { player_id: "a1", minute: 12 },
    { player_id: "a1", minute: 70 },
  ],
};

describe("settleSnapshot", () => {
  /**
   * Given an M12 quick simulation that finished 4-2 while the page still holds the pre-match
   *       0-0 snapshot
   * When the snapshot is settled with the simulation's result
   * Then the score and the goal events are the simulation's, not the pre-match ones.
   */
  it("replaces a pre-match 0-0 snapshot with the simulated 4-2", () => {
    const events = [goal("Home", 5, "h1"), goal("Away", 12, "a1")];

    const settled = settleSnapshot(snapshot(), {
      fixture: null,
      finalized: true,
      spatialResult: { homeGoals: 4, awayGoals: 2, events },
    });

    expect([settled.home_score, settled.away_score]).toEqual([4, 2]);
    expect(settled.events).toEqual(events);
  });

  /**
   * Given a managed match whose events were not returned but whose fixture holds the result
   * When the snapshot is settled
   * Then the score is the stored fixture result and a goal event exists for every stored scorer.
   */
  it("builds goal events from the stored fixture scorers when nothing else has them", () => {
    const settled = settleSnapshot(snapshot(), {
      fixture: fixtureWith(storedFourTwo),
      finalized: true,
      spatialResult: { homeGoals: 4, awayGoals: 2, events: [] },
    });

    expect([settled.home_score, settled.away_score]).toEqual([4, 2]);
    const goals = settled.events.filter((event) => event.event_type === "Goal");
    expect(goals.filter((event) => event.side === "Home").map((event) => event.player_id)).toEqual([
      "h1",
      "h1",
      "h2",
      "h3",
    ]);
    expect(goals.filter((event) => event.side === "Away")).toHaveLength(2);
  });

  /**
   * Given a stored fixture result that disagrees with a response's goals
   * When the snapshot is settled
   * Then the stored fixture result wins: it is what the career holds.
   */
  it("prefers the stored fixture result over the response goals", () => {
    const settled = settleSnapshot(snapshot(), {
      fixture: fixtureWith(storedFourTwo),
      finalized: true,
      spatialResult: { homeGoals: 0, awayGoals: 0, events: [] },
    });

    expect([settled.home_score, settled.away_score]).toEqual([4, 2]);
  });

  /**
   * Given a classic live match whose own snapshot already reads 4-2 with its events
   * When the snapshot is settled against the same stored result
   * Then the live events are kept as they are.
   */
  it("keeps a live snapshot's events when its score already matches the stored result", () => {
    const events = [goal("Home", 5, "h1"), goal("Away", 12, "a1")];
    const live = snapshot({ phase: "FullTime", home_score: 4, away_score: 2, events });

    const settled = settleSnapshot(live, {
      fixture: fixtureWith(storedFourTwo),
      finalized: true,
      spatialResult: null,
    });

    expect(settled.events).toBe(events);
    expect([settled.home_score, settled.away_score]).toEqual([4, 2]);
  });

  /**
   * Given a match that has not been finalised
   * When the snapshot is settled
   * Then it is returned untouched: nothing authoritative exists yet.
   */
  it("returns the snapshot unchanged before the match is finalised", () => {
    const live = snapshot({ home_score: 1, away_score: 0 });

    expect(
      settleSnapshot(live, {
        fixture: fixtureWith(storedFourTwo),
        finalized: false,
        spatialResult: null,
      }),
    ).toBe(live);
  });
});
