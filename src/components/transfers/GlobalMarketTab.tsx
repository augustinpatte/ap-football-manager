import { useId } from "react";
import { useTranslation } from "react-i18next";

import { Badge, Button, Card, CardBody, Dialog } from "../ui";
import { formatDate } from "../../lib/dateFormatting";
import { formatExactMoney, formatVal } from "../../lib/valueFormatting";
import GlobalMarketDetail from "./GlobalMarketDetail";
import GlobalMarketFilters from "./GlobalMarketFilters";
import GlobalMarketResults from "./GlobalMarketResults";
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

  return (
    <div className="space-y-5">
      <Card accent="primary">
        <CardBody className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
          <div>
            <p className="text-xs font-heading font-bold uppercase tracking-[0.25em] text-primary-500">
              {t("globalMarket.kicker")}
            </p>
            <h2 className="text-2xl font-heading font-bold uppercase tracking-wide text-fg">
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

      {controller.notice ? (
        <p
          role="status"
          className="rounded-lg border border-positive/40 bg-positive-tint px-4 py-3 text-body text-positive"
        >
          {t(controller.notice.key, controller.notice.params)}
        </p>
      ) : null}

      <GlobalMarketFilters
        query={controller.query}
        onChange={controller.setFilter}
        onSearch={controller.reload}
        resultCount={controller.page?.total ?? 0}
      />

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
      className="w-full max-w-lg p-6"
    >
      <h2 id={titleId} className="font-heading text-xl font-bold uppercase tracking-wide text-fg">
        {t("globalMarket.confirmTitle")}
      </h2>
      <p className="mt-3 text-body text-fg-muted">{body}</p>
      <dl className="mt-4 space-y-1 text-caption text-fg-muted">
        <Term label={t("globalMarket.feeOffer")} value={formatVal(outcome.fee)} />
        <Term
          label={t("globalMarket.wageOffer")}
          value={`${formatExactMoney(outcome.weeklyWage)} / w`}
        />
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
      <dd className="font-heading font-bold tabular-nums text-fg">{value}</dd>
    </div>
  );
}
