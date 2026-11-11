import type { TeamData } from "../../store/gameStore";

/**
 * The club's name for the id the replay carries, or `null` when the game has no such club. Ids are
 * exact strings; a case-insensitive match is accepted because FM24 ids have been written in both
 * cases. The caller shows a translated fallback for `null`, never the id itself.
 */
export function resolveReplayTeamName(
  teamId: string,
  teams: ReadonlyArray<Pick<TeamData, "id" | "name">>,
): string | null {
  const exact = teams.find((team) => team.id === teamId);
  const found = exact ?? teams.find((team) => team.id.toLowerCase() === teamId.toLowerCase());
  const name = found?.name.trim();
  return name ? name : null;
}
