import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { resolveBackendKeyError } from "../utils/backendI18n";
import type { GameStateData } from "../store/gameStore";
import { useGameStore } from "../store/gameStore";
import type { BlockerModal } from "./useAdvanceTime.helpers";
import {
  acknowledgeContractWarnings,
  advanceTimeWithMode,
  type BlockerData,
  checkBlockingActions,
  skipToMatchDay,
  type SkipToMatchDayResponse,
} from "../services/advanceTimeService";
import { buildDigestEntries, toDatePart } from "../components/dashboard/advanceRecap";
import { useDigestAdvance } from "./useDigestAdvance";
import type { DigestEntry, DigestStopReason } from "./useDigestAdvance";

export type MatchModeType = "live" | "spectator" | "delegate";

export interface AdvanceTimeState {
  isAdvancing: boolean;
  showContinueMenu: boolean;
  setShowContinueMenu: (v: boolean) => void;
  showMatchConfirm: boolean;
  setShowMatchConfirm: (v: boolean) => void;
  matchMode: MatchModeType;
  setMatchMode: (v: MatchModeType) => void;
  blockerModal: BlockerModal | null;
  setBlockerModal: (v: BlockerModal | null) => void;
  /** Why nothing was advanced when the blocker check itself could not be answered, translated. */
  advanceError: string | null;
  handleContinue: (mode?: string) => Promise<void>;
  handleConfirmMatch: () => void;
  handleSkipToMatchDay: () => Promise<void>;
  // Digest feed state — every advance flow presents through this feed.
  digestEntries: DigestEntry[];
  digestStopReason: DigestStopReason | null;
  isDigestVisible: boolean;
  isDigestRunning: boolean;
  isDigestAborting: boolean;
  abortDigest: () => void;
  dismissDigest: () => void;
  /** Resume the streaming digest after an attention-event pause. */
  resumeDigest: () => void;
  /** Re-run the flow that hit a mid-advance blocker ("Continue Anyway"). */
  resumeAfterBlocker: () => void;
}

export function useAdvanceTime(
  setGameState: (state: GameStateData) => void,
  hasMatchToday: boolean,
  defaultMatchMode: MatchModeType | undefined,
  settingsLoaded: boolean,
  isUnemployed: boolean,
  continueToNextEvent: boolean = false,
): AdvanceTimeState {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [advanceError, setAdvanceError] = useState<string | null>(null);
  const setShowFiredModal = useGameStore((s) => s.setShowFiredModal);
  const [isAdvancing, setIsAdvancing] = useState(false);
  const [showContinueMenu, setShowContinueMenu] = useState(false);
  const [showMatchConfirm, setShowMatchConfirm] = useState(false);
  const [matchMode, setMatchMode] = useState<MatchModeType>("live");
  const [blockerModal, setBlockerModal] = useState<BlockerModal | null>(null);
  // How to carry on when the digest stops on a mid-advance blocker — depends
  // on which flow (streaming Continue vs batch Skip) hit it.
  const resumeAfterBlockerRef = useRef<() => void>(() => {});
  // Snapshot of the current feed for appends after a blocked batch resume;
  // a ref because runMultiDayAdvance closes over a stale `digestEntries`.
  const digestEntriesRef = useRef<DigestEntry[]>([]);
  // Re-entrancy guard for the backend advances (mirrors useDigestAdvance's
  // inFlightRef). A ref, not `isAdvancing`: a double-click on "Continue
  // Anyway" lands before the state commit re-renders, so both clicks would
  // read the stale false and fire two concurrent backend calls.
  const advanceInFlightRef = useRef(false);

  const {
    isRunning: isDigestRunning,
    isAborting: isDigestAborting,
    entries: digestEntries,
    stopReason: digestStopReason,
    isVisible: isDigestVisible,
    startDigest,
    showStaticDigest,
    abortDigest,
    dismissDigest,
  } = useDigestAdvance(setGameState, () => setShowFiredModal(true));

  // Sync matchMode with settings when loaded
  useEffect(() => {
    if (settingsLoaded && defaultMatchMode) {
      setMatchMode(defaultMatchMode);
    }
  }, [settingsLoaded, defaultMatchMode]);

  // Synced post-commit: writing the ref during render could capture entries
  // from a render React discarded.
  useEffect(() => {
    digestEntriesRef.current = digestEntries;
  }, [digestEntries]);

  function resetTransientUi(options?: {
    showContinueMenu?: boolean;
    showMatchConfirm?: boolean;
    blockerModal?: BlockerModal | null;
  }): void {
    setShowContinueMenu(options?.showContinueMenu ?? false);
    setShowMatchConfirm(options?.showMatchConfirm ?? false);
    setBlockerModal(options?.blockerModal ?? null);
  }

  // `null` means the check could not be answered: the caller must stop, not treat it as "clear".
  const verifyNoBlockers = async (logContext: string): Promise<BlockerData[] | null> => {
    setAdvanceError(null);
    try {
      return await checkBlockingActions(logContext);
    } catch (err) {
      setAdvanceError(resolveBackendKeyError(err, t("notifications.blockers.checkFailed")));
      return null;
    }
  };

  const runStreamingDigest = (options?: { resume?: boolean; ignoreBlockers?: boolean }) => {
    resumeAfterBlockerRef.current = () => void startDigest({ resume: true, ignoreBlockers: true });
    void startDigest(options);
  };

  // "Continue anyway" on a blocker list: the contract warnings in it are dealt with, so they
  // are not raised again the next day, then the flow carries on. A failure to record it must
  // not trap the manager behind the blocker, so it is logged and the flow carries on.
  const continueAnyway = async (run: () => void): Promise<void> => {
    try {
      await acknowledgeContractWarnings();
    } catch (err) {
      console.warn("[useAdvanceTime] continueAnyway:acknowledgeFailed", err);
    }
    run();
  };

  const doAdvance = async (effectiveMode: string, ignoreBlockers = false) => {
    if (advanceInFlightRef.current) return;
    advanceInFlightRef.current = true;
    console.info("[useAdvanceTime] doAdvance:start", {
      effectiveMode,
      hasMatchToday,
      matchMode,
    });
    setIsAdvancing(true);
    setAdvanceError(null);
    resetTransientUi();
    // Any lingering feed belongs to a previous advance.
    dismissDigest();
    // Clock date before advancing — the cursor for "what happened" in the digest.
    const sinceDate = toDatePart(useGameStore.getState().gameState?.clock?.current_date);
    try {
      const result = await advanceTimeWithMode(effectiveMode, ignoreBlockers);
      console.info("[useAdvanceTime] doAdvance:result", {
        action: result.action,
        fixtureIndex: result.fixture_index,
        mode: result.mode || effectiveMode,
        hasGame: !!result.game,
        hasSnapshot: !!result.snapshot,
      });
      if (result.action === "blocked") {
        // The backend refused the day (a blocker appeared after the check, or none was made):
        // the day was not played, and "continue anyway" asks it to play past them.
        setBlockerModal({
          blockers: result.blockers ?? [],
          pendingAction: () => void continueAnyway(() => void doAdvance(effectiveMode, true)),
        });
      } else if (result.action === "fired") {
        if (result.game) setGameState(result.game as GameStateData);
        setShowFiredModal(true);
      } else if (result.action === "live_match") {
        navigate("/match", {
          state: {
            fixtureIndex: result.fixture_index,
            competitionId: result.competition_id,
            fixtureId: result.fixture_id,
            mode: result.mode || effectiveMode,
            snapshot: result.snapshot,
          },
        });
      } else if (result.action === "advanced" && result.game) {
        const game = result.game as GameStateData;
        setGameState(game);
        showStaticDigest(buildDigestEntries(game, sinceDate, result.results ?? []), null);
      }
    } catch (err) {
      console.error("Failed to advance time:", err);
      setAdvanceError(resolveBackendKeyError(err, t("notifications.blockers.advanceFailed")));
    } finally {
      console.info("[useAdvanceTime] doAdvance:complete", { effectiveMode });
      advanceInFlightRef.current = false;
      setIsAdvancing(false);
    }
  };

  const handleContinue = async (mode?: string) => {
    const effectiveMode = mode || matchMode;
    const resolvedMode = isUnemployed ? "delegate" : effectiveMode;
    console.info("[useAdvanceTime] handleContinue", {
      effectiveMode: resolvedMode,
      hasMatchToday,
      isAdvancing,
      matchMode,
      showMatchConfirm,
    });
    // If there's a match today, show confirmation modal first
    if (hasMatchToday && !showMatchConfirm) {
      console.info("[useAdvanceTime] handleContinue:showMatchConfirm", {
        effectiveMode: resolvedMode,
      });
      if (mode) setMatchMode(mode as MatchModeType);
      resetTransientUi({ showMatchConfirm: true });
      return;
    }
    if (isAdvancing) return;
    // With the opt-in setting, Continue runs the day-by-day digest loop (which
    // pauses on attention events) instead of the single-day advance.
    const runContinue = continueToNextEvent
      ? () => runStreamingDigest()
      : () => doAdvance(resolvedMode);
    const blockers = await verifyNoBlockers("handleContinue");
    if (blockers === null) return;
    if (blockers.length > 0) {
      setBlockerModal({
        blockers,
        // The day-by-day run asks the backend again, which would block on the same list.
        pendingAction: continueToNextEvent
          ? () => runStreamingDigest({ ignoreBlockers: true })
          : () => void continueAnyway(() => void doAdvance(resolvedMode, true)),
      });
      return;
    }
    runContinue();
  };

  const handleConfirmMatch = () => {
    console.info("[useAdvanceTime] handleConfirmMatch", { matchMode });
    doAdvance(matchMode);
  };

  const handleSkipToMatchDay = async () => {
    if (isAdvancing) return;
    console.info("[useAdvanceTime] handleSkipToMatchDay:start");
    const blockers = await verifyNoBlockers("handleSkipToMatchDay");
    if (blockers === null) return;
    if (blockers.length > 0) {
      setBlockerModal({
        blockers,
        pendingAction: () =>
          void continueAnyway(() => void doSkipToMatchDay({ ignoreBlockers: true })),
      });
      return;
    }
    doSkipToMatchDay();
  };

  // Shared driver for the batch multi-day advances: roll forward in one
  // backend call, then present the processed days through the same digest
  // feed the streaming loop fills. `append` keeps the existing feed when the
  // run is a continuation after a mid-advance blocker.
  const runMultiDayAdvance = async (
    run: () => Promise<SkipToMatchDayResponse>,
    label: string,
    options?: { append?: boolean },
  ) => {
    // Guards the direct resumeAfterBlocker path too, which skips the
    // isAdvancing check in handleSkipToMatchDay.
    if (advanceInFlightRef.current) return;
    advanceInFlightRef.current = true;
    setIsAdvancing(true);
    setAdvanceError(null);
    resetTransientUi();
    // Drop the stop footer while the batch crunches: a resumed run keeps its
    // feed (the new days append to it), a fresh run starts from the spinner.
    if (options?.append) {
      showStaticDigest(digestEntriesRef.current, null);
    } else {
      dismissDigest();
    }
    const sinceDate = toDatePart(useGameStore.getState().gameState?.clock?.current_date);
    try {
      const result = await run();
      console.info(`[useAdvanceTime] ${label}:result`, {
        action: result.action,
        daysSkipped: result.days_skipped,
        blockerCount: result.blockers?.length ?? 0,
        hasGame: !!result.game,
      });
      const game = result.game as GameStateData | undefined;
      if (game) setGameState(game);
      if (!game) return;

      const entries = buildDigestEntries(game, sinceDate, result.results ?? []);
      const merged = options?.append ? [...digestEntriesRef.current, ...entries] : entries;
      let stopReason: DigestStopReason | null = null;
      if (result.action === "fired") {
        stopReason = { kind: "fired" };
        setShowFiredModal(true);
      } else if (result.action === "blocked") {
        stopReason = { kind: "blocked", blockers: result.blockers ?? [] };
      } else {
        // "arrived": the skip landed on the user's match day.
        stopReason = { kind: "match_day" };
      }
      showStaticDigest(merged, stopReason);
    } catch (err) {
      console.error(`Failed to ${label}:`, err);
      setAdvanceError(resolveBackendKeyError(err, t("notifications.blockers.advanceFailed")));
    } finally {
      console.info(`[useAdvanceTime] ${label}:complete`);
      advanceInFlightRef.current = false;
      setIsAdvancing(false);
    }
  };

  const doSkipToMatchDay = (options?: { append?: boolean; ignoreBlockers?: boolean }) => {
    resumeAfterBlockerRef.current = () =>
      void continueAnyway(() => void doSkipToMatchDay({ append: true, ignoreBlockers: true }));
    return runMultiDayAdvance(
      () => skipToMatchDay(options?.ignoreBlockers ?? false),
      "doSkipToMatchDay",
      options,
    );
  };

  return {
    isAdvancing,
    showContinueMenu,
    setShowContinueMenu,
    showMatchConfirm,
    setShowMatchConfirm,
    matchMode,
    setMatchMode,
    blockerModal,
    setBlockerModal,
    advanceError,
    handleContinue,
    handleConfirmMatch,
    handleSkipToMatchDay,
    digestEntries,
    digestStopReason,
    isDigestVisible,
    isDigestRunning,
    isDigestAborting,
    abortDigest,
    dismissDigest,
    resumeDigest: () => runStreamingDigest({ resume: true }),
    resumeAfterBlocker: () => resumeAfterBlockerRef.current(),
  };
}
