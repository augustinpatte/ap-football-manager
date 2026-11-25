import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Calendar as CalendarIcon, Globe, TableProperties, Trophy } from "lucide-react";

import {
  getActiveCompetitions,
  getNationalTeamFixtures,
  formatMatchDate,
  getFixtureDisplayLabel,
  getUserCalledUpPlayers,
} from "../../lib/helpers";
import { competitionDisplayName } from "../../lib/competitionName";
import type { GameStateData } from "../../store/gameStore";
import {
  fetchSchedule,
  fetchUserSchedule,
  type MatchdayGroup,
  type ScheduleSlice,
  type UserSchedule,
} from "../../services/scheduleService";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../utils/errorMessage";
import ContextMenu, { type ContextMenuItem } from "../ContextMenu";
import { Badge, Card, CardBody, Select } from "../ui";
import ScheduleCalendarGrid from "./ScheduleCalendarGrid";
import MyMatchesView from "./MyMatchesView";
import MovedFromNote from "./MovedFromNote";
import M12ReplayViewer from "../match/M12ReplayViewer";
import { loadMatchReplay } from "../../services/matchReplayService";

interface ScheduleTabProps {
  gameState: GameStateData;
  onSelectTeam: (id: string) => void;
}

const PAST_PAGE_SIZE = 4;

type View = "myMatches" | "calendar" | "fixtures" | "standings" | "international";

export default function ScheduleTab({ gameState, onSelectTeam }: ScheduleTabProps) {
  const { t } = useTranslation();
  const [view, setView] = useState<View>("myMatches");
  const [selectedCompetitionId, setSelectedCompetitionId] = useState<string | null>(null);
  const [slice, setSlice] = useState<ScheduleSlice | null>(null);
  const [userSchedule, setUserSchedule] = useState<UserSchedule | null>(null);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [visiblePastCount, setVisiblePastCount] = useState(PAST_PAGE_SIZE);
  const [replayJson, setReplayJson] = useState<string | null>(null);
  const [replayError, setReplayError] = useState<string | null>(null);
  const [replayLoadingFixture, setReplayLoadingFixture] = useState<string | null>(null);
  const replayRequestInFlight = useRef(false);
  const groupRefs = useRef<Map<string, HTMLDivElement>>(new Map());

  const userTeamId = gameState.manager.team_id;
  const activeCompetitions = getActiveCompetitions(gameState);
  const userCompetitions = activeCompetitions.filter((c) =>
    c.participant_ids?.includes(userTeamId ?? ""),
  );
  const nationalFixtures = getNationalTeamFixtures(gameState);
  const hasInternational = nationalFixtures.length > 0;
  const calledUpPlayers = getUserCalledUpPlayers(gameState);
  const seasonContext = gameState.season_context;
  const isPreseason = seasonContext.phase === "Preseason";
  const openReplay = useCallback(
    async (competitionId: string, fixtureId: string) => {
      if (replayRequestInFlight.current) return;
      replayRequestInFlight.current = true;
      setReplayLoadingFixture(fixtureId);
      setReplayError(null);
      try {
        const replay = await loadMatchReplay(competitionId, fixtureId);
        if (!replay) {
          // Nothing stored is a normal outcome (live, delegated and pre-M12 matches keep no
          // spatial data); it must not read like a corrupted file.
          setReplayError(t("match.replayNotRecorded"));
          return;
        }
        setReplayJson(replay.replay_json);
      } catch {
        setReplayError(t("match.replayUnavailable"));
      } finally {
        replayRequestInFlight.current = false;
        setReplayLoadingFixture(null);
      }
    },
    [t],
  );

  // Resolve which competition to show.
  const selectedCompetition =
    activeCompetitions.find((c) => c.id === selectedCompetitionId) ??
    userCompetitions.find((c) => c.standings.length > 0) ??
    activeCompetitions.find((c) => c.standings.length > 0) ??
    userCompetitions[0] ??
    activeCompetitions[0] ??
    null;
  const sliceCompetition = activeCompetitions.find((c) => c.id === slice?.competition_id);
  const sliceCompetitionName = sliceCompetition
    ? competitionDisplayName(sliceCompetition, t)
    : (slice?.competition_name ?? "");

  // Initialise the competition selection.
  useEffect(() => {
    if (activeCompetitions.length === 0) {
      if (selectedCompetitionId !== null) setSelectedCompetitionId(null);
      return;
    }
    const hasSelection = activeCompetitions.some((c) => c.id === selectedCompetitionId);
    if (hasSelection) return;
    const preferred =
      userCompetitions.find((c) => c.standings.length > 0)?.id ??
      activeCompetitions.find((c) => c.standings.length > 0)?.id ??
      userCompetitions[0]?.id ??
      activeCompetitions[0].id;
    setSelectedCompetitionId(preferred);
  }, [activeCompetitions, selectedCompetitionId, userCompetitions]);

  // Fetch schedule slice whenever the competition changes.
  useEffect(() => {
    if (!selectedCompetition) {
      setSlice(null);
      return;
    }
    let cancelled = false;
    fetchSchedule({ competition_id: selectedCompetition.id })
      .then((result) => {
        if (cancelled) return;
        setSlice(result);
        setFetchError(null);
      })
      .catch((err) => {
        if (cancelled) return;
        setFetchError(resolveTranslatedErrorMessage(getErrorMessage(err), t));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedCompetition?.id, t]);

  // The manager's own matches across every competition: read again when the day or the
  // competitions change, from the competitions' own fixtures.
  const today = gameState.clock?.current_date;
  const hasCompetitions = activeCompetitions.length > 0;
  useEffect(() => {
    if (!hasCompetitions) {
      setUserSchedule(null);
      return;
    }
    let cancelled = false;
    fetchUserSchedule()
      .then((result) => {
        if (!cancelled) setUserSchedule(result ?? null);
      })
      .catch((err) => {
        if (!cancelled) setFetchError(resolveTranslatedErrorMessage(getErrorMessage(err), t));
      });
    return () => {
      cancelled = true;
    };
  }, [today, hasCompetitions, gameState.competitions, t]);

  // Reset past-group paging when competition changes.
  useEffect(() => {
    setVisiblePastCount(PAST_PAGE_SIZE);
  }, [selectedCompetition?.id]);

  const scrollToDate = useCallback((date: string) => {
    const el = groupRefs.current.get(date);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }, []);

  // Auto-scroll to the next user fixture on load when mid-season (past groups present).
  useEffect(() => {
    if (!slice?.next_user_match_date || slice.past_groups.length === 0) return;
    const date = slice.next_user_match_date;
    const id = setTimeout(() => scrollToDate(date), 50);
    return () => clearTimeout(id);
  }, [slice?.competition_id, slice?.next_user_match_date, slice?.past_groups.length, scrollToDate]);

  if (replayJson) {
    return (
      <M12ReplayViewer
        replayJson={replayJson}
        readOnly
        onFinish={() => setReplayJson(null)}
        onBack={() => setReplayJson(null)}
      />
    );
  }

  const buildTeamMenuItem = (label: string, teamId: string): ContextMenuItem => ({
    label,
    onClick: () => onSelectTeam(teamId),
  });

  if (activeCompetitions.length === 0) {
    return (
      <p className="py-8 text-center text-gray-500 dark:text-gray-400">{t("schedule.noLeague")}</p>
    );
  }

  const competitionSwitcher = activeCompetitions.length > 1 && (
    <Select
      value={selectedCompetition?.id ?? ""}
      onChange={(e) => setSelectedCompetitionId(e.target.value)}
      wrapperClassName="ml-auto"
      aria-label={t("common.competition")}
    >
      {activeCompetitions.map((c) => (
        <option key={c.id} value={c.id}>
          {competitionDisplayName(c, t)}
        </option>
      ))}
    </Select>
  );

  return (
    <div>
      {isPreseason && (
        <Card accent="accent" className="mb-5">
          <CardBody>
            <div className="flex flex-col gap-1.5">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="accent" size="sm">
                  {t(`season.phases.${seasonContext.phase}`)}
                </Badge>
                <span className="text-sm font-heading font-bold text-gray-800 dark:text-gray-100">
                  {seasonContext.season_start
                    ? t("season.startsOn", { date: formatMatchDate(seasonContext.season_start) })
                    : t("season.noOpener")}
                </span>
              </div>
              <p className="text-xs text-gray-500 dark:text-gray-400">
                {t("season.standingsLocked")}
              </p>
            </div>
          </CardBody>
        </Card>
      )}

      <div className="mb-5 flex flex-wrap gap-2">
        <ViewButton active={view === "myMatches"} onClick={() => setView("myMatches")}>
          <CalendarIcon className="mr-1.5 inline h-4 w-4 -mt-0.5" />
          {t("schedule.myMatches.tab")}
        </ViewButton>
        <ViewButton active={view === "calendar"} onClick={() => setView("calendar")}>
          <CalendarIcon className="mr-1.5 inline h-4 w-4 -mt-0.5" />
          {t("schedule.calendar.title", "Calendar")}
        </ViewButton>
        <ViewButton active={view === "fixtures"} onClick={() => setView("fixtures")}>
          <TableProperties className="mr-1.5 inline h-4 w-4 -mt-0.5" />
          {t("schedule.fixtures")}
        </ViewButton>
        <ViewButton active={view === "standings"} onClick={() => setView("standings")}>
          <Trophy className="mr-1.5 inline h-4 w-4 -mt-0.5" />
          {t("schedule.standings")}
        </ViewButton>
        {hasInternational && (
          <ViewButton active={view === "international"} onClick={() => setView("international")}>
            <Globe className="mr-1.5 inline h-4 w-4 -mt-0.5" />
            {t("schedule.international")}
          </ViewButton>
        )}
        {view !== "myMatches" && competitionSwitcher}
      </div>

      {fetchError && (
        <p role="alert" className="mb-4 text-sm text-red-500">
          {fetchError}
        </p>
      )}
      {replayError && (
        <p role="alert" className="mb-4 text-body text-danger">
          {replayError}
        </p>
      )}

      {view === "myMatches" && (
        <MyMatchesView
          schedule={userSchedule}
          gameState={gameState}
          onOpenReplay={openReplay}
          replayLoadingFixture={replayLoadingFixture}
          t={t}
        />
      )}

      {view === "calendar" && (
        <CalendarView
          slice={slice}
          competitionName={sliceCompetitionName}
          userTeamId={userTeamId ?? null}
          groupRefs={groupRefs}
          visiblePastCount={visiblePastCount}
          onShowMorePast={() => setVisiblePastCount((n) => n + PAST_PAGE_SIZE)}
          onScrollToDate={scrollToDate}
          onSelectTeam={onSelectTeam}
          buildTeamMenuItem={buildTeamMenuItem}
          competitionId={selectedCompetition?.id ?? null}
          onOpenReplay={openReplay}
          replayLoadingFixture={replayLoadingFixture}
          t={t}
        />
      )}

      {view === "fixtures" && (
        <FixturesListView
          slice={slice}
          competitionName={sliceCompetitionName}
          userTeamId={userTeamId ?? null}
          groupRefs={groupRefs}
          onSelectTeam={onSelectTeam}
          buildTeamMenuItem={buildTeamMenuItem}
          competitionId={selectedCompetition?.id ?? null}
          onOpenReplay={openReplay}
          replayLoadingFixture={replayLoadingFixture}
          t={t}
        />
      )}

      {view === "international" && (
        <InternationalView
          gameState={gameState}
          calledUpPlayers={calledUpPlayers}
          nationalFixtures={nationalFixtures}
          t={t}
        />
      )}

      {view === "standings" && (
        <StandingsView
          competition={selectedCompetition}
          userTeamId={userTeamId ?? null}
          isPreseason={isPreseason}
          seasonContext={seasonContext}
          onSelectTeam={onSelectTeam}
          buildTeamMenuItem={buildTeamMenuItem}
          gameState={gameState}
          t={t}
        />
      )}
    </div>
  );
}

// ─── Sub-views ──────────────────────────────────────────────────────────────

function ViewButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-lg px-4 py-2 font-heading text-sm font-bold uppercase tracking-wider transition-all ${
        active
          ? "bg-primary-500 text-white shadow-md shadow-primary-500/20"
          : "border border-gray-200 bg-white text-gray-500 hover:text-gray-700 dark:border-navy-600 dark:bg-navy-800 dark:text-gray-400 dark:hover:text-gray-200"
      }`}
    >
      {children}
    </button>
  );
}

function CalendarView({
  slice,
  competitionName,
  userTeamId,
  groupRefs,
  visiblePastCount,
  onShowMorePast,
  onScrollToDate,
  onSelectTeam,
  buildTeamMenuItem,
  competitionId,
  onOpenReplay,
  replayLoadingFixture,
  t,
}: {
  slice: ScheduleSlice | null;
  competitionName: string;
  userTeamId: string | null;
  groupRefs: React.MutableRefObject<Map<string, HTMLDivElement>>;
  visiblePastCount: number;
  onShowMorePast: () => void;
  onScrollToDate: (date: string) => void;
  onSelectTeam: (id: string) => void;
  buildTeamMenuItem: (label: string, teamId: string) => ContextMenuItem;
  competitionId: string | null;
  onOpenReplay: (competitionId: string, fixtureId: string) => Promise<void>;
  replayLoadingFixture: string | null;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  if (!slice) return <LoadingPlaceholder />;

  const allGroups = [...slice.upcoming_groups, ...slice.past_groups];
  const visiblePastGroups = slice.past_groups.slice(0, visiblePastCount);

  return (
    <div className="flex flex-col gap-5">
      <ScheduleCalendarGrid
        groups={allGroups}
        userTeamId={userTeamId}
        today={slice.today}
        focusDate={slice.next_user_match_date}
        onSelectDate={onScrollToDate}
      />

      {/* Upcoming groups */}
      {slice.upcoming_groups.length > 0 ? (
        <div className="flex flex-col gap-4">
          {slice.upcoming_groups.map((group) => (
            <MatchdayGroupCard
              key={group.key}
              group={group}
              userTeamId={userTeamId}
              groupRefs={groupRefs}
              competitionName={competitionName}
              onSelectTeam={onSelectTeam}
              buildTeamMenuItem={buildTeamMenuItem}
              competitionId={competitionId}
              onOpenReplay={onOpenReplay}
              replayLoadingFixture={replayLoadingFixture}
              t={t}
            />
          ))}
        </div>
      ) : (
        <p className="py-4 text-center text-sm text-gray-500 dark:text-gray-400">
          {t("schedule.noUpcoming", "No upcoming fixtures.")}
        </p>
      )}

      {/* Past groups */}
      {visiblePastGroups.length > 0 && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3 py-1">
            <div className="flex-1 h-px bg-gray-200 dark:bg-navy-600" />
            <span className="text-xs font-heading font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500">
              {t("schedule.pastResults", "Past results")}
            </span>
            <div className="flex-1 h-px bg-gray-200 dark:bg-navy-600" />
          </div>
          <div className="flex flex-col gap-4">
            {visiblePastGroups.map((group) => (
              <MatchdayGroupCard
                key={group.key}
                group={group}
                userTeamId={userTeamId}
                groupRefs={groupRefs}
                competitionName={competitionName}
                dimmed
                onSelectTeam={onSelectTeam}
                buildTeamMenuItem={buildTeamMenuItem}
                competitionId={competitionId}
                onOpenReplay={onOpenReplay}
                replayLoadingFixture={replayLoadingFixture}
                t={t}
              />
            ))}
          </div>
          {slice.past_groups.length > visiblePastCount && (
            <button
              type="button"
              onClick={onShowMorePast}
              className="mx-auto mt-1 rounded-lg border border-gray-200 bg-white px-4 py-2 font-heading text-sm font-bold uppercase tracking-wider text-gray-500 transition-all hover:text-gray-700 dark:border-navy-600 dark:bg-navy-800 dark:text-gray-400 dark:hover:text-gray-200"
            >
              {t("schedule.loadMore")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function FixturesListView({
  slice,
  competitionName,
  userTeamId,
  groupRefs,
  onSelectTeam,
  buildTeamMenuItem,
  competitionId,
  onOpenReplay,
  replayLoadingFixture,
  t,
}: {
  slice: ScheduleSlice | null;
  competitionName: string;
  userTeamId: string | null;
  groupRefs: React.MutableRefObject<Map<string, HTMLDivElement>>;
  onSelectTeam: (id: string) => void;
  buildTeamMenuItem: (label: string, teamId: string) => ContextMenuItem;
  competitionId: string | null;
  onOpenReplay: (competitionId: string, fixtureId: string) => Promise<void>;
  replayLoadingFixture: string | null;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  if (!slice) return <LoadingPlaceholder />;

  const allGroups = [...slice.upcoming_groups, ...slice.past_groups];

  if (allGroups.length === 0) {
    return (
      <p className="py-8 text-center text-gray-500 dark:text-gray-400">{t("schedule.noLeague")}</p>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {allGroups.map((group) => (
        <MatchdayGroupCard
          key={group.key}
          group={group}
          userTeamId={userTeamId}
          groupRefs={groupRefs}
          competitionName={competitionName}
          onSelectTeam={onSelectTeam}
          buildTeamMenuItem={buildTeamMenuItem}
          competitionId={competitionId}
          onOpenReplay={onOpenReplay}
          replayLoadingFixture={replayLoadingFixture}
          t={t}
        />
      ))}
    </div>
  );
}

function groupLabel(
  group: MatchdayGroup,
  competitionName: string,
  t: ReturnType<typeof useTranslation>["t"],
): string {
  const date = formatMatchDate(group.date);
  if (group.competition === "League" && group.matchday <= 0) {
    return `${competitionName} – ${date}`;
  }
  const label = getFixtureDisplayLabel(t, group, competitionName);
  // A friendly belongs to no competition, and a cup's label already is its name.
  if (group.competition === "Friendly" || label === competitionName) {
    return `${label} – ${date}`;
  }
  return `${competitionName} – ${label} – ${date}`;
}

function MatchdayGroupCard({
  group,
  userTeamId,
  groupRefs,
  competitionName,
  dimmed = false,
  onSelectTeam,
  buildTeamMenuItem,
  competitionId,
  onOpenReplay,
  replayLoadingFixture,
  t,
}: {
  group: MatchdayGroup;
  userTeamId: string | null;
  groupRefs: React.MutableRefObject<Map<string, HTMLDivElement>>;
  competitionName: string;
  dimmed?: boolean;
  onSelectTeam: (id: string) => void;
  buildTeamMenuItem: (label: string, teamId: string) => ContextMenuItem;
  competitionId: string | null;
  onOpenReplay: (competitionId: string, fixtureId: string) => Promise<void>;
  replayLoadingFixture: string | null;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const setRef = (el: HTMLDivElement | null) => {
    if (el) {
      groupRefs.current.set(group.date, el);
    } else {
      groupRefs.current.delete(group.date);
    }
  };

  return (
    <div ref={setRef} data-group-key={group.key}>
      <Card className={dimmed ? "opacity-70" : undefined}>
        <div className="rounded-t-xl border-b border-gray-100 bg-gray-50 px-5 py-3 dark:border-navy-600 dark:bg-navy-800">
          <div className="flex items-center gap-2">
            {group.is_next_user_match && (
              <Badge variant="primary" size="sm">
                {t("schedule.nextMatch", "Next match")}
              </Badge>
            )}
            <h4 className="font-heading text-sm font-bold uppercase tracking-wider text-gray-600 dark:text-gray-300">
              {groupLabel(group, competitionName, t)}
            </h4>
          </div>
        </div>
        <CardBody className="p-0">
          <div className="divide-y divide-gray-100 dark:divide-navy-600">
            {group.fixtures.map((fixture) => {
              const isUserMatch =
                fixture.home_team_id === userTeamId || fixture.away_team_id === userTeamId;
              const completed = fixture.status === "Completed";
              const contextItems = [
                buildTeamMenuItem(
                  `${t("common.viewTeam")}: ${fixture.home_team_name}`,
                  fixture.home_team_id,
                ),
                buildTeamMenuItem(
                  `${t("common.viewTeam")}: ${fixture.away_team_name}`,
                  fixture.away_team_id,
                ),
              ];

              return (
                <ContextMenu items={contextItems} key={fixture.id}>
                  <div
                    className={`flex items-center px-5 py-3 transition-colors ${
                      isUserMatch ? "bg-primary-50/50 dark:bg-primary-500/5" : ""
                    }`}
                    data-testid={`schedule-fixture-${fixture.id}`}
                  >
                    <button
                      type="button"
                      onClick={() => onSelectTeam(fixture.home_team_id)}
                      className={`flex-1 bg-transparent p-0 text-right text-sm font-semibold hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus ${
                        fixture.home_team_id === userTeamId
                          ? "text-primary-600 dark:text-primary-400"
                          : "text-gray-800 dark:text-gray-200"
                      }`}
                    >
                      {fixture.home_team_name}
                    </button>
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
                      {fixture.rescheduled_from && (
                        <MovedFromNote
                          originalDate={fixture.rescheduled_from}
                          testId={`moved-from-${fixture.id}`}
                        />
                      )}
                      {completed && isUserMatch && competitionId && (
                        <button
                          type="button"
                          onClick={() => void onOpenReplay(competitionId, fixture.id)}
                          disabled={replayLoadingFixture !== null}
                          aria-busy={replayLoadingFixture === fixture.id}
                          className="mt-1 text-caption text-primary hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
                        >
                          {replayLoadingFixture === fixture.id
                            ? t("dashboard.loading")
                            : t("match.reviewMatch")}
                        </button>
                      )}
                    </div>
                    <button
                      type="button"
                      onClick={() => onSelectTeam(fixture.away_team_id)}
                      className={`flex-1 bg-transparent p-0 text-left text-sm font-semibold hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-focus ${
                        fixture.away_team_id === userTeamId
                          ? "text-primary-600 dark:text-primary-400"
                          : "text-gray-800 dark:text-gray-200"
                      }`}
                    >
                      {fixture.away_team_name}
                    </button>
                  </div>
                </ContextMenu>
              );
            })}
          </div>
        </CardBody>
      </Card>
    </div>
  );
}

import { InternationalView, LoadingPlaceholder, StandingsView } from "./ScheduleStandingsViews";
