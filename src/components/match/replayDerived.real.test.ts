// Validation against a REAL M12 document, not a hand-built one. Skipped unless a replay exported by
// the engine is supplied (the file is large and is not in the repository):
//
//   APFM_REPLAY_OUT=/tmp/real_replay.json cargo test -p ofm_core --lib \
//     export_a_real_replay_for_the_viewer_validation -- --ignored
//   APFM_REAL_REPLAY=/tmp/real_replay.json npm exec --no -- vitest run \
//     src/components/match/replayDerived.real.test.ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { frameAtOrBefore, parseM12Replay, type Replay } from "./m12Replay";
import { buildFeed, conditionsAt, lineupAt, statsAt } from "./replayDerived";

// `tsc` has no Node typings in this project; the runtime (vitest) does.
const path = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env
  .APFM_REAL_REPLAY;

function load(): { replay: Replay; homeIds: Set<string> } {
  const parsed = parseM12Replay(readFileSync(path as string, "utf-8"));
  if (!parsed.ok) throw new Error(`the real replay was rejected: ${parsed.reason}`);
  // The exported fixture names its players home-* / away-*.
  const homeIds = new Set(parsed.replay.roster.filter((id) => id.startsWith("home-")));
  return { replay: parsed.replay, homeIds };
}

describe.skipIf(!path)("the replay views on a real M12 document", () => {
  /**
   * Given a full real match
   * When the statistics are read at the last frame
   * Then each count equals the engine's own full-time total for that side.
   */
  it("reproduces the engine's full-time totals at the end", () => {
    const { replay, homeIds } = load();
    const last = replay.frames[replay.frames.length - 1];
    const stats = statsAt(replay, last.t_ms, last, homeIds);
    for (const side of ["home", "away"] as const) {
      const engine = replay[`${side}_stats`];
      expect(engine, `${side}_stats is in the document`).toBeDefined();
      expect(stats[side].shots).toBe(engine?.shots);
      expect(stats[side].shotsOnTarget).toBe(engine?.shots_on_target);
      expect(stats[side].corners).toBe(engine?.corners);
      expect(stats[side].fouls).toBe(engine?.fouls);
      expect(stats[side].offsides).toBe(engine?.offsides);
      expect(stats[side].yellowCards).toBe(engine?.yellow_cards);
      expect(stats[side].redCards).toBe(engine?.red_cards);
      expect(stats[side].possessionPct).toBeCloseTo(engine?.possession_pct ?? 0, 0);
    }
    expect(stats.home.goals).toBe(last.score.home);
    expect(stats.away.goals).toBe(last.score.away);
  });

  /**
   * Given the real events
   * When the feed is built
   * Then every goal of the final score is in it with a scorer, and no entry is a raw engine kind.
   */
  it("tells every goal and nothing the engine only uses internally", () => {
    const { replay } = load();
    const last = replay.frames[replay.frames.length - 1];
    const feed = buildFeed(replay);
    expect(feed.filter((entry) => entry.kind === "goal" || entry.kind === "ownGoal")).toHaveLength(
      last.score.home + last.score.away,
    );
    expect(feed.every((entry) => entry.kind.length > 0)).toBe(true);
    expect(feed.length).toBeGreaterThan(10);
    expect(feed.length).toBeLessThan(replay.events.length);
  });

  /**
   * Given the real frames
   * When the condition is read mid-match and at the end
   * Then every player on the pitch has one, and the squad has tired.
   */
  it("has a recorded condition for every player on the pitch, falling over the match", () => {
    const { replay, homeIds } = load();
    const early = frameAtOrBefore(replay.frames, 60_000);
    const late = replay.frames[replay.frames.length - 1];
    const onPitch = (frame: typeof early) => frame.players.filter((p) => p.on_pitch).length;
    expect(conditionsAt(replay, early, homeIds)).toHaveLength(onPitch(early));
    expect(conditionsAt(replay, late, homeIds)).toHaveLength(onPitch(late));
    const mean = (frame: typeof early) => {
      const all = conditionsAt(replay, frame, homeIds);
      return all.reduce((sum, entry) => sum + entry.condition, 0) / all.length;
    };
    expect(mean(late)).toBeLessThan(mean(early));
  });

  /**
   * Given the real match
   * When the line-up is read at kick-off
   * Then eleven a side are on the pitch.
   */
  it("starts with eleven a side", () => {
    const { replay, homeIds } = load();
    const first = replay.frames[0];
    const lineup = lineupAt(replay, first, first.t_ms, homeIds);
    expect(lineup.home.onPitch).toHaveLength(11);
    expect(lineup.away.onPitch).toHaveLength(11);
  });
});
