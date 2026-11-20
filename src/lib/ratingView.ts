import type { RatingView } from "../services/playerRatingService";

/**
 * A rating as the one short figure a cramped spot can hold (a pitch token, a narrow column): the
 * number for a player the club knows, the scout's range for a reported one, and a dash for anyone
 * known only by his public standing or not yet answered. It never invents a number.
 */
export function ratingFigure(rating: RatingView | null | undefined): string {
  if (!rating) return "—";
  switch (rating.kind) {
    case "exact":
      return String(rating.ovr);
    case "estimate":
      return rating.low === rating.high ? String(rating.low) : `${rating.low}–${rating.high}`;
    case "standing":
      return "—";
  }
}
