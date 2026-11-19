import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { InteractiveView } from "../../services/interactiveMatchService";
import { Badge, Button, Checkbox, SegmentedControl } from "../ui";
import type { DirectorSnapshot } from "./liveDirector";
import { HIGHLIGHT_MODES, type HighlightMode, type PlaybackSpeed, SPEEDS } from "./livePlayback";

/** The phase of the match as a person names it, from what the picture shows. */
export function phaseLabelKey(snapshot: DirectorSnapshot): string {
  if (snapshot.status === "finished") return "match.fullTime";
  if (snapshot.frameState === "half_time" || snapshot.status === "break") return "match.halfTime";
  if (snapshot.status === "loading") return "match.liveView.status.loading";
  if (snapshot.status === "paused" && snapshot.playheadMs === 0) {
    return "match.liveView.kickoffPending";
  }
  if (snapshot.status === "paused" || snapshot.status === "pausing") return "match.paused";
  return "match.live";
}

interface ScoreboardProps {
  homeName: string;
  awayName: string;
  snapshot: DirectorSnapshot;
  /** A goal just shown, in words, announced once through the status line. */
  announcement?: string | null;
}

/** The scoreboard: the score and clock are the picture's, never the engine's head, so they match what is drawn. */
export function LiveScoreboard({ homeName, awayName, snapshot, announcement }: ScoreboardProps) {
  const { t } = useTranslation();
  const clock = snapshot.view?.clock;
  const addedTime =
    clock && clock.added_time_ms !== null && snapshot.period === snapshot.view?.session.period
      ? Math.round(clock.added_time_ms / 60_000)
      : null;
  const phase = t(phaseLabelKey(snapshot));
  return (
    <header className="bg-chrome px-6 py-4 text-on-brand">
      <h1 className="sr-only">{t("match.interactive.heading")}</h1>
      <div className="flex items-center justify-center gap-6 font-heading text-title uppercase tracking-label">
        <span>{homeName}</span>
        <span data-testid="live-score">
          {snapshot.score.home} – {snapshot.score.away}
        </span>
        <span>{awayName}</span>
      </div>
      <p className="mt-1 flex items-center justify-center gap-2 text-center text-body">
        <span data-testid="live-clock">{snapshot.clock}</span>
        <Badge variant="neutral">{phase}</Badge>
        {addedTime !== null && addedTime > 0 && (
          <Badge variant="accent">{t("match.liveView.addedTime", { minutes: addedTime })}</Badge>
        )}
      </p>
      {/* One status line that changes only when the score or the phase does, not with the clock. */}
      <p role="status" className="sr-only">
        {`${homeName} ${snapshot.score.home}, ${awayName} ${snapshot.score.away}, ${phase}`}
        {announcement ? `. ${announcement}` : ""}
      </p>
    </header>
  );
}

interface ControlsProps {
  snapshot: DirectorSnapshot;
  view: InteractiveView | null;
  finishing: boolean;
  onTogglePlay: () => void;
  onSkip: () => void;
  onSpeed: (speed: PlaybackSpeed) => void;
  onMode: (mode: HighlightMode) => void;
  onAssistant: (active: boolean) => void;
  onFinish: () => void;
}

function playLabelKey(snapshot: DirectorSnapshot): string {
  if (snapshot.status === "playing") return "match.pause";
  if (snapshot.status === "break") return "match.liveView.startSecondHalf";
  return snapshot.playheadMs === 0 ? "match.interactive.play" : "match.interactive.resume";
}

/** Play, pause, speed, highlights, skip, the assistant and finishing: the controls of one match. */
export function LiveControls({
  snapshot,
  view,
  finishing,
  onTogglePlay,
  onSkip,
  onSpeed,
  onMode,
  onAssistant,
  onFinish,
}: ControlsProps) {
  const { t } = useTranslation();
  const speedLabel = useId();
  const modeLabel = useId();
  const finished = snapshot.status === "finished";
  const loading = snapshot.status === "loading";
  return (
    <section
      aria-label={t("match.liveView.controls")}
      className="flex flex-wrap items-end gap-4 border-b border-line bg-sunken px-6 py-3"
    >
      <Button size="sm" onClick={onTogglePlay} disabled={finished || loading}>
        {t(playLabelKey(snapshot))}
      </Button>
      <Button size="sm" variant="outline" onClick={onSkip} disabled={finished || loading}>
        {t("match.liveView.skip")}
      </Button>
      <div className="flex flex-col gap-1">
        <span id={speedLabel} className="text-caption text-fg-muted">
          {t("match.simSpeed")}
        </span>
        <SegmentedControl
          labelledBy={speedLabel}
          emphasis={snapshot.status === "playing" ? "strong" : "outlined"}
          value={String(snapshot.speed)}
          onChange={(value) => onSpeed(Number(value) as PlaybackSpeed)}
          options={SPEEDS.map((speed) => ({ value: String(speed), label: `${speed}×` }))}
        />
      </div>
      <div className="flex flex-col gap-1">
        <span id={modeLabel} className="text-caption text-fg-muted">
          {t("match.liveView.mode")}
        </span>
        <SegmentedControl
          labelledBy={modeLabel}
          emphasis="outlined"
          value={snapshot.mode}
          onChange={(value) => onMode(value as HighlightMode)}
          options={HIGHLIGHT_MODES.map((mode) => ({
            value: mode,
            label: t(`match.liveView.modes.${mode}`),
          }))}
        />
      </div>
      {view && view.user_side !== null && (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2 text-body text-fg">
            <Checkbox
              id="interactive-assistant"
              checked={view.assistant_active}
              disabled={(!view.assistant_available && !view.assistant_active) || !snapshot.canOrder}
              onChange={(event) => onAssistant(event.target.checked)}
              aria-describedby="assistant-hint"
            />
            <label htmlFor="interactive-assistant">{t("match.interactive.assistant")}</label>
          </div>
          <span id="assistant-hint" className="text-caption text-fg-muted">
            {view.assistant_available
              ? t("match.interactive.assistantHint")
              : t("match.interactive.assistantUnavailable")}
          </span>
        </div>
      )}
      <div className="ml-auto">
        <Button
          id="interactive-finish"
          size="sm"
          onClick={onFinish}
          disabled={!finished || finishing}
          aria-busy={finishing}
        >
          {t("match.interactive.finish")}
        </Button>
      </div>
    </section>
  );
}
