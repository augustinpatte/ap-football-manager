import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { GameStateData } from "../../store/gameStore";
import {
  getTransferDeadlineSummary,
  type DeadlineItemData,
  type TransferDeadlineSummaryData,
} from "../../services/transfersService";
import { formatDate } from "../../lib/dateFormatting";
import { formatVal } from "../../lib/helpers";
import { resolveBackendError } from "../../utils/backendI18n";
import { Badge, Card, CardBody } from "../ui";

interface TransferDeadlinePanelProps {
  gameState: GameStateData;
  onSelectPlayer: (id: string) => void;
}

const SHOWN_PHASES = new Set<TransferDeadlineSummaryData["phase"]>([
  "last_week",
  "last_72h",
  "deadline_day",
]);

/** Whether the panel earns its place: the window is closing, or something awaits an answer. */
export function shouldShowDeadlinePanel(summary: TransferDeadlineSummaryData): boolean {
  return (
    summary.window_open && (SHOWN_PHASES.has(summary.phase) || summary.urgent_response_count > 0)
  );
}

function DeadlineItemRow({
  item,
  onSelectPlayer,
}: {
  item: DeadlineItemData;
  onSelectPlayer: (id: string) => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
      <Badge variant={item.needs_response ? "danger" : "neutral"}>
        {t(`transfers.deadline.state.${item.state}`)}
      </Badge>
      <button
        type="button"
        onClick={() => onSelectPlayer(item.player_id)}
        className="text-body font-semibold text-fg hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page rounded"
      >
        {item.player_name}
      </button>
      <span className="text-caption text-fg-muted">
        {t(item.kind === "loan" ? "transfers.loan" : "transfers.transfer")} ·{" "}
        {item.counterparty_team_name}
        {item.fee !== null ? ` · ${formatVal(item.fee)}` : ""}
        {item.registration_date
          ? ` · ${t("transfers.deadline.scheduledFor", {
              date: formatDate(item.registration_date, i18n.language),
            })}`
          : ""}
      </span>
    </li>
  );
}

export default function TransferDeadlinePanel({
  gameState,
  onSelectPlayer,
}: TransferDeadlinePanelProps) {
  const { t, i18n } = useTranslation();
  const [summary, setSummary] = useState<TransferDeadlineSummaryData | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The game state changes with every offer answered and every day played, so it is the cue to ask
  // the backend again: the panel never works out for itself what is pending.
  useEffect(() => {
    let cancelled = false;
    getTransferDeadlineSummary()
      .then((result) => {
        if (!cancelled) {
          setSummary(result);
          setError(null);
        }
      })
      .catch((failure: unknown) => {
        if (!cancelled) {
          setError(resolveBackendError(failure));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [gameState]);

  if (error) {
    return (
      <p role="alert" className="mb-4 text-caption font-bold text-danger">
        {error}
      </p>
    );
  }
  if (!summary || !shouldShowDeadlinePanel(summary)) {
    return null;
  }

  const closing =
    summary.phase === "deadline_day"
      ? t("season.windowClosesToday")
      : summary.days_remaining !== null
        ? t("season.windowClosesInDays", { count: summary.days_remaining })
        : null;

  return (
    <section aria-labelledby="transfer-deadline-title" className="mb-4">
      <Card accent={summary.urgent ? "danger" : "primary"}>
        <CardBody>
          <div className="flex flex-wrap items-center gap-3">
            <h3
              id="transfer-deadline-title"
              className="font-heading text-heading font-bold uppercase tracking-label text-fg"
            >
              {t("transfers.deadline.title")}
            </h3>
            <Badge variant={summary.urgent ? "danger" : "accent"}>
              {t(`transfers.deadline.phase.${summary.phase}`)}
            </Badge>
            {closing ? <span className="text-body text-fg-muted">{closing}</span> : null}
            {summary.closes_on ? (
              <span className="text-caption text-fg-subtle">
                {formatDate(summary.closes_on, i18n.language)}
              </span>
            ) : null}
          </div>

          <p
            aria-live="polite"
            className={`mt-2 text-body font-semibold ${
              summary.urgent_response_count > 0 ? "text-danger" : "text-fg-muted"
            }`}
          >
            {summary.urgent_response_count > 0
              ? t("transfers.deadline.needsResponse", { count: summary.urgent_response_count })
              : t("transfers.deadline.nothingPending")}
          </p>
          {summary.urgent_response_count > 0 ? (
            <p className="text-caption text-fg-muted">{t("transfers.deadline.lapseWarning")}</p>
          ) : null}

          {summary.items.length > 0 ? (
            <ul
              className="mt-2 divide-y divide-line"
              aria-label={t("transfers.deadline.pendingList")}
            >
              {summary.items.map((item) => (
                <DeadlineItemRow
                  key={`${item.kind}-${item.offer_id}`}
                  item={item}
                  onSelectPlayer={onSelectPlayer}
                />
              ))}
            </ul>
          ) : null}

          {summary.recent_deals.length > 0 ? (
            <div className="mt-3">
              <h4 className="text-caption font-heading font-bold uppercase tracking-label text-fg-muted">
                {t("transfers.deadline.recentDeals")}
              </h4>
              <ul className="mt-1 space-y-1">
                {summary.recent_deals.map((deal) => (
                  <li
                    key={`${deal.player_id}-${deal.date}-${deal.fee}`}
                    className="text-caption text-fg-muted"
                  >
                    {t("transfers.deadline.dealLine", {
                      player: deal.player_name,
                      from: deal.from_team_name,
                      to: deal.to_team_name,
                      fee: formatVal(deal.fee),
                    })}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </CardBody>
      </Card>
    </section>
  );
}
