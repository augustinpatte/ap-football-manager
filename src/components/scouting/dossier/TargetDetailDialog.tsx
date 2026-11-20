import { useId } from "react";
import { useTranslation } from "react-i18next";

import { formatDate } from "../../../lib/dateFormatting";
import { formatVal } from "../../../lib/helpers";
import type { ScoutingTargetView } from "../../../services/scoutingService";
import ReadableRating from "../../players/ReadableRating";
import { Badge, Button, Dialog } from "../../ui";
import { formatRange } from "./dossierFormat";

interface TargetDetailDialogProps {
  target: ScoutingTargetView;
  /** Whether a scout is free to be sent for another look. */
  canScout: boolean;
  onClose: () => void;
  onToggleShortlist: (target: ScoutingTargetView) => void;
  onScoutAgain: (playerId: string) => void;
  onBid?: (playerId: string) => void;
  onOfferContract?: (playerId: string) => void;
  onOpenProfile?: (playerId: string) => void;
}

const SECTION = "text-caption font-heading font-bold uppercase tracking-label text-fg-muted";

function AttributeList({ names, empty }: { names: string[]; empty: string }) {
  const { t } = useTranslation();
  if (names.length === 0) return <p className="text-body text-fg-subtle">{empty}</p>;
  return (
    <ul className="flex flex-wrap gap-2">
      {names.map((name) => (
        <li key={name}>
          <Badge variant="neutral">{t(`common.attributes.${name}`)}</Badge>
        </li>
      ))}
    </ul>
  );
}

/** One player's report in full: what the scout said, how sure he was, and what to do about it. */
export default function TargetDetailDialog({
  target,
  canScout,
  onClose,
  onToggleShortlist,
  onScoutAgain,
  onBid,
  onOfferContract,
  onOpenProfile,
}: TargetDetailDialogProps) {
  const { t, i18n } = useTranslation();
  const titleId = useId();
  const report = target.report;
  const ability = formatRange(report?.ability);
  const potential = formatRange(report?.potential);

  return (
    <Dialog
      labelledBy={titleId}
      onClose={onClose}
      dismissOnBackdrop
      className="w-full max-w-2xl max-h-screen overflow-y-auto rounded-xl p-6"
    >
      <div className="flex flex-col gap-4">
        <div>
          <h3 id={titleId} className="font-heading text-heading font-bold text-fg">
            {target.full_name}
          </h3>
          <p className="flex flex-wrap items-center gap-x-2 text-body text-fg-muted">
            <span>
              {target.age} · {target.team_name ?? t("common.freeAgent")} ·
            </span>
            <ReadableRating rating={target.rating} />
          </p>
        </div>

        {report ? (
          <section className="flex flex-col gap-2" aria-label={t("scouting.dossier.reportSection")}>
            <h4 className={SECTION}>{t("scouting.dossier.reportSection")}</h4>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-body">
              <dt className="text-fg-muted">{t("scouting.dossier.colAbility")}</dt>
              <dd className="tabular-nums text-fg">{ability}</dd>
              <dt className="text-fg-muted">{t("scouting.dossier.colPotential")}</dt>
              <dd className="tabular-nums text-fg">
                {potential ?? t("scouting.dossier.notJudged")}
              </dd>
              <dt className="text-fg-muted">{t("scouting.confidence")}</dt>
              <dd className="text-fg">{t(`common.scoutConfidence.${report.confidence}`)}</dd>
              <dt className="text-fg-muted">{t("scouting.dossier.outlookTitle")}</dt>
              <dd className="text-fg">{t(`scouting.dossier.outlook.${target.outlook}`)}</dd>
            </dl>
            <p className="text-caption text-fg-subtle">
              {t("scouting.dossier.scoutedBy", {
                scout: report.scout_name,
                date: formatDate(report.scouted_on, i18n.language),
              })}{" "}
              {report.reports > 1 &&
                t("scouting.dossier.reportsCombined", { count: report.reports })}{" "}
              {t("scouting.dossier.estimateNote")}
            </p>
            <h4 className={SECTION}>{t("scouting.dossier.strengths")}</h4>
            <AttributeList names={report.strengths} empty={t("scouting.dossier.noneStoodOut")} />
            <h4 className={SECTION}>{t("scouting.dossier.weaknesses")}</h4>
            <AttributeList names={report.weaknesses} empty={t("scouting.dossier.noneStoodOut")} />
          </section>
        ) : (
          <p className="text-body text-fg-muted">{t("scouting.dossier.noReportYet")}</p>
        )}

        <section className="flex flex-col gap-1" aria-label={t("scouting.dossier.financeTitle")}>
          <h4 className={SECTION}>{t("scouting.dossier.financeTitle")}</h4>
          <p className="text-body text-fg">
            {target.fee_affordable === null
              ? t("scouting.dossier.freeNoFee")
              : target.fee_affordable
                ? t("scouting.dossier.feeFits", { fee: formatVal(target.market_value) })
                : t("scouting.dossier.feeOver", { fee: formatVal(target.market_value) })}
          </p>
          <p className="text-body text-fg">
            {target.wage_permitted
              ? t("scouting.dossier.wageFits")
              : t("scouting.dossier.wageOver")}
          </p>
          <p className="text-body text-fg-muted">
            {target.contract_end
              ? t("scouting.dossier.contractUntil", {
                  date: formatDate(target.contract_end, i18n.language),
                })
              : t("scouting.dossier.noContract")}
          </p>
        </section>

        <section className="flex flex-col gap-1" aria-label={t("scouting.dossier.risksTitle")}>
          <h4 className={SECTION}>{t("scouting.dossier.risksTitle")}</h4>
          {target.risks.length === 0 ? (
            <p className="text-body text-fg-subtle">{t("scouting.dossier.noRisks")}</p>
          ) : (
            <ul className="list-disc pl-5 text-body text-fg">
              {target.risks.map((risk) => (
                <li key={risk}>{t(`scouting.risk.${risk}`)}</li>
              ))}
            </ul>
          )}
        </section>

        <div className="flex flex-wrap gap-2 border-t border-line pt-4">
          {target.team_id && onBid && (
            <Button size="sm" onClick={() => onBid(target.player_id)}>
              {t("scouting.dossier.makeBid")}
            </Button>
          )}
          {!target.team_id && onOfferContract && (
            <Button size="sm" onClick={() => onOfferContract(target.player_id)}>
              {t("scouting.dossier.offerContract")}
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            disabled={!canScout || target.being_scouted}
            onClick={() => onScoutAgain(target.player_id)}
          >
            {target.being_scouted
              ? t("scouting.scoutingInProgress")
              : canScout
                ? t("scouting.dossier.scoutAgain")
                : t("scouting.noScoutsFree")}
          </Button>
          <Button size="sm" variant="outline" onClick={() => onToggleShortlist(target)}>
            {target.shortlisted
              ? t("scouting.dossier.removeShortlist")
              : t("scouting.dossier.addShortlist")}
          </Button>
          {onOpenProfile && (
            <Button size="sm" variant="ghost" onClick={() => onOpenProfile(target.player_id)}>
              {t("scouting.dossier.openProfile")}
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={onClose} className="ml-auto">
            {t("common.close")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
