import { useTranslation } from "react-i18next";
import type { MatchSnapshot } from "./types";
import MatchViewModePanel, { type SpatialAction } from "./MatchViewModePanel";

interface SpectatorViewChoiceProps {
  snapshot: MatchSnapshot;
  busy: SpatialAction | null;
  error: string | null;
  onPlayLive: () => void;
  onWatchIn2d: () => void;
  onQuickSimulation: () => void;
}

/**
 * Spectators have no line-up to set, so they used to be sent straight into the classic live
 * screen and never saw the 2D option. This screen is the one step in between.
 */
export default function SpectatorViewChoice({
  snapshot,
  busy,
  error,
  onPlayLive,
  onWatchIn2d,
  onQuickSimulation,
}: SpectatorViewChoiceProps) {
  const { t } = useTranslation();
  return (
    <main className="min-h-screen bg-page text-fg">
      <header className="border-b border-line bg-panel px-6 py-6 text-center">
        <p className="font-heading text-caption uppercase tracking-label text-fg-muted">
          {t("match.spectatorMode")}
        </p>
        <h1 className="mt-1 font-heading text-title uppercase tracking-label">
          {snapshot.home_team.name} <span className="text-fg-subtle">{t("common.vs")}</span>{" "}
          {snapshot.away_team.name}
        </h1>
      </header>
      <MatchViewModePanel
        busy={busy}
        error={error}
        onPlayLive={onPlayLive}
        onWatchIn2d={onWatchIn2d}
        onQuickSimulation={onQuickSimulation}
      />
    </main>
  );
}
