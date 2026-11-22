import type { ReactNode } from "react";
import type { TOptions } from "i18next";

type TranslateFn = (key: string, options?: TOptions) => string;

interface PlayerProfileOwnerToClubProps {
  ownerName: string;
  clubName: string;
  /** What sits between the two clubs (an arrow, hidden from assistive technology). */
  separator: ReactNode;
  t: TranslateFn;
}

/** "Contract owner X → Playing for Y": both clubs named, each under its own label. */
export default function PlayerProfileOwnerToClub({
  ownerName,
  clubName,
  separator,
  t,
}: PlayerProfileOwnerToClubProps) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
      <span className="flex flex-col">
        <span className="text-xs text-gray-500 dark:text-gray-400">
          {t("playerProfile.loanOwnerLabel")}
        </span>
        <span className="text-sm font-semibold text-gray-800 dark:text-gray-100">{ownerName}</span>
      </span>
      {separator}
      <span className="flex flex-col">
        <span className="text-xs text-gray-500 dark:text-gray-400">
          {t("playerProfile.loanPlayingForLabel")}
        </span>
        <span className="text-sm font-semibold text-gray-800 dark:text-gray-100">{clubName}</span>
      </span>
    </div>
  );
}
