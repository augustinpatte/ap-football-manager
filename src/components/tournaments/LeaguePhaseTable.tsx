import { useTranslation } from "react-i18next";

import { type LeaguePhaseZone, leaguePhasePlaces, zoneOfPosition } from "../../lib/leaguePhase";
import type { LeagueData, LeaguePhaseRowData } from "../../store/types";
import ContextMenu from "../ContextMenu";
import { buildViewTeamMenuItem } from "../playerActions/playerContextMenuItems";
import type { TournamentsTeamLookup } from "./teamLookup";

interface LeaguePhaseTableProps {
  competition: LeagueData;
  /** In the regulations' finishing order, as the backend sends them. */
  rows: LeaguePhaseRowData[];
  teams: TournamentsTeamLookup;
  variant: "compact" | "full";
  testIdPrefix: string;
}

// The rank number stays in the neutral text colour: only the edge carries the zone's colour, and
// the zone is also named in text (see the rank cell) and in the legend.
const ZONE_BORDER: Record<LeaguePhaseZone, string> = {
  direct: "border-l-4 border-primary-500",
  playoff: "border-l-4 border-amber-500",
  eliminated: "border-l-4 border-red-500",
};

/**
 * The 36-club league-phase table with its three qualification zones.
 *
 * A zone is shown by the left edge of the rank cell, and also *said*: each rank cell carries the
 * zone's name for assistive technology and as a tooltip, and the legend names all three, so no
 * zone depends on colour alone.
 */
export default function LeaguePhaseTable({
  competition,
  rows,
  teams,
  variant,
  testIdPrefix,
}: LeaguePhaseTableProps) {
  const { t } = useTranslation();
  const { userTeamId, isClubTeam, resolveTeamName, onSelectTeam } = teams;
  const places = leaguePhasePlaces(competition);
  const pad = variant === "full" ? "py-3 px-4" : "py-2 px-3";
  const head = `${pad} font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400`;
  const stat = `${pad} text-center text-sm text-gray-600 dark:text-gray-400 tabular-nums`;

  return (
    <table className="w-full text-left border-collapse">
      <caption className="sr-only">{t("tournaments.leaguePhase.tableCaption")}</caption>
      <thead>
        <tr className="bg-gray-50 dark:bg-navy-800 border-b border-gray-200 dark:border-navy-600 text-xs">
          <th scope="col" aria-label={t("common.position")} className={`${head} w-8`}>
            {t("common.rank")}
          </th>
          <th scope="col" className={head}>
            {t("common.team")}
          </th>
          <th scope="col" className={`${head} text-center`}>
            {t("common.played")}
          </th>
          {variant === "full" && (
            <>
              <th scope="col" className={`${head} text-center`}>
                {t("common.won")}
              </th>
              <th scope="col" className={`${head} text-center`}>
                {t("common.drawn")}
              </th>
              <th scope="col" className={`${head} text-center`}>
                {t("common.lost")}
              </th>
              <th scope="col" className={`${head} text-center`}>
                {t("common.gf")}
              </th>
              <th scope="col" className={`${head} text-center`}>
                {t("common.ga")}
              </th>
            </>
          )}
          <th scope="col" className={`${head} text-center`}>
            {t("common.gd")}
          </th>
          <th scope="col" className={`${head} text-center`}>
            {t("common.pts")}
          </th>
        </tr>
      </thead>
      <tbody className="divide-y divide-gray-100 dark:divide-navy-600">
        {rows.map((row, index) => {
          const position = index + 1;
          const zone = zoneOfPosition(position, places);
          const zoneName = t(`tournaments.leaguePhase.zones.${zone}`);
          const isUser = row.team_id === userTeamId;
          const clickable = isClubTeam(row.team_id);
          const goalDifference = row.goals_for - row.goals_against;
          return (
            <ContextMenu
              key={row.team_id}
              items={clickable ? [buildViewTeamMenuItem(t, () => onSelectTeam(row.team_id))] : []}
            >
              <tr
                onClick={clickable ? () => onSelectTeam(row.team_id) : undefined}
                className={`transition-colors ${clickable ? "cursor-pointer" : ""} ${
                  isUser
                    ? "bg-primary-50 dark:bg-primary-500/10"
                    : "hover:bg-gray-50 dark:hover:bg-navy-700/50"
                }`}
                data-testid={`${testIdPrefix}-${row.team_id}`}
                data-zone={zone}
              >
                <td
                  className={`${pad} font-heading font-bold text-sm text-gray-700 dark:text-gray-200 ${ZONE_BORDER[zone]}`}
                  title={zoneName}
                >
                  {position}
                  <span className="sr-only"> {zoneName}</span>
                </td>
                <th
                  scope="row"
                  className={`${pad} font-semibold text-sm ${
                    isUser
                      ? "text-primary-600 dark:text-primary-400"
                      : "text-gray-800 dark:text-gray-200"
                  }`}
                >
                  {clickable ? (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onSelectTeam(row.team_id);
                      }}
                      className="text-left hover:underline rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
                    >
                      {resolveTeamName(row.team_id)}
                    </button>
                  ) : (
                    resolveTeamName(row.team_id)
                  )}
                </th>
                <td className={stat}>{row.played}</td>
                {variant === "full" && (
                  <>
                    <td className={stat}>{row.won}</td>
                    <td className={stat}>{row.drawn}</td>
                    <td className={stat}>{row.lost}</td>
                    <td className={stat}>{row.goals_for}</td>
                    <td className={stat}>{row.goals_against}</td>
                  </>
                )}
                <td
                  className={`${pad} text-center text-sm font-semibold tabular-nums ${
                    goalDifference > 0
                      ? "text-primary-500 dark:text-primary-400"
                      : goalDifference < 0
                        ? "text-red-500 dark:text-red-400"
                        : "text-gray-500 dark:text-gray-400"
                  }`}
                >
                  {goalDifference > 0 ? `+${goalDifference}` : goalDifference}
                </td>
                <td
                  className={`${pad} text-center font-heading font-bold text-sm text-gray-800 dark:text-gray-100 tabular-nums`}
                >
                  {row.points}
                </td>
              </tr>
            </ContextMenu>
          );
        })}
      </tbody>
    </table>
  );
}
