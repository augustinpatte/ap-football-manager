import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { GameStateData } from "../../store/gameStore";
import { formatExactMoney, getTeamName } from "../../lib/helpers";
import { formatDate } from "../../lib/dateFormatting";
import { Badge, Card, CardBody } from "../ui";
import { getTransferOfferBadgeVariant, getTransferOfferStatusLabel } from "./TransfersTab.helpers";
import {
  buildTransferActivity,
  type ActivityEntry,
  type ActivitySection,
} from "./transferActivity";

interface TransferActivityPanelProps {
  gameState: GameStateData;
  onSelectPlayer: (playerId: string) => void;
}

const SECTIONS: ActivitySection[] = [
  "received",
  "negotiating",
  "pendingRegistration",
  "completed",
  "activeLoans",
  "closed",
];

function directionKey(entry: ActivityEntry): string {
  if (entry.kind === "loan") {
    return entry.direction === "buying"
      ? "transfers.activity.direction.loanIn"
      : "transfers.activity.direction.loanOut";
  }
  return entry.direction === "buying"
    ? "transfers.activity.direction.buying"
    : "transfers.activity.direction.selling";
}

/**
 * Every transfer and loan the manager is part of, by stage, from what the game has stored. A bid or
 * loan whose result used to be visible only as an unchanged player row has a line here with its
 * status in words, and a way into the player's own page, where its full history is.
 */
export default function TransferActivityPanel({
  gameState,
  onSelectPlayer,
}: TransferActivityPanelProps) {
  const { t, i18n } = useTranslation();
  const [open, setOpen] = useState(false);
  const userTeamId = gameState.manager.team_id;
  if (!userTeamId) return null;

  const today = (gameState.clock?.current_date ?? "").slice(0, 10);
  const activity = buildTransferActivity(gameState.players, userTeamId, today);
  const total = SECTIONS.reduce((sum, section) => sum + activity[section].length, 0);
  const needsAnswer = activity.received.length;

  return (
    <Card className="mb-5">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="transfer-activity-body"
        onClick={() => setOpen((current) => !current)}
        className="flex w-full cursor-pointer items-center justify-between gap-3 p-4 text-left text-heading font-heading font-bold text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
      >
        <span>{t("transfers.activity.title")}</span>
        <span className="flex items-center gap-2">
          {needsAnswer > 0 && (
            <Badge variant="accent" size="sm">
              {t("transfers.activity.needsAnswer", { count: needsAnswer })}
            </Badge>
          )}
          <Badge variant="neutral" size="sm">
            {total}
          </Badge>
        </span>
      </button>
      {open && (
        <div id="transfer-activity-body">
          <CardBody>
            <p className="mb-3 text-caption text-fg-muted">{t("transfers.activity.intro")}</p>
            {total === 0 ? (
              <p className="text-body text-fg-muted">{t("transfers.activity.empty")}</p>
            ) : (
              SECTIONS.filter((section) => activity[section].length > 0).map((section) => (
                <section key={section} aria-labelledby={`activity-${section}`} className="mb-4">
                  <h3
                    id={`activity-${section}`}
                    className="mb-1 text-body font-heading font-bold text-fg"
                  >
                    {t(`transfers.activity.section.${section}`)} ({activity[section].length})
                  </h3>
                  <ul className="divide-y divide-line">
                    {activity[section].map((entry) => (
                      <li
                        key={entry.key}
                        className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2"
                      >
                        <button
                          type="button"
                          onClick={() => onSelectPlayer(entry.playerId)}
                          aria-label={t("transfers.activity.openPlayer", {
                            name: entry.playerName,
                          })}
                          className="text-body font-semibold text-fg underline-offset-2 hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
                        >
                          {entry.playerName}
                        </button>
                        <span className="text-caption text-fg-muted">{t(directionKey(entry))}</span>
                        <span className="text-caption text-fg-muted">
                          {getTeamName(
                            gameState.teams,
                            entry.counterpartyTeamId,
                            t("common.unknown"),
                          )}
                        </span>
                        {entry.fee !== null && (
                          <span className="text-caption text-fg">
                            {formatExactMoney(entry.fee)}
                          </span>
                        )}
                        {entry.wageSharePct !== null && (
                          <span className="text-caption text-fg-muted">
                            {t("transfers.activity.wageShare", { pct: entry.wageSharePct })}
                          </span>
                        )}
                        <Badge variant={getTransferOfferBadgeVariant(entry.status)} size="sm">
                          {getTransferOfferStatusLabel(
                            t,
                            entry.status,
                            entry.failureReason as never,
                          )}
                        </Badge>
                        {entry.registrationDate && section === "pendingRegistration" && (
                          <span className="text-caption text-fg-muted">
                            {t("transfers.activity.registersOn", {
                              date: formatDate(entry.registrationDate, i18n.language),
                            })}
                          </span>
                        )}
                        {entry.loanEndDate && section === "activeLoans" && (
                          <span className="text-caption text-fg-muted">
                            {t("transfers.activity.loanUntil", {
                              date: formatDate(entry.loanEndDate, i18n.language),
                            })}
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                </section>
              ))
            )}
            <p className="mt-2 text-caption text-fg-subtle">
              {t("transfers.activity.playerTermsNote")}
            </p>
          </CardBody>
        </div>
      )}
    </Card>
  );
}
