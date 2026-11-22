import type { TOptions } from "i18next";
import { ArrowRight } from "lucide-react";

type TranslateFn = (key: string, options?: TOptions) => string;

interface PlayerProfileOwnerToClubProps {
  ownerName: string;
  clubName: string;
  t: TranslateFn;
}

/** "Contract owner X → Playing for Y": both clubs named, each under its own label, as term/definition pairs. */
export default function PlayerProfileOwnerToClub({
  ownerName,
  clubName,
  t,
}: PlayerProfileOwnerToClubProps) {
  return (
    <dl className="flex flex-col gap-1 sm:flex-row sm:flex-wrap sm:items-center sm:gap-x-3">
      <div className="flex flex-col">
        <dt className="text-caption text-fg-muted">{t("playerProfile.loanOwnerLabel")}</dt>
        <dd className="text-body font-semibold text-fg">{ownerName}</dd>
      </div>
      <ArrowRight aria-hidden="true" className="hidden h-4 w-4 text-fg-subtle sm:block" />
      <div className="flex flex-col">
        <dt className="text-caption text-fg-muted">{t("playerProfile.loanPlayingForLabel")}</dt>
        <dd className="text-body font-semibold text-fg">{clubName}</dd>
      </div>
    </dl>
  );
}
