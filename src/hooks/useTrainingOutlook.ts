import { useEffect, useState } from "react";

import { getTrainingOutlook, type TrainingOutlookData } from "../services/trainingService";

/**
 * What the club's training will really do, from the backend (taper, fixtures, coaching). It asks
 * again when `revision` (the game) changes and keeps the last answer meanwhile; `null` until the
 * first answer and after a failed one.
 */
export function useTrainingOutlook(revision?: unknown): TrainingOutlookData | null {
  const [outlook, setOutlook] = useState<TrainingOutlookData | null>(null);

  useEffect(() => {
    let cancelled = false;
    getTrainingOutlook()
      .then((answer) => {
        if (!cancelled) setOutlook(answer ?? null);
      })
      .catch((error: unknown) => {
        console.error("Failed to read the training outlook:", error);
        if (!cancelled) setOutlook(null);
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  return outlook;
}
