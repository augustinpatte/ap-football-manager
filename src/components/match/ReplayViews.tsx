import { type RefObject, useId } from "react";
import { useTranslation } from "react-i18next";

import { TabPanel, Tabs } from "../ui";
import type { Replay, ReplayFrame } from "./m12Replay";
import { ReplayConditionPanel, ReplayLineupPanel, ReplayStatsPanel } from "./ReplayPanels";
import { conditionsAt, lineupAt, statsAt } from "./replayDerived";

export const VIEWS = ["pitch", "condition", "stats", "lineup"] as const;
export type ViewId = (typeof VIEWS)[number];

interface ReplayViewsProps {
  replay: Replay;
  frame: ReplayFrame;
  time: number;
  view: ViewId;
  onViewChange: (view: ViewId) => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  playersOnPitch: { home: number; away: number };
  homePlayerIds?: ReadonlySet<string>;
  teamNames: { home: string; away: string };
  nameOf: (playerId: string | undefined) => string;
}

/**
 * The views of one match. Each reads the same replay at the same `time`, so changing view never
 * moves the clock, the score or the players: there is one match and these are ways of looking.
 * Only the visible view derives anything; the pitch stays mounted so it keeps its canvas.
 */
export default function ReplayViews({
  replay,
  frame,
  time,
  view,
  onViewChange,
  canvasRef,
  playersOnPitch,
  homePlayerIds,
  teamNames,
  nameOf,
}: ReplayViewsProps) {
  const { t } = useTranslation();
  const tabsId = useId();
  return (
    <>
      <Tabs
        idPrefix={tabsId}
        label={t("match.replayViews")}
        tabs={VIEWS.map((id) => ({ id, label: t(`match.replayTab.${id}`) }))}
        selectedId={view}
        onSelect={(id) => onViewChange(id as ViewId)}
      />
      <TabPanel idPrefix={tabsId} tabId="pitch" hidden={view !== "pitch"} className="pt-3">
        <canvas
          ref={canvasRef}
          aria-label={t("match.replayPitch")}
          className="h-[68vh] min-h-96 w-full rounded bg-sunken"
        />
        <p className="mt-2 text-caption text-fg-muted" data-testid="replay-players-on-pitch">
          {t("match.replayPlayersOnPitch", {
            home: playersOnPitch.home,
            away: playersOnPitch.away,
            total: playersOnPitch.home + playersOnPitch.away,
          })}
        </p>
      </TabPanel>
      <TabPanel idPrefix={tabsId} tabId="condition" hidden={view !== "condition"} className="pt-3">
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
            minute={Math.floor(time / 60_000)}
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
    </>
  );
}
