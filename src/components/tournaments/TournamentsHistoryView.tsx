import { useTranslation } from "react-i18next";
import type { EditionSummaryData, LeagueData } from "../../store/types";
import { Card, CardBody, CardHeader } from "../ui";
import type { TournamentsTeamLookup } from "./teamLookup";

interface TournamentsHistoryViewProps {
  competition: LeagueData;
  history: EditionSummaryData[];
  teams: TournamentsTeamLookup;
}

/** "2026/27" for the season that starts in 2026. */
export function seasonSpan(season: number): string {
  return `${season}/${String((season + 1) % 100).padStart(2, "0")}`;
}

function TeamList({ ids, teams }: { ids: string[]; teams: TournamentsTeamLookup }) {
  return <>{ids.map((id) => teams.resolveTeamName(id)).join(", ")}</>;
}

/**
 * The finished editions of the competition on show: the winner, the beaten finalist and the score
 * of a cup final, and, for a division, who went up and who went down (and through which play-off).
 */
export default function TournamentsHistoryView({
  competition,
  history,
  teams,
}: TournamentsHistoryViewProps) {
  const { t } = useTranslation();
  const editions = history.filter((edition) => edition.competition_id === competition.id);

  if (editions.length === 0) {
    return (
      <Card>
        <CardBody>
          <p
            className="py-6 text-center text-sm text-gray-500 dark:text-gray-400"
            data-testid="tournaments-history-empty"
          >
            {t("tournaments.history.empty")}
          </p>
        </CardBody>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-4" data-testid="tournaments-history">
      {editions.map((edition) => {
        const movements = edition.movements;
        const isUpper = movements?.upper_competition_id === competition.id;
        const final = edition.final_result;
        return (
          <Card key={`${edition.competition_id}-${edition.season}`}>
            <CardHeader>{t("tournaments.history.season", { season: seasonSpan(edition.season) })}</CardHeader>
            <CardBody>
              <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
                <dt className="text-gray-500 dark:text-gray-400">
                  {t("tournaments.history.champion")}
                </dt>
                <dd
                  className="font-heading font-bold text-gray-800 dark:text-gray-100"
                  data-testid={`tournaments-history-champion-${edition.season}`}
                >
                  {teams.resolveTeamName(edition.champion_id)}
                </dd>
                {edition.runner_up_id && (
                  <>
                    <dt className="text-gray-500 dark:text-gray-400">
                      {t("tournaments.history.runnerUp")}
                    </dt>
                    <dd>{teams.resolveTeamName(edition.runner_up_id)}</dd>
                  </>
                )}
                {final && (
                  <>
                    <dt className="text-gray-500 dark:text-gray-400">
                      {t("tournaments.history.final")}
                    </dt>
                    <dd data-testid={`tournaments-history-final-${edition.season}`}>
                      {teams.resolveTeamName(final.home_team_id)} {final.home_goals}–
                      {final.away_goals} {teams.resolveTeamName(final.away_team_id)}
                      {final.home_penalties != null && final.away_penalties != null && (
                        <span className="ml-1 text-gray-500 dark:text-gray-400">
                          (
                          {t("tournaments.history.penalties", {
                            home: final.home_penalties,
                            away: final.away_penalties,
                          })}
                          )
                        </span>
                      )}
                    </dd>
                  </>
                )}
                {movements && (
                  <>
                    <dt className="text-gray-500 dark:text-gray-400">
                      {isUpper
                        ? t("tournaments.history.relegated")
                        : t("tournaments.history.promoted")}
                    </dt>
                    <dd data-testid={`tournaments-history-moves-${edition.season}`}>
                      <TeamList
                        ids={
                          isUpper
                            ? [...movements.relegated_direct, ...movements.relegated_barrage]
                            : [...movements.promoted_direct, ...movements.promoted_playoff]
                        }
                        teams={teams}
                      />
                    </dd>
                    {!isUpper && movements.ties.length > 0 && (
                      <>
                        <dt className="text-gray-500 dark:text-gray-400">
                          {t("tournaments.history.playoffs")}
                        </dt>
                        <dd className="flex flex-col gap-0.5">
                          {movements.ties.map((tie) => (
                            <span key={`${tie.stage}-${tie.home_team_id}-${tie.away_team_id}`}>
                              {tie.stage}: {teams.resolveTeamName(tie.home_team_id)}{" "}
                              {tie.legs.map(([home, away]) => `${home}–${away}`).join(", ")}{" "}
                              {teams.resolveTeamName(tie.away_team_id)}
                              {tie.penalties &&
                                ` (${t("tournaments.history.penalties", {
                                  home: tie.penalties[0],
                                  away: tie.penalties[1],
                                })})`}
                            </span>
                          ))}
                        </dd>
                      </>
                    )}
                    {movements.reserves_blocked.length > 0 && !isUpper && (
                      <>
                        <dt className="text-gray-500 dark:text-gray-400">
                          {t("tournaments.history.reserveBlocked")}
                        </dt>
                        <dd>
                          <TeamList ids={movements.reserves_blocked} teams={teams} />
                        </dd>
                      </>
                    )}
                    {movements.reserves_forced_down.length > 0 && (
                      <>
                        <dt className="text-gray-500 dark:text-gray-400">
                          {t("tournaments.history.reserveForcedDown")}
                        </dt>
                        <dd>
                          <TeamList ids={movements.reserves_forced_down} teams={teams} />
                        </dd>
                      </>
                    )}
                    {movements.reserves_readmitted.length > 0 && (
                      <>
                        <dt className="text-gray-500 dark:text-gray-400">
                          {t("tournaments.history.reserveReadmitted")}
                        </dt>
                        <dd>
                          <TeamList ids={movements.reserves_readmitted} teams={teams} />
                        </dd>
                      </>
                    )}
                  </>
                )}
              </dl>
            </CardBody>
          </Card>
        );
      })}
    </div>
  );
}
