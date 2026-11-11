import type { SquadListSortKey } from "./SquadRosterView.state";
import { DEFAULT_SQUAD_LIST_SORT_STATE } from "./SquadRosterView.state";

/** Every column of the squad table that the player can choose to show or hide. */
export type HideableSquadColumnId =
  | "jersey"
  | "pos"
  | "fit"
  | "traits"
  | "age"
  | "condition"
  | "morale"
  | "ovr"
  | "contract";

/**
 * In table order, with the key the column's name is translated under. The player's name is not
 * here on purpose: it identifies the row, so it is always shown (as is the row's actions menu).
 */
export const HIDEABLE_SQUAD_COLUMNS: readonly { id: HideableSquadColumnId; labelKey: string }[] = [
  { id: "jersey", labelKey: "squad.jerseyNumberFull" },
  { id: "pos", labelKey: "squad.pos" },
  { id: "fit", labelKey: "squad.formationFit" },
  { id: "traits", labelKey: "squad.traits" },
  { id: "age", labelKey: "common.age" },
  { id: "condition", labelKey: "common.condition" },
  { id: "morale", labelKey: "common.morale" },
  { id: "ovr", labelKey: "common.ovr" },
  { id: "contract", labelKey: "common.contract" },
];

const HIDEABLE_IDS = new Set<string>(HIDEABLE_SQUAD_COLUMNS.map((column) => column.id));

/** The columns a stored list hides here: an id this build does not know hides nothing. */
export function hiddenSquadColumns(stored: readonly string[]): Set<HideableSquadColumnId> {
  return new Set(stored.filter((id): id is HideableSquadColumnId => HIDEABLE_IDS.has(id)));
}

/**
 * The stored list after the player toggles a column. Ids this build does not know are kept, since
 * a newer build may have written them, and the name cannot be hidden.
 */
export function toggleHiddenSquadColumn(stored: readonly string[], id: string): string[] {
  if (!HIDEABLE_IDS.has(id)) return [...stored];
  return stored.includes(id) ? stored.filter((entry) => entry !== id) : [...stored, id];
}

/** A sort on a hidden column would order rows by something the player cannot see. */
export function sortKeyAfterHiding(
  sortKey: SquadListSortKey,
  hidden: ReadonlySet<string>,
): SquadListSortKey {
  return hidden.has(sortKey) ? DEFAULT_SQUAD_LIST_SORT_STATE.sortKey : sortKey;
}
