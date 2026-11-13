import { type ChangeEvent, useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Badge, Button, Card, CardBody, CardHeader } from "../ui";
import { buildPlayerAttributeGroups } from "../playerProfile/PlayerProfile.attributes";
import { formatDate } from "../../lib/dateFormatting";
import { formatExactMoney, formatVal, formatWeeklyWage } from "../../lib/valueFormatting";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import {
  type NegotiationForm,
  canSign,
  isCounterOffer,
  marketErrorMessage,
  negotiationTone,
  signHintKey,
} from "./globalMarketModel";
import type { GlobalMarketController } from "./useGlobalMarket";
import type { ExternalNegotiationOutcome } from "../../services/registryService";

const FIELD_CLASS =
  "w-full rounded-lg border border-line bg-panel px-3 py-2 text-body text-fg placeholder:text-fg-subtle focus:outline-none focus-visible:ring-2 focus-visible:ring-focus";

const TONE_CLASS = {
  positive: "border-positive/40 bg-positive-tint text-positive",
  warning: "border-warning/40 bg-warning-tint text-warning",
  danger: "border-danger/40 bg-danger-tint text-danger",
  neutral: "border-line bg-sunken text-fg-muted",
} as const;

interface GlobalMarketDetailProps {
  controller: GlobalMarketController;
}

export default function GlobalMarketDetail({ controller }: GlobalMarketDetailProps) {
  const { t } = useTranslation();
  const { detail, detailLoading, selectedId, actionError } = controller;
  const detailRef = useRef<HTMLDivElement>(null);

  // Choosing a player is navigation, not just a tint: the profile takes focus so a keyboard or
  // screen-reader user is taken to the thing they asked for.
  useEffect(() => {
    if (detail) detailRef.current?.focus();
  }, [detail]);

  return (
    <Card className="min-w-0 xl:sticky xl:top-6 xl:flex xl:max-h-[calc(100dvh-8rem)] xl:flex-col xl:overflow-hidden">
      <CardHeader
        action={
          <Button variant="ghost" size="sm" onClick={controller.closeDetail}>
            {t("common.close")}
          </Button>
        }
      >
        {t("globalMarket.profile")}
      </CardHeader>
      <CardBody className="space-y-4 xl:min-h-0 xl:overflow-y-auto xl:overscroll-contain">
        <div
          ref={detailRef}
          role="region"
          aria-label={t("globalMarket.profile")}
          tabIndex={-1}
          className="space-y-4 focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-panel"
        >
          {detailLoading ? (
            <p className="text-body text-fg-muted">{t("globalMarket.loadingProfile")}</p>
          ) : detail ? (
            <>
              <PlayerSummary controller={controller} />
              {detail.attributes ? <AttributeGroups controller={controller} /> : null}
              <NegotiationPanel controller={controller} />
            </>
          ) : (
            <p className="text-body text-fg-muted">
              {selectedId ? t("globalMarket.profileUnavailable") : t("globalMarket.selectPrompt")}
            </p>
          )}
        </div>
        <div
          role="alert"
          aria-live="assertive"
          aria-atomic="true"
          className={
            actionError ? "rounded-lg border border-danger/40 bg-danger-tint px-3 py-2" : "sr-only"
          }
        >
          {actionError ? (
            <p className="text-body text-danger">{marketErrorMessage(actionError, t)}</p>
          ) : null}
        </div>
      </CardBody>
    </Card>
  );
}

function PlayerSummary({ controller }: GlobalMarketDetailProps) {
  const { t, i18n } = useTranslation();
  const detail = controller.detail;
  if (!detail) return null;
  const position = detail.naturalPosition
    ? translatePositionAbbreviation(t, detail.naturalPosition)
    : (detail.position ?? "—");
  const ownership =
    detail.ownership === "free_agent" ? t("globalMarket.freeAgent") : t("globalMarket.contracted");

  return (
    <div className="space-y-3">
      <div>
        <h3 className="break-words font-heading text-title font-bold uppercase tracking-wide text-fg">
          {detail.name}
        </h3>
        <p className="break-words text-caption text-fg-muted">
          {position} · {detail.nationality ?? detail.nation ?? "—"} · {detail.age ?? "—"}
        </p>
      </div>
      <div className="grid grid-cols-1 gap-2 text-body sm:grid-cols-2">
        <Stat label={t("common.ovr")} value={String(detail.ovr)} />
        <Stat label={t("globalMarket.potential")} value={String(detail.potential)} />
        <Stat label={t("globalMarket.marketValue")} value={formatVal(detail.marketValue)} />
        <Stat label={t("common.wage")} value={formatWeeklyWage(detail.weeklyWage)} />
        <Stat
          label={t("globalMarket.club")}
          value={detail.clubName ?? t("globalMarket.freeAgent")}
        />
        <Stat
          label={t("globalMarket.contractUntil")}
          value={
            detail.contractEnd
              ? formatDate(detail.contractEnd, i18n.language)
              : t("globalMarket.noContract")
          }
        />
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sunken px-3 py-2 text-body">
        <span className="text-fg-muted">{t("globalMarket.askingPrice")}</span>
        <span className="font-heading font-bold text-fg">
          {detail.ownership === "free_agent"
            ? t("globalMarket.freeAgent")
            : formatVal(detail.askingPrice)}
        </span>
      </div>
      <Badge variant={detail.ownership === "free_agent" ? "accent" : "primary"}>{ownership}</Badge>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-lg bg-sunken px-3 py-2">
      <p className="text-caption font-heading font-bold uppercase tracking-label text-fg-subtle">
        {label}
      </p>
      <p className="break-words font-heading font-bold text-fg">{value}</p>
    </div>
  );
}

function AttributeGroups({ controller }: GlobalMarketDetailProps) {
  const { t } = useTranslation();
  const detail = controller.detail;
  if (!detail?.attributes) return null;
  const groups = buildPlayerAttributeGroups(
    {
      position: detail.position ?? "",
      natural_position: detail.naturalPosition ?? undefined,
      attributes: detail.attributes,
    },
    t,
  );
  return (
    <div className="space-y-3">
      {groups.map((group) => (
        <div key={group.label}>
          <h4 className="mb-1 text-caption font-heading font-bold uppercase tracking-label text-fg-subtle">
            {group.label}
          </h4>
          <dl className="grid grid-cols-1 gap-x-3 gap-y-1 text-caption sm:grid-cols-2">
            {group.attrs.map((attribute) => (
              <div key={attribute.name} className="flex justify-between gap-2">
                <dt className="text-fg-muted">{attribute.name}</dt>
                <dd className="font-heading font-bold tabular-nums text-fg">{attribute.value}</dd>
              </div>
            ))}
          </dl>
        </div>
      ))}
    </div>
  );
}

function NegotiationPanel({ controller }: GlobalMarketDetailProps) {
  const { t } = useTranslation();
  const { form, outcome, quoting, signing, confirmOpen } = controller;
  const field = (key: keyof NegotiationForm) => (event: ChangeEvent<HTMLInputElement>) =>
    controller.setFormField(key, event.target.value);

  return (
    <div className="space-y-3 border-t border-line pt-4">
      <h4 className="font-heading text-body font-bold uppercase tracking-wide text-fg">
        {t("globalMarket.negotiation")}
      </h4>
      <p className="text-caption text-fg-muted">{t("globalMarket.negotiationHelp")}</p>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <NumberField
          label={t("globalMarket.feeOffer")}
          value={form.fee}
          onChange={field("fee")}
          hint={
            controller.detail
              ? `${t("globalMarket.askingPrice")}: ${formatVal(controller.detail.askingPrice)}`
              : undefined
          }
        />
        <NumberField
          label={t("globalMarket.wageOffer")}
          value={form.weeklyWage}
          onChange={field("weeklyWage")}
        />
        <NumberField
          label={t("globalMarket.yearsOffer")}
          value={form.contractYears}
          onChange={field("contractYears")}
        />
        <NumberField
          label={t("globalMarket.bonusOffer")}
          value={form.signingBonus}
          onChange={field("signingBonus")}
        />
      </div>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={controller.checkTerms} disabled={quoting || signing}>
          {quoting ? t("globalMarket.checking") : t("globalMarket.checkTerms")}
        </Button>
        {isCounterOffer(outcome?.status) ? (
          <Button variant="outline" size="sm" onClick={controller.applyCounter} disabled={quoting}>
            {t("globalMarket.useTheirTerms")}
          </Button>
        ) : null}
      </div>
      <div
        role="status"
        aria-live="polite"
        aria-atomic="true"
        aria-label={t("globalMarket.negotiation")}
      >
        {outcome ? <OutcomePanel outcome={outcome} /> : null}
      </div>
      <Button
        className="w-full"
        onClick={controller.requestSign}
        disabled={!canSign(outcome) || signing || confirmOpen}
      >
        {signing ? t("globalMarket.signing") : t("globalMarket.sign")}
      </Button>
      {!canSign(outcome) ? (
        <p className="text-caption text-fg-muted">{t(signHintKey(outcome))}</p>
      ) : null}
    </div>
  );
}

function NumberField({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: string;
  onChange: (event: ChangeEvent<HTMLInputElement>) => void;
  hint?: string;
}) {
  const hintId = useId();
  return (
    <label className="block text-caption text-fg-muted">
      <span className="mb-1 block">{label}</span>
      <input
        type="number"
        min={0}
        inputMode="numeric"
        className={FIELD_CLASS}
        value={value}
        aria-describedby={hint ? hintId : undefined}
        onChange={onChange}
      />
      {hint ? (
        <span id={hintId} className="mt-1 block text-caption text-fg-subtle">
          {hint}
        </span>
      ) : null}
    </label>
  );
}

function OutcomePanel({ outcome }: { outcome: ExternalNegotiationOutcome }) {
  const { t, i18n } = useTranslation();
  return (
    <div
      className={`space-y-2 rounded-lg border px-3 py-2 text-caption ${TONE_CLASS[negotiationTone(outcome.status)]}`}
    >
      <p className="font-heading font-bold uppercase tracking-wide">
        {t(`globalMarket.status.${outcome.status}`)}
      </p>
      <p>{outcomeMessage(outcome, t)}</p>
      {outcome.mainObstacle ? (
        <p className="text-fg-muted">
          {t("globalMarket.mainConcern")}: {t(`freeAgents.reason.${outcome.mainObstacle}`)}
        </p>
      ) : null}
      <dl className="grid grid-cols-1 gap-x-4 gap-y-1 border-t border-line pt-2 text-fg sm:grid-cols-2">
        <TermRow
          label={t("globalMarket.feeOffer")}
          value={outcome.freeAgent ? t("globalMarket.freeAgent") : formatVal(outcome.fee)}
        />
        <TermRow label={t("globalMarket.wageOffer")} value={formatExactMoney(outcome.weeklyWage)} />
        <TermRow
          label={t("globalMarket.yearsOffer")}
          value={t("globalMarket.years", { count: outcome.contractYears })}
        />
        {outcome.signingBonus > 0 ? (
          <TermRow
            label={t("globalMarket.bonusOffer")}
            value={formatExactMoney(outcome.signingBonus)}
          />
        ) : null}
        <TermRow
          label={t("globalMarket.contractUntil")}
          value={formatDate(outcome.contractEnd, i18n.language)}
        />
      </dl>
      <ul className="grid gap-1 text-fg-muted">
        <Check ok={outcome.canAffordFee} label={t("globalMarket.checkFee")} />
        <Check ok={outcome.wagePolicyAllows} label={t("globalMarket.checkWage")} />
        <Check ok={outcome.canAffordBonus} label={t("globalMarket.checkBonus")} />
        <Check ok={outcome.windowOpen} label={t("globalMarket.checkWindow")} />
      </ul>
      {outcome.blocker ? (
        <p className="font-semibold">{marketErrorMessage(outcome.blocker, t)}</p>
      ) : null}
    </div>
  );
}

function TermRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3">
      <dt className="text-fg-muted">{label}</dt>
      <dd className="text-right font-heading font-bold tabular-nums">{value}</dd>
    </div>
  );
}

function Check({ ok, label }: { ok: boolean; label: string }) {
  const { t } = useTranslation();
  return (
    <li className="flex items-center gap-2">
      <span aria-hidden="true">{ok ? "✓" : "✗"}</span>
      <span>{label}</span>
      <span className="sr-only">
        {ok ? t("globalMarket.checkPassed") : t("globalMarket.checkFailed")}
      </span>
    </li>
  );
}

function outcomeMessage(
  outcome: ExternalNegotiationOutcome,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  switch (outcome.status) {
    case "agreed":
      return t("globalMarket.outcome.agreed");
    case "club_countered":
      return t("globalMarket.outcome.clubCountered", {
        club: outcome.fromClub ?? t("globalMarket.club"),
        fee: formatVal(outcome.fee),
      });
    case "club_rejected":
      return t("globalMarket.outcome.clubRejected", {
        club: outcome.fromClub ?? t("globalMarket.club"),
        fee: formatVal(outcome.minimumFee),
      });
    case "player_countered":
      return t("globalMarket.outcome.playerCountered", {
        wage: formatExactMoney(outcome.weeklyWage),
        years: outcome.contractYears,
      });
    case "player_refused":
      return t("globalMarket.outcome.playerRefused");
    case "blocked":
      return t("globalMarket.outcome.blocked");
    default:
      return t("globalMarket.outcome.blocked");
  }
}
