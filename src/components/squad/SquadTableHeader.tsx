import { useTranslation } from "react-i18next";

import type { SquadListSortKey } from "./SquadRosterView.state";
import { SquadSortHeader } from "./SquadSortHeader";
import type { HideableSquadColumnId } from "./squadColumns";

interface SquadTableHeaderProps {
  hidden: ReadonlySet<HideableSquadColumnId>;
  sortKey: SquadListSortKey;
  sortDir: "asc" | "desc";
  onSort: (key: SquadListSortKey) => void;
}

/** The roster table's header row: the name and the actions column always, the rest unless hidden. */
export function SquadTableHeader({ hidden, sortKey, sortDir, onSort }: SquadTableHeaderProps) {
  const { t } = useTranslation();
  const sortable = (col: SquadListSortKey, label: string, ariaLabel?: string) =>
    hidden.has(col as HideableSquadColumnId) ? null : (
      <SquadSortHeader
        col={col}
        label={label}
        ariaLabel={ariaLabel}
        sortKey={sortKey}
        sortDir={sortDir}
        onSort={onSort}
      />
    );

  return (
    <tr className="bg-gray-50 dark:bg-navy-800 border-b border-gray-200 dark:border-navy-600 text-xs">
      {sortable("jersey", t("squad.jerseyNumber"), t("squad.jerseyNumberFull"))}
      {sortable("name", t("common.name"))}
      {sortable("pos", t("squad.pos"))}
      {sortable("fit", t("squad.formationFit"))}
      {hidden.has("traits") ? null : (
        <th className="py-2.5 px-4 font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400">
          {t("squad.traits")}
        </th>
      )}
      {sortable("age", t("common.age"))}
      {sortable("condition", t("common.condition"))}
      {sortable("morale", t("common.morale"))}
      {sortable("ovr", t("common.ovr"))}
      {sortable("contract", t("common.contract"))}
      <th className="py-2.5 px-4 font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 text-right">
        <span className="sr-only">{t("common.actions")}</span>
      </th>
    </tr>
  );
}
