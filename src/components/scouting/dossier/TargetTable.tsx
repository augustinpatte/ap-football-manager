import { Star } from "lucide-react";
import { useTranslation } from "react-i18next";

import { formatVal, positionBadgeVariant } from "../../../lib/helpers";
import type { ScoutingTargetView } from "../../../services/scoutingService";
import ReadableRating from "../../players/ReadableRating";
import { translatePositionAbbreviation } from "../../squad/SquadTab.helpers";
import { Badge, Checkbox } from "../../ui";
import { COMPARE_MAX, formatRange } from "./dossierFormat";

interface TargetTableProps {
  targets: ScoutingTargetView[];
  /** Ids chosen for the comparison; omit to hide the column. */
  compared?: string[];
  onToggleCompared?: (playerId: string) => void;
  onOpen: (target: ScoutingTargetView) => void;
  onToggleShortlist: (target: ScoutingTargetView) => void;
  busyId: string | null;
}

const HEAD =
  "py-2 px-2 text-left text-caption font-heading font-bold uppercase tracking-label text-fg-muted";
const CELL = "py-2 px-2 text-body text-fg";
const LINK =
  "text-left font-heading font-bold text-fg hover:text-positive focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page rounded";
const ACTION =
  "inline-flex items-center gap-1 rounded-lg px-2 py-1 text-caption font-heading font-bold uppercase tracking-label text-fg-muted hover:text-fg hover:bg-hover disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page";

/** Reports or shortlisted players, one row each: what is public, and what the club's scout said. */
export default function TargetTable({
  targets,
  compared,
  onToggleCompared,
  onOpen,
  onToggleShortlist,
  busyId,
}: TargetTableProps) {
  const { t } = useTranslation();
  return (
    <div className="overflow-x-auto">
      <table className="w-full">
        <thead>
          <tr className="border-b border-line">
            {compared && (
              <th className={HEAD}>
                <span className="sr-only">{t("scouting.dossier.compareColumn")}</span>
              </th>
            )}
            <th className={HEAD}>{t("scouting.player")}</th>
            <th className={HEAD}>{t("scouting.pos")}</th>
            <th className={`${HEAD} text-center`}>{t("scouting.age")}</th>
            <th className={HEAD}>{t("scouting.team")}</th>
            <th className={`${HEAD} text-center`}>{t("scouting.dossier.colRating")}</th>
            <th className={`${HEAD} text-center`}>{t("scouting.dossier.colAbility")}</th>
            <th className={`${HEAD} text-center`}>{t("scouting.dossier.colPotential")}</th>
            <th className={HEAD}>{t("scouting.dossier.colImpact")}</th>
            <th className={`${HEAD} text-center`}>{t("scouting.value")}</th>
            <th className={`${HEAD} text-right`}>{t("scouting.action")}</th>
          </tr>
        </thead>
        <tbody>
          {targets.map((target) => {
            const ability = formatRange(target.report?.ability);
            const potential = formatRange(target.report?.potential);
            return (
              <tr key={target.player_id} className="border-b border-line">
                {compared && onToggleCompared && (
                  <td className={CELL}>
                    <Checkbox
                      checked={compared.includes(target.player_id)}
                      disabled={
                        !compared.includes(target.player_id) && compared.length >= COMPARE_MAX
                      }
                      onChange={() => onToggleCompared(target.player_id)}
                      aria-label={t("scouting.dossier.selectToCompare", {
                        player: target.full_name,
                      })}
                    />
                  </td>
                )}
                <td className={CELL}>
                  <button type="button" onClick={() => onOpen(target)} className={LINK}>
                    {target.full_name}
                  </button>
                  {target.shortlisted && (
                    <Star
                      className="ml-1 inline h-3 w-3 text-warning"
                      aria-label={t("scouting.dossier.onShortlist")}
                    />
                  )}
                </td>
                <td className={CELL}>
                  <Badge variant={positionBadgeVariant(target.natural_position)} size="sm">
                    {translatePositionAbbreviation(t, target.natural_position)}
                  </Badge>
                </td>
                <td className={`${CELL} text-center`}>{target.age}</td>
                <td className={`${CELL} max-w-40 truncate text-fg-muted`}>
                  {target.team_name ?? t("common.freeAgent")}
                </td>
                <td className={`${CELL} text-center`}>
                  <ReadableRating rating={target.rating} />
                </td>
                <td className={`${CELL} text-center tabular-nums`}>
                  {ability ?? (
                    <span className="text-fg-subtle">{t("scouting.dossier.unknown")}</span>
                  )}
                </td>
                <td className={`${CELL} text-center tabular-nums`}>
                  {potential ?? (
                    <span className="text-fg-subtle">
                      {target.report
                        ? t("scouting.dossier.notJudged")
                        : t("scouting.dossier.unknown")}
                    </span>
                  )}
                </td>
                <td className={`${CELL} text-fg-muted`}>
                  {target.squad_impact
                    ? t(`scouting.dossier.impact.${target.squad_impact}`)
                    : t("scouting.dossier.unknown")}
                </td>
                <td className={`${CELL} text-center text-fg-muted`}>
                  {formatVal(target.market_value)}
                </td>
                <td className={`${CELL} text-right`}>
                  <button
                    type="button"
                    className={ACTION}
                    aria-label={t(
                      target.shortlisted
                        ? "scouting.dossier.removeShortlistFor"
                        : "scouting.dossier.addShortlistFor",
                      { player: target.full_name },
                    )}
                    disabled={busyId === target.player_id}
                    onClick={() => onToggleShortlist(target)}
                  >
                    {target.shortlisted
                      ? t("scouting.dossier.removeShortlist")
                      : t("scouting.dossier.addShortlist")}
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
