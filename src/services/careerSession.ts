import { getActiveGame } from "./sessionService";
import { routeForActiveCareer } from "./careerService";
import type { GameStateData } from "../store/types";

/**
 * Where a career stands, as the windows of this app see it. One module owns the answer because the
 * menu, the team-selection screen and the dashboard each used to ask the backend on their own and
 * navigate on whatever came back first, so a stale or half-finished answer could send the player
 * to the menu in the middle of opening a career.
 *
 * - `none`          the backend holds no career.
 * - `creating`      a new career is being built (a world is generated).
 * - `loading`       a save is being opened.
 * - `awaitingClub`  a career exists but has not taken its first club.
 * - `active`        a career with a club (or a sacked manager looking for one).
 * - `failed`        asking the backend failed for a reason other than "no career".
 */
export type CareerPhase = "none" | "creating" | "loading" | "awaitingClub" | "active" | "failed";

export type CareerRoute = "/select-team" | "/dashboard";

export type ActiveCareerLookup =
  | { kind: "none" }
  | { kind: "found"; state: GameStateData; route: CareerRoute }
  | { kind: "failed"; error: unknown }
  /** Only for a lookup that asked to be skipped while an operation is running. */
  | { kind: "busy" };

/** The error key the backend answers with when no career is installed. */
const NO_SESSION_ERROR = "be.error.noActiveGameSession";

let phase: CareerPhase = "none";
let operation: symbol | null = null;
let inFlight: Promise<ActiveCareerLookup> | null = null;
let rereadRequested = false;
let autoResumeSuppressed = false;

function setPhase(next: CareerPhase): void {
  if (phase === next) return;
  phase = next;
}

export function getCareerPhase(): CareerPhase {
  return phase;
}

/** Test seam: forget everything a previous test left behind. */
export function resetCareerSessionForTests(): void {
  phase = "none";
  operation = null;
  inFlight = null;
  rereadRequested = false;
  autoResumeSuppressed = false;
}

function isNoSessionError(error: unknown): boolean {
  const text = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return text.startsWith(NO_SESSION_ERROR);
}

async function readActiveCareer(): Promise<ActiveCareerLookup> {
  try {
    const state = await getActiveGame();
    // A command can resolve with nothing; that is a broken reading, never "a career with no data".
    if (!state?.manager) return { kind: "failed", error: new Error("empty active game") };
    const route = await routeForActiveCareer();
    return { kind: "found", state, route };
  } catch (error) {
    return isNoSessionError(error) ? { kind: "none" } : { kind: "failed", error };
  }
}

function phaseFor(lookup: ActiveCareerLookup): CareerPhase | null {
  switch (lookup.kind) {
    case "none":
      return "none";
    case "failed":
      return "failed";
    case "found":
      return lookup.route === "/select-team" ? "awaitingClub" : "active";
    case "busy":
      return null;
  }
}

/**
 * Reads the career the backend holds. Concurrent callers (the menu's mount probe and a
 * `game-state-changed` event, say) share one pass, and a request that arrives while a read is
 * pending makes the pass read once more before anyone is answered, so every caller receives a
 * reading taken after its own request and never an older one that merely finished late.
 *
 * `skipWhileBusy` is for event handlers: while a career is being created or loaded, that operation
 * navigates when it is done, and answering the event as well would be a second, competing route.
 */
export function lookUpActiveCareer(
  options: { skipWhileBusy?: boolean } = {},
): Promise<ActiveCareerLookup> {
  if (options.skipWhileBusy && operation !== null) {
    return Promise.resolve({ kind: "busy" });
  }
  if (inFlight) {
    rereadRequested = true;
    return inFlight;
  }
  inFlight = (async () => {
    try {
      let lookup = await readActiveCareer();
      while (rereadRequested) {
        rereadRequested = false;
        lookup = await readActiveCareer();
      }
      // An operation in progress owns the phase; the lookup must not overwrite "creating".
      const next = phaseFor(lookup);
      if (next && operation === null) setPhase(next);
      return lookup;
    } finally {
      inFlight = null;
    }
  })();
  return inFlight;
}

export interface CareerOperation {
  /** Ends the operation. `outcome` is the phase to settle on; omit it to re-derive on the next lookup. */
  release: (outcome?: CareerPhase) => void;
}

/**
 * Claims the right to create or load a career. Returns `null` when another operation holds it, so a
 * double click or a second window action cannot open two careers at once.
 */
export function beginCareerOperation(kind: "creating" | "loading"): CareerOperation | null {
  if (operation !== null) return null;
  const token = Symbol(kind);
  operation = token;
  setPhase(kind);
  return {
    release: (outcome) => {
      if (operation !== token) return;
      operation = null;
      setPhase(outcome ?? "none");
    },
  };
}

/**
 * Marks the next arrival at the menu as the player's own doing. Leaving a clubless career through
 * the back arrow leaves that career installed in the backend (only `exit_to_menu` clears it), so a
 * menu that resumed whatever the backend holds would send the player straight back.
 */
export function suppressNextAutoResume(): void {
  autoResumeSuppressed = true;
}

/** True once, for the menu mount that follows `suppressNextAutoResume`. */
export function consumeAutoResumeSuppression(): boolean {
  const suppressed = autoResumeSuppressed;
  autoResumeSuppressed = false;
  return suppressed;
}
