import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui";

export type SpatialAction = "simulating" | "preparing" | "finalizing";

interface MatchViewModePanelProps {
  /** The action currently running; every option is disabled while one is. */
  busy: SpatialAction | null;
  /** Translated, player-facing failure of the last attempt. */
  error: string | null;
  onWatchIn2d: () => void;
  onQuickSimulation: () => void;
  /** Manage the match yourself: tactics, substitutions and pace, with the result applied at full time. */
  onManage?: () => void;
  /**
   * Only for screens that have no "Start Match" button of their own (spectators): adds the
   * classic live option so the three ways of seeing the match are listed together.
   */
  onPlayLive?: () => void;
}

function Option({
  title,
  description,
  descriptionId,
  action,
  highlighted = false,
}: {
  title: string;
  description: string;
  /** The button's `aria-describedby`: a screen reader hears what the option can and cannot do. */
  descriptionId: string;
  action?: ReactNode;
  /** The way to play the match with full control, drawn as the main choice. */
  highlighted?: boolean;
}) {
  return (
    <li
      className={`flex flex-1 flex-col gap-2 rounded border bg-panel p-3 ${
        highlighted ? "border-focus ring-1 ring-focus" : "border-line"
      }`}
    >
      <p className="font-heading text-body font-bold uppercase tracking-label text-fg">{title}</p>
      <p id={descriptionId} className="flex-1 text-caption text-fg-muted">
        {description}
      </p>
      {action}
    </li>
  );
}

/**
 * The ways to play a prepared match, side by side and in plain words, the one that gives the
 * manager control first:
 * manage in 2D (a live match: tactics, substitutions and pace, time only moves forward),
 * watch the simulation (a replay of a match the engine has already played: it can be sought, not
 * changed), quick simulation (result straight away, replay stored for later) and the classic
 * text-commentary match.
 */
export default function MatchViewModePanel({
  busy,
  error,
  onWatchIn2d,
  onQuickSimulation,
  onManage,
  onPlayLive,
}: MatchViewModePanelProps) {
  const { t } = useTranslation();
  const ids = useId();
  return (
    <section
      aria-labelledby="match-view-mode-heading"
      className="shrink-0 border-b border-line bg-sunken px-6 py-3"
    >
      <h2
        id="match-view-mode-heading"
        className="mb-2 font-heading text-caption font-bold uppercase tracking-label text-fg-muted"
      >
        {t("match.viewMode.heading")}
      </h2>
      <ul className="flex flex-col gap-3 sm:flex-row">
        {onManage && (
          <Option
            highlighted
            title={t("match.interactive.manage")}
            description={t("match.interactive.manageDesc")}
            descriptionId={`${ids}-manage`}
            action={
              <Button
                size="sm"
                onClick={onManage}
                disabled={busy !== null}
                aria-describedby={`${ids}-manage`}
              >
                {t("match.interactive.manage")}
              </Button>
            }
          />
        )}
        <Option
          title={t("match.watchWithM12")}
          description={t("match.viewMode.watchDesc")}
          descriptionId={`${ids}-watch`}
          action={
            <Button
              size="sm"
              variant="accent"
              onClick={onWatchIn2d}
              disabled={busy !== null}
              aria-busy={busy === "preparing"}
              aria-describedby={`${ids}-watch`}
            >
              {t("match.watchWithM12")}
            </Button>
          }
        />
        <Option
          title={t("match.simulateWithM12")}
          description={t("match.viewMode.quickDesc")}
          descriptionId={`${ids}-quick`}
          action={
            <Button
              size="sm"
              variant="outline"
              onClick={onQuickSimulation}
              disabled={busy !== null}
              aria-busy={busy === "simulating"}
              aria-describedby={`${ids}-quick`}
            >
              {t("match.simulateWithM12")}
            </Button>
          }
        />
        <Option
          title={t("match.startMatch")}
          description={t("match.viewMode.liveDesc")}
          descriptionId={`${ids}-live`}
          action={
            onPlayLive && (
              <Button
                size="sm"
                variant="outline"
                onClick={onPlayLive}
                disabled={busy !== null}
                aria-describedby={`${ids}-live`}
              >
                {t("match.startMatch")}
              </Button>
            )
          }
        />
      </ul>
      {busy && (
        <p className="mt-2 text-caption text-fg" role="status">
          {t("dashboard.loading")}
        </p>
      )}
      {error && (
        <p className="mt-2 text-caption text-danger" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
