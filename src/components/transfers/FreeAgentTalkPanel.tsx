import { useTranslation } from "react-i18next";

import { formatDate } from "../../lib/dateFormatting";
import { formatExactMoney } from "../../lib/helpers";
import { Button } from "../ui";
import type { FreeAgentTalkPanelData } from "./useFreeAgentContractFlow";

const INTEREST_CLASSES = {
  keen: "text-positive",
  hesitant: "text-warning",
  unlikely: "text-danger",
} as const;

/**
 * The negotiation side of signing a free agent: how he would take the terms, where the talk
 * stands (waiting for his answer, a counter to answer, ended) and who else is bidding. The
 * terms themselves are the form's.
 */
export default function FreeAgentTalkPanel({ panel }: { panel: FreeAgentTalkPanelData }) {
  const { t, i18n } = useTranslation();
  const { interest, talk, rivals, busy } = panel;
  const day = (value: string): string => formatDate(value, i18n.language);

  const awaiting = talk?.status === "awaiting_response" ? talk : null;
  const counter = talk?.status === "counter_offered" ? talk : null;
  const ended =
    talk && talk.status !== "awaiting_response" && talk.status !== "counter_offered" ? talk : null;

  return (
    <section
      aria-labelledby="free-agent-talk-title"
      className="rounded-lg border border-line bg-sunken p-3 mb-3 space-y-2"
    >
      <h4
        id="free-agent-talk-title"
        className="text-caption font-heading font-bold uppercase tracking-label text-fg-muted"
      >
        {t("freeAgents.talk.title")}
      </h4>

      {interest ? (
        <div className="space-y-1">
          <p className={`text-body font-semibold ${INTEREST_CLASSES[interest.verdict]}`}>
            {t(`freeAgents.talk.interest.${interest.verdict}`)}
          </p>
          {interest.main_obstacle ? (
            <p className="text-caption text-fg-muted">
              {t(`freeAgents.reason.${interest.main_obstacle}`)}
            </p>
          ) : null}
          <p className="text-caption text-fg-muted">
            {t("freeAgents.talk.expects", {
              wage: formatExactMoney(interest.expected_wage),
              years: interest.expected_years,
            })}
          </p>
          <p className="text-caption text-fg-muted">
            {t(`freeAgents.talk.place.${interest.place_in_side}`)}
          </p>
          <p className="text-caption text-fg-muted">
            {t("freeAgents.talk.responseTime", {
              min: interest.min_response_days,
              max: interest.max_response_days,
            })}
          </p>
        </div>
      ) : null}

      {awaiting?.respond_on ? (
        <div role="status" className="space-y-2">
          <p className="text-body font-semibold text-fg">
            {t("freeAgents.talk.awaiting", { date: day(awaiting.respond_on) })}
          </p>
          <Button variant="outline" size="sm" onClick={panel.onWithdraw} disabled={busy}>
            {t("freeAgents.talk.withdraw")}
          </Button>
        </div>
      ) : null}

      {counter?.counter_wage != null && counter.counter_years != null ? (
        <div role="status" className="space-y-2">
          <p className="text-body font-semibold text-fg">
            {t("freeAgents.talk.counter", {
              wage: formatExactMoney(counter.counter_wage),
              years: counter.counter_years,
              date: counter.counter_expires_on ? day(counter.counter_expires_on) : "",
            })}
          </p>
          {counter.reason ? (
            <p className="text-caption text-fg-muted">{t(`freeAgents.reason.${counter.reason}`)}</p>
          ) : null}
          <div className="flex gap-2">
            <Button size="sm" onClick={panel.onAcceptCounter} disabled={busy}>
              {t("freeAgents.talk.acceptCounter")}
            </Button>
            <Button variant="outline" size="sm" onClick={panel.onWithdraw} disabled={busy}>
              {t("freeAgents.talk.withdraw")}
            </Button>
          </div>
        </div>
      ) : null}

      {ended && ended.status !== "signed" ? (
        <p role="status" className="text-body text-fg">
          {t(`freeAgents.talk.ended.${ended.status}`)}
          {ended.reason && (ended.status === "rejected" || ended.status === "lapsed")
            ? ` ${t(`freeAgents.reason.${ended.reason}`)}`
            : ""}
        </p>
      ) : null}

      {rivals.length > 0 ? (
        <p className="text-caption text-warning">
          {t("freeAgents.talk.rivals", {
            clubs: rivals.map((rival) => rival.club_name).join(", "),
          })}
        </p>
      ) : null}
    </section>
  );
}
