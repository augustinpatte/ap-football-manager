import type { FixtureData } from "../../store/gameStore";
import type { MatchEvent, MatchSnapshot } from "./types";

type StoredResult = NonNullable<FixtureData["result"]>;

interface FinalGoals {
  homeGoals: number;
  awayGoals: number;
  events: MatchEvent[];
}

interface SettleInput {
  /** The fixture as the career now holds it; its `result` is authoritative once finalised. */
  fixture: FixtureData | null;
  /** The match has been applied to the career (by the backend's one finalisation). */
  finalized: boolean;
  /** What a quick simulation, a 2D replay or a managed match handed back, if one ran. */
  spatialResult: FinalGoals | null;
}

function goalEventsFromStoredResult(result: StoredResult): MatchEvent[] {
  const goals = (side: "Home" | "Away", scorers: StoredResult["home_scorers"]): MatchEvent[] =>
    scorers.map((scorer) => ({
      minute: scorer.minute,
      event_type: "Goal",
      side,
      zone: "",
      player_id: scorer.player_id,
      secondary_player_id: null,
    }));
  return [...goals("Home", result.home_scorers), ...goals("Away", result.away_scorers)].sort(
    (a, b) => a.minute - b.minute,
  );
}

/**
 * The snapshot the screens after full time must read.
 *
 * Only the classic live path keeps its snapshot up to date. A quick simulation, a 2D replay and a
 * managed match all finish elsewhere and leave the page holding the pre-match 0-0 snapshot, so a
 * screen that reads `snapshot.home_score` after the match talks about a different match than the
 * one just played. The score comes from the finalised fixture, then from the response that
 * finalised it, and only then from the snapshot; goal events follow the same order and a stale
 * snapshot's events are never kept against a different score.
 */
export function settleSnapshot(snapshot: MatchSnapshot, input: SettleInput): MatchSnapshot {
  if (!input.finalized) return snapshot;

  const stored = input.fixture?.result ?? null;
  const homeGoals = stored?.home_goals ?? input.spatialResult?.homeGoals ?? snapshot.home_score;
  const awayGoals = stored?.away_goals ?? input.spatialResult?.awayGoals ?? snapshot.away_score;
  const snapshotAgrees = homeGoals === snapshot.home_score && awayGoals === snapshot.away_score;

  let events: MatchEvent[] = [];
  if (input.spatialResult && input.spatialResult.events.length > 0) {
    events = input.spatialResult.events;
  } else if (snapshotAgrees && snapshot.events.length > 0) {
    events = snapshot.events;
  } else if (stored) {
    events = goalEventsFromStoredResult(stored);
  }

  if (snapshotAgrees && events === snapshot.events) return snapshot;
  return { ...snapshot, home_score: homeGoals, away_score: awayGoals, events };
}
