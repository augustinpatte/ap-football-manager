import { useEffect, useState } from "react";
import { getPlayerHistory, type PlayerHistory } from "../../services/playerHistoryService";
import type { PlayerData } from "../../store/gameStore";

/**
 * The season-by-season career and the moves of one player. The game state sent to the screen
 * leaves them off (they are a quarter of it), so they are fetched when a profile opens and
 * again when the game date or the player's club moves. A player the game no longer holds
 * (a retiree) has no answer: the profile then shows what the player object itself carries.
 */
export function usePlayerHistory(player: PlayerData, currentDate: string): PlayerHistory {
  const [fetched, setFetched] = useState<PlayerHistory | null>(null);
  const playerId = player.id;
  const teamId = player.team_id;

  useEffect(() => {
    let cancelled = false;
    getPlayerHistory(playerId)
      .then((history) => {
        if (!cancelled) setFetched(history);
      })
      .catch(() => {
        if (!cancelled) setFetched(null);
      });

    return () => {
      cancelled = true;
    };
    // `teamId` and `currentDate` are refetch triggers, not read inside.
  }, [playerId, teamId, currentDate]);

  // The effect only refetches after the render in which the player changed, so a history
  // fetched for another player must never be shown for this one.
  if (fetched && fetched.player_id === playerId) return fetched;
  return {
    player_id: playerId,
    career: player.career,
    movement_history: player.movement_history ?? [],
  };
}
