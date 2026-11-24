import type { JSX } from "react";
import { useTranslation } from "react-i18next";

import type { LeagueData, LeaguePhaseData } from "../../store/types";
import { Card, CardBody, CardHeader } from "../ui";
import LeaguePhaseTable from "./LeaguePhaseTable";
import { LeaguePhaseDrawNotice, LeaguePhaseStatus } from "./LeaguePhaseView";
import { leaguePhasePlaces } from "../../lib/leaguePhase";
import TournamentsTopScorers from "./TournamentsTopScorers";
import type { TopScorerEntry } from "./TournamentsTab.helpers";
import type { TournamentsTeamLookup } from "./teamLookup";

interface LeaguePhaseOverviewProps {
  competition: LeagueData;
  state: LeaguePhaseData;
  topScorers: TopScorerEntry[];
  teams: TournamentsTeamLookup;
  onSelectPlayer?: (id: string) => void;
}

/** The competition at a glance: its stage, the managed club's next match, the table, the scorers. */
export default function LeaguePhaseOverview({
  competition,
  state,
  topScorers,
  teams,
  onSelectPlayer,
}: LeaguePhaseOverviewProps): JSX.Element {
  const { t } = useTranslation();
  const places = leaguePhasePlaces(competition);
  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-5">
      <Card className="lg:col-span-2">
        <CardHeader>{t("tournaments.leaguePhase.title")}</CardHeader>
        <CardBody>
          <LeaguePhaseStatus competition={competition} state={state} teams={teams} />
        </CardBody>
        {state.stage === "Created" ? (
          <CardBody>
            <LeaguePhaseDrawNotice state={state} />
          </CardBody>
        ) : (
          <>
            <div className="overflow-x-auto border-t border-gray-100 dark:border-navy-600">
              <LeaguePhaseTable
                competition={competition}
                rows={state.table}
                teams={teams}
                variant="compact"
                testIdPrefix="leaguephase-overview-standing"
              />
            </div>
            <p className="border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400">
              {t("tournaments.leaguePhase.legend.compact", {
                direct: places.direct,
                playoffFrom: places.direct + 1,
                playoffTo: places.direct + places.playoff,
                outFrom: places.direct + places.playoff + 1,
              })}
            </p>
          </>
        )}
      </Card>
      <TournamentsTopScorers
        topScorers={topScorers}
        onSelectTeam={teams.onSelectTeam}
        onSelectPlayer={onSelectPlayer}
      />
    </div>
  );
}
