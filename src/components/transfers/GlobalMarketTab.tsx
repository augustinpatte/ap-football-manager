import { useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";

import { Badge, Button, Card, CardBody, Dialog } from "../ui";
import { formatDate } from "../../lib/dateFormatting";
import { formatExactMoney, formatVal, formatWeeklyWage } from "../../lib/valueFormatting";
import GlobalMarketDetail from "./GlobalMarketDetail";
import GlobalMarketFilters from "./GlobalMarketFilters";
import GlobalMarketResults from "./GlobalMarketResults";
import { marketErrorMessage } from "./globalMarketModel";
import { useGlobalMarket, type GlobalMarketController } from "./useGlobalMarket";
import type { GameStateData } from "../../store/gameStore";

interface GlobalMarketTabProps {
  gameState: GameStateData;
  onGameUpdate: (state: GameStateData) => void;
}

export default function GlobalMarketTab({ onGameUpdate }: GlobalMarketTabProps) {
  const { t } = useTranslation();
  const controller = useGlobalMarket(onGameUpdate);
  const counts = controller.page?.counts;
  const noticeRef = useRef<HTMLDivElement>(null);

  // The confirmation is the result of the signing the manager just completed: take the keyboard
  // there instead of dropping it on a row that has just closed.
  useEffect(() => {
    if (controller.notice) noticeRef.current?.focus();
  }, [controller.notice]);

  const statusMessage = controller.notice
    ? t(controller.notice.key, controller.notice.params)
    : controller.loading && !controller.page
      ? t("globalMarket.loading")
      : !controller.error && controller.page && controller.page.items.length === 0
        ? t("globalMarket.noResults")
        : "";

  return (
    <div className="space-y-5">
      <Card accent="primary">
        <CardBody className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
          <div>
            <p className="text-caption font-heading font-bold uppercase tracking-[0.25em] text-primary-500">
              {t("globalMarket.kicker")}
            </p>
            <h2 className="text-title font-heading font-bold uppercase tracking-wide text-fg">
              {t("globalMarket.title")}
            </h2>
            <p className="text-caption text-fg-muted">{t("globalMarket.subtitle")}</p>
          </div>
          {counts ? (
            <div className="flex flex-wrap gap-2">
              <Badge variant="primary">{t("globalMarket.countAll", { count: counts.all })}</Badge>
              <Badge variant="accent">
                {t("globalMarket.countFree", { count: counts.freeAgent })}
              </Badge>
              <Badge variant="neutral">
                {t("globalMarket.countContracted", { count: counts.contracted })}
              </Badge>
            </div>
          ) : null}
        </CardBody>
      </Card>

      <div
        ref={noticeRef}
        role="status"
        aria-live="polite"
        aria-atomic="true"
        aria-label={t("globalMarket.title")}
        tabIndex={-1}
        className={
          controller.notice
            ? "rounded-lg border border-positive/40 bg-positive-tint px-4 py-3 text-body text-positive focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
            : "sr-only"
        }
      >
        {statusMessage}
      </div>

      <GlobalMarketFilters
        query={controller.query}
        onChange={controller.setFilter}
        onSearch={controller.reload}
        resultCount={controller.page?.total ?? 0}
      />

      <div
        role="alert"
        aria-live="assertive"
        aria-atomic="true"
        aria-label={t("globalMarket.title")}
        className={controller.error ? "" : "sr-only"}
      >
        {controller.error ? (
          <Card accent="danger">
            <CardBody className="space-y-3">
              <p className="text-body text-danger">{marketErrorMessage(controller.error, t)}</p>
              <Button onClick={controller.reload}>{t("globalMarket.retry")}</Button>
            </CardBody>
          </Card>
        ) : null}
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <GlobalMarketResults controller={controller} />
        <GlobalMarketDetail controller={controller} />
      </div>

      {controller.confirmOpen ? <ConfirmSigningDialog controller={controller} /> : null}
    </div>
  );
}

function ConfirmSigningDialog({ controller }: { controller: GlobalMarketController }) {
  const { t, i18n } = useTranslation();
  const titleId = useId();
  const { detail, outcome, signing } = controller;
  if (!detail || !outcome) return null;
  const body = outcome.freeAgent
    ? t("globalMarket.confirmFreeBody", {
        player: detail.name,
        wage: formatExactMoney(outcome.weeklyWage),
        date: formatDate(outcome.contractEnd, i18n.language),
      })
    : t("globalMarket.confirmBody", {
        player: detail.name,
        club: detail.clubName ?? t("globalMarket.club"),
        fee: formatVal(outcome.fee),
        wage: formatExactMoney(outcome.weeklyWage),
        date: formatDate(outcome.contractEnd, i18n.language),
      });

  return (
    <Dialog
      labelledBy={titleId}
      onClose={controller.cancelSign}
      busy={signing}
      className="max-h-[calc(100dvh-2rem)] w-full max-w-lg overflow-y-auto p-6"
    >
      <h2
        id={titleId}
        className="font-heading text-heading font-bold uppercase tracking-wide text-fg"
      >
        {t("globalMarket.confirmTitle")}
      </h2>
      <p className="mt-3 text-body text-fg-muted">{body}</p>
      <dl className="mt-4 space-y-1 text-caption text-fg-muted">
        <Term label={t("globalMarket.feeOffer")} value={formatVal(outcome.fee)} />
        <Term label={t("globalMarket.wageOffer")} value={formatWeeklyWage(outcome.weeklyWage)} />
        <Term
          label={t("globalMarket.yearsOffer")}
          value={t("globalMarket.years", { count: outcome.contractYears })}
        />
        {outcome.signingBonus > 0 ? (
          <Term
            label={t("globalMarket.bonusOffer")}
            value={formatExactMoney(outcome.signingBonus)}
          />
        ) : null}
        <Term
          label={t("globalMarket.contractUntil")}
          value={formatDate(outcome.contractEnd, i18n.language)}
        />
      </dl>
      <div className="mt-6 flex justify-end gap-2">
        <Button variant="ghost" onClick={controller.cancelSign} disabled={signing}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => void controller.confirmSign()} disabled={signing}>
          {signing ? t("globalMarket.signing") : t("globalMarket.confirm")}
        </Button>
      </div>
    </Dialog>
  );
}

function Term({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4">
      <dt>{label}</dt>
      <dd className="text-right font-heading font-bold tabular-nums text-fg">{value}</dd>
    </div>
  );
}
