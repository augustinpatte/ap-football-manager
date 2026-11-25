// The international and standings views of the Schedule tab, and the placeholder shared with them.
import { orderStandings } from "../../lib/standings";
import { zoneOfRow } from "../../lib/pyramid";
import type { useTranslation } from "react-i18next";
import { Globe, Trophy } from "lucide-react";

import {
  getActiveCompetitions,
  type getNationalTeamFixtures,
  getNationalTeamName,
  getPromotionRelegationZones,
  getTeamName,
  formatMatchDate,
  type getUserCalledUpPlayers,
  nationalTeamDisplayName,
} from "../../lib/helpers";
import { competitionDisplayName } from "../../lib/competitionName";
import type { GameStateData, LeagueData } from "../../store/gameStore";
import ContextMenu, { type ContextMenuItem } from "../ContextMenu";
import { Badge, Card, CardBody } from "../ui";

export function InternationalView({
  gameState,
  calledUpPlayers,
  nationalFixtures,
  t,
}: {
  gameState: GameStateData;
  calledUpPlayers: ReturnType<typeof getUserCalledUpPlayers>;
  nationalFixtures: ReturnType<typeof getNationalTeamFixtures>;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const internationalByDate = new Map<string, typeof nationalFixtures>();
  [...nationalFixtures]
    .sort((a, b) => a.date.localeCompare(b.date) || a.id.localeCompare(b.id))
    .forEach((fixture) => {
      const list = internationalByDate.get(fixture.date) ?? [];
      list.push(fixture);
      internationalByDate.set(fixture.date, list);
    });

  return (
    <div className="flex flex-col gap-4">
      {calledUpPlayers.length > 0 && (
        <Card accent="accent">
          <div className="rounded-t-xl border-b border-gray-100 bg-gray-50 px-5 py-3 dark:border-navy-600 dark:bg-navy-800">
            <h4 className="flex items-center gap-2 font-heading text-sm font-bold uppercase tracking-wider text-gray-600 dark:text-gray-300">
              <Globe className="h-4 w-4 text-accent-500" />
              {t("schedule.internationalDuty")}
            </h4>
          </div>
          <CardBody className="p-0">
            <div className="divide-y divide-gray-100 dark:divide-navy-600">
              {calledUpPlayers.map(
                ({ player, nationalTeamId, nationalTeamName, nationalTeamNameKey }) => (
                  <div
                    key={`${player.id}-${nationalTeamId}`}
                    className="flex items-center justify-between px-5 py-2.5"
                    data-testid={`schedule-callup-${player.id}`}
                  >
                    <span className="text-sm font-semibold text-gray-800 dark:text-gray-200">
                      {player.match_name}
                    </span>
                    <Badge variant="neutral" size="sm">
                      {nationalTeamDisplayName(nationalTeamNameKey, nationalTeamName, t)}
                    </Badge>
                  </div>
                ),
              )}
            </div>
          </CardBody>
        </Card>
      )}

      {Array.from(internationalByDate.entries()).map(([date, fixtures]) => (
        <Card key={date}>
          <div className="rounded-t-xl border-b border-gray-100 bg-gray-50 px-5 py-3 dark:border-navy-600 dark:bg-navy-800">
            <h4 className="font-heading text-sm font-bold uppercase tracking-wider text-gray-600 dark:text-gray-300">
              {t("schedule.international")} - {formatMatchDate(date)}
            </h4>
          </div>
          <CardBody className="p-0">
            <div className="divide-y divide-gray-100 dark:divide-navy-600">
              {fixtures.map((fixture) => {
                const completed = fixture.status === "Completed";
                return (
                  <div
                    key={fixture.id}
                    className="flex items-center px-5 py-3"
                    data-testid={`schedule-international-${fixture.id}`}
                  >
                    <span className="flex-1 text-right text-sm font-semibold text-gray-800 dark:text-gray-200">
                      {getNationalTeamName(gameState, fixture.home_team_id, t)}
                    </span>
                    <div className="mx-3 w-24 text-center">
                      {completed && fixture.result ? (
                        <span className="font-heading text-lg font-bold text-gray-800 dark:text-gray-100">
                          {fixture.result.home_goals} - {fixture.result.away_goals}
                        </span>
                      ) : (
                        <Badge variant="neutral" size="sm">
                          vs
                        </Badge>
                      )}
                    </div>
                    <span className="flex-1 text-left text-sm font-semibold text-gray-800 dark:text-gray-200">
                      {getNationalTeamName(gameState, fixture.away_team_id, t)}
                    </span>
                  </div>
                );
              })}
            </div>
          </CardBody>
        </Card>
      ))}
    </div>
  );
}

export function StandingsView({
  competition,
  userTeamId,
  isPreseason,
  seasonContext,
  onSelectTeam,
  buildTeamMenuItem,
  gameState,
  t,
}: {
  competition: LeagueData | null;
  userTeamId: string | null;
  isPreseason: boolean;
  seasonContext: GameStateData["season_context"];
  onSelectTeam: (id: string) => void;
  buildTeamMenuItem: (label: string, teamId: string) => ContextMenuItem;
  gameState: GameStateData;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const standings = orderStandings(competition);
  const zones = competition
    ? getPromotionRelegationZones(getActiveCompetitions(gameState), competition)
    : { promotionSlots: 0, relegationSlots: 0 };

  if (isPreseason) {
    return (
      <Card>
        <CardBody>
          <div className="flex flex-col items-center gap-2 py-6 text-center">
            <Trophy className="h-8 w-8 text-gray-300 dark:text-navy-600" />
            <p className="text-sm font-heading font-bold text-gray-800 dark:text-gray-100">
              {t("season.standingsLocked")}
            </p>
            <p className="text-xs text-gray-500 dark:text-gray-400">
              {seasonContext.season_start
                ? t("season.startsOn", { date: formatMatchDate(seasonContext.season_start) })
                : t("season.noOpener")}
            </p>
          </div>
        </CardBody>
      </Card>
    );
  }

  return (
    <Card>
      <div className="rounded-t-xl border-b border-gray-100 bg-gradient-to-r from-navy-700 to-navy-800 p-5 dark:border-navy-600">
        <h3 className="flex items-center gap-2 font-heading text-lg font-bold uppercase tracking-wide text-white">
          <Trophy className="h-5 w-5 text-accent-400" />
          {(competition && competitionDisplayName(competition, t)) || t("schedule.fixtures")} –{" "}
          {t("schedule.season", { number: competition?.season ?? 0 })}
        </h3>
      </div>
      {standings.length === 0 ? (
        <CardBody>
          <p className="py-6 text-center text-sm text-gray-500 dark:text-gray-400">
            {t("schedule.standingsUnavailable")}
          </p>
        </CardBody>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-gray-200 bg-gray-50 text-xs dark:border-navy-600 dark:bg-navy-800">
                {[
                  "#",
                  t("common.team"),
                  t("common.played"),
                  t("common.won"),
                  t("common.drawn"),
                  t("common.lost"),
                  t("common.gf"),
                  t("common.ga"),
                  t("common.gd"),
                  t("common.pts"),
                ].map((header, idx) => (
                  <th
                    key={idx}
                    className={`px-4 py-3 font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 ${idx === 0 ? "w-8" : ""} ${idx >= 2 ? "text-center" : ""}`}
                  >
                    {header}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 dark:divide-navy-600">
              {standings.map((entry, index) => {
                const isUser = entry.team_id === userTeamId;
                const gd = entry.goals_for - entry.goals_against;
                const zone = zoneOfRow(zones, index, standings.length);
                const inPromotionZone = zone === "promotion";
                const inRelegationZone = zone === "relegation";
                const inPlayoffZone = zone === "playoff" || zone === "barrage";
                const contextItems = [buildTeamMenuItem(t("common.viewTeam"), entry.team_id)];

                return (
                  <ContextMenu items={contextItems} key={entry.team_id}>
                    <tr
                      className={`transition-colors ${
                        isUser
                          ? "bg-primary-50 dark:bg-primary-500/10"
                          : "hover:bg-gray-50 dark:hover:bg-navy-700/50"
                      }`}
                      data-testid={`schedule-standings-row-${entry.team_id}`}
                    >
                      <td
                        className={`px-4 py-3 font-heading text-sm font-bold ${
                          inPromotionZone
                            ? "border-l-2 border-primary-500 text-primary-500"
                            : inRelegationZone
                              ? "border-l-2 border-red-500 text-red-500"
                              : inPlayoffZone
                                ? "border-l-2 border-amber-500 text-amber-500"
                                : "text-gray-400 dark:text-gray-500"
                        }`}
                        data-testid={
                          inPromotionZone
                            ? `standings-promotion-${entry.team_id}`
                            : inRelegationZone
                              ? `standings-relegation-${entry.team_id}`
                              : inPlayoffZone
                                ? `standings-playoff-${entry.team_id}`
                                : undefined
                        }
                      >
                        {index + 1}
                      </td>
                      <td className="px-4 py-3 text-sm font-semibold">
                        <button
                          type="button"
                          onClick={() => onSelectTeam(entry.team_id)}
                          className={`w-full bg-transparent p-0 text-left hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus ${
                            isUser
                              ? "text-primary-600 dark:text-primary-400"
                              : "text-gray-800 dark:text-gray-200"
                          }`}
                        >
                          {getTeamName(gameState.teams, entry.team_id)}
                        </button>
                      </td>
                      {[
                        entry.played,
                        entry.won,
                        entry.drawn,
                        entry.lost,
                        entry.goals_for,
                        entry.goals_against,
                      ].map((val, i) => (
                        <td
                          key={i}
                          className="px-4 py-3 text-center text-sm tabular-nums text-gray-600 dark:text-gray-400"
                        >
                          {val}
                        </td>
                      ))}
                      <td
                        className={`px-4 py-3 text-center text-sm font-semibold tabular-nums ${
                          gd > 0
                            ? "text-primary-500"
                            : gd < 0
                              ? "text-red-500"
                              : "text-gray-500 dark:text-gray-400"
                        }`}
                      >
                        {gd > 0 ? `+${gd}` : gd}
                      </td>
                      <td className="px-4 py-3 text-center font-heading text-sm font-bold tabular-nums text-gray-800 dark:text-gray-100">
                        {entry.points}
                      </td>
                    </tr>
                  </ContextMenu>
                );
              })}
            </tbody>
          </table>
          {(zones.promotionSlots > 0 ||
            zones.relegationSlots > 0 ||
            zones.promotionPlayoff ||
            zones.barrageRank != null) && (
            <div className="flex gap-5 border-t border-gray-100 px-4 py-2.5 text-xs text-gray-500 dark:border-navy-600 dark:text-gray-400">
              {zones.promotionSlots > 0 && (
                <span className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-primary-500" />
                  {t("schedule.promotionZone")}
                </span>
              )}
              {(zones.promotionPlayoff || zones.barrageRank != null) && (
                <span className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-amber-500" />
                  {t("schedule.playoffZone")}
                </span>
              )}
              {zones.relegationSlots > 0 && (
                <span className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-red-500" />
                  {t("schedule.relegationZone")}
                </span>
              )}
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

export function LoadingPlaceholder() {
  return (
    <div className="flex flex-col gap-4">
      {[1, 2, 3].map((n) => (
        <div key={n} className="h-32 rounded-xl bg-gray-100 dark:bg-navy-800 animate-pulse" />
      ))}
    </div>
  );
}
