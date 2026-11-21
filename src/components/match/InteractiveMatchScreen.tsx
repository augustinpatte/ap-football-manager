import { useCallback, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardBody } from "../ui";
import DashboardCloseConfirmModal from "../dashboard/DashboardCloseConfirmModal";
import type { MatchKeyEvent } from "../../services/interactiveMatchService";
import { EventLog, SubstitutionPanel, TacticsPanel, TeamLineup } from "./InteractiveMatchPanels";
import { LiveControls, LiveScoreboard } from "./LiveMatchChrome";
import LivePitch from "./LivePitch";
import { playerNames, playerShirts } from "./interactiveMatch.helpers";
import type { DirectorSnapshot } from "./liveDirector";
import { goalLineNearest, homeAttacksPositiveGoal } from "./livePlayback";
import { ReplayStatsPanel } from "./ReplayPanels";
import { type FeedEntry, feedEntryOf, statsAt } from "./replayDerived";
import { TEAM_COLOURS } from "./replayPitch";
import { useLiveMatch } from "./useLiveMatch";
import { useMatchSave } from "./useMatchSave";

/** How long a goal stays announced above the pitch, in match time. */
const GOAL_BANNER_MS = 8_000;
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

interface InteractiveMatchScreenProps<TFinish> {
  homeName: string;
  awayName: string;
  /** Receives what the career's own finalisation returned once the result has been applied. */
  onFinished: (response: TFinish) => void;
  /** The career was saved and cleared for the main menu: leave the match screen. */
  onExited: () => void;
}

/** The goal the picture has just shown, if it is still being announced. */
function recentGoal(snapshot: DirectorSnapshot): MatchKeyEvent | null {
  const goals = snapshot.events.filter((event) => event.type === "goal");
  const last = goals[goals.length - 1];
  return last && snapshot.playheadMs - last.t_ms <= GOAL_BANNER_MS ? last : null;
}

/**
 * Watch and manage a real match on a 2D pitch. The pitch, clock, score and feed are the engine's own
 * data as of the instant the picture shows; this screen predicts nothing. The engine runs a little
 * ahead of the picture, so orders are only possible once the picture has caught up with it (the
 * Pause button does that), and an accepted order only ever changes what has not been produced yet.
 */
export default function InteractiveMatchScreen<TFinish>({
  homeName,
  awayName,
  onFinished,
  onExited,
}: InteractiveMatchScreenProps<TFinish>) {
  const { t } = useTranslation();
  const live = useLiveMatch<TFinish>(onFinished);
  const { director, snapshot, notice, drawRef } = live;
  const save = useMatchSave({
    enqueue: live.enqueue,
    unsaved: live.unsaved,
    progressAtRisk: live.progressAtRisk,
    markSaved: live.markSaved,
    markLeaving: live.markLeaving,
    halt: () => director.halt(),
    onExited,
  });
  const view = snapshot.view;
  const session = view?.session;

  const names = useMemo(() => (view ? playerNames(view) : {}), [view]);
  const shirts = useMemo(() => (view ? playerShirts(view) : {}), [view]);
  const nameOf = useCallback(
    (id: string | undefined) => (id ? (names[id] ?? t("match.replayUnknownPlayer")) : ""),
    [names, t],
  );
  // The feed and the statistics are counted up to the picture's instant, so they never run ahead of it.
  const entries = useMemo(
    () =>
      snapshot.events
        .map((event) => feedEntryOf(event))
        .filter((entry): entry is FeedEntry => entry !== null),
    [snapshot.events],
  );
  const stats = useMemo(() => {
    const { source, homeIds } = director.statsSource();
    return statsAt(source, snapshot.playheadMs, { score: snapshot.score }, homeIds);
  }, [director, snapshot.version, snapshot.playheadMs, snapshot.score]);

  const finished = snapshot.status === "finished";
  // When the match ends the Play button disables itself: move focus to the next useful control.
  useEffect(() => {
    // A focused control that has just disabled itself no longer holds focus in every engine.
    const active = document.activeElement;
    if (finished && (active === document.body || (active as HTMLButtonElement | null)?.disabled)) {
      document.getElementById("interactive-finish")?.focus();
    }
  }, [finished]);

  if (!view || !session) {
    return (
      <main className="min-h-screen bg-page p-6">
        <h1 className="sr-only">{t("match.interactive.heading")}</h1>
        {notice ? (
          <p role="alert" className="text-body text-danger">
            {notice}
          </p>
        ) : (
          <p className="text-body text-fg-muted">{t("match.liveView.status.loading")}</p>
        )}
      </main>
    );
  }

  const goal = recentGoal(snapshot);
  const goalText = goal
    ? t("match.liveView.goalBanner", {
        player: nameOf(goal.scorer),
        minute: director.labelAt(goal.t_ms),
      })
    : null;
  const goalX = goal?.position ? goalLineNearest(goal.position.x) : undefined;
  const mine = view.user_side === "away" ? session.away : session.home;
  const myPlan = view.user_side === "away" ? view.away_plan : view.home_plan;
  const controllable = view.user_side !== null && !finished;
  const homeAttacksRight = snapshot.period !== null && homeAttacksPositiveGoal(snapshot.period);

  return (
    <main className="min-h-screen bg-page text-fg">
      <LiveScoreboard
        homeName={homeName}
        awayName={awayName}
        snapshot={snapshot}
        announcement={goalText}
      />
      <LiveControls
        snapshot={snapshot}
        view={view}
        finishing={live.finishing}
        save={{
          status: save.status,
          busy: save.busy,
          onSave: save.save,
          onSaveAndExit: save.saveAndExit,
        }}
        onTogglePlay={live.togglePlay}
        onSkip={() => void live.skipAhead()}
        onSpeed={(speed) => director.setSpeed(speed)}
        onMode={(mode) => director.setMode(mode)}
        onAssistant={(active) => void live.toggleAssistant(active)}
        onFinish={() => void live.finish()}
      />

      <p role="note" className="bg-sunken px-6 py-2 text-body text-fg-muted">
        {t("match.interactive.liveBadge")}
      </p>
      {finished && (
        <p className="bg-positive-tint px-6 py-2 text-body text-positive">
          {t("match.interactive.fullTimeHint")}
        </p>
      )}
      {notice && (
        <p role="alert" className="bg-danger-tint px-6 py-2 text-body text-danger">
          {notice}
        </p>
      )}
      {save.error && (
        <p role="alert" className="bg-danger-tint px-6 py-2 text-body text-danger">
          {save.error}
        </p>
      )}
      {save.closePrompt && (
        <DashboardCloseConfirmModal
          onCancel={save.dismissClosePrompt}
          onQuitWithoutSave={() => save.closeWindow(false)}
          onSaveAndQuit={() => save.closeWindow(true)}
        />
      )}
      {snapshot.rejectedFrames > 0 && (
        <p className="bg-warning-tint px-6 py-2 text-body text-fg">
          {t("match.liveView.incompleteData")}
        </p>
      )}

      <div className="grid gap-4 p-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <div className="flex min-w-0 flex-col gap-3">
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
            roster={view.roster}
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
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          {controllable && !snapshot.canOrder && (
            <p className="rounded bg-sunken px-3 py-2 text-body text-fg-muted" role="note">
              {snapshot.status === "pausing"
                ? t("match.liveView.status.pausing")
                : t("match.liveView.pauseToAct")}
            </p>
          )}
          {controllable && (
            <TacticsPanel
              plan={myPlan}
              supportedFormations={view.supported_formations}
              disabled={!snapshot.canOrder}
              onApply={(change) => void live.giveOrder({ Tactics: change })}
            />
          )}
          {controllable && (
            <SubstitutionPanel
              team={mine}
              names={names}
              disabled={!snapshot.canOrder}
              onSubstitute={(off, on) =>
                void live.giveOrder({ Substitute: { player_off_id: off, player_on_id: on } })
              }
              onCancel={(off) =>
                void live.giveOrder({ CancelSubstitution: { player_off_id: off } })
              }
            />
          )}
          <TeamLineup name={homeName} team={session.home} />
          <TeamLineup name={awayName} team={session.away} />
        </div>
      </div>
    </main>
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
