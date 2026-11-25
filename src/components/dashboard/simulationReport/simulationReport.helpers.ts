import type {
  ClubReport,
  FastForwardResponse,
  Listed,
  ReportContractLine,
  ReportEvent,
  ReportInjuryLine,
  ReportMatchLine,
  ReportTransferLine,
} from "../../../services/fastForwardService";
import type { DelegationLogEntry } from "../../../services/staffService";

/** One dated thing the run recorded, with the line it comes from. */
export type TimelineEntry = { id: string; date: string } & (
  | { source: "event"; line: ReportEvent }
  | { source: "match"; line: ReportMatchLine }
  | { source: "transfer"; line: ReportTransferLine }
  | { source: "contract"; line: ReportContractLine }
  | { source: "injury"; line: ReportInjuryLine }
  | { source: "delegation"; line: DelegationLogEntry }
);

/** On a shared day: what happened to the club first, then what the assistant did about it. */
const SOURCE_ORDER: Record<TimelineEntry["source"], number> = {
  event: 0,
  injury: 1,
  transfer: 2,
  match: 3,
  contract: 4,
  delegation: 5,
};

/**
 * Every dated item of a club's report in one list, oldest first. Nothing is added: an interval
 * with nothing in it has an empty timeline.
 */
export function buildTimeline(club: ClubReport): TimelineEntry[] {
  const entries: TimelineEntry[] = [
    ...club.events.map((line, i) => ({
      id: `event-${i}`,
      date: line.date,
      source: "event" as const,
      line,
    })),
    ...club.matches.fixtures.items.map((line) => ({
      id: `match-${line.fixture_id}`,
      date: line.date,
      source: "match" as const,
      line,
    })),
    ...club.transfers.moves.items.map((line, i) => ({
      id: `transfer-${i}-${line.player_id}`,
      date: line.date,
      source: "transfer" as const,
      line,
    })),
    ...club.contracts.changes.items.map((line, i) => ({
      id: `contract-${i}-${line.player_id}`,
      date: line.date,
      source: "contract" as const,
      line,
    })),
    ...club.squad.injuries.items.map((line, i) => ({
      id: `injury-${i}-${line.player_id}`,
      date: line.first_seen,
      source: "injury" as const,
      line,
    })),
    ...club.delegation.entries.items.map((line, i) => ({
      id: `delegation-${i}`,
      date: line.date,
      source: "delegation" as const,
      line,
    })),
  ];
  return entries.sort(
    (a, b) => a.date.localeCompare(b.date) || SOURCE_ORDER[a.source] - SOURCE_ORDER[b.source],
  );
}

/** How many items of a list the backend kept out of it (its `total` is always exact). */
export function omitted<T>(list: Listed<T>): number {
  return Math.max(0, list.total - list.items.length);
}

/** The club stretches of a response; none when the backend sent no report. */
export function reportClubs(response: FastForwardResponse): ClubReport[] {
  return response.report?.clubs ?? [];
}

/** Whether the report covers fewer days than the run was asked for (it stopped early). */
export function isPartialRun(response: FastForwardResponse): boolean {
  const clubs = reportClubs(response);
  // A manager with no club has nothing to cover: the days were played, there is only nothing to say.
  if (clubs.length === 0) return false;
  const played = clubs.reduce((days, club) => days + club.days_played, 0);
  return played < response.progress.days_total;
}
