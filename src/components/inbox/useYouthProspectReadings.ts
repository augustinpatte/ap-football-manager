import { useEffect, useState } from "react";

import { getYouthProspectReadings, type ProspectReading } from "../../services/playerRatingService";

/**
 * How the manager reads each prospect of a youth recruitment message, by player id. Asks again when
 * `refreshKey` changes (signing a prospect makes him the club's own, who reads exactly), and
 * returns nothing until the backend has answered, so a card never falls back to the true numbers
 * the message carries.
 */
export function useYouthProspectReadings(
  messageId: string | null,
  refreshKey: unknown,
): Record<string, ProspectReading> {
  const [answer, setAnswer] = useState<{
    messageId: string;
    readings: Record<string, ProspectReading>;
  } | null>(null);

  useEffect(() => {
    if (messageId === null) return;
    let current = true;
    getYouthProspectReadings(messageId)
      .then((readings) => {
        if (!current) return;
        setAnswer({
          messageId,
          readings: Object.fromEntries((readings ?? []).map((reading) => [reading.player_id, reading])),
        });
      })
      .catch((error: unknown) => {
        console.error("Failed to read the youth prospects:", error);
      });
    return () => {
      current = false;
    };
  }, [messageId, refreshKey]);

  return answer?.messageId === messageId ? answer.readings : {};
}
