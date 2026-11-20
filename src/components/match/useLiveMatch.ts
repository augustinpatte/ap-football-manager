import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { resolveBackendKeyError } from "../../utils/backendI18n";
import {
  type AdvanceSnapshot,
  type UserOrder,
  advanceInteractiveMatch,
  finishInteractiveMatch,
  getInteractiveMatchState,
  setInteractiveAssistant,
  startInteractiveMatch,
  submitMatchCommand,
} from "../../services/interactiveMatchService";
import { rejectionMessage } from "./interactiveMatch.helpers";
import { LiveDirector } from "./liveDirector";

/** The longest wall-clock step the picture takes in one animation frame (a throttled tab must not jump). */
const MAX_FRAME_STEP_MS = 100;

/** A short random string that makes this screen's command ids distinct from any earlier screen's. */
function newOrderSalt(): string {
  return globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
}

/**
 * Everything a live match screen does with the backend, behind one hook: it starts the match,
 * feeds the director with what the engine produces, runs the animation loop while the match is
 * playing, and sends the manager's orders. Every backend call goes through one queue, so a
 * request never overlaps another and a failure always resynchronises with the engine.
 */
export function useLiveMatch<TFinish>(onFinished: (response: TFinish) => void) {
  const { t } = useTranslation();
  // The language may change while the match is open; that must not restart anything.
  const tRef = useRef(t);
  tRef.current = t;
  const [director] = useState(() => new LiveDirector());
  const snapshot = useSyncExternalStore(director.subscribe, director.getSnapshot);
  const [notice, setNotice] = useState<string | null>(null);
  const [finishing, setFinishing] = useState(false);
  // True once the engine has moved or taken an order since the career was last saved: closing the
  // window then would lose that progress. A match opened from a save starts false.
  const [unsaved, setUnsaved] = useState(false);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  // Set once the career has been saved and cleared for the menu: engine calls still waiting in the
  // queue then have no match to talk to and must not report that as a failure.
  const leaving = useRef(false);
  const drawRef = useRef<(() => void) | null>(null);
  const orderCount = useRef(0);
  // The engine answers a repeated command id with its first answer and applies nothing, and it
  // remembers ids across a reopened screen and a reloaded save: an id is unique to this screen.
  const orderSalt = useRef(newOrderSalt());

  /**
   * Runs one backend call after the ones before it and hands its failure back to the caller. The
   * career's own save and exit use this: they must not halt the match or replace their reason with
   * the engine's generic notice.
   */
  const enqueue = useCallback(<T>(request: () => Promise<T>): Promise<T> => {
    const run = queue.current.then(request);
    queue.current = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }, []);

  /** Runs one engine call in the queue; a failure is shown and the match resynchronised. */
  const call = useCallback(
    <T>(request: () => Promise<T>): Promise<T | null> =>
      enqueue(async () => {
        if (leaving.current) return null;
        try {
          return await request();
        } catch (error) {
          setNotice(resolveBackendKeyError(error, tRef.current("match.interactive.matchFailed")));
          director.halt();
          // Whatever went wrong, show the match as the engine has it now, not as it was.
          await getInteractiveMatchState(director.eventCursor)
            .then((state) => director.ingestSnapshot(state))
            .catch(() => undefined);
          return null;
        }
      }),
    [director, enqueue],
  );

  const advance = useCallback(
    (seconds: number): Promise<AdvanceSnapshot | null> =>
      call(async () => {
        const reply = await advanceInteractiveMatch(
          { kind: "for_seconds", value: seconds },
          director.eventCursor,
          false,
          true,
        );
        // The kickoff fetch asks for no time and changes nothing the save would lose.
        if (seconds > 0) setUnsaved(true);
        return reply;
      }),
    [call, director],
  );

  // Start (or take over) the match and fetch the kickoff frame, so the pitch is there before play.
  useEffect(() => {
    void call(startInteractiveMatch).then(async (first) => {
      if (!first) return;
      director.ingestSnapshot(first);
      const kickoff = await advance(0);
      if (kickoff) director.ingestAdvance(kickoff);
    });
  }, [call, advance, director]);

  const status = snapshot.status;
  useEffect(() => {
    if (status !== "playing" && status !== "pausing") return;
    let frame = 0;
    let last = performance.now();
    const step = (now: number) => {
      director.tick(Math.min(MAX_FRAME_STEP_MS, now - last));
      last = now;
      void director.pump(advance);
      drawRef.current?.();
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [status, director, advance]);

  const togglePlay = useCallback(() => {
    setNotice(null);
    if (status === "playing") director.pause();
    else director.play();
  }, [status, director]);

  /** Plays the engine on to the next break or full time and shows where it got to. */
  const skipAhead = useCallback(async () => {
    director.pause();
    await director.track(async () => {
      const reply = await call(() =>
        advanceInteractiveMatch({ kind: "to_end" }, director.eventCursor, false, true),
      );
      if (!reply) return;
      setUnsaved(true);
      director.ingestAdvance(reply);
      director.skipToHead();
    });
  }, [call, director]);

  const giveOrder = useCallback(
    async (order: UserOrder) => {
      const view = director.getSnapshot().view;
      // Orders are given on the moment the picture shows, never on one still ahead of it.
      if (!view || !director.getSnapshot().canOrder) return;
      orderCount.current += 1;
      const id = `${view.session.session_id}:${orderSalt.current}:${orderCount.current}`;
      await director.track(async () => {
        const answer = await call(() =>
          submitMatchCommand(id, view.session.revision, order, director.eventCursor),
        );
        if (!answer) return;
        if (answer.receipt.accepted) setUnsaved(true);
        director.ingestSnapshot(answer.snapshot);
        setNotice(
          answer.receipt.accepted
            ? null
            : t("match.interactive.orderRefused", {
                reason: rejectionMessage(answer.receipt.rejection ?? "other", t),
              }),
        );
      });
    },
    [call, director, t],
  );

  const toggleAssistant = useCallback(
    async (active: boolean) => {
      await director.track(async () => {
        const next = await call(() => setInteractiveAssistant(active, director.eventCursor));
        if (next) {
          setUnsaved(true);
          director.ingestSnapshot(next);
        }
      });
    },
    [call, director],
  );

  // True from the first click until the result is applied; it stays true after a success, so a
  // late second click cannot ask the engine to finalise a match that no longer exists. A failure
  // clears it, because the match is kept and the manager may try again.
  const finishRequested = useRef(false);
  const finish = useCallback(async () => {
    if (finishRequested.current) return;
    finishRequested.current = true;
    setFinishing(true);
    const response = await call(() => finishInteractiveMatch<TFinish>());
    setFinishing(false);
    if (!response) {
      finishRequested.current = false;
      return;
    }
    onFinished(response);
  }, [call, onFinished]);

  return {
    director,
    snapshot,
    notice,
    finishing,
    unsaved,
    markSaved: useCallback(() => setUnsaved(false), []),
    enqueue,
    markLeaving: useCallback(() => {
      leaving.current = true;
    }, []),
    drawRef,
    togglePlay,
    skipAhead,
    giveOrder,
    toggleAssistant,
    finish,
  };
}
