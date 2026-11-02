import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  eventPlayerId,
  frameAtOrBefore,
  isKnownReplayEventType,
  parseM12Replay,
  playersOnPitch,
  type PitchPlayer,
  type ReplayEvent,
  type ReplayFrame,
  type ReplayRejection,
} from "./m12Replay";

const PLAYBACK_INTERVAL_MS = 100;
const PLAYBACK_SPEEDS = [0.5, 1, 2, 4] as const;
const VISIBLE_EVENTS = 6;

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
}

const REJECTION_MESSAGE_KEY: Record<ReplayRejection, string> = {
  unreadable: "match.replayUnavailable",
  malformed: "match.replayUnavailable",
  inconsistentFrames: "match.replayUnavailable",
  unsupportedSchema: "match.replayUnsupportedSchema",
  noFrames: "match.replayNoFrames",
};

function drawPitch(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  frame: ReplayFrame,
  onPitch: PitchPlayer[],
) {
  const scale = Math.min(width / 113, height / 76);
  const originX = (width - 105 * scale) / 2;
  const originY = (height - 68 * scale) / 2;
  const x = (value: number) => originX + value * scale;
  const y = (value: number) => originY + (68 - value) * scale;

  context.fillStyle = "#16301f";
  context.fillRect(0, 0, width, height);
  for (let stripe = 0; stripe < 14; stripe += 1) {
    context.fillStyle = stripe % 2 ? "#2b6a3b" : "#2f7340";
    context.fillRect(x(stripe * 7.5), y(68), 7.6 * scale, 68 * scale);
  }
  context.strokeStyle = "#e9f3e5";
  context.lineWidth = Math.max(1, scale * 0.14);
  context.strokeRect(x(0), y(68), 105 * scale, 68 * scale);
  context.beginPath();
  context.moveTo(x(52.5), y(0));
  context.lineTo(x(52.5), y(68));
  context.stroke();
  context.beginPath();
  context.arc(x(52.5), y(34), 9.15 * scale, 0, Math.PI * 2);
  context.stroke();

  onPitch.forEach((player) => {
    context.fillStyle = player.side === "home" ? "#2f7bf6" : "#f2744a";
    context.beginPath();
    context.arc(x(player.x), y(player.y), Math.max(5, scale), 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#ffffff";
    context.font = `${Math.max(9, scale)}px sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(String(player.label), x(player.x), y(player.y));
  });

  const lift = Math.max(0, frame.ball.z) * scale * 0.25;
  context.fillStyle = "#ffffff";
  context.beginPath();
  context.arc(x(frame.ball.x), y(frame.ball.y) - lift, Math.max(3, scale * 0.4), 0, Math.PI * 2);
  context.fill();
  context.strokeStyle = "#111111";
  context.stroke();
}

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
}: M12ReplayViewerProps) {
  const { t } = useTranslation();
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
    drawPitch(context, width, height, frame, onPitch);
  }, [frame, onPitch]);

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

  const nameOf = (playerId: string | undefined): string | undefined =>
    playerId === undefined ? undefined : (playerNames?.[playerId] ?? playerId);
  const describeEvent = (event: ReplayEvent): string => {
    const label = isKnownReplayEventType(event.type)
      ? t(`match.replayEventType.${event.type}`)
      : event.type.replace(/_/g, " ");
    const subject =
      event.type === "substitution" && event.player_off && event.player_on
        ? `${nameOf(event.player_off)} → ${nameOf(event.player_on)}`
        : nameOf(eventPlayerId(event));
    return `${Math.floor(event.t_ms / 60_000)}' · ${label}${subject ? ` · ${subject}` : ""}`;
  };
  const events = replay.events.filter((event) => event.t_ms <= time).slice(-VISIBLE_EVENTS);

  return (
    <main className="min-h-screen bg-page p-4 text-fg">
      <section className="mx-auto max-w-6xl rounded border border-line bg-panel p-4">
        <header className="mb-3 flex items-center justify-between font-heading text-title uppercase tracking-label">
          <span>
            {replay.home_team} {frame.score.home} – {frame.score.away} {replay.away_team}
          </span>
          <span>{Math.floor(time / 60_000)}'</span>
        </header>
        <canvas
          ref={canvas}
          aria-label={t("match.replayPitch")}
          className="h-[60vh] min-h-72 w-full rounded bg-sunken"
        />
        <p className="mt-2 text-caption text-fg-muted" data-testid="replay-players-on-pitch">
          {t("match.replayPlayersOnPitch", {
            home: homeOnPitch,
            away: awayOnPitch,
            total: onPitch.length,
          })}
        </p>
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
        <ul aria-live="polite" className="mt-3 text-body text-fg-muted">
          {events.map((event, index) => (
            <li key={`${event.t_ms}-${index}`}>{describeEvent(event)}</li>
          ))}
        </ul>
      </section>
    </main>
  );
}
