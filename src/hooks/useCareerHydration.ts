import { useCallback, useEffect, useState } from "react";
import { applyExtraTranslations } from "../lib/extraTranslations";
import { lookUpActiveCareer } from "../services/careerSession";
import type { GameStateData } from "../store/types";

export type CareerHydration = "ready" | "loading" | "none" | "failed";

interface Options {
  /** True when the store already holds a game; nothing is asked of the backend then. */
  hasGame: boolean;
  setGameState: (state: GameStateData) => void;
  setGameActive: (active: boolean, managerName?: string) => void;
}

/**
 * Makes sure a screen that needs a career has one before it decides to leave. An empty store is
 * not proof there is no career (a reloaded webview, or a route reached before the store was
 * filled), so the backend is asked, and only its answer of "no session" means the player has
 * nowhere to be but the menu. A failure to ask is a state of its own, with a retry.
 */
export function useCareerHydration({ hasGame, setGameState, setGameActive }: Options): {
  status: CareerHydration;
  retry: () => void;
} {
  const [attempt, setAttempt] = useState(0);
  const [outcome, setOutcome] = useState<"none" | "failed" | null>(null);

  useEffect(() => {
    if (hasGame) return;
    let cancelled = false;
    setOutcome(null);
    void lookUpActiveCareer().then((lookup) => {
      if (cancelled) return;
      if (lookup.kind === "found") {
        applyExtraTranslations(lookup.state.extra_translations);
        setGameState(lookup.state);
        setGameActive(true, lookup.state.manager.display_name);
      } else if (lookup.kind === "none" || lookup.kind === "failed") {
        setOutcome(lookup.kind);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [hasGame, attempt, setGameState, setGameActive]);

  const retry = useCallback(() => setAttempt((n) => n + 1), []);
  if (hasGame) return { status: "ready", retry };
  return { status: outcome ?? "loading", retry };
}
