import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { useDelegationParams } from "../../staff/delegationParams";
import { competitionDisplayName } from "../../../lib/competitionName";
import { resolveInjuryName } from "../../../lib/injury";
import { useGameStore } from "../../../store/gameStore";
import { formatDate, formatExactMoney, formatWeeklyWage } from "../../../lib/helpers";
import type {
  ReportContractLine,
  ReportEvent,
  ReportInjuryLine,
  ReportMatchLine,
  ReportTransferLine,
} from "../../../services/fastForwardService";
import type { DelegationLogEntry } from "../../../services/staffService";
import type { TimelineEntry } from "./simulationReport.helpers";

/** Where a line of the report can lead: the screens the game already has for a player or a club. */
export interface ReportNavigation {
  openPlayer: (playerId: string) => void;
  openClub: (clubId: string) => void;
  openInboxMessage: (messageId: string) => void;
  openSchedule: () => void;
}

const LINK_CLASS =
  "rounded text-left font-medium text-fg underline decoration-line-strong underline-offset-2 hover:bg-hover focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-panel";

interface EntityLinkProps {
  label: string;
  onOpen?: () => void;
}

/** A name that opens its record when the screen can, and is plain text when it cannot. */
export function EntityLink({ label, onOpen }: EntityLinkProps) {
  const { t } = useTranslation();
  if (!onOpen) return <span className="font-medium text-fg">{label}</span>;
  return (
    <button
      type="button"
      className={LINK_CLASS}
      onClick={onOpen}
      aria-label={t("fastForward.report.openRecord", { name: label })}
    >
      {label}
    </button>
  );
}

function Player({ id, name, nav }: { id: string; name: string; nav?: ReportNavigation }) {
  return <EntityLink label={name} onOpen={nav ? () => nav.openPlayer(id) : undefined} />;
}

function Club({
  id,
  name,
  nav,
}: {
  id: string | null;
  name: string | null;
  nav?: ReportNavigation;
}) {
  const { t } = useTranslation();
  if (!name) return <span>{t("fastForward.report.unknownClub")}</span>;
  return <EntityLink label={name} onOpen={nav && id ? () => nav.openClub(id) : undefined} />;
}

/** Marks where a rendered piece goes inside a translated sentence; never part of any real text. */
const MARK = "\u241F";

/**
 * A translated sentence with pieces of the screen (a name that links) placed where its
 * `{{placeholders}}` stand, in whatever order the language puts them. The sentence is translated
 * with a marker in each placeholder and split on the markers, so no language needs markup in its text.
 */
function useSentence() {
  const { t } = useTranslation();
  return (key: string, parts: Record<string, ReactNode>): ReactNode => {
    const marked = Object.fromEntries(
      Object.keys(parts).map((name) => [name, `${MARK}${name}${MARK}`]),
    );
    return t(key, marked)
      .split(new RegExp(`${MARK}(\\w+)${MARK}`))
      .map((token, index) => {
        // `split` with a group alternates text and captured names.
        const key = `${index}-${token}`;
        return index % 2 === 1 ? (
          <span key={key}>{parts[token]}</span>
        ) : (
          <span key={key}>{token}</span>
        );
      });
  };
}

/**
 * A competition's name the way the rest of the game words it: from the competition in the game
 * (its key needs the season and the nation), falling back to what the report carries.
 */
function useCompetitionName() {
  const { t } = useTranslation();
  const competitions = useGameStore((state) => state.gameState?.competitions);
  return (competitionId: string, name: string, nameKey?: string | null): string => {
    const known = competitions?.find((competition) => competition.id === competitionId);
    if (known) return competitionDisplayName(known, t);
    return nameKey ? t(nameKey, { defaultValue: name }) : name;
  };
}

function feeText(fee: number | null, t: (key: string) => string): string {
  return fee === null || fee === 0 ? t("fastForward.report.noFee") : formatExactMoney(fee);
}

export function MatchText({ line, nav }: { line: ReportMatchLine; nav?: ReportNavigation }) {
  const { t } = useTranslation();
  const competitionName = useCompetitionName();
  const competition = competitionName(
    line.competition_id,
    line.competition_name,
    line.competition_name_key,
  );
  const shootout =
    line.penalties_for !== null && line.penalties_against !== null
      ? ` ${t("fastForward.report.penalties", { for: line.penalties_for, against: line.penalties_against })}`
      : "";
  return (
    <>
      <span className="text-fg-muted">{competition}</span>{" "}
      <span className="text-fg-muted">
        {line.at_home ? t("fastForward.report.home") : t("fastForward.report.away")}
      </span>{" "}
      <Club id={line.opponent_id} name={line.opponent_name} nav={nav} />{" "}
      <span className="font-bold tabular-nums text-fg">
        {line.goals_for}–{line.goals_against}
      </span>
      {shootout}{" "}
      <span className="text-fg-muted">({t(`fastForward.report.outcome.${line.outcome}`)})</span>
    </>
  );
}

export function TransferText({ line, nav }: { line: ReportTransferLine; nav?: ReportNavigation }) {
  const { t } = useTranslation();
  const sentence = useSentence();
  const viaOption = line.via_purchase_option && (line.kind === "purchase" || line.kind === "sale");
  const key = `fastForward.report.transfer.${line.kind}${viaOption ? "Option" : ""}`;
  return (
    <>
      {sentence(key, {
        player: <Player id={line.player_id} name={line.player_name} nav={nav} />,
        club: <Club id={line.counterparty_id} name={line.counterparty_name} nav={nav} />,
      })}
      {line.kind === "free_signing" ||
      line.kind === "loan_returned" ||
      line.kind === "loan_ended" ? null : (
        <span className="ml-2 text-fg-muted tabular-nums">
          {t("fastForward.report.feeLabel", { fee: feeText(line.fee, t) })}
        </span>
      )}
      {line.fee_in_journal === false ? (
        <span className="ml-2 text-caption text-danger">
          {t("fastForward.report.feeNotInJournal")}
        </span>
      ) : null}
    </>
  );
}

export function ContractText({ line, nav }: { line: ReportContractLine; nav?: ReportNavigation }) {
  const { t } = useTranslation();
  const sentence = useSentence();
  const wage = (value: number | null) => (value === null ? "–" : formatWeeklyWage(value));
  return (
    <>
      {sentence(`fastForward.report.contract.${line.kind}`, {
        player: <Player id={line.player_id} name={line.player_name} nav={nav} />,
      })}
      {line.kind === "renewal" ? (
        <span className="ml-2 text-fg-muted">
          {t("fastForward.report.contract.terms", {
            before: wage(line.weekly_wage_before),
            after: wage(line.weekly_wage_after),
            end: line.end_after ? formatDate(line.end_after) : "–",
          })}
        </span>
      ) : null}
    </>
  );
}

export function InjuryText({ line, nav }: { line: ReportInjuryLine; nav?: ReportNavigation }) {
  const { t } = useTranslation();
  let state: string;
  if (line.recovered_by) {
    state = t("fastForward.report.injury.recovered", { date: formatDate(line.recovered_by) });
  } else if (line.still_at_club) {
    state = t("fastForward.report.injury.stillInjured");
  } else {
    state = t("fastForward.report.injury.left");
  }
  return (
    <>
      <Player id={line.player_id} name={line.player_name} nav={nav} />{" "}
      <span className="text-fg-muted">
        {resolveInjuryName(line.injury, t)} ·{" "}
        {t("fastForward.report.injury.firstSeen", { date: formatDate(line.first_seen) })} · {state}
      </span>
    </>
  );
}

export function DelegationText({ line }: { line: DelegationLogEntry }) {
  const { t } = useTranslation();
  const named = useDelegationParams();
  // A resolved proposal is logged as "<proposal key>.accepted"; it reads as the proposal itself.
  const key = line.title_key.replace(/\.accepted$/, "");
  return (
    <>
      <span className="text-fg">{t(key, named(line.params))}</span>{" "}
      <span className="text-fg-muted">
        ({t(`fastForward.report.delegationOutcome.${line.outcome}`)})
      </span>
    </>
  );
}

export function EventText({ line, nav }: { line: ReportEvent; nav?: ReportNavigation }) {
  const { t } = useTranslation();
  const competitionName = useCompetitionName();
  switch (line.kind) {
    case "dismissed":
      return <>{t("fastForward.report.event.dismissed", { date: formatDate(line.date) })}</>;
    case "season_completed":
      return (
        <>
          {t("fastForward.report.event.seasonCompleted", {
            season: line.season,
            position: line.position,
            teams: line.total_teams,
            league: line.league_name,
            points: line.points,
          })}{" "}
          <span className="text-fg-muted">
            (
            {t(`fastForward.report.event.divisionMove.${line.division_move}`, {
              defaultValue: line.division_move,
            })}
            )
          </span>
        </>
      );
    case "knockout_advanced":
    case "knockout_eliminated":
      return (
        <>
          {t(
            line.kind === "knockout_advanced"
              ? "fastForward.report.event.knockoutAdvanced"
              : "fastForward.report.event.knockoutEliminated",
            { competition: competitionName(line.competition_id, line.competition_name) },
          )}
          {nav ? (
            <>
              {" "}
              <button type="button" className={LINK_CLASS} onClick={nav.openSchedule}>
                {t("fastForward.report.openSchedule")}
              </button>
            </>
          ) : null}
        </>
      );
    case "board_message":
      return (
        <>
          {t("fastForward.report.event.boardMessage", {
            subject: line.subject_key
              ? t(line.subject_key, { defaultValue: line.subject })
              : line.subject,
          })}
          {nav ? (
            <>
              {" "}
              <button
                type="button"
                className={LINK_CLASS}
                onClick={() => nav.openInboxMessage(line.message_id)}
              >
                {t("fastForward.report.openMessage")}
              </button>
            </>
          ) : null}
        </>
      );
  }
}

/** One timeline entry, whatever record it comes from. */
export function TimelineText({ entry, nav }: { entry: TimelineEntry; nav?: ReportNavigation }) {
  switch (entry.source) {
    case "event":
      return <EventText line={entry.line} nav={nav} />;
    case "match":
      return <MatchText line={entry.line} nav={nav} />;
    case "transfer":
      return <TransferText line={entry.line} nav={nav} />;
    case "contract":
      return <ContractText line={entry.line} nav={nav} />;
    case "injury":
      return <InjuryText line={entry.line} nav={nav} />;
    case "delegation":
      return <DelegationText line={entry.line} />;
  }
}
