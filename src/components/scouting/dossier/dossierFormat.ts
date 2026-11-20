import { formatRange as formatKnownRange } from "../../../lib/ratingView";
import type { RatingRange, ScoutingTargetView } from "../../../services/scoutingService";

/** "62–70": the range a scout gave, or `null` when nobody has judged it. */
export function formatRange(range: RatingRange | null | undefined): string | null {
  return range ? formatKnownRange(range) : null;
}

/** Most players the comparison holds: more than four stops being a comparison. */
export const COMPARE_MIN = 2;
export const COMPARE_MAX = 4;

/** The dossier rows by player, so a search row can show what the club already knows. */
export function indexTargets(views: ScoutingTargetView[]): Map<string, ScoutingTargetView> {
  return new Map(views.map((view) => [view.player_id, view]));
}

/** Toggles a player in the comparison, refusing a fifth. */
export function toggleCompared(selected: string[], playerId: string): string[] {
  if (selected.includes(playerId)) return selected.filter((id) => id !== playerId);
  return selected.length >= COMPARE_MAX ? selected : [...selected, playerId];
}
