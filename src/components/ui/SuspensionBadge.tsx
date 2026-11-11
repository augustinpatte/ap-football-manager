import { Ban } from "lucide-react";
import { useTranslation } from "react-i18next";

interface SuspensionBadgeProps {
  /** Matches of his club the player must still miss. */
  matches: number;
}

/** A player kept out by a sending-off. Danger-toned like a long injury, with the count in the name. */
export function SuspensionBadge({ matches }: SuspensionBadgeProps) {
  const { t } = useTranslation();
  const title = t("playerProfile.suspendedTitle", { matches });
  return (
    <span
      className="inline-flex max-w-44 items-center gap-1 rounded-md border border-danger-solid bg-danger-tint px-2 py-0.5 font-heading text-caption font-bold uppercase tracking-label text-danger"
      role="img"
      title={title}
      aria-label={title}
    >
      <Ban className="h-3.5 w-3.5 shrink-0" />
      <span className="truncate">{t("playerProfile.suspendedShort")}</span>
      <span className="shrink-0 tabular-nums">{matches}</span>
    </span>
  );
}
