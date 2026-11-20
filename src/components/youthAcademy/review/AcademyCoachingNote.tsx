import { useTranslation } from "react-i18next";

import type { AcademySummary } from "../../../services/academyService";

/**
 * How well the club's youth staff can read a ceiling, and what they add to the academy's progress:
 * the reason the bands in the table are as wide as they are.
 */
export default function AcademyCoachingNote({ summary }: { summary: AcademySummary }) {
  const { t } = useTranslation();
  const { coaching } = summary;
  return (
    <div className="flex flex-col gap-1 rounded border border-line bg-sunken p-3 text-body text-fg-muted">
      <p className="font-heading font-bold text-fg">{t("academy.coaching")}</p>
      <p>
        {coaching.has_staff
          ? t("academy.coachingSummary", {
              rating: coaching.rating,
              width: coaching.band_width,
              confidence: t(`playerRating.confidence.${coaching.confidence}`),
            })
          : t("academy.coachingNoStaff", { width: coaching.band_width })}
      </p>
      {coaching.has_staff && coaching.progression_bonus_pct > 0 && (
        <p>{t("academy.coachingBonus", { percent: coaching.progression_bonus_pct })}</p>
      )}
    </div>
  );
}
