import type { RatingView } from "../services/playerRatingService";

/** "62–70", or "62" when the range is one number: the range a scout gave, never a number he could not know. */
export function formatRange(range: { low: number; high: number }): string {
  return range.low === range.high ? String(range.low) : `${range.low}–${range.high}`;
}

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
      return formatRange(rating);
    case "standing":
      return "—";
  }
}
