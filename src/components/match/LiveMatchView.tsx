import { useTranslation } from "react-i18next";
import type { MutableRefObject } from "react";
import type { InteractiveView } from "../../services/interactiveMatchService";
import { Card, CardBody } from "../ui";
import { EventLog } from "./InteractiveMatchPanels";
import LivePitch from "./LivePitch";
import type { DirectorSnapshot, LiveDirector } from "./liveDirector";
import { ReplayStatsPanel } from "./ReplayPanels";
import type { FeedEntry, statsAt } from "./replayDerived";
import { TEAM_COLOURS } from "./replayPitch";

/** The engine's stoppage reasons that have their own wording. */
const STOPPAGE_REASONS = [
  "ball_out",
  "foul",
  "offside",
  "goal",
  "injury",
  "substitution",
  "period_end",
];

interface LiveMatchViewProps {
  director: LiveDirector;
  snapshot: DirectorSnapshot;
  roster: InteractiveView["roster"];
  shirts: Readonly<Record<string, number>>;
  homeName: string;
  awayName: string;
  /** The goal announced above the pitch, if one has just been scored. */
  goalText: string | null;
  goalX: number | undefined;
  drawRef: MutableRefObject<(() => void) | null>;
  homeAttacksRight: boolean;
  entries: FeedEntry[];
  nameOf: (id: string | undefined) => string;
  stats: ReturnType<typeof statsAt>;
}

/** The moving pitch with its caption, the event feed and the statistics: the match as it is being played. */
export default function LiveMatchView({
  director,
  snapshot,
  roster,
  shirts,
  homeName,
  awayName,
  goalText,
  goalX,
  drawRef,
  homeAttacksRight,
  entries,
  nameOf,
  stats,
}: LiveMatchViewProps) {
  return (
    <>
      {/* The banner's height is always reserved so the pitch does not jump when a goal comes. */}
      <p
        aria-hidden="true"
        className="min-h-11 rounded bg-warning-tint px-3 py-2 text-center font-heading text-heading font-bold tracking-label data-[idle=true]:invisible"
        data-idle={goalText === null}
      >
        {goalText ?? "\u00a0"}
      </p>
      <LivePitch
        director={director}
        roster={roster}
        shirtOf={shirts}
        homeName={homeName}
        awayName={awayName}
        flashGoalX={goalX}
        drawRef={drawRef}
        version={snapshot.version}
      />
      <PitchCaption
        snapshot={snapshot}
        homeName={homeName}
        awayName={awayName}
        homeAttacksRight={homeAttacksRight}
      />
      <div className="grid gap-4 lg:grid-cols-2">
        <EventLog
          entries={entries}
          teamNames={{ home: homeName, away: awayName }}
          nameOf={nameOf}
          minuteLabel={(t_ms) => director.labelAt(t_ms)}
        />
        <Card>
          <CardBody>
            <ReplayStatsPanel
              stats={stats}
              teamNames={{ home: homeName, away: awayName }}
              minute={Math.floor(snapshot.playheadMs / 60_000)}
            />
          </CardBody>
        </Card>
      </div>
    </>
  );
}

interface PitchCaptionProps {
  snapshot: DirectorSnapshot;
  homeName: string;
  awayName: string;
  homeAttacksRight: boolean;
}

/** Who has the ball and which way each team attacks, in words: colour and arrows are never the only signal. */
function PitchCaption({ snapshot, homeName, awayName, homeAttacksRight }: PitchCaptionProps) {
  const { t } = useTranslation();
  const team = snapshot.possessionSide === "away" ? awayName : homeName;
  let situation: string;
  if (snapshot.frameState === "stoppage" && snapshot.stoppage) {
    // A reason a newer engine adds is "play stopped", never a raw key.
    const reason = STOPPAGE_REASONS.includes(snapshot.stoppage) ? snapshot.stoppage : "other";
    situation = t(`match.liveView.stoppage.${reason}`);
  } else if (snapshot.frameState === "kickoff_pending") {
    situation = t("match.liveView.kickoffPending");
  } else if (snapshot.frameState === "in_play" && snapshot.possessionSide) {
    situation = t("match.liveView.hasBall", { team });
  } else if (snapshot.frameState === "in_play") {
    situation = t("match.liveView.looseBall");
  } else {
    situation = "";
  }
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 text-caption text-fg-muted">
      <ul className="flex flex-wrap gap-x-4">
        <li className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className="inline-block size-3 rounded-full"
            style={{ backgroundColor: TEAM_COLOURS.home }}
          />
          {t(homeAttacksRight ? "match.liveView.attacksRight" : "match.liveView.attacksLeft", {
            team: homeName,
          })}
        </li>
        <li className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className="inline-block size-3 rounded-full"
            style={{ backgroundColor: TEAM_COLOURS.away }}
          />
          {t(homeAttacksRight ? "match.liveView.attacksLeft" : "match.liveView.attacksRight", {
            team: awayName,
          })}
        </li>
      </ul>
      <span>{situation}</span>
    </div>
  );
}
