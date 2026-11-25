import { useEffect, useState } from "react";
import type { GameStateData } from "../../store/gameStore";
import { getDepartureBlockers } from "../../services/transfersService";

/**
 * Which of the manager's players cannot be sold, lent or released today, and why, as the backend's
 * own refusal keys. Asked only while an offer waits for an answer, and again when the day or the
 * number of waiting offers changes: the answer depends on the squad, and nothing else on screen
 * needs it.
 */
export function useDepartureBlockers(gameState: GameStateData): Record<string, string> {
  const [blockers, setBlockers] = useState<Record<string, string>>({});
  const userTeamId = gameState.manager.team_id;
  const waiting = gameState.players.reduce(
    (count, player) =>
      player.team_id === userTeamId
        ? count +
          player.transfer_offers.filter((offer) => offer.status === "Pending").length +
          (player.loan_offers ?? []).filter((offer) => offer.status === "Pending").length
        : count,
    0,
  );
  const day = gameState.clock?.current_date ?? "";
  const seniors = gameState.players.filter((player) => player.team_id === userTeamId).length;

  useEffect(() => {
    if (waiting === 0) {
      setBlockers({});
      return;
    }
    let cancelled = false;
    getDepartureBlockers()
      .then((answer) => {
        if (!cancelled) setBlockers(answer);
      })
      .catch((error) => console.error("Could not read the squad-floor blockers", error));
    return () => {
      cancelled = true;
    };
  }, [waiting, day, seniors]);

  return blockers;
}
