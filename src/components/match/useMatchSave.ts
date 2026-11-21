import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { exitToMenu, saveGame } from "../../services/sessionService";
import { resolveBackendKeyError } from "../../utils/backendI18n";

export type MatchSaveStatus = "idle" | "saving" | "saved" | "exiting";

interface MatchSaveOptions {
  /** Runs a backend call after the engine calls already queued, so a checkpoint carries every accepted order. */
  enqueue: <T>(request: () => Promise<T>) => Promise<T>;
  /** True while the engine has moved or taken an order since the career was last saved. */
  unsaved: boolean;
  /** Read when the window is asked to close: unsaved progress, or engine calls still to answer. */
  progressAtRisk: () => boolean;
  markSaved: () => void;
  /** Called inside the queue once the career has been saved and cleared, before any later engine call. */
  markLeaving: () => void;
  /** Stops the picture asking the engine for more. */
  halt: () => void;
  onExited: () => void;
}

/**
 * The career-level actions of a live match: save in place, save and return to the menu, and the
 * guard that stops the window closing over unsaved progress. It uses the one save the Dashboard
 * uses; the backend stores the match in progress with the career and exit keeps that checkpoint.
 */
export function useMatchSave({
  enqueue,
  unsaved,
  progressAtRisk,
  markSaved,
  markLeaving,
  halt,
  onExited,
}: MatchSaveOptions) {
  const { t } = useTranslation();
  const tRef = useRef(t);
  tRef.current = t;
  const [state, setState] = useState<MatchSaveStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [closePrompt, setClosePrompt] = useState(false);
  // A ref as well as state: two clicks in one frame must not both start an action.
  const working = useRef(false);
  const progressAtRiskRef = useRef(progressAtRisk);
  progressAtRiskRef.current = progressAtRisk;
  const closing = useRef(false);

  const attempt = useCallback(
    async (
      request: () => Promise<void>,
      during: "saving" | "exiting",
      failureKey: string,
    ): Promise<boolean> => {
      if (working.current) return false;
      working.current = true;
      setError(null);
      setState(during);
      try {
        await enqueue(request);
      } catch (failure) {
        setError(resolveBackendKeyError(failure, tRef.current(failureKey)));
        setState("idle");
        working.current = false;
        return false;
      }
      // A successful exit keeps the action locked: the screen is about to go.
      if (during === "saving") working.current = false;
      return true;
    },
    [enqueue],
  );

  const save = useCallback(async (): Promise<boolean> => {
    const saved = await attempt(saveGame, "saving", "match.interactive.saveFailed");
    if (saved) {
      markSaved();
      setState("saved");
    }
    return saved;
  }, [attempt, markSaved]);

  const saveAndExit = useCallback(async () => {
    if (working.current) return;
    halt();
    const exited = await attempt(
      async () => {
        await exitToMenu();
        markLeaving();
      },
      "exiting",
      "match.interactive.exitFailed",
    );
    if (exited) onExited();
  }, [attempt, halt, markLeaving, onExited]);

  // The Dashboard's close guard is gone once the match route replaces it: the match holds its own.
  useEffect(() => {
    let release: (() => void) | undefined;
    let disposed = false;
    try {
      void getCurrentWindow()
        .onCloseRequested((event) => {
          if (closing.current || !progressAtRiskRef.current()) return;
          event.preventDefault();
          setClosePrompt(true);
        })
        .then((unlisten) => {
          if (disposed) unlisten();
          else release = unlisten;
        })
        .catch(() => undefined);
    } catch {
      // Outside the desktop shell there is no window to guard.
    }
    return () => {
      disposed = true;
      release?.();
    };
  }, []);

  const closeWindow = useCallback(
    async (saveFirst: boolean) => {
      setClosePrompt(false);
      if (saveFirst && !(await save())) return;
      closing.current = true;
      await getCurrentWindow().destroy();
    },
    [save],
  );

  return {
    // The confirmation lasts only while nothing has happened since the save it reports.
    status: state === "saved" && unsaved ? "idle" : state,
    error,
    busy: state === "saving" || state === "exiting",
    save: () => void save(),
    saveAndExit: () => void saveAndExit(),
    closePrompt,
    dismissClosePrompt: () => setClosePrompt(false),
    closeWindow: (saveFirst: boolean) => void closeWindow(saveFirst),
  };
}
