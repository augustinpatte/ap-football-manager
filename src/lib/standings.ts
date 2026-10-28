import type { LeagueData } from "../store/gameStore";

type Row = LeagueData["standings"][number];

/** Points, then goal difference, then goals scored: the order every table had before rules existed. */
export function byGenericTablePosition(a: Row, b: Row): number {
  return (
    b.points - a.points ||
    b.goals_for - b.goals_against - (a.goals_for - a.goals_against) ||
    b.goals_for - a.goals_for
  );
}

/**
 * A table in finishing order.
 *
 * A competition that authors its tiebreakers (head-to-head, away goals, wins...) is ranked by the
 * backend, which has the results to read them from, and sends the table already in order: this
 * keeps that order. Any other competition is ranked by points, goal difference and goals scored.
 */
export function orderStandings(competition: Pick<LeagueData, "standings" | "rules"> | null): Row[] {
  if (!competition) return [];
  if ((competition.rules?.tiebreakers?.length ?? 0) > 0) return [...competition.standings];
  return [...competition.standings].sort(byGenericTablePosition);
}
