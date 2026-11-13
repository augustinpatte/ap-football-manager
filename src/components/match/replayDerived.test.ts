import { describe, expect, it } from "vitest";

import { frameAtOrBefore, parseM12Replay, type Replay } from "./m12Replay";
import { buildReplayFixture, MINUTE_MS } from "./m12Replay.fixtures";
import { buildFeed, conditionsAt, feedAt, lineupAt, statsAt } from "./replayDerived";

function replayOf(
  extra: Record<string, unknown> = {},
  eventsExtra: Record<string, unknown>[] = [],
): Replay {
  const fixture = buildReplayFixture();
  const document = {
    ...fixture.document,
    ...extra,
    events: [...(fixture.document.events as Record<string, unknown>[]), ...eventsExtra].sort(
      (a, b) => Number(a.t_ms) - Number(b.t_ms),
    ),
  };
  const parsed = parseM12Replay(JSON.stringify(document));
  if (!parsed.ok) throw new Error(`fixture rejected: ${parsed.reason}`);
  return parsed.replay;
}

describe("buildFeed", () => {
  /**
   * Given the engine's many event kinds (passes, touches, state changes) and a few that matter
   * When the feed is built
   * Then only the ones a player reads are in it, and none is left as the engine spelled it.
   */
  it("keeps the events a player reads and drops the engine's internal ones", () => {
    const replay = replayOf({}, [
      { t_ms: 1000, type: "ball_control", side: "home" },
      { t_ms: 2000, type: "pass_completed", side: "home" },
      { t_ms: 3000, type: "state_change" },
      { t_ms: 4000, type: "corner", side: "away" },
      { t_ms: 5000, type: "shot", side: "home", player: "home-1", outcome: "saved" },
      { t_ms: 6000, type: "shot", side: "home", player: "home-1", outcome: "goal" },
    ]);
    const kinds = buildFeed(replay).map((entry) => entry.kind);
    expect(kinds).toContain("corner");
    expect(kinds).toContain("shotSaved");
    // A shot that scored is told by the goal, not twice.
    expect(kinds.filter((kind) => kind === "shotSaved")).toHaveLength(1);
    expect(kinds.join(" ")).not.toMatch(/ball|pass|state/);
  });

  /**
   * Given period starts and ends
   * When the feed is built
   * Then they become kick-off, half time, second half and full time.
   */
  it("names the whistles of the match", () => {
    const replay = replayOf({}, [
      { t_ms: 0, type: "period_start", period: "first_half" },
      { t_ms: 45 * MINUTE_MS, type: "period_end", period: "first_half" },
      { t_ms: 45 * MINUTE_MS + 1, type: "period_start", period: "second_half" },
      { t_ms: 90 * MINUTE_MS, type: "period_end", period: "second_half" },
    ]);
    expect(buildFeed(replay).map((entry) => entry.kind)).toEqual(
      expect.arrayContaining(["kickoff", "halfTime", "secondHalfStart", "fullTime"]),
    );
  });

  /**
   * Given a feed and a clock
   * When the visible entries are asked
   * Then only those at or before the clock appear, newest last, limited to the count.
   */
  it("shows only what has happened by the clock", () => {
    const replay = replayOf({}, [
      { t_ms: 1000, type: "corner", side: "home" },
      { t_ms: 2000, type: "corner", side: "away" },
      { t_ms: 9000, type: "corner", side: "home" },
    ]);
    const feed = buildFeed(replay);
    const shown = feedAt(feed, 2500, 2);
    expect(shown.every((entry) => entry.t_ms <= 2500)).toBe(true);
    expect(shown.length).toBeLessThanOrEqual(2);
  });
});

describe("statsAt", () => {
  /**
   * Given shots, a corner, a foul, an offside and cards for each side
   * When statistics are read at two clocks
   * Then they count only events up to the clock, and the later clock never shows less.
   */
  it("counts recorded events up to the clock", () => {
    const replay = replayOf({}, [
      { t_ms: 1000, type: "shot", side: "home", player: "home-1", outcome: "saved" },
      { t_ms: 2000, type: "shot", side: "home", player: "home-1", outcome: "off_target" },
      { t_ms: 3000, type: "corner", side: "away" },
      { t_ms: 4000, type: "card", side: "away", player: "away-1", colour: "yellow" },
      { t_ms: 5000, type: "card", side: "away", player: "away-1", colour: "second_yellow" },
    ]);
    const early = statsAt(replay, 1500, frameAtOrBefore(replay.frames, 1500));
    const late = statsAt(replay, 6000, frameAtOrBefore(replay.frames, 6000));
    expect(early.home.shots).toBe(1);
    expect(early.home.shotsOnTarget).toBe(1);
    expect(late.home.shots).toBe(2);
    expect(late.home.shotsOnTarget).toBe(1);
    expect(late.away.corners).toBe(1);
    expect(late.away.yellowCards).toBe(2);
    expect(late.away.redCards).toBe(1);
    expect(early.away.corners).toBe(0);
  });

  /**
   * Given the home side's players holding the ball in three frames and the away side's in one
   * When possession is read
   * Then it is the share of frames with a holder.
   */
  it("derives possession from the frames so far", () => {
    const replay = replayOf();
    const frames = replay.frames.slice(0, 4);
    frames[0].possession = "home-1";
    frames[1].possession = "home-2";
    frames[2].possession = "home-3";
    frames[3].possession = "away-1";
    const stats = statsAt(replay, frames[3].t_ms, frames[3]);
    expect(stats.home.possessionPct).toBeCloseTo(75);
    expect(stats.away.possessionPct).toBeCloseTo(25);
  });
});

describe("conditionsAt", () => {
  /**
   * Given a frame that records conditions
   * When they are read
   * Then each player on the pitch has his side and condition.
   */
  it("reads the recorded condition of each player", () => {
    const replay = replayOf();
    const frame = {
      ...replay.frames[0],
      extras: {
        phase: [1, 2] as [number, number],
        agents: [
          { id: "home-1", cond: 92, act: 0 },
          { id: "away-1", cond: 64, act: 0 },
        ],
      },
    };
    expect(conditionsAt(replay, frame)).toEqual([
      { playerId: "home-1", side: "home", condition: 92 },
      { playerId: "away-1", side: "away", condition: 64 },
    ]);
  });

  /**
   * Given a frame without recorded conditions
   * When they are read
   * Then there are none: no value is made up.
   */
  it("returns nothing when the replay recorded no conditions", () => {
    const replay = replayOf();
    expect(conditionsAt(replay, replay.frames[0])).toEqual([]);
  });
});

describe("lineupAt", () => {
  /**
   * Given the fixture's red card at 30' and a substitution at 60'
   * When the line-up is read before and after
   * Then the sent-off and replaced players are listed as gone, and the bench shrinks accordingly.
   */
  it("follows substitutions and dismissals", () => {
    const replay = replayOf();
    const at = (minute: number) => {
      const time = minute * MINUTE_MS;
      return lineupAt(replay, frameAtOrBefore(replay.frames, time), time);
    };
    const start = at(1);
    expect(start.home.onPitch).toHaveLength(11);
    expect(start.home.gone).toEqual([]);
    const end = at(75);
    expect(end.away.gone.map((entry) => entry.reason)).toContain("sentOff");
    expect(end.home.gone.map((entry) => entry.reason)).toContain("substituted");
    expect(end.home.bench.length).toBe(start.home.bench.length - 1);
  });
});
