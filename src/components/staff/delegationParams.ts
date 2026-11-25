import { useTranslation } from "react-i18next";

import { formatExactMoney } from "../../lib/helpers";

/** Backend params carry raw enum names; the ones that name a post or a position are translated. */
export function useDelegationParams() {
  const { t } = useTranslation();
  return (params: [string, string][]): Record<string, string> =>
    Object.fromEntries(
      params.map(([key, value]) => [
        key,
        key === "post"
          ? t(`staff.posts.${value}`, { defaultValue: value })
          : key === "position"
            ? t(`common.positions.${value}`, { defaultValue: value })
            : key === "focus" || key === "currentFocus"
              ? t(`training.focuses.${value}.label`, { defaultValue: value })
              : key === "intensity" || key === "currentIntensity"
                ? t(`training.intensities.${value}.label`, { defaultValue: value })
                : key === "schedule"
                  ? t(`training.schedules.${value}.label`, { defaultValue: value })
                  : key === "style"
                    ? t(`common.playStyles.${value}`, { defaultValue: value })
                    : key === "fee" && Number.isFinite(Number(value))
                      ? formatExactMoney(Number(value))
                      : value,
      ]),
    );
}
