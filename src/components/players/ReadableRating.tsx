import { useTranslation } from "react-i18next";

import { useRatingTiers } from "../../hooks/useRatingTiers";
import { ratingTierClass } from "../../lib/ratingTier";
import type { RatingView } from "../../services/playerRatingService";

interface ReadableRatingProps {
  /** How the manager may read the rating; `null` while it is on its way or could not be read. */
  rating: RatingView | null;
  /** A table cell, or the large figure of a profile. */
  size?: "cell" | "hero";
  className?: string;
}

/** A range or a word is wider than a number, so it is set smaller on the profile's large figure. */
const FIGURE_SIZE = {
  cell: { exact: "text-body", estimate: "text-body", standing: "text-body" },
  hero: { exact: "text-display", estimate: "text-title", standing: "text-heading" },
} as const;

/**
 * A player's overall rating, as the manager may read it. The backend chooses what he may know
 * (`ofm_core::rating_view`); this only draws it: the number for a player the club knows, a range
 * marked as an estimate (and as old when the report predates the season) for a scouted one, and a
 * word, never a number, for everyone else. The spoken text says the same thing in full.
 */
export default function ReadableRating({
  rating,
  size = "cell",
  className = "",
}: ReadableRatingProps) {
  const { t } = useTranslation();
  const tierOf = useRatingTiers();
  const figure = (kind: RatingView["kind"]) =>
    `font-heading font-bold tabular-nums ${FIGURE_SIZE[size][kind]}`;
  const wrapper = `inline-flex flex-col items-center leading-tight ${className}`;

  if (rating === null) {
    return (
      <span className={wrapper}>
        <span aria-hidden="true" className={`${figure("exact")} text-fg-subtle`}>
          —
        </span>
        <span className="sr-only">{t("playerRating.unavailable")}</span>
      </span>
    );
  }

  if (rating.kind === "exact") {
    return (
      <span className={wrapper}>
        <span
          aria-hidden="true"
          className={`${figure("exact")} ${ratingTierClass(tierOf(rating.ovr), "text")}`}
        >
          {rating.ovr}
        </span>
        <span className="sr-only">{t("playerRating.a11y.exact", { value: rating.ovr })}</span>
      </span>
    );
  }

  if (rating.kind === "estimate") {
    const confidence = t(`playerRating.confidence.${rating.confidence}`);
    const spoken = {
      low: rating.low,
      high: rating.high,
      confidence,
    };
    return (
      <span
        className={wrapper}
        title={t(rating.current ? "playerRating.hint.estimate" : "playerRating.hint.estimateStale")}
      >
        <span aria-hidden="true" className={`${figure("estimate")} text-fg`}>
          {rating.low === rating.high ? rating.low : `${rating.low}–${rating.high}`}
        </span>
        <span aria-hidden="true" className="text-caption text-fg-subtle">
          {t("playerRating.estimate")}
        </span>
        {!rating.current && (
          <span aria-hidden="true" className="text-caption text-warning">
            {t("playerRating.stale")}
          </span>
        )}
        <span className="sr-only">
          {t(
            rating.current ? "playerRating.a11y.estimate" : "playerRating.a11y.estimateStale",
            spoken,
          )}
        </span>
      </span>
    );
  }

  const standing = t(`playerRating.standing.${rating.standing}`);
  return (
    <span className={wrapper} title={t("playerRating.hint.standing")}>
      <span
        aria-hidden="true"
        className={`font-heading font-bold ${FIGURE_SIZE[size].standing} text-fg-muted`}
      >
        {standing}
      </span>
      <span className="sr-only">{t("playerRating.a11y.standing", { standing })}</span>
    </span>
  );
}
