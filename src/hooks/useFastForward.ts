import { useCallback, useEffect, useRef, useState } from "react";

import {
  cancelFastForward,
  fastForwardToDate,
  onFastForwardProgress,
  type FastForwardPolicy,
  type FastForwardProgress,
  type FastForwardResponse,
} from "../services/fastForwardService";
import type { GameStateData } from "../store/gameStore";
import { addDays, daysBetween, isCalendarDate } from "./useFastForward.helpers";

export type FastForwardPhase = "closed" | "configure" | "running" | "result";

export interface FastForwardState {
  phase: FastForwardPhase;
  /** The date the game reads today, `YYYY-MM-DD`. */
  today: string;
  target: string;
  policy: FastForwardPolicy;
  progress: FastForwardProgress | null;
  isCancelling: boolean;
  result: FastForwardResponse | null;
  /** A translation key or message from the backend, shown beside the form or the result. */
  error: string | null;
  /** The target is a real date after today. */
  targetIsValid: boolean;
  daysAhead: number;
  open: () => void;
  close: () => void;
  setTarget: (target: string) => void;
  setPolicy: (policy: FastForwardPolicy) => void;
  start: () => Promise<void>;
  cancel: () => void;
  /** Carry on towards the same target after a stop, from where the game now stands. */
  resume: () => Promise<void>;
}

/**
 * The fast-forward dialog's state: configure, run (with progress and a cancel), result.
 * The backend does the work in one call; this hook only listens for its progress and, at the end,
 * hands the single world it returns to the store.
 */
export function useFastForward(
  currentDate: string,
  setGameState: (state: GameStateData) => void,
  onFired: () => void,
): FastForwardState {
  const [phase, setPhase] = useState<FastForwardPhase>("closed");
  const [target, setTarget] = useState("");
  const [policy, setPolicy] = useState<FastForwardPolicy>("stop_for_decisions");
  const [progress, setProgress] = useState<FastForwardProgress | null>(null);
  const [isCancelling, setIsCancelling] = useState(false);
  const [result, setResult] = useState<FastForwardResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A ref, not `phase`: a double click lands before the state commit re-renders.
  const inFlightRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const today = currentDate.slice(0, 10);
  const targetIsValid = isCalendarDate(target) && daysBetween(today, target) > 0;
  const daysAhead = targetIsValid ? daysBetween(today, target) : 0;

  const open = useCallback(() => {
    if (inFlightRef.current) return;
    setTarget(addDays(today, 7));
    setError(null);
    setResult(null);
    setProgress(null);
    setPhase("configure");
  }, [today]);

  const close = useCallback(() => {
    if (inFlightRef.current) return;
    setPhase("closed");
    setResult(null);
    setError(null);
  }, []);

  const run = useCallback(
    async (runTarget: string) => {
      if (inFlightRef.current) return;
      inFlightRef.current = true;
      setError(null);
      setResult(null);
      setIsCancelling(false);
      setProgress(null);
      setPhase("running");
      let unlisten: (() => void) | null = null;
      try {
        unlisten = await onFastForwardProgress((next) => {
          if (mountedRef.current) setProgress(next);
        });
        const response = await fastForwardToDate({ target_date: runTarget, policy });
        setGameState(response.game);
        if (response.stop.kind === "manager_fired") onFired();
        if (mountedRef.current) {
          setProgress(response.progress);
          setResult(response);
          setPhase("result");
        }
      } catch (err) {
        console.error("[useFastForward] failed:", err);
        if (mountedRef.current) {
          setError(err instanceof Error ? err.message : String(err));
          // Nothing was played if the request was refused; the form is where to fix it.
          setPhase("configure");
        }
      } finally {
        unlisten?.();
        inFlightRef.current = false;
      }
    },
    [onFired, policy, setGameState],
  );

  const start = useCallback(() => run(target), [run, target]);

  const resume = useCallback(async () => {
    if (result) await run(result.progress.target_date);
  }, [result, run]);

  const cancel = useCallback(() => {
    setIsCancelling(true);
    void cancelFastForward();
  }, []);

  return {
    phase,
    today,
    target,
    policy,
    progress,
    isCancelling,
    result,
    error,
    targetIsValid,
    daysAhead,
    open,
    close,
    setTarget,
    setPolicy,
    start,
    cancel,
    resume,
  };
}
