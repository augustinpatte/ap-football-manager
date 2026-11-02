import { useEffect, useState, useCallback, useRef } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useGameStore, type GameStateData } from "../store/gameStore";
import { useSettingsStore } from "../store/settingsStore";
import { resolveBackendKeyError } from "../utils/backendI18n";
import type {
  MatchSnapshot,
  MatchEvent,
  MatchDayStage,
  CupRoundSummary,
  RoundSummary,
} from "../components/match/types";
import { resolveMatchFixture } from "../components/match/helpers";
import { resetMatchCommands } from "../components/match/matchCommands";
import PreMatchSetup from "../components/match/PreMatchSetup";
import MatchLive from "../components/match/MatchLive";
import HalfTimeBreak from "../components/match/HalfTimeBreak";
import PostMatchScreen from "../components/match/PostMatchScreen";
import RoundDigestScreen from "../components/match/RoundDigestScreen";
import PressConference from "../components/match/PressConference";
import PenaltyShootoutScreen from "../components/match/PenaltyShootoutScreen";
import M12ReplayViewer from "../components/match/M12ReplayViewer";
import MatchViewModePanel, { type SpatialAction } from "../components/match/MatchViewModePanel";
import SpectatorViewChoice from "../components/match/SpectatorViewChoice";
import {
  finishSpatialReplay as finishPreparedSpatialReplay,
  loadMatchReplay,
  prepareSpatialReplay,
  simulateSpatialMatch,
} from "../services/matchReplayService";

// ---------------------------------------------------------------------------
// Multi-stage Match Day Orchestrator
// ---------------------------------------------------------------------------

interface MatchRouteState {
  fixtureIndex?: number;
  competitionId?: string;
  fixtureId?: string;
  mode?: string;
  snapshot?: MatchSnapshot;
}

interface FinishLiveMatchResponse {
  game: GameStateData;
  round_summary?: RoundSummary | null;
  cup_round_summaries?: CupRoundSummary[];
}
interface SpatialMatchResult {
  homeGoals: number;
  awayGoals: number;
  events: MatchEvent[];
}

/** A replay opened while the match is still pending (finalised on confirmation) or afterwards. */
interface OpenReplay {
  json: string;
  /** A completed match: closing the viewer must never finalise anything. */
  readOnly: boolean;
}

function snapshotPlayerNames(snapshot: MatchSnapshot): Record<string, string> {
  const everyone = [
    ...snapshot.home_team.players,
    ...snapshot.away_team.players,
    ...snapshot.home_bench,
    ...snapshot.away_bench,
  ];
  return Object.fromEntries(everyone.map((player) => [player.id, player.name]));
}

function snapshotHomePlayerIds(snapshot: MatchSnapshot): ReadonlySet<string> {
  return new Set([...snapshot.home_team.players, ...snapshot.home_bench].map((p) => p.id));
}

export default function MatchSimulation() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const routeState = (location.state as MatchRouteState | null) ?? null;
  const matchMode = routeState?.mode || "live";
  const { gameState, setGameState } = useGameStore();
  const { settings } = useSettingsStore();
  const [snapshot, setSnapshot] = useState<MatchSnapshot | null>(routeState?.snapshot ?? null);
  const [stage, setStage] = useState<MatchDayStage>("prematch");
  const [importantEvents, setImportantEvents] = useState<MatchEvent[]>([]);
  const [spatialResult, setSpatialResult] = useState<SpatialMatchResult | null>(null);
  const [spatialReplay, setSpatialReplay] = useState<OpenReplay | null>(null);
  const [replayRef, setReplayRef] = useState<{ competitionId: string; fixtureId: string } | null>(
    null,
  );
  const [hasStoredReplay, setHasStoredReplay] = useState(false);
  const [userSide, setUserSide] = useState<"Home" | "Away" | null>(null);
  const [isSpectator, setIsSpectator] = useState(matchMode === "spectator");
  const [roundSummary, setRoundSummary] = useState<RoundSummary | null>(null);
  const [cupRoundSummaries, setCupRoundSummaries] = useState<CupRoundSummary[]>([]);
  const [hasFinalizedMatch, setHasFinalizedMatch] = useState(false);
  const [preferredSpeed, setPreferredSpeed] = useState<"slow" | "normal" | "fast">("normal");
  const [hasUserOverriddenSpeed, setHasUserOverriddenSpeed] = useState(false);
  const [spatialAction, setSpatialAction] = useState<SpatialAction | null>(null);
  const [spatialError, setSpatialError] = useState<string | null>(null);
  const spatialActionInFlight = useRef(false);

  // Each match screen is a new live session; commands from an earlier one must not hold it up.
  useEffect(() => {
    resetMatchCommands();
  }, []);

  useEffect(() => {
    console.info("[MatchSimulation] mount", {
      fixtureIndex: routeState?.fixtureIndex,
      hasGameState: !!gameState,
      hasRouteSnapshot: !!routeState?.snapshot,
      matchMode,
    });
  }, [gameState, matchMode, routeState?.fixtureIndex, routeState?.snapshot]);

  useEffect(() => {
    if (hasUserOverriddenSpeed) return;
    setPreferredSpeed(
      settings.match_speed === "slow" || settings.match_speed === "fast"
        ? settings.match_speed
        : "normal",
    );
  }, [settings.match_speed, hasUserOverriddenSpeed]);

  // Determine user side from game state
  useEffect(() => {
    if (!gameState || !snapshot) return;
    const utid = gameState.manager.team_id;
    if (!utid) {
      setIsSpectator(true);
      return;
    }
    if (snapshot.home_team.id === utid) setUserSide("Home");
    else if (snapshot.away_team.id === utid) setUserSide("Away");
    else setIsSpectator(true);

    // If mode is spectator, force spectator regardless of team
    if (matchMode === "spectator") setIsSpectator(true);

    console.info("[MatchSimulation] resolveSide", {
      awayTeamId: snapshot.away_team.id,
      homeTeamId: snapshot.home_team.id,
      matchMode,
      managerTeamId: utid,
      resolvedUserSide:
        snapshot.home_team.id === utid ? "Home" : snapshot.away_team.id === utid ? "Away" : null,
    });
  }, [gameState, snapshot?.home_team.id, snapshot?.away_team.id, matchMode]);

  // Fetch initial snapshot
  useEffect(() => {
    // Don't refetch after finalize — finish_live_match has cleared the backend
    // session, so get_match_snapshot would fail and the catch path would
    // start_live_match a fresh 0-0 session, clobbering the final score.
    if (hasFinalizedMatch) return;
    let isCancelled = false;

    const fetchSnapshot = async () => {
      console.info("[MatchSimulation] fetchSnapshot:start", {
        fixtureIndex: routeState?.fixtureIndex,
        hasRouteSnapshot: !!routeState?.snapshot,
        matchMode,
      });
      try {
        const snap = await invoke<MatchSnapshot | null>("get_match_snapshot");
        if (!snap) throw new Error("No active match snapshot");
        console.info("[MatchSimulation] fetchSnapshot:success", {
          awayPlayers: snap.away_team.players.length,
          awayTeam: snap.away_team.name,
          homePlayers: snap.home_team.players.length,
          homeTeam: snap.home_team.name,
          phase: snap.phase,
        });
        if (!isCancelled) {
          setSnapshot(snap);
        }
        return;
      } catch (snapshotError) {
        console.warn("[MatchSimulation] fetchSnapshot:failed", snapshotError);
        if (
          typeof routeState?.fixtureIndex !== "number" ||
          !routeState.competitionId ||
          !routeState.fixtureId
        ) {
          console.error("Failed to get match snapshot:", snapshotError);
          navigate("/dashboard");
          return;
        }

        try {
          console.info("[MatchSimulation] restoreLiveMatch:start", {
            fixtureIndex: routeState.fixtureIndex,
            matchMode,
          });
          // The raw index may point into a cup while game.league mirrors the
          // domestic league after a restart. Use stable fixture identity.
          // The backend derives extra-time eligibility from the fixture; the
          // argument is still required by the command signature.
          const restoredSnapshot = await invoke<MatchSnapshot>("start_live_match", {
            allowsExtraTime: false,
            fixtureIndex: routeState.fixtureIndex,
            competitionId: routeState.competitionId,
            fixtureId: routeState.fixtureId,
            mode: matchMode,
          });

          console.info("[MatchSimulation] restoreLiveMatch:success", {
            awayPlayers: restoredSnapshot.away_team.players.length,
            awayTeam: restoredSnapshot.away_team.name,
            homePlayers: restoredSnapshot.home_team.players.length,
            homeTeam: restoredSnapshot.home_team.name,
            phase: restoredSnapshot.phase,
          });

          if (!isCancelled) {
            setSnapshot(restoredSnapshot);
          }
        } catch (restoreError) {
          console.error("Failed to restore live match session:", restoreError);
          navigate("/dashboard");
        }
      }
    };

    fetchSnapshot();

    return () => {
      isCancelled = true;
    };
  }, [
    hasFinalizedMatch,
    gameState,
    matchMode,
    navigate,
    routeState?.fixtureIndex,
    routeState?.snapshot,
  ]);

  // Callbacks for stage transitions
  const handleStartMatch = useCallback(() => {
    console.info("[MatchSimulation] handleStartMatch");
    setStage("first_half");
  }, []);
  // The stable identity of the fixture being played, kept so its stored replay can be reopened
  // after the live session (and with it the route state) is gone.
  const identifyFixture = useCallback(() => {
    const fixture = resolveMatchFixture(gameState, snapshot, routeState ?? undefined);
    const competitionId = routeState?.competitionId ?? fixture?.competition_id;
    const fixtureId = routeState?.fixtureId ?? fixture?.id;
    return competitionId && fixtureId ? { competitionId, fixtureId } : null;
  }, [gameState, snapshot, routeState]);
  const beginSpatialAction = useCallback(
    (action: SpatialAction): boolean => {
      if (spatialActionInFlight.current || hasFinalizedMatch) return false;
      spatialActionInFlight.current = true;
      setSpatialAction(action);
      setSpatialError(null);
      return true;
    },
    [hasFinalizedMatch],
  );

  const endSpatialAction = useCallback(() => {
    spatialActionInFlight.current = false;
    setSpatialAction(null);
  }, []);

  const handleSpatialSimulation = useCallback(async () => {
    if (!beginSpatialAction("simulating")) return;
    try {
      const response = await simulateSpatialMatch<GameStateData, MatchEvent>();
      setGameState(response.game);
      setImportantEvents(response.events);
      setSpatialResult({
        homeGoals: response.home_goals,
        awayGoals: response.away_goals,
        events: response.events,
      });
      setReplayRef(identifyFixture());
      setHasStoredReplay(true);
      setHasFinalizedMatch(true);
      setStage("postmatch");
    } catch (error) {
      console.error("M12 simulation failed", error);
      setSpatialError(resolveBackendKeyError(error, t("match.m12SimulationFailed")));
    } finally {
      endSpatialAction();
    }
  }, [beginSpatialAction, endSpatialAction, identifyFixture, setGameState, t]);

  const handleSpatialReplay = useCallback(async () => {
    if (!beginSpatialAction("preparing")) return;
    try {
      const response = await prepareSpatialReplay();
      setSpatialReplay({ json: response.replay_json, readOnly: false });
    } catch (error) {
      console.error("M12 replay preparation failed", error);
      // A match that could not be prepared is not a corrupted replay: say what was refused.
      setSpatialError(resolveBackendKeyError(error, t("match.replayPreparationFailed")));
    } finally {
      endSpatialAction();
    }
  }, [beginSpatialAction, endSpatialAction, t]);

  const finishSpatialReplay = useCallback(async () => {
    if (!beginSpatialAction("finalizing")) return;
    try {
      const response = await finishPreparedSpatialReplay<GameStateData, MatchEvent>();
      setGameState(response.game);
      setImportantEvents(response.events);
      setSpatialResult({
        homeGoals: response.home_goals,
        awayGoals: response.away_goals,
        events: response.events,
      });
      setReplayRef(identifyFixture());
      setHasStoredReplay(true);
      setHasFinalizedMatch(true);
      setSpatialReplay(null);
      setStage("postmatch");
    } catch (error) {
      console.error("M12 replay finalization failed", error);
      setSpatialError(t("match.m12SimulationFailed"));
      throw error;
    } finally {
      endSpatialAction();
    }
  }, [beginSpatialAction, endSpatialAction, identifyFixture, setGameState, t]);

  // Reads the stored document only: the backend never simulates or finalises here.
  const handleWatchStoredReplay = useCallback(async () => {
    if (!replayRef || spatialActionInFlight.current) return;
    spatialActionInFlight.current = true;
    setSpatialError(null);
    try {
      const stored = await loadMatchReplay(replayRef.competitionId, replayRef.fixtureId);
      if (stored) setSpatialReplay({ json: stored.replay_json, readOnly: true });
      else setSpatialError(t("match.replayNotRecorded"));
    } catch (error) {
      console.error("Stored replay could not be read", error);
      setSpatialError(t("match.replayUnavailable"));
    } finally {
      spatialActionInFlight.current = false;
    }
  }, [replayRef, t]);

  const handleHalfTime = useCallback((phase: "HalfTime" | "ExtraTimeHalfTime") => {
    console.info("[MatchSimulation] handleHalfTime", { phase });
    if (phase === "ExtraTimeHalfTime") {
      setStage("extra_time_halftime");
    } else {
      setStage("halftime");
    }
  }, []);

  const handleResumeFromHalfTime = useCallback(() => {
    console.info("[MatchSimulation] handleResumeFromHalfTime", { stage });
    setStage(stage === "extra_time_halftime" ? "extra_time_second_half" : "second_half");
  }, [stage]);

  const handlePenaltyShootout = useCallback(() => {
    console.info("[MatchSimulation] handlePenaltyShootout");
    setStage("penalty_shootout");
  }, []);

  const finalizeMatch = useCallback(async (): Promise<boolean> => {
    if (hasFinalizedMatch) {
      return true;
    }

    try {
      console.info("[MatchSimulation] finalizeMatch:start");
      const response = await invoke<FinishLiveMatchResponse>("finish_live_match");
      console.info("[MatchSimulation] finalizeMatch:success", {
        hasRoundSummary: !!response.round_summary,
        hasUpdatedGame: !!response.game,
      });
      setGameState(response.game);
      setRoundSummary(response.round_summary ?? null);
      setCupRoundSummaries(response.cup_round_summaries ?? []);
      setHasFinalizedMatch(true);
      return true;
    } catch (err) {
      console.error("Failed to finish match:", err);
      return false;
    }
  }, [hasFinalizedMatch, setGameState]);

  const handleFullTime = useCallback(() => {
    console.info("[MatchSimulation] handleFullTime");
    void (async () => {
      const finalized = await finalizeMatch();
      if (finalized) {
        setStage("postmatch");
      }
    })();
  }, [finalizeMatch]);

  const handleFinishMatch = useCallback(async () => {
    console.info("[MatchSimulation] handleFinishMatch:start");
    const finalized = await finalizeMatch();
    if (finalized) {
      navigate("/dashboard");
    }
  }, [finalizeMatch, navigate]);

  const handlePostMatchContinue = useCallback(() => {
    if (isSpectator) {
      void handleFinishMatch();
      return;
    }
    console.info("[MatchSimulation] handlePostMatchContinue");
    setStage("digest");
  }, [isSpectator, handleFinishMatch]);

  const handlePressConference = useCallback(() => {
    console.info("[MatchSimulation] handlePressConference");
    setStage("press");
  }, []);

  // Deliberately unlogged: this fires once per simulated minute, and the effect that used to log
  // here ran on every tick for the whole match. The remaining logs are the low-frequency ones —
  // stage transitions and match lifecycle — not the per-minute path.
  const handleSnapshotUpdate = useCallback((snap: MatchSnapshot) => {
    setSnapshot(snap);
  }, []);

  const handleImportantEvent = useCallback((evt: MatchEvent) => {
    console.info("[MatchSimulation] handleImportantEvent", {
      eventType: evt.event_type,
      minute: evt.minute,
      side: evt.side,
    });
    setImportantEvents((prev) => [...prev, evt]);
  }, []);

  const handlePreferredSpeedChange = useCallback((speed: "slow" | "normal" | "fast") => {
    setHasUserOverriddenSpeed(true);
    setPreferredSpeed(speed);
  }, []);

  // Loading state
  if (!snapshot || !gameState) {
    return (
      <div className="min-h-screen bg-gray-100 dark:bg-navy-900 flex items-center justify-center transition-colors duration-300">
        <div className="flex flex-col items-center gap-3">
          <div className="w-8 h-8 border-4 border-primary-500 border-t-transparent rounded-full animate-spin" />
          <span className="text-gray-500 dark:text-gray-400 font-heading uppercase tracking-wider text-sm">
            {t("dashboard.loading")}
          </span>
        </div>
      </div>
    );
  }

  const currentFixture = resolveMatchFixture(gameState, snapshot, routeState ?? undefined);
  if (spatialReplay) {
    const closeReplay = () => setSpatialReplay(null);
    return (
      <M12ReplayViewer
        replayJson={spatialReplay.json}
        readOnly={spatialReplay.readOnly}
        onFinish={spatialReplay.readOnly ? closeReplay : finishSpatialReplay}
        onBack={closeReplay}
        playerNames={snapshotPlayerNames(snapshot)}
        homePlayerIds={spatialReplay.readOnly ? undefined : snapshotHomePlayerIds(snapshot)}
      />
    );
  }

  // Render the current stage
  switch (stage) {
    case "prematch":
      if (isSpectator) {
        return (
          <SpectatorViewChoice
            snapshot={snapshot}
            busy={spatialAction}
            error={spatialError}
            onPlayLive={handleStartMatch}
            onWatchIn2d={() => void handleSpatialReplay()}
            onQuickSimulation={() => void handleSpatialSimulation()}
          />
        );
      }
      if (!userSide) return null;
      return (
        <PreMatchSetup
          snapshot={snapshot}
          gameState={gameState}
          currentFixture={currentFixture}
          userSide={userSide}
          onStart={handleStartMatch}
          onUpdateSnapshot={handleSnapshotUpdate}
          viewModeSlot={
            <MatchViewModePanel
              busy={spatialAction}
              error={spatialError}
              onWatchIn2d={() => void handleSpatialReplay()}
              onQuickSimulation={() => void handleSpatialSimulation()}
            />
          }
        />
      );

    case "first_half":
    case "second_half":
    case "extra_time_second_half":
      return (
        <MatchLive
          key={stage}
          snapshot={snapshot}
          gameState={gameState}
          userSide={userSide}
          isSpectator={isSpectator}
          importantEvents={importantEvents}
          preferredSpeed={preferredSpeed}
          onPreferredSpeedChange={handlePreferredSpeedChange}
          onSnapshotUpdate={handleSnapshotUpdate}
          onImportantEvent={handleImportantEvent}
          onHalfTime={handleHalfTime}
          onFullTime={handleFullTime}
          onPenaltyShootout={handlePenaltyShootout}
        />
      );

    case "halftime":
    case "extra_time_halftime":
      if (!userSide) return null;
      return (
        <HalfTimeBreak
          key={stage}
          snapshot={snapshot}
          gameState={gameState}
          userSide={userSide}
          isSpectator={isSpectator}
          importantEvents={importantEvents}
          onResume={handleResumeFromHalfTime}
          onUpdateSnapshot={handleSnapshotUpdate}
        />
      );

    case "penalty_shootout":
      return (
        <PenaltyShootoutScreen
          snapshot={snapshot}
          gameState={gameState}
          userSide={userSide}
          isSpectator={isSpectator}
          importantEvents={importantEvents}
          onSnapshotUpdate={handleSnapshotUpdate}
          onImportantEvent={handleImportantEvent}
          onFullTime={handleFullTime}
        />
      );

    case "postmatch":
      return (
        <>
          {spatialError && (
            <p className="bg-danger-tint px-6 py-2 text-body text-danger" role="alert">
              {spatialError}
            </p>
          )}
          <PostMatchScreen
            snapshot={snapshot}
            gameState={gameState}
            userSide={userSide}
            isSpectator={isSpectator}
            importantEvents={importantEvents}
            spatialResult={spatialResult}
            onContinue={handlePostMatchContinue}
            onFinish={handleFinishMatch}
            onWatchReplay={
              hasStoredReplay && replayRef ? () => void handleWatchStoredReplay() : undefined
            }
          />
        </>
      );

    case "digest": {
      return (
        <RoundDigestScreen
          snapshot={snapshot}
          gameState={gameState}
          currentFixture={currentFixture}
          userSide={userSide}
          roundSummary={roundSummary}
          cupRoundSummaries={cupRoundSummaries}
          onPressConference={handlePressConference}
          onFinish={handleFinishMatch}
        />
      );
    }

    case "press":
      if (!userSide) return null;
      return (
        <PressConference
          snapshot={snapshot}
          gameState={gameState}
          userSide={userSide}
          onFinish={handleFinishMatch}
          onGameUpdate={setGameState}
        />
      );

    default:
      return null;
  }
}
