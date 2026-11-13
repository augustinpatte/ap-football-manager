import { useTranslation } from "react-i18next";

interface ReplayPlaybackBarProps<Speed extends number> {
  playing: boolean;
  onTogglePlaying: () => void;
  speed: Speed;
  speeds: readonly Speed[];
  onSpeedChange: (speed: Speed) => void;
  time: number;
  start: number;
  end: number;
  onSeek: (time: number) => void;
  finishing: boolean;
  /** "Back to the schedule" for a replay of a finished match, "go to result" before one counts. */
  finishLabelKey: string;
  onFinish: () => void;
}

/** The one clock every view of the replay shares: play, speed, seek and leave. */
export default function ReplayPlaybackBar<Speed extends number>({
  playing,
  onTogglePlaying,
  speed,
  speeds,
  onSpeedChange,
  time,
  start,
  end,
  onSeek,
  finishing,
  finishLabelKey,
  onFinish,
}: ReplayPlaybackBarProps<Speed>) {
  const { t } = useTranslation();
  return (
    <div className="mt-3 flex flex-wrap items-center gap-3">
      <button
        type="button"
        onClick={onTogglePlaying}
        disabled={time >= end}
        className="rounded bg-primary-solid px-4 py-2 text-on-brand focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-not-allowed disabled:opacity-60"
      >
        {playing ? t("match.pauseReplay") : t("match.playReplay")}
      </button>
      <label className="text-body">
        {t("match.replaySpeed")}
        <select
          value={speed}
          onChange={(event) => onSpeedChange(Number(event.target.value) as Speed)}
          className="ml-2 rounded border border-line bg-panel p-2"
        >
          {speeds.map((value) => (
            <option key={value} value={value}>
              {value}×
            </option>
          ))}
        </select>
      </label>
      <input
        aria-label={t("match.replayTimeline")}
        type="range"
        min={start}
        max={end}
        value={time}
        onChange={(event) => onSeek(Number(event.target.value))}
        className="min-w-48 flex-1"
      />
      <button
        type="button"
        onClick={onFinish}
        disabled={finishing}
        aria-busy={finishing}
        className="rounded bg-accent-solid px-4 py-2 text-on-brand focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
      >
        {t(finishLabelKey)}
      </button>
    </div>
  );
}
