import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { formatDate, formatExactMoney } from "../../../lib/helpers";
import type { ClubReport, Listed } from "../../../services/fastForwardService";
import { Badge } from "../../ui";
import {
  ContractText,
  DelegationText,
  EntityLink,
  EventText,
  InjuryText,
  MatchText,
  type ReportNavigation,
  TimelineText,
  TransferText,
} from "./ReportLines";
import { buildTimeline, omitted } from "./simulationReport.helpers";

interface SectionProps {
  club: ClubReport;
  nav?: ReportNavigation;
}

const LIST = "mt-2 flex flex-col divide-y divide-line";
const ROW = "py-1.5 text-body text-fg";

/** A heading and what sits under it; the heading names the region for assistive technology. */
function Block({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="mt-4 first:mt-0">
      <h3 id={id} className="text-caption font-bold uppercase tracking-label text-fg-muted">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Empty() {
  const { t } = useTranslation();
  return <p className="mt-2 text-body text-fg-muted">{t("fastForward.report.nothing")}</p>;
}

/** "and 12 more" when the backend kept a long list short; the totals elsewhere stay exact. */
function MoreNote<T>({ list }: { list: Listed<T> }) {
  const { t } = useTranslation();
  const hidden = omitted(list);
  if (hidden === 0) return null;
  return (
    <p className="mt-1 text-caption text-fg-muted">
      {t("fastForward.report.more", { count: hidden })}
    </p>
  );
}

interface Stat {
  label: string;
  value: string | number;
}

function StatGrid({ stats }: { stats: Stat[] }) {
  return (
    <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-3">
      {stats.map((stat) => (
        <div key={stat.label} className="flex items-baseline justify-between gap-2">
          <dt className="text-caption text-fg-muted">{stat.label}</dt>
          <dd className="text-body font-bold tabular-nums text-fg">{stat.value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function SummaryTab({ club }: SectionProps) {
  const { t } = useTranslation();
  const label = (key: string) => t(`fastForward.report.stat.${key}`);
  const { matches, transfers, contracts, finances, squad, delegation } = club;
  return (
    <>
      <Block id="report-sum-matches" title={t("fastForward.report.tab.matches")}>
        <StatGrid
          stats={[
            { label: label("played"), value: matches.played },
            { label: label("won"), value: matches.won },
            { label: label("drawn"), value: matches.drawn },
            { label: label("lost"), value: matches.lost },
            { label: label("goalsFor"), value: matches.goals_for },
            { label: label("goalsAgainst"), value: matches.goals_against },
          ]}
        />
      </Block>
      <Block id="report-sum-transfers" title={t("fastForward.report.tab.transfers")}>
        <StatGrid
          stats={[
            { label: label("bought"), value: transfers.purchases },
            { label: label("sold"), value: transfers.sales },
            { label: label("loans"), value: transfers.loans_in + transfers.loans_out },
            { label: label("freeSignings"), value: transfers.free_signings },
            { label: label("spent"), value: formatExactMoney(transfers.fees_paid) },
            { label: label("received"), value: formatExactMoney(transfers.fees_received) },
          ]}
        />
      </Block>
      <Block id="report-sum-contracts" title={t("fastForward.report.contractsTitle")}>
        <StatGrid
          stats={[
            { label: label("renewed"), value: contracts.renewed },
            { label: label("expired"), value: contracts.expired },
            { label: label("terminated"), value: contracts.terminated },
            { label: label("released"), value: contracts.released },
          ]}
        />
      </Block>
      <Block id="report-sum-finances" title={t("fastForward.report.tab.finances")}>
        <StatGrid
          stats={[
            { label: label("openingCash"), value: formatExactMoney(finances.opening_cash) },
            { label: label("closingCash"), value: formatExactMoney(finances.closing_cash) },
            { label: label("change"), value: formatExactMoney(finances.change) },
          ]}
        />
      </Block>
      <Block id="report-sum-squad" title={t("fastForward.report.tab.squad")}>
        <StatGrid
          stats={[
            { label: label("squadBefore"), value: squad.size_before },
            { label: label("squadAfter"), value: squad.size_after },
            { label: label("arrivals"), value: squad.arrivals.total },
            { label: label("departures"), value: squad.departures.total },
            { label: label("injuries"), value: squad.injuries.total },
          ]}
        />
      </Block>
      <Block id="report-sum-delegation" title={t("fastForward.report.tab.delegation")}>
        <StatGrid
          stats={[
            { label: label("executed"), value: delegation.executed },
            { label: label("approved"), value: delegation.approved },
            { label: label("declined"), value: delegation.declined },
            { label: label("leftToYou"), value: delegation.left_to_manager },
          ]}
        />
      </Block>
      <Block id="report-sum-events" title={t("fastForward.report.eventsTitle")}>
        {club.events.length === 0 ? (
          <Empty />
        ) : (
          <ul className={LIST}>
            {club.events.map((event, index) => (
              <li key={`${event.kind}-${event.date}-${index}`} className={ROW}>
                <span className="mr-2 text-caption text-fg-muted">{formatDate(event.date)}</span>
                <EventText line={event} />
              </li>
            ))}
          </ul>
        )}
      </Block>
    </>
  );
}

export function MatchesTab({ club, nav }: SectionProps) {
  const { items } = club.matches.fixtures;
  if (items.length === 0) return <Empty />;
  return (
    <>
      <ul className={LIST}>
        {items.map((line) => (
          <li key={line.fixture_id} className={ROW}>
            <span className="mr-2 text-caption text-fg-muted">{formatDate(line.date)}</span>
            <MatchText line={line} nav={nav} />
          </li>
        ))}
      </ul>
      <MoreNote list={club.matches.fixtures} />
    </>
  );
}

export function TransfersTab({ club, nav }: SectionProps) {
  const { t } = useTranslation();
  const { moves } = club.transfers;
  const { changes } = club.contracts;
  return (
    <>
      <Block id="report-transfers" title={t("fastForward.report.tab.transfers")}>
        {moves.items.length === 0 ? (
          <Empty />
        ) : (
          <ul className={LIST}>
            {moves.items.map((line, index) => (
              <li key={`${line.date}-${line.player_id}-${index}`} className={ROW}>
                <span className="mr-2 text-caption text-fg-muted">{formatDate(line.date)}</span>
                <TransferText line={line} nav={nav} />
              </li>
            ))}
          </ul>
        )}
        <MoreNote list={moves} />
      </Block>
      <Block id="report-contracts" title={t("fastForward.report.contractsTitle")}>
        {changes.items.length === 0 ? (
          <Empty />
        ) : (
          <ul className={LIST}>
            {changes.items.map((line, index) => (
              <li key={`${line.date}-${line.player_id}-${index}`} className={ROW}>
                <span className="mr-2 text-caption text-fg-muted">{formatDate(line.date)}</span>
                <ContractText line={line} nav={nav} />
              </li>
            ))}
          </ul>
        )}
        <MoreNote list={changes} />
      </Block>
    </>
  );
}

export function FinancesTab({ club }: SectionProps) {
  const { t } = useTranslation();
  const { finances } = club;
  const label = (key: string) => t(`fastForward.report.stat.${key}`);
  return (
    <>
      <StatGrid
        stats={[
          { label: label("openingCash"), value: formatExactMoney(finances.opening_cash) },
          { label: label("closingCash"), value: formatExactMoney(finances.closing_cash) },
          { label: label("change"), value: formatExactMoney(finances.change) },
          { label: label("paidForPlayers"), value: formatExactMoney(finances.transfer_fees_out) },
          {
            label: label("receivedForPlayers"),
            value: formatExactMoney(finances.transfer_fees_in),
          },
          { label: label("payments"), value: finances.posts },
        ]}
      />
      {finances.unexplained === 0 ? (
        <p role="status" className="mt-3 text-caption text-fg-muted">
          {t("fastForward.report.reconciled")}
        </p>
      ) : (
        <p role="alert" className="mt-3 text-caption text-danger">
          {t("fastForward.report.unexplained", { amount: formatExactMoney(finances.unexplained) })}
        </p>
      )}
      {finances.detail_complete ? null : (
        <p className="mt-1 text-caption text-fg-muted">{t("fastForward.report.folded")}</p>
      )}
      <Block id="report-by-kind" title={t("fastForward.report.byKind")}>
        {finances.by_kind.length === 0 ? (
          <Empty />
        ) : (
          <table className="mt-2 w-full text-body">
            <thead>
              <tr className="text-left text-caption text-fg-muted">
                <th scope="col" className="py-1 font-medium">
                  {t("fastForward.report.kindColumn")}
                </th>
                <th scope="col" className="py-1 text-right font-medium">
                  {t("fastForward.report.incomeColumn")}
                </th>
                <th scope="col" className="py-1 text-right font-medium">
                  {t("fastForward.report.expenseColumn")}
                </th>
                <th scope="col" className="py-1 text-right font-medium">
                  {t("fastForward.report.netColumn")}
                </th>
              </tr>
            </thead>
            <tbody>
              {finances.by_kind.map((total) => (
                <tr key={total.kind} className="border-t border-line">
                  <th scope="row" className="py-1 text-left font-normal text-fg">
                    {t(`fastForward.report.cashKind.${total.kind}`)}
                  </th>
                  <td className="py-1 text-right tabular-nums">{formatExactMoney(total.income)}</td>
                  <td className="py-1 text-right tabular-nums">
                    {formatExactMoney(total.expense)}
                  </td>
                  <td className="py-1 text-right font-bold tabular-nums">
                    {formatExactMoney(total.net)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Block>
    </>
  );
}

function PeopleList({
  title,
  id,
  list,
  nav,
}: {
  title: string;
  id: string;
  list: ClubReport["squad"]["arrivals"];
  nav?: ReportNavigation;
}) {
  const { t } = useTranslation();
  return (
    <Block id={id} title={`${title} (${list.total})`}>
      {list.items.length === 0 ? (
        <Empty />
      ) : (
        <ul className={LIST}>
          {list.items.map((move) => (
            <li key={move.player_id} className={ROW}>
              <PlayerName id={move.player_id} name={move.player_name} nav={nav} />{" "}
              <span className="text-fg-muted">
                {move.cause
                  ? t(`fastForward.report.cause.${move.cause}`, { defaultValue: move.cause })
                  : t("fastForward.report.cause.none")}
                {move.counterparty_name ? ` · ${move.counterparty_name}` : ""}
                {move.fee ? ` · ${formatExactMoney(move.fee)}` : ""}
                {move.date ? ` · ${formatDate(move.date)}` : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
      <MoreNote list={list} />
    </Block>
  );
}

function PlayerName({ id, name, nav }: { id: string; name: string; nav?: ReportNavigation }) {
  return <EntityLink label={name} onOpen={nav ? () => nav.openPlayer(id) : undefined} />;
}

export function SquadTab({ club, nav }: SectionProps) {
  const { t } = useTranslation();
  const { squad } = club;
  return (
    <>
      <p className="text-body text-fg-muted">
        {t("fastForward.report.squadSize", { before: squad.size_before, after: squad.size_after })}
      </p>
      <PeopleList
        id="report-arrivals"
        title={t("fastForward.report.stat.arrivals")}
        list={squad.arrivals}
        nav={nav}
      />
      <PeopleList
        id="report-departures"
        title={t("fastForward.report.stat.departures")}
        list={squad.departures}
        nav={nav}
      />
      <Block
        id="report-injuries"
        title={`${t("fastForward.report.stat.injuries")} (${squad.injuries.total})`}
      >
        {squad.injuries.items.length === 0 ? (
          <Empty />
        ) : (
          <ul className={LIST}>
            {squad.injuries.items.map((line, index) => (
              <li key={`${line.player_id}-${index}`} className={ROW}>
                <InjuryText line={line} nav={nav} />
              </li>
            ))}
          </ul>
        )}
        <MoreNote list={squad.injuries} />
      </Block>
    </>
  );
}

export function DelegationTab({ club }: SectionProps) {
  const { t } = useTranslation();
  const { delegation } = club;
  const label = (key: string) => t(`fastForward.report.stat.${key}`);
  return (
    <>
      <StatGrid
        stats={[
          { label: label("executed"), value: delegation.executed },
          { label: label("approved"), value: delegation.approved },
          { label: label("declined"), value: delegation.declined },
          { label: label("leftToYou"), value: delegation.left_to_manager },
        ]}
      />
      {delegation.entries.items.length === 0 ? (
        <Empty />
      ) : (
        <ul className={LIST}>
          {delegation.entries.items.map((line, index) => (
            <li key={`${line.date}-${line.title_key}-${index}`} className={ROW}>
              <span className="mr-2 text-caption text-fg-muted">{formatDate(line.date)}</span>
              <DelegationText line={line} />
            </li>
          ))}
        </ul>
      )}
      <MoreNote list={delegation.entries} />
    </>
  );
}

export function TimelineTab({ club, nav }: SectionProps) {
  const { t } = useTranslation();
  const entries = buildTimeline(club);
  if (entries.length === 0) return <Empty />;
  return (
    <>
      <p className="text-caption text-fg-muted">{t("fastForward.report.timelineNote")}</p>
      <ol className={LIST}>
        {entries.map((entry) => (
          <li key={entry.id} className={ROW}>
            <Badge variant="neutral" className="mr-2">
              {t(`fastForward.report.source.${entry.source}`)}
            </Badge>
            <span className="mr-2 text-caption text-fg-muted">{formatDate(entry.date)}</span>
            <TimelineText entry={entry} nav={nav} />
          </li>
        ))}
      </ol>
    </>
  );
}
