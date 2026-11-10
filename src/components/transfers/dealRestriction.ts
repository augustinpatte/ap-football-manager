import type { PlayerData } from "../../store/gameStore";

/** Why a player cannot be approached right now, with the date it ends when the record holds one. */
export type DealRestriction =
  | { kind: "onLoan"; clubId: string; until: string }
  | { kind: "pendingMove"; date: string | null };

/**
 * Read from the player's own record, so the screen says what the backend will enforce and a date
 * is only ever one the game stored: the end of his loan, or the day an agreed move registers.
 */
export function dealRestrictionOf(player: PlayerData): DealRestriction | null {
  if (player.active_loan) {
    return {
      kind: "onLoan",
      clubId: player.active_loan.loan_team_id,
      until: player.active_loan.end_date,
    };
  }
  const transfer = (player.transfer_offers ?? []).find(
    (offer) => offer.status === "PendingRegistration",
  );
  if (transfer) {
    return { kind: "pendingMove", date: transfer.registration_date ?? null };
  }
  const loan = (player.loan_offers ?? []).find((offer) => offer.status === "PendingRegistration");
  if (loan) {
    return { kind: "pendingMove", date: loan.start_date || null };
  }
  return null;
}
