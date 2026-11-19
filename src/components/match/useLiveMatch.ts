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

/**
 * Everything a live match screen does with the backend, behind one hook: it starts the match,
 * feeds the director with what the engine produces, runs the animation loop while the match is
 * playing, and sends the manager's orders. Every backend call goes through one queue, so a
 * request never overlaps another and a failure always resynchronises with the engine.
 */
export function useLiveMatch<TFinish>(onFinished: (response: TFinish) => void) {
  const { t } = useTranslation();
  const [director] = useState(() => new LiveDirector());
  const snapshot = useSyncExternalStore(director.subscribe, director.getSnapshot);
  const [notice, setNotice] = useState<string | null>(null);
  const [finishing, setFinishing] = useState(false);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const drawRef = useRef<(() => void) | null>(null);
  const orderCount = useRef(0);

  /** Runs one backend call after the ones before it; a failure is shown and the match resynchronised. */
  const call = useCallback(
    <T>(request: () => Promise<T>): Promise<T | null> => {
      const run = queue.current.then(async () => {
        try {
          return await request();
        } catch (error) {
          setNotice(resolveBackendKeyError(error, t("match.interactive.matchFailed")));
          director.halt();
          // Whatever went wrong, show the match as the engine has it now, not as it was.
          await getInteractiveMatchState(director.eventCursor)
            .then((state) => director.ingestSnapshot(state))
            .catch(() => undefined);
          return null;
        }
      });
      queue.current = run.then(
        () => undefined,
        () => undefined,
      );
      return run;
    },
    [director, t],
  );

  const advance = useCallback(
    (seconds: number): Promise<AdvanceSnapshot | null> =>
      call(() =>
        advanceInteractiveMatch(
          { kind: "for_seconds", value: seconds },
          director.eventCursor,
          false,
          true,
        ),
      ),
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
    const reply = await call(() =>
      advanceInteractiveMatch({ kind: "to_end" }, director.eventCursor, false, true),
    );
    if (!reply) return;
    director.ingestAdvance(reply);
    director.skipToHead();
  }, [call, director]);

  const giveOrder = useCallback(
    async (order: UserOrder) => {
      const view = director.getSnapshot().view;
      // Orders are given on the moment the picture shows, never on one still ahead of it.
      if (!view || !director.getSnapshot().canOrder) return;
      orderCount.current += 1;
      const id = `${view.session.session_id}:${orderCount.current}`;
      const answer = await call(() =>
        submitMatchCommand(id, view.session.revision, order, director.eventCursor),
      );
      if (!answer) return;
      director.ingestSnapshot(answer.snapshot);
      setNotice(
        answer.receipt.accepted
          ? null
          : t("match.interactive.orderRefused", {
              reason: rejectionMessage(answer.receipt.rejection ?? "other", t),
            }),
      );
    },
    [call, director, t],
  );

  const toggleAssistant = useCallback(
    async (active: boolean) => {
      const next = await call(() => setInteractiveAssistant(active, director.eventCursor));
      if (next) director.ingestSnapshot(next);
    },
    [call, director],
  );

  const finishInFlight = useRef(false);
  const finish = useCallback(async () => {
    // A double click must not finalise twice: the second one would only report "no match".
    if (finishInFlight.current) return;
    finishInFlight.current = true;
    setFinishing(true);
    const response = await call(() => finishInteractiveMatch<TFinish>());
    setFinishing(false);
    finishInFlight.current = false;
    if (response) onFinished(response);
  }, [call, onFinished]);

  return {
    director,
    snapshot,
    notice,
    finishing,
    drawRef,
    togglePlay,
    skipAhead,
    giveOrder,
    toggleAssistant,
    finish,
  };
}
