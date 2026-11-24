import { AlertTriangle, Trophy } from "lucide-react";
import type { JSX } from "react";
import { useTranslation } from "react-i18next";

import { competitionDisplayName } from "../../lib/competitionName";
import { findNextFixture } from "../../lib/fixtures";
import { formatMatchDate } from "../../lib/helpers";
import {
  clubStanding,
  isTableProvisional,
  leaguePhasePlaces,
  stageKey,
} from "../../lib/leaguePhase";
import type { LeagueData, LeaguePhaseData } from "../../store/types";
import { Badge, Card, CardBody, CardHeader } from "../ui";
import LeaguePhaseBracket from "./LeaguePhaseBracket";
import LeaguePhaseTable from "./LeaguePhaseTable";
import type { TournamentsTeamLookup } from "./teamLookup";

interface LeaguePhaseViewProps {
  competition: LeagueData;
  state: LeaguePhaseData;
  teams: TournamentsTeamLookup;
}

/** What the competition is doing now, who won it, and what the managed club plays next. */
export function LeaguePhaseStatus({
  competition,
  state,
  teams,
}: LeaguePhaseViewProps): JSX.Element {
  const { t } = useTranslation();
  const next = teams.userTeamId
    ? findNextFixture(competition.fixtures, teams.userTeamId)
    : undefined;
  const stage = t(`tournaments.leaguePhase.stages.${stageKey(state.stage)}`);
  const standing =
    teams.userTeamId && state.stage !== "Created"
      ? clubStanding(competition, state, teams.userTeamId)
      : null;
  return (
    <div className="flex flex-col gap-2" data-testid="leaguephase-status">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={state.stage === "Completed" ? "accent" : "primary"} size="sm">
          {stage}
        </Badge>
        {state.champion_id && (
          <span
            className="flex items-center gap-1.5 text-sm font-heading font-bold text-gray-800 dark:text-gray-100"
            data-testid="leaguephase-champion"
          >
            <Trophy className="w-4 h-4 text-accent-400" aria-hidden="true" />
            {t("tournaments.leaguePhase.champion", {
              team: teams.resolveTeamName(state.champion_id),
            })}
          </span>
        )}
      </div>
      {teams.userTeamId &&
        competition.participant_ids?.includes(teams.userTeamId) &&
        state.stage !== "Completed" && (
          <p className="text-sm text-gray-600 dark:text-gray-300" data-testid="leaguephase-next">
            {next
              ? t("tournaments.leaguePhase.nextMatch", {
                  home: teams.resolveTeamName(next.home_team_id),
                  away: teams.resolveTeamName(next.away_team_id),
                  date: formatMatchDate(next.date),
                })
              : t("tournaments.leaguePhase.noNextMatch")}
          </p>
        )}
      {standing && state.stage !== "Completed" && (
        <p className="text-sm text-gray-600 dark:text-gray-300" data-testid="leaguephase-standing">
          {t("tournaments.leaguePhase.yourPosition", {
            position: standing.position,
            total: standing.total,
            points: standing.points,
          })}
          {standing.cutoff && state.outcome == null && (
            <>
              {" "}
              {t("tournaments.leaguePhase.cutoffPoints", {
                position: standing.cutoff.position,
                points: standing.cutoff.points,
              })}
            </>
          )}
        </p>
      )}
    </div>
  );
}

/** Why there is no table yet: the draw was refused (and why), or has not been made. */
export function LeaguePhaseDrawNotice({ state }: { state: LeaguePhaseData }): JSX.Element {
  const { t } = useTranslation();
  return (
    <p
      role={state.draw_error ? "alert" : undefined}
      className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300"
    >
      {state.draw_error && (
        <AlertTriangle className="w-4 h-4 text-red-500 shrink-0" aria-hidden="true" />
      )}
      {state.draw_error
        ? t("tournaments.leaguePhase.drawFailed", { reason: t(state.draw_error) })
        : t("tournaments.leaguePhase.drawNotMade")}
    </p>
  );
}

/**
 * A league-phase competition's standings screen: the 36-club table with its three zones, then the
 * two-legged knockout phase. A competition whose draw could not be made says so and why, rather
 * than showing an empty table.
 */
export default function LeaguePhaseView({
  competition,
  state,
  teams,
}: LeaguePhaseViewProps): JSX.Element {
  const { t } = useTranslation();
  const places = leaguePhasePlaces(competition);
  const drawMade = state.stage !== "Created";

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          {competitionDisplayName(competition, t)} —{" "}
          {t("schedule.season", { number: competition.season })}
        </CardHeader>
        <CardBody>
          <LeaguePhaseStatus competition={competition} state={state} teams={teams} />
        </CardBody>
      </Card>

      {!drawMade ? (
        <Card>
          <CardBody>
            <LeaguePhaseDrawNotice state={state} />
          </CardBody>
        </Card>
      ) : (
        <Card>
          <CardHeader>{t("tournaments.leaguePhase.title")}</CardHeader>
          <div className="overflow-x-auto">
            <LeaguePhaseTable
              competition={competition}
              rows={state.table}
              teams={teams}
              variant="full"
              testIdPrefix="leaguephase-standing"
            />
          </div>
          <div className="flex flex-wrap gap-x-5 gap-y-1 border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400">
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-primary-500" aria-hidden="true" />
              {t("tournaments.leaguePhase.legend.direct", { to: places.direct })}
            </span>
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-amber-500" aria-hidden="true" />
              {t("tournaments.leaguePhase.legend.playoff", {
                from: places.direct + 1,
                to: places.direct + places.playoff,
              })}
            </span>
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-red-500" aria-hidden="true" />
              {t("tournaments.leaguePhase.legend.eliminated", {
                from: places.direct + places.playoff + 1,
              })}
            </span>
          </div>
          {isTableProvisional(state) && (
            <p className="border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400">
              {t("tournaments.leaguePhase.provisional")}
            </p>
          )}
          <ul
            className="border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400 flex flex-col gap-1"
            data-testid="leaguephase-notes"
          >
            <li>{t("tournaments.leaguePhase.drawByGame")}</li>
            <li>
              {Object.keys(state.coefficients ?? {}).length > 0
                ? t("tournaments.leaguePhase.seededByCoefficient")
                : t("tournaments.leaguePhase.seededByReputation")}
            </li>
            {(state.pot_adjustments ?? []).map((adjustment) => (
              <li key={`${adjustment.moved_club_id}-${adjustment.exchanged_club_id}`}>
                {t("tournaments.leaguePhase.potAdjustment", {
                  moved: teams.resolveTeamName(adjustment.moved_club_id),
                  exchanged: teams.resolveTeamName(adjustment.exchanged_club_id),
                })}
              </li>
            ))}
            <li>{t("tournaments.leaguePhase.prizeNote")}</li>
          </ul>
          {state.outcome?.coefficient_fallback_used && (
            <p className="border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400">
              {t("tournaments.leaguePhase.coefficientFallback")}
            </p>
          )}
        </Card>
      )}

      {drawMade && (
        <div className="flex flex-col gap-2">
          <h3 className="font-heading font-bold uppercase tracking-wider text-sm text-gray-600 dark:text-gray-300">
            {t("tournaments.leaguePhase.bracketTitle")}
          </h3>
          <LeaguePhaseBracket state={state} fixtures={competition.fixtures} teams={teams} />
        </div>
      )}
    </div>
  );
}
