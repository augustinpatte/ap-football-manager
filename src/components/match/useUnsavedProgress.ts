import { useCallback, useRef, useState } from "react";

/**
 * What a live match knows about progress the saved career does not have yet.
 *
 * Progress is "at risk" in two stretches: after the engine has answered with something the last
 * save lacks (`unsaved`), and before it answers, while a call that can change the match is still
 * queued or in flight. `unsaved` alone only covers the first, so a window-close request in the
 * second saw a clean match and let the window go with the progress still on its way.
 */
export function useUnsavedProgress() {
  // True once the engine has moved or taken an order since the career was last saved: closing the
  // window then would lose that progress. A match opened from a save starts false.
  const [unsaved, setUnsavedState] = useState(false);
  // The same fact for code that runs outside a render, such as the window's close request.
  const unsavedRef = useRef(false);
  const mutationsInFlight = useRef(0);

  const setUnsaved = useCallback((value: boolean) => {
    unsavedRef.current = value;
    setUnsavedState(value);
  }, []);

  /** Counts a call that can change the match from the moment it is asked for until it settles. */
  const trackMutation = useCallback(<T>(work: () => Promise<T>): Promise<T> => {
    mutationsInFlight.current += 1;
    return work().finally(() => {
      mutationsInFlight.current -= 1;
    });
  }, []);

  return {
    unsaved,
    setUnsaved,
    markSaved: useCallback(() => setUnsaved(false), [setUnsaved]),
    trackMutation,
    /** Whether closing the window now could lose match progress: unsaved, or still on its way. */
    progressAtRisk: useCallback(() => unsavedRef.current || mutationsInFlight.current > 0, []),
  };
}
