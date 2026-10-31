import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

const PLAYBACK_INTERVAL_MS = 100;
const PLAYBACK_SPEEDS = [0.5, 1, 2, 4] as const;

type FramePlayer = {
  x: number;
  y: number;
  heading: number;
  on_pitch: boolean;
};

type ReplayFrame = {
  t_ms: number;
  period: string;
  ball: { x: number; y: number; z: number };
  score: { home: number; away: number };
  players: FramePlayer[];
};

type ReplayEvent = {
  t_ms: number;
  type: string;
  scorer?: string;
};

type Replay = {
  schema_version: number;
  home_team: string;
  away_team: string;
  roster: string[];
  frames: ReplayFrame[];
  events: ReplayEvent[];
};

interface M12ReplayViewerProps {
  replayJson: string;
  onFinish: () => void | Promise<void>;
  onBack?: () => void;
  readOnly?: boolean;
}

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isFramePlayer(value: unknown): value is FramePlayer {
  if (!value || typeof value !== "object") return false;
  const player = value as Partial<FramePlayer>;
  return (
    isNumber(player.x) &&
    isNumber(player.y) &&
    isNumber(player.heading) &&
    typeof player.on_pitch === "boolean"
  );
}

function isReplayFrame(value: unknown): value is ReplayFrame {
  if (!value || typeof value !== "object") return false;
  const frame = value as Partial<ReplayFrame>;
  return (
    isNumber(frame.t_ms) &&
    typeof frame.period === "string" &&
    !!frame.ball &&
    isNumber(frame.ball.x) &&
    isNumber(frame.ball.y) &&
    isNumber(frame.ball.z) &&
    !!frame.score &&
    isNumber(frame.score.home) &&
    isNumber(frame.score.away) &&
    Array.isArray(frame.players) &&
    frame.players.every(isFramePlayer)
  );
}

function parseReplay(source: string): Replay | null {
  try {
    const parsed: unknown = JSON.parse(source);
    if (!parsed || typeof parsed !== "object") return null;
    const replay = parsed as Partial<Replay>;
    const roster = replay.roster;
    const frames = replay.frames;
    const events = replay.events;
    if (
      replay.schema_version !== 1 ||
      typeof replay.home_team !== "string" ||
      typeof replay.away_team !== "string" ||
      !Array.isArray(roster) ||
      !roster.every((player) => typeof player === "string") ||
      !Array.isArray(frames) ||
      frames.length < 2 ||
      !frames.every(isReplayFrame) ||
      !Array.isArray(events)
    ) {
      return null;
    }
    if (
      frames.some(
        (frame, index) =>
          frame.players.length !== roster.length ||
          (index > 0 && frame.t_ms < frames[index - 1].t_ms),
      )
    ) {
      return null;
    }
    return {
      schema_version: replay.schema_version,
      home_team: replay.home_team,
      away_team: replay.away_team,
      roster,
      frames,
      events: events as ReplayEvent[],
    };
  } catch {
    return null;
  }
}

function frameAtOrBefore(frames: ReplayFrame[], time: number): ReplayFrame {
  let low = 0;
  let high = frames.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (frames[middle].t_ms <= time) low = middle;
    else high = middle - 1;
  }
  return frames[low];
}

function drawPitch(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  frame: ReplayFrame,
  roster: string[],
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

  frame.players.forEach((player, index) => {
    if (!player.on_pitch) return;
    const homePlayer = index < roster.length / 2;
    context.fillStyle = homePlayer ? "#2f7bf6" : "#f2744a";
    context.beginPath();
    context.arc(x(player.x), y(player.y), Math.max(5, scale), 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#ffffff";
    context.font = `${Math.max(9, scale)}px sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(String((index % (roster.length / 2)) + 1), x(player.x), y(player.y));
  });

  const lift = Math.max(0, frame.ball.z) * scale * 0.25;
  context.fillStyle = "#ffffff";
  context.beginPath();
  context.arc(x(frame.ball.x), y(frame.ball.y) - lift, Math.max(3, scale * 0.4), 0, Math.PI * 2);
  context.fill();
  context.strokeStyle = "#111111";
  context.stroke();
}

export default function M12ReplayViewer({
  replayJson,
  onFinish,
  onBack,
  readOnly = false,
}: M12ReplayViewerProps) {
  const { t } = useTranslation();
  const replay = useMemo(() => parseReplay(replayJson), [replayJson]);
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
    if (!canvas.current || !frame || !replay) return;
    const rectangle = canvas.current.getBoundingClientRect();
    const width = rectangle.width || 960;
    const height = rectangle.height || 540;
    const ratio = window.devicePixelRatio || 1;
    canvas.current.width = width * ratio;
    canvas.current.height = height * ratio;
    const context = canvas.current.getContext("2d");
    if (!context) return;
    context.scale(ratio, ratio);
    drawPitch(context, width, height, frame, replay.roster);
  }, [frame, replay]);

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
  if (!replay || !frame) {
    return (
      <main className="min-h-screen bg-page p-4 text-fg">
        <div
          className="mx-auto max-w-2xl rounded border border-danger-solid bg-danger-tint p-4"
          role="alert"
        >
          <p>{t("match.replayUnavailable")}</p>
          {closeViewer && (
            <button
              type="button"
              onClick={closeViewer}
              className="mt-3 rounded bg-panel px-4 py-2 text-body focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
            >
              {t("match.backToSchedule")}
            </button>
          )}
        </div>
      </main>
    );
  }

  const events = replay.events.filter((event) => event.t_ms <= time).slice(-4);

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
            <li key={`${event.t_ms}-${index}`}>
              {Math.floor(event.t_ms / 60_000)}' · {event.type}
              {event.scorer ? ` · ${event.scorer}` : ""}
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
