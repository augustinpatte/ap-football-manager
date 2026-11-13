import type { ReactNode } from "react";
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
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <li className="flex flex-1 flex-col gap-2 rounded border border-line bg-panel p-3">
      <p className="font-heading text-body font-bold uppercase tracking-label text-fg">{title}</p>
      <p className="flex-1 text-caption text-fg-muted">{description}</p>
      {action}
    </li>
  );
}

/**
 * The three ways to play a prepared match, side by side and in plain words:
 * live (classic engine, commentary), watch in 2D (22 players, result applied on confirmation)
 * and quick simulation (result straight away, replay stored for later).
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
        <Option
          title={t("match.startMatch")}
          description={t("match.viewMode.liveDesc")}
          action={
            onPlayLive && (
              <Button size="sm" onClick={onPlayLive} disabled={busy !== null}>
                {t("match.startMatch")}
              </Button>
            )
          }
        />
        <Option
          title={t("match.watchWithM12")}
          description={t("match.viewMode.watchDesc")}
          action={
            <Button
              size="sm"
              variant="accent"
              onClick={onWatchIn2d}
              disabled={busy !== null}
              aria-busy={busy === "preparing"}
            >
              {t("match.watchWithM12")}
            </Button>
          }
        />
        <Option
          title={t("match.simulateWithM12")}
          description={t("match.viewMode.quickDesc")}
          action={
            <Button
              size="sm"
              variant="outline"
              onClick={onQuickSimulation}
              disabled={busy !== null}
              aria-busy={busy === "simulating"}
            >
              {t("match.simulateWithM12")}
            </Button>
          }
        />
        {onManage && (
          <Option
            title={t("match.interactive.manage")}
            description={t("match.interactive.manageDesc")}
            action={
              <Button size="sm" onClick={onManage} disabled={busy !== null}>
                {t("match.interactive.manage")}
              </Button>
            }
          />
        )}
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
