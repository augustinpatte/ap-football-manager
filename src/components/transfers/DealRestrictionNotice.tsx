import { CalendarClock } from "lucide-react";
import { useTranslation } from "react-i18next";

import { formatDate } from "../../lib/dateFormatting";
import { getTeamName } from "../../lib/helpers";
import type { PlayerData, TeamData } from "../../store/gameStore";
import { dealRestrictionOf } from "./dealRestriction";

interface DealRestrictionNoticeProps {
  player: PlayerData;
  teams: TeamData[];
}

/** Why this player cannot be approached now, and when that ends if the game knows. */
export default function DealRestrictionNotice({ player, teams }: DealRestrictionNoticeProps) {
  const { t, i18n } = useTranslation();
  const restriction = dealRestrictionOf(player);
  if (!restriction) {
    return null;
  }
  const title =
    restriction.kind === "onLoan"
      ? t("transfers.restriction.onLoanTitle")
      : t("transfers.restriction.pendingTitle");
  let detail: string;
  if (restriction.kind === "onLoan") {
    detail = t("transfers.restriction.onLoanDetail", {
      club: getTeamName(teams, restriction.clubId),
      date: formatDate(restriction.until, i18n.language),
    });
  } else if (restriction.date) {
    detail = t("transfers.restriction.pendingDetail", {
      date: formatDate(restriction.date, i18n.language),
    });
  } else {
    detail = t("transfers.restriction.pendingDetailNoDate");
  }
  return (
    <div
      role="status"
      className="mb-4 flex gap-2 rounded-lg border border-line bg-warning-tint px-3 py-2 text-fg"
    >
      <CalendarClock className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      <div className="text-caption">
        <p className="font-heading font-bold uppercase tracking-label">{title}</p>
        <p className="mt-1">{detail}</p>
      </div>
    </div>
  );
}
