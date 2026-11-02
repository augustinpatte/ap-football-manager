import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TeamData } from "../../store/gameStore";
import { TabPanel, Tabs } from "../ui";
import MatchScreenLayout from "./MatchScreenLayout";
import { frameAtOrBefore, parseM12Replay, playersOnPitch, type ReplayRejection } from "./m12Replay";
import ReplayEventFeed from "./ReplayEventFeed";
import { ReplayConditionPanel, ReplayLineupPanel, ReplayStatsPanel } from "./ReplayPanels";
import { buildFeed, conditionsAt, feedAt, lineupAt, statsAt } from "./replayDerived";
import { resolveReplayTeamName } from "./replayNames";
import { drawPitch } from "./replayPitch";

const PLAYBACK_INTERVAL_MS = 100;
const PLAYBACK_SPEEDS = [0.5, 1, 2, 4] as const;
const VISIBLE_EVENTS = 8;
/** How long a goal stays announced above the pitch, in replay time. */
const GOAL_BANNER_MS = 8_000;
const VIEWS = ["pitch", "condition", "stats", "lineup"] as const;
type ViewId = (typeof VIEWS)[number];

interface M12ReplayViewerProps {
  replayJson: string;
  onFinish: () => void | Promise<void>;
  onBack?: () => void;
  /** A completed match: "back" only closes the viewer, nothing is finalised. */
  readOnly?: boolean;
  /** Player id -> display name. Ids are shown only when a name is unknown. */
  playerNames?: Readonly<Record<string, string>>;
  /** Ids of the home side's players, when the roster order cannot be trusted. */
  homePlayerIds?: ReadonlySet<string>;
  /** The game's clubs, to put names on the scoreboard in place of the ids the replay carries. */
  teams?: ReadonlyArray<Pick<TeamData, "id" | "name">>;
}

const REJECTION_MESSAGE_KEY: Record<ReplayRejection, string> = {
  unreadable: "match.replayUnavailable",
  malformed: "match.replayUnavailable",
  inconsistentFrames: "match.replayUnavailable",
  unsupportedSchema: "match.replayUnsupportedSchema",
  noFrames: "match.replayNoFrames",
};

function ReplayUnavailable({ messageKey, onClose }: { messageKey: string; onClose?: () => void }) {
  const { t } = useTranslation();
  return (
    <main className="min-h-screen bg-page p-4 text-fg">
      <div
        className="mx-auto max-w-2xl rounded border border-danger-solid bg-danger-tint p-4"
        role="alert"
      >
        <p>{t(messageKey)}</p>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="mt-3 rounded bg-panel px-4 py-2 text-body focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          >
            {t("match.backToSchedule")}
          </button>
        )}
      </div>
    </main>
  );
}

export default function M12ReplayViewer({
  replayJson,
  onFinish,
  onBack,
  readOnly = false,
  playerNames,
  homePlayerIds,
  teams = [],
}: M12ReplayViewerProps) {
  const { t } = useTranslation();
  const tabsId = useId();
  const [view, setView] = useState<ViewId>("pitch");
  const parsed = useMemo(() => parseM12Replay(replayJson), [replayJson]);
  const replay = parsed.ok ? parsed.replay : null;
  const canvas = useRef<HTMLCanvasElement>(null);
  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState<(typeof PLAYBACK_SPEEDS)[number]>(1);
  const [time, setTime] = useState(() => replay?.frames[0]?.t_ms ?? 0);
  const [finishing, setFinishing] = useState(false);
  const [finishError, setFinishError] = useState(false);
  const finishInFlight = useRef(false);

  useEffect(() => {
    if (!replay) return;
    setTime(replay.frames[0].t_ms);
    setPlaying(true);
    setFinishError(false);
  }, [replay]);

  const frame = replay ? frameAtOrBefore(replay.frames, time) : null;
  const end = replay?.frames[replay.frames.length - 1]?.t_ms ?? 0;
  const onPitch = useMemo(
    () => (replay && frame ? playersOnPitch(frame, replay.roster, homePlayerIds) : []),
    [frame, replay, homePlayerIds],
  );
  const homeOnPitch = onPitch.filter((player) => player.side === "home").length;
  const awayOnPitch = onPitch.length - homeOnPitch;
  const feed = useMemo(() => (replay ? buildFeed(replay) : []), [replay]);
  // The goal crossed in the last few seconds of replay time, to announce it and light its goal.
  const recentGoal = useMemo(() => {
    if (!replay) return null;
    const goals = replay.events.filter((event) => event.type === "goal" && event.t_ms <= time);
    const last = goals[goals.length - 1];
    return last && time - last.t_ms <= GOAL_BANNER_MS ? last : null;
  }, [replay, time]);
  const flashGoalX = recentGoal?.position ? (recentGoal.position.x < 52.5 ? 0 : 105) : undefined;

  useEffect(() => {
    if (!playing || !replay || time >= end) return;
    const interval = window.setInterval(() => {
      setTime((previous) => Math.min(end, previous + PLAYBACK_INTERVAL_MS * speed));
    }, PLAYBACK_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [end, playing, replay, speed, time]);

  useEffect(() => {
    if (time >= end && playing) setPlaying(false);
  }, [end, playing, time]);

  const render = useCallback(() => {
    if (!canvas.current || !frame) return;
    const rectangle = canvas.current.getBoundingClientRect();
    const width = rectangle.width || 960;
    const height = rectangle.height || 540;
    const ratio = window.devicePixelRatio || 1;
    canvas.current.width = width * ratio;
    canvas.current.height = height * ratio;
    const context = canvas.current.getContext("2d");
    if (!context) return;
    context.scale(ratio, ratio);
    drawPitch(context, width, height, { frame, onPitch, flashGoalX });
  }, [frame, onPitch, flashGoalX]);

  useEffect(() => {
    render();
  }, [render]);

  const handleFinish = useCallback(async () => {
    if (finishInFlight.current) return;
    finishInFlight.current = true;
    setFinishing(true);
    setFinishError(false);
    try {
      await onFinish();
    } catch {
      setFinishError(true);
    } finally {
      finishInFlight.current = false;
      setFinishing(false);
    }
  }, [onFinish]);

  const closeViewer = onBack ?? (readOnly ? onFinish : undefined);
  if (!parsed.ok || !replay || !frame) {
    const reason: ReplayRejection = parsed.ok ? "malformed" : parsed.reason;
    return (
      <ReplayUnavailable
        messageKey={REJECTION_MESSAGE_KEY[reason]}
        onClose={closeViewer ? () => void closeViewer() : undefined}
      />
    );
  }

  const teamNames = {
    home: resolveReplayTeamName(replay.home_team, teams) ?? t("match.replayTeam.home"),
    away: resolveReplayTeamName(replay.away_team, teams) ?? t("match.replayTeam.away"),
  };
  // An id is never shown: a player the game cannot name is "a player".
  const nameOf = (playerId: string | undefined): string =>
    (playerId !== undefined ? playerNames?.[playerId] : undefined) ??
    t("match.replayUnknownPlayer");
  const minute = Math.floor(time / 60_000);
  const entries = feedAt(feed, time, VISIBLE_EVENTS);
  const header = (
    <div className="flex items-center justify-between py-3 font-heading text-title uppercase tracking-label">
      <span>
        {teamNames.home} {frame.score.home} – {frame.score.away} {teamNames.away}
      </span>
      <span>{minute}'</span>
    </div>
  );

  return (
    <MatchScreenLayout
      header={header}
      headerContentClassName="max-w-7xl"
      contentClassName="px-6 py-4"
    >
      <section className="mx-auto max-w-7xl rounded border border-line bg-panel p-4 text-fg">
        {recentGoal ? (
          <p
            role="status"
            className="mb-3 rounded bg-warning-tint px-3 py-2 text-center font-heading text-heading font-bold uppercase tracking-label"
          >
            {t("match.replayGoalBanner", {
              player: nameOf(recentGoal.scorer),
              minute: Math.floor(recentGoal.t_ms / 60_000),
            })}
          </p>
        ) : null}
        <Tabs
          idPrefix={tabsId}
          label={t("match.replayViews")}
          tabs={VIEWS.map((id) => ({ id, label: t(`match.replayTab.${id}`) }))}
          selectedId={view}
          onSelect={(id) => setView(id as ViewId)}
        />
        <TabPanel idPrefix={tabsId} tabId="pitch" hidden={view !== "pitch"} className="pt-3">
          <canvas
            ref={canvas}
            aria-label={t("match.replayPitch")}
            className="h-[68vh] min-h-96 w-full rounded bg-sunken"
          />
          <p className="mt-2 text-caption text-fg-muted" data-testid="replay-players-on-pitch">
            {t("match.replayPlayersOnPitch", {
              home: homeOnPitch,
              away: awayOnPitch,
              total: onPitch.length,
            })}
          </p>
        </TabPanel>
        <TabPanel
          idPrefix={tabsId}
          tabId="condition"
          hidden={view !== "condition"}
          className="pt-3"
        >
          {view === "condition" ? (
            <ReplayConditionPanel
              entries={conditionsAt(replay, frame, homePlayerIds)}
              teamNames={teamNames}
              nameOf={nameOf}
            />
          ) : null}
        </TabPanel>
        <TabPanel idPrefix={tabsId} tabId="stats" hidden={view !== "stats"} className="pt-3">
          {view === "stats" ? (
            <ReplayStatsPanel
              stats={statsAt(replay, time, frame, homePlayerIds)}
              teamNames={teamNames}
              minute={minute}
            />
          ) : null}
        </TabPanel>
        <TabPanel idPrefix={tabsId} tabId="lineup" hidden={view !== "lineup"} className="pt-3">
          {view === "lineup" ? (
            <ReplayLineupPanel
              lineup={lineupAt(replay, frame, time, homePlayerIds)}
              teamNames={teamNames}
              nameOf={nameOf}
            />
          ) : null}
        </TabPanel>
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={() => setPlaying((value) => !value)}
            disabled={time >= end}
            className="rounded bg-primary-solid px-4 py-2 text-on-brand focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-not-allowed disabled:opacity-60"
          >
            {playing ? t("match.pauseReplay") : t("match.playReplay")}
          </button>
          <label className="text-body">
            {t("match.replaySpeed")}
            <select
              value={speed}
              onChange={(event) =>
                setSpeed(Number(event.target.value) as (typeof PLAYBACK_SPEEDS)[number])
              }
              className="ml-2 rounded border border-line bg-panel p-2"
            >
              {PLAYBACK_SPEEDS.map((value) => (
                <option key={value} value={value}>
                  {value}×
                </option>
              ))}
            </select>
          </label>
          <input
            aria-label={t("match.replayTimeline")}
            type="range"
            min={replay.frames[0].t_ms}
            max={end}
            value={time}
            onChange={(event) => {
              setPlaying(false);
              setTime(Number(event.target.value));
            }}
            className="min-w-48 flex-1"
          />
          <button
            type="button"
            onClick={() => void handleFinish()}
            disabled={finishing}
            aria-busy={finishing}
            className="rounded bg-accent-solid px-4 py-2 text-on-brand focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
          >
            {t(readOnly ? "match.backToSchedule" : "match.goToResult")}
          </button>
        </div>
        {finishError && (
          <p className="mt-3 text-body text-danger" role="alert">
            {t("match.m12SimulationFailed")}
          </p>
        )}
        <div className="mt-4 border-t border-line pt-3">
          <ReplayEventFeed entries={entries} teamNames={teamNames} nameOf={nameOf} />
        </div>
      </section>
    </MatchScreenLayout>
  );
}
