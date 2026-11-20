import { useEffect, useState } from "react";

import { getPlayerRatings, type RatingView } from "../services/playerRatingService";

type Readings = Record<string, RatingView>;

/**
 * How the manager may read each player's rating, for a screen that holds the players itself (a
 * profile, a deal) rather than receiving a page of them. Asks again whenever `refreshKey` changes (a
 * scout finishing or a day passing changes what the club knows), drops an answer to a request that
 * was overtaken, and returns nothing for a player it has no answer for: the screen shows a dash,
 * never a number of its own.
 */
export function usePlayerRatings(playerIds: string[], refreshKey: unknown): Readings {
  const [answer, setAnswer] = useState<{ ids: string; readings: Readings } | null>(null);
  // The array is new on every render; the ids themselves are what the request depends on.
  const ids = playerIds.join("\u0000");

  useEffect(() => {
    if (ids === "") return;
    let current = true;
    getPlayerRatings(ids.split("\u0000"))
      .then((readings) => {
        if (current) setAnswer({ ids, readings: readings ?? {} });
      })
      .catch((error: unknown) => {
        console.error("Failed to read the player ratings:", error);
      });
    return () => {
      current = false;
    };
  }, [ids, refreshKey]);

  // An answer for other players is not an answer for these.
  return answer?.ids === ids ? answer.readings : {};
}
