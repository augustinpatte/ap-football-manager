import { useTranslation } from "react-i18next";

import { useRatingTiers } from "../../hooks/useRatingTiers";
import { ratingTierClass } from "../../lib/ratingTier";
import { formatRange, ratingFigure } from "../../lib/ratingView";
import type { RatingView } from "../../services/playerRatingService";

interface ReadableRatingProps {
  /** How the manager may read the rating; `null` while it is on its way or could not be read. */
  rating: RatingView | null;
  /** A table cell, or the large figure of a profile. */
  size?: "cell" | "hero";
  /** What the figure is: the rating itself, or the ceiling (potential) a screen puts beside it. */
  subject?: "rating" | "potential";
  /** Stacked under its figure (a table cell), or on one line (inside a badge). */
  layout?: "stacked" | "inline";
  className?: string;
}

type Translate = (key: string, params?: Record<string, string | number>) => string;

/** The whole meaning of a reading as a sentence, for a screen reader or a tooltip. */
function spokenRating(
  t: Translate,
  rating: RatingView | null,
  subject: "rating" | "potential",
): string {
  if (rating === null) return t("playerRating.unavailable");
  if (rating.kind === "exact") {
    return subject === "potential"
      ? t("playerRating.a11y.potentialExact", { value: rating.ovr })
      : t("playerRating.a11y.exact", { value: rating.ovr });
  }
  if (rating.kind === "estimate") {
    return t(rating.current ? "playerRating.a11y.estimate" : "playerRating.a11y.estimateStale", {
      low: rating.low,
      high: rating.high,
      confidence: t(`playerRating.confidence.${rating.confidence}`),
    });
  }
  const standing = t(`playerRating.standing.${rating.standing}`);
  return subject === "potential"
    ? t("playerRating.a11y.potentialStanding", { standing })
    : t("playerRating.a11y.standing", { standing });
}

/** `spokenRating` for a component that holds a translator already (a token's description). */
export function useSpokenRating(): (
  rating: RatingView | null,
  subject?: "rating" | "potential",
) => string {
  const { t } = useTranslation();
  return (rating, subject = "rating") => spokenRating(t, rating, subject);
}

/**
 * A rating's short figure (a number, a scout's range or a dash) for a cramped spot, with the full
 * sentence for a screen reader so a bare "62–70" or a lone dash never stands alone.
 */
export function SpokenRatingFigure({
  rating,
  className = "",
}: {
  rating: RatingView | null;
  className?: string;
}) {
  const spoken = useSpokenRating();
  return (
    <span className={className}>
      <span aria-hidden="true">{ratingFigure(rating)}</span>
      <span className="sr-only">{spoken(rating)}</span>
    </span>
  );
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
  subject = "rating",
  layout = "stacked",
  className = "",
}: ReadableRatingProps) {
  const { t } = useTranslation();
  const tierOf = useRatingTiers();
  const figure = (kind: RatingView["kind"]) =>
    `font-heading font-bold tabular-nums ${FIGURE_SIZE[size][kind]}`;
  const wrapper =
    layout === "inline"
      ? `inline-flex flex-row items-baseline gap-1 leading-tight ${className}`
      : `inline-flex flex-col items-center leading-tight ${className}`;

  if (rating === null) {
    return (
      <span className={wrapper}>
        <span aria-hidden="true" className={`${figure("exact")} text-fg-subtle`}>
          —
        </span>
        <span className="sr-only">{spokenRating(t, rating, subject)}</span>
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
        <span className="sr-only">{spokenRating(t, rating, subject)}</span>
      </span>
    );
  }

  if (rating.kind === "estimate") {
    return (
      <span
        className={wrapper}
        title={t(rating.current ? "playerRating.hint.estimate" : "playerRating.hint.estimateStale")}
      >
        <span aria-hidden="true" className={`${figure("estimate")} text-fg`}>
          {formatRange(rating)}
        </span>
        <span aria-hidden="true" className="text-caption text-fg-subtle">
          {t("playerRating.estimate")}
        </span>
        {!rating.current && (
          <span aria-hidden="true" className="text-caption text-warning">
            {t("playerRating.stale")}
          </span>
        )}
        <span className="sr-only">{spokenRating(t, rating, subject)}</span>
      </span>
    );
  }

  const standing = t(`playerRating.standing.${rating.standing}`);
  return (
    <span
      className={wrapper}
      title={t(
        subject === "potential"
          ? "playerRating.hint.potentialStanding"
          : "playerRating.hint.standing",
      )}
    >
      <span
        aria-hidden="true"
        className={`font-heading font-bold ${FIGURE_SIZE[size].standing} text-fg-muted`}
      >
        {standing}
      </span>
      <span className="sr-only">{spokenRating(t, rating, subject)}</span>
    </span>
  );
}
