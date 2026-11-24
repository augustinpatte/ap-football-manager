import type { JSX } from "react";
import { useTranslation } from "react-i18next";

import {
  KNOCKOUT_STAGES,
  aggregateSoFar,
  backendRoundName,
  ties as tiesOfStage,
} from "../../lib/leaguePhase";
import type {
  FixtureData,
  KnockoutStageData,
  KnockoutTieData,
  LeaguePhaseData,
} from "../../store/types";
import { Badge } from "../ui";
import { localizedRoundName } from "./TournamentsTab.helpers";
import type { TournamentsTeamLookup } from "./teamLookup";

interface LeaguePhaseBracketProps {
  state: LeaguePhaseData;
  fixtures: FixtureData[];
  teams: TournamentsTeamLookup;
}

interface TeamLineProps {
  name: string;
  legs: (number | null)[];
  aggregate: number | null;
  advanced: boolean;
  eliminated: boolean;
  mine: boolean;
  penalties: number | null;
}

function TeamLine({
  name,
  legs,
  aggregate,
  advanced,
  eliminated,
  mine,
  penalties,
}: TeamLineProps): JSX.Element {
  const { t } = useTranslation();
  const tone = advanced
    ? "font-bold text-gray-900 dark:text-white bg-primary-50 dark:bg-primary-900/20"
    : eliminated
      ? "text-gray-500 dark:text-gray-400"
      : "text-gray-700 dark:text-gray-300";
  return (
    <div className={`flex items-center gap-1.5 rounded px-2 py-1 text-xs ${tone}`}>
      <span
        className={`flex-1 truncate max-w-[9rem] ${mine ? "text-primary-600 dark:text-primary-400" : ""}`}
      >
        {name}
        {advanced && <span className="sr-only"> {t("tournaments.leaguePhase.advances")}</span>}
      </span>
      {legs.map((goals, leg) => (
        <span key={leg} className="w-5 text-center tabular-nums">
          <span aria-hidden="true">{goals ?? "–"}</span>
          <span className="sr-only">
            {goals == null
              ? t("tournaments.leaguePhase.legNotPlayed", { n: leg + 1 })
              : t("tournaments.leaguePhase.legScore", { n: leg + 1, goals })}
          </span>
        </span>
      ))}
      {legs.length > 1 && (
        <span className="w-6 text-center font-bold font-heading tabular-nums">
          <span aria-hidden="true">{aggregate ?? "–"}</span>
          {aggregate != null && (
            <span className="sr-only">
              {t("tournaments.leaguePhase.aggregateScore", { goals: aggregate })}
            </span>
          )}
        </span>
      )}
      {penalties != null && (
        <span className="text-caption font-semibold text-gray-500 dark:text-gray-400 tabular-nums">
          <span aria-hidden="true">({penalties})</span>
          <span className="sr-only">
            {t("tournaments.leaguePhase.penaltiesScore", { goals: penalties })}
          </span>
        </span>
      )}
    </div>
  );
}

function Tie({
  tie,
  fixtureById,
  teams,
}: {
  tie: KnockoutTieData;
  fixtureById: Map<string, FixtureData>;
  teams: TournamentsTeamLookup;
}): JSX.Element {
  const { t } = useTranslation();
  const legs = tie.fixture_ids.map((id) => fixtureById.get(id));
  const [seededSoFar, unseededSoFar] = aggregateSoFar(tie, fixtureById);
  const played = legs.some((fixture) => fixture?.result);
  const winner = tie.resolution?.winner_id ?? null;
  // Goals of each side in each leg: the unseeded club hosts the first leg, the seeded the return.
  const goalsOf = (clubId: string): (number | null)[] =>
    legs.map((fixture) => {
      if (!fixture?.result) return null;
      return fixture.home_team_id === clubId
        ? fixture.result.home_goals
        : fixture.result.away_goals;
    });
  const penalties = tie.resolution?.penalties ?? null;
  const decision = tie.resolution?.decision ?? null;
  const userInvolved =
    !!teams.userTeamId &&
    (tie.seeded_team_id === teams.userTeamId || tie.unseeded_team_id === teams.userTeamId);

  const line = (clubId: string, seeded: boolean): JSX.Element => (
    <TeamLine
      name={teams.resolveTeamName(clubId)}
      legs={goalsOf(clubId)}
      aggregate={played ? (seeded ? seededSoFar : unseededSoFar) : null}
      advanced={winner === clubId}
      eliminated={winner != null && winner !== clubId}
      mine={clubId === teams.userTeamId}
      penalties={penalties ? (seeded ? penalties[0] : penalties[1]) : null}
    />
  );

  return (
    <li
      data-testid={`leaguephase-tie-${tie.id}`}
      aria-label={t("tournaments.leaguePhase.tieLabel", {
        seeded: teams.resolveTeamName(tie.seeded_team_id),
        unseeded: teams.resolveTeamName(tie.unseeded_team_id),
      })}
      className={`rounded-lg border overflow-hidden ${
        userInvolved
          ? "border-primary-400/50 dark:border-primary-500/40"
          : "border-gray-200 dark:border-navy-600"
      }`}
    >
      {line(tie.seeded_team_id, true)}
      <div className="h-px bg-gray-100 dark:bg-navy-700" />
      {line(tie.unseeded_team_id, false)}
      {tie.stage === "Final" && (
        <div
          className="px-2 py-0.5 text-caption text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-navy-900/40"
          data-testid="leaguephase-neutral-venue"
        >
          <span className="font-semibold">{t("tournaments.leaguePhase.neutralVenue")}</span>
          <span className="sr-only"> {t("tournaments.leaguePhase.neutralFinalNote")}</span>
        </div>
      )}
      {decision && decision !== "Aggregate" && (
        <div className="px-2 py-0.5 text-caption text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-navy-900/40">
          {decision === "ExtraTime"
            ? t("tournaments.leaguePhase.afterExtraTime")
            : t("tournaments.leaguePhase.onPenalties")}
        </div>
      )}
    </li>
  );
}

/**
 * The knockout phase of a league-phase competition: every drawn round as a column of two-legged
 * ties (one match for the final), with the aggregate of the legs played so far, the winner marked
 * in words as well as weight, and the rounds not drawn yet named as such.
 */
export default function LeaguePhaseBracket({
  state,
  fixtures,
  teams,
}: LeaguePhaseBracketProps): JSX.Element {
  const { t } = useTranslation();
  const fixtureById = new Map(fixtures.map((fixture) => [fixture.id, fixture]));
  const roundName = (stage: KnockoutStageData): string =>
    localizedRoundName(t, backendRoundName(stage));

  return (
    <section
      aria-label={t("tournaments.leaguePhase.bracketTitle")}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: a horizontally scrolling region must take a tab stop, or a keyboard user cannot scroll to the later rounds.
      tabIndex={0}
      className="overflow-x-auto rounded-xl border border-gray-200 dark:border-navy-600 bg-white dark:bg-navy-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
    >
      <div className="flex gap-0 min-w-max">
        {KNOCKOUT_STAGES.map((stage) => {
          const stageTies = tiesOfStage(state, stage);
          const complete = stageTies.length > 0 && stageTies.every((tie) => tie.resolution);
          return (
            <div
              key={stage}
              className="flex flex-col min-w-[14rem] border-r border-gray-100 dark:border-navy-700 last:border-r-0"
            >
              <div className="flex items-center justify-between gap-2 px-3 py-2.5 border-b border-gray-100 dark:border-navy-700 bg-gray-50 dark:bg-navy-900/40">
                <span className="text-xs font-heading font-bold uppercase tracking-wider text-gray-600 dark:text-gray-300 truncate">
                  {roundName(stage)}
                </span>
                {stageTies.length > 0 && (
                  <Badge variant={complete ? "accent" : "neutral"} size="sm">
                    {complete ? t("tournaments.roundComplete") : t("tournaments.roundInProgress")}
                  </Badge>
                )}
              </div>
              {stageTies.length === 0 ? (
                <p className="text-xs italic text-gray-500 dark:text-gray-400 text-center p-7">
                  {t("tournaments.leaguePhase.notDrawn")}
                </p>
              ) : (
                <ul className="flex flex-col justify-around gap-2 p-3">
                  {stageTies.map((tie) => (
                    <Tie key={tie.id} tie={tie} fixtureById={fixtureById} teams={teams} />
                  ))}
                </ul>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
