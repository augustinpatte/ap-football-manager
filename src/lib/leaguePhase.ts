import type {
  FixtureData,
  KnockoutStageData,
  KnockoutTieData,
  LeagueData,
  LeaguePhaseData,
  LeaguePhaseStageData,
} from "../store/types";

/** Where a club's league-phase rank leads. */
export type LeaguePhaseZone = "direct" | "playoff" | "eliminated";

export const KNOCKOUT_STAGES: KnockoutStageData[] = [
  "Playoffs",
  "RoundOf16",
  "QuarterFinals",
  "SemiFinals",
  "Final",
];

/** The league-phase state of a competition, when it has the league-phase format. */
export function leaguePhaseOf(competition: LeagueData | null): LeaguePhaseData | null {
  if (competition?.rules?.format !== "LeaguePhaseAndKnockout") return null;
  return competition.league_phase ?? null;
}

export function isLeaguePhaseCompetition(competition: LeagueData | null): boolean {
  return competition?.rules?.format === "LeaguePhaseAndKnockout";
}

/** How many ranks go direct and to the play-offs; the regulation's 8 and 16 when unstated. */
export function leaguePhasePlaces(competition: LeagueData): { direct: number; playoff: number } {
  const rules = competition.rules?.league_phase;
  return { direct: rules?.direct_places ?? 8, playoff: rules?.playoff_places ?? 16 };
}

/** The zone of the club at 1-based `position`. */
export function zoneOfPosition(
  position: number,
  places: { direct: number; playoff: number },
): LeaguePhaseZone {
  if (position <= places.direct) return "direct";
  if (position <= places.direct + places.playoff) return "playoff";
  return "eliminated";
}

/**
 * Whether the table is still provisional: the tiebreakers that read every club's whole league
 * phase (opponents' results, cards) only settle when the last match is played.
 */
export function isTableProvisional(state: LeaguePhaseData): boolean {
  return state.outcome == null;
}

export function ties(state: LeaguePhaseData, stage: KnockoutStageData): KnockoutTieData[] {
  return state.ties.filter((tie) => tie.stage === stage).sort((a, b) => a.slot - b.slot);
}

/** The stages that have been drawn, in the order they are played. */
export function drawnStages(state: LeaguePhaseData): KnockoutStageData[] {
  return KNOCKOUT_STAGES.filter((stage) => state.ties.some((tie) => tie.stage === stage));
}

/** Goals each side scored in the tie so far, from the played legs. `[seeded, unseeded]`. */
export function aggregateSoFar(
  tie: KnockoutTieData,
  fixtureById: Map<string, FixtureData>,
): [number, number] {
  let seeded = 0;
  let unseeded = 0;
  for (const id of tie.fixture_ids) {
    const result = fixtureById.get(id)?.result;
    const fixture = fixtureById.get(id);
    if (!result || !fixture) continue;
    if (fixture.home_team_id === tie.seeded_team_id) {
      seeded += result.home_goals;
      unseeded += result.away_goals;
    } else {
      seeded += result.away_goals;
      unseeded += result.home_goals;
    }
  }
  return [seeded, unseeded];
}

/** Translation key suffix for a stage, under `tournaments.leaguePhase.stages` (a competition
 *  stage) or `.rounds` (a knockout round): the stage name with its first letter lowered. */
export function stageKey(stage: LeaguePhaseStageData | KnockoutStageData): string {
  return stage.charAt(0).toLowerCase() + stage.slice(1);
}

/** The name the backend gives a knockout round, which `localizedRoundName` translates. */
export function backendRoundName(stage: KnockoutStageData): string {
  switch (stage) {
    case "Playoffs":
      return "Knockout Play-offs";
    case "RoundOf16":
      return "Round of 16";
    case "QuarterFinals":
      return "Quarterfinal";
    case "SemiFinals":
      return "Semifinal";
    case "Final":
      return "Final";
  }
}
