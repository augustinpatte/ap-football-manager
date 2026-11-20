import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";

import { formatDate } from "../../../lib/dateFormatting";
import { formatVal } from "../../../lib/helpers";
import type { ScoutingTargetView } from "../../../services/scoutingService";
import ReadableRating from "../../players/ReadableRating";
import { translatePositionAbbreviation } from "../../squad/SquadTab.helpers";
import { Button, Dialog } from "../../ui";
import { formatRange } from "./dossierFormat";

interface CompareDialogProps {
  targets: ScoutingTargetView[];
  onClose: () => void;
}

const HEAD =
  "py-2 px-3 text-left text-caption font-heading font-bold uppercase tracking-label text-fg-muted";
const CELL = "py-2 px-3 text-body text-fg align-top";

/** The chosen targets side by side. A figure nobody has earned reads as unknown, not as zero. */
export default function CompareDialog({ targets, onClose }: CompareDialogProps) {
  const { t, i18n } = useTranslation();
  const titleId = useId();
  const unknown = t("scouting.dossier.unknown");

  const rows: { label: string; cell: (target: ScoutingTargetView) => ReactNode }[] = [
    { label: t("scouting.team"), cell: (v) => v.team_name ?? t("common.freeAgent") },
    {
      label: t("scouting.pos"),
      cell: (v) => translatePositionAbbreviation(t, v.natural_position),
    },
    { label: t("scouting.age"), cell: (v) => String(v.age) },
    { label: t("scouting.dossier.colRating"), cell: (v) => <ReadableRating rating={v.rating} /> },
    {
      label: t("scouting.dossier.colAbility"),
      cell: (v) => formatRange(v.report?.ability) ?? unknown,
    },
    {
      label: t("scouting.dossier.colPotential"),
      cell: (v) =>
        formatRange(v.report?.potential) ?? (v.report ? t("scouting.dossier.notJudged") : unknown),
    },
    {
      label: t("scouting.confidence"),
      cell: (v) => (v.report ? t(`common.scoutConfidence.${v.report.confidence}`) : unknown),
    },
    {
      label: t("scouting.dossier.outlookTitle"),
      cell: (v) => t(`scouting.dossier.outlook.${v.outlook}`),
    },
    {
      label: t("scouting.dossier.colImpact"),
      cell: (v) => (v.squad_impact ? t(`scouting.dossier.impact.${v.squad_impact}`) : unknown),
    },
    { label: t("scouting.value"), cell: (v) => formatVal(v.market_value) },
    { label: t("scouting.dossier.colWage"), cell: (v) => formatVal(v.weekly_wage) },
    {
      label: t("scouting.dossier.colContract"),
      cell: (v) => (v.contract_end ? formatDate(v.contract_end, i18n.language) : unknown),
    },
    {
      label: t("scouting.dossier.financeTitle"),
      cell: (v) =>
        [
          v.fee_affordable === false ? t("scouting.risk.feeOverBudget") : null,
          v.wage_permitted ? null : t("scouting.risk.wageOverPolicy"),
        ]
          .filter(Boolean)
          .join(", ") || t("scouting.dossier.fitsBudget"),
    },
    {
      label: t("scouting.dossier.strengths"),
      cell: (v) =>
        v.report?.strengths.map((s) => t(`common.attributes.${s}`)).join(", ") || unknown,
    },
    {
      label: t("scouting.dossier.risksTitle"),
      cell: (v) =>
        v.risks.map((r) => t(`scouting.risk.${r}`)).join("; ") || t("scouting.dossier.noRisks"),
    },
  ];

  return (
    <Dialog
      labelledBy={titleId}
      onClose={onClose}
      dismissOnBackdrop
      className="w-full max-w-5xl max-h-screen overflow-auto rounded-xl p-6"
    >
      <div className="flex flex-col gap-4">
        <h3 id={titleId} className="font-heading text-heading font-bold text-fg">
          {t("scouting.dossier.compareTitle")}
        </h3>
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-b border-line">
                <th className={HEAD}>
                  <span className="sr-only">{t("scouting.dossier.compareMeasure")}</span>
                </th>
                {targets.map((target) => (
                  <th key={target.player_id} scope="col" className={`${HEAD} text-fg`}>
                    {target.full_name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.label} className="border-b border-line">
                  <th scope="row" className={`${CELL} text-left font-normal text-fg-muted`}>
                    {row.label}
                  </th>
                  {targets.map((target) => (
                    <td key={target.player_id} className={CELL}>
                      {row.cell(target)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="text-caption text-fg-subtle">{t("scouting.dossier.estimateNote")}</p>
        <div className="flex justify-end">
          <Button size="sm" variant="outline" onClick={onClose}>
            {t("common.close")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
