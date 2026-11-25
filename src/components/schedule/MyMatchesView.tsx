import { useMemo, useState } from "react";
import type { TFunction } from "i18next";

import { competitionDisplayName } from "../../lib/competitionName";
import { formatMatchDate } from "../../lib/dateFormatting";
import type { GameStateData } from "../../store/gameStore";
import type { UserFixture, UserSchedule } from "../../services/scheduleService";
import { Badge, Card, CardBody, Select } from "../ui";
import MovedFromNote from "./MovedFromNote";

interface MyMatchesViewProps {
  schedule: UserSchedule | null;
  gameState: GameStateData;
  onOpenReplay: (competitionId: string, fixtureId: string) => Promise<void>;
  replayLoadingFixture: string | null;
  t: TFunction;
}

const ALL = "";

/**
 * The manager's matches, every competition at once, in date order: the league, the cups, Europe and
 * friendlies he is in, with the next one marked. The rows are the competitions' own fixtures, so
 * each keeps the competition and fixture ids a replay or a jump into that competition needs; a
 * filter narrows the list without leaving the view.
 */
export default function MyMatchesView({
  schedule,
  gameState,
  onOpenReplay,
  replayLoadingFixture,
  t,
}: MyMatchesViewProps) {
  const [filter, setFilter] = useState(ALL);

  const competitions = useMemo(() => {
    const seen = new Map<string, string>();
    for (const { fixture, competition_name } of schedule?.fixtures ?? []) {
      if (!seen.has(fixture.competition_id)) seen.set(fixture.competition_id, competition_name);
    }
    return [...seen.entries()].map(([id, fallback]) => {
      const competition = (gameState.competitions ?? []).find((c) => c.id === id);
      return { id, name: competition ? competitionDisplayName(competition, t) : fallback };
    });
  }, [schedule, gameState.competitions, t]);

  if (!schedule) {
    return <p className="py-8 text-center text-body text-fg-muted">{t("dashboard.loading")}</p>;
  }

  const nameOf = (competitionId: string) =>
    competitions.find((competition) => competition.id === competitionId)?.name ?? "";
  const visible = schedule.fixtures.filter(
    (entry) => filter === ALL || entry.fixture.competition_id === filter,
  );

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-heading text-heading font-bold text-fg">
          {t("schedule.myMatches.title")}
        </h2>
        {competitions.length > 1 && (
          <Select
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            aria-label={t("schedule.myMatches.filter")}
          >
            <option value={ALL}>{t("schedule.myMatches.allCompetitions")}</option>
            {competitions.map((competition) => (
              <option key={competition.id} value={competition.id}>
                {competition.name}
              </option>
            ))}
          </Select>
        )}
      </div>
      {visible.length === 0 ? (
        <p className="py-6 text-center text-body text-fg-muted">{t("schedule.myMatches.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {visible.map((entry) => (
            <MatchRow
              key={`${entry.fixture.competition_id}:${entry.fixture.id}`}
              entry={entry}
              competitionName={nameOf(entry.fixture.competition_id)}
              onOpenReplay={onOpenReplay}
              replayLoadingFixture={replayLoadingFixture}
              t={t}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function MatchRow({
  entry,
  competitionName,
  onOpenReplay,
  replayLoadingFixture,
  t,
}: {
  entry: UserFixture;
  competitionName: string;
  onOpenReplay: (competitionId: string, fixtureId: string) => Promise<void>;
  replayLoadingFixture: string | null;
  t: TFunction;
}) {
  const { fixture, is_home: isHome, is_next: isNext } = entry;
  const opponent = isHome ? fixture.away_team_name : fixture.home_team_name;
  const played = fixture.status === "Completed" && fixture.result !== null;
  return (
    <li>
      <Card accent={isNext ? "primary" : "none"}>
        <CardBody>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
            <span className="min-w-28 text-body font-semibold text-fg">
              {formatMatchDate(fixture.date)}
            </span>
            <span className="text-caption text-fg-muted">{competitionName}</span>
            <span className="text-body text-fg">
              {opponent}{" "}
              <span className="text-caption text-fg-muted">
                ({isHome ? t("schedule.myMatches.home") : t("schedule.myMatches.away")})
              </span>
            </span>
            {played && fixture.result ? (
              <span className="font-heading font-bold text-fg">
                {fixture.result.home_goals} – {fixture.result.away_goals}
              </span>
            ) : null}
            {isNext && (
              <Badge variant="primary" size="sm">
                {t("schedule.nextMatch")}
              </Badge>
            )}
            {fixture.rescheduled_from && (
              <MovedFromNote
                originalDate={fixture.rescheduled_from}
                testId={`moved-from-${fixture.id}`}
              />
            )}
            {played && (
              <button
                type="button"
                onClick={() => void onOpenReplay(fixture.competition_id, fixture.id)}
                disabled={replayLoadingFixture !== null}
                aria-busy={replayLoadingFixture === fixture.id}
                className="ml-auto rounded-md px-2 py-1 text-caption font-semibold text-fg-muted hover:bg-hover focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page disabled:opacity-50"
              >
                {replayLoadingFixture === fixture.id
                  ? t("dashboard.loading")
                  : t("match.reviewMatch")}
              </button>
            )}
          </div>
        </CardBody>
      </Card>
    </li>
  );
}
