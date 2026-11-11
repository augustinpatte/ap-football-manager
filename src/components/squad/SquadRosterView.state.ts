export type SquadListSortKey =
  | "jersey"
  | "name"
  | "pos"
  | "fit"
  | "age"
  | "condition"
  | "morale"
  | "ovr"
  | "contract";

export type SquadStatusFilter =
  | "all"
  | "xi"
  | "bench"
  | "naturalFit"
  | "needsCover"
  | "outOfPosition"
  | "injured";

/** What narrows the roster: a name search, a position ("All" or one), and a status scope. */
export interface SquadListFilters {
  search: string;
  position: string;
  status: SquadStatusFilter;
}

export const DEFAULT_SQUAD_LIST_FILTERS: SquadListFilters = {
  search: "",
  position: "All",
  status: "all",
};

export interface SquadListSortState {
  sortKey: SquadListSortKey;
  sortDir: "asc" | "desc";
}

/**
 * Default: sort by position — starters first (XI members top), then by
 * natural_position in football-canonical order (GK → back line → midfield →
 * forwards), then by OVR desc so the strongest at each slot rises within its
 * cluster. This gives a "who plays where" view at a glance without the user
 * having to touch the sort headers.
 */
export const DEFAULT_SQUAD_LIST_SORT_STATE: SquadListSortState = {
  sortKey: "pos",
  sortDir: "asc",
};
