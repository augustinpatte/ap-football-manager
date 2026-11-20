import { useTranslation } from "react-i18next";

import type { ActionAdvice, AdviceState, RowAdvice } from "../../../services/academyService";
import { Badge } from "../../ui";

const STATE_BADGE: Record<AdviceState, "success" | "neutral" | "danger"> = {
  recommended: "success",
  available: "neutral",
  blocked: "danger",
};

type ActionKey = "promote" | "loan" | "keep" | "renew";

interface AcademyAdviceListProps {
  advice: RowAdvice;
  /** Gives the promotion's reasons an id the Promote button can point `aria-describedby` at. */
  promoteReasonsId: string;
}

/**
 * What the review makes of each action open on a youngster: advised, possible or blocked, and why.
 * The state is a word as well as a colour, and the reasons are the backend's own refusals and
 * verdicts, so a disabled button always has its explanation on the screen.
 */
export default function AcademyAdviceList({ advice, promoteReasonsId }: AcademyAdviceListProps) {
  const { t } = useTranslation();
  const entries: [ActionKey, ActionAdvice][] = [
    ["promote", advice.promote],
    ["loan", advice.loan],
    ["keep", advice.keep],
  ];
  if (advice.renew) entries.push(["renew", advice.renew]);

  return (
    <ul className="flex flex-col gap-2">
      {entries.map(([action, entry]) => (
        <li key={action} className="flex flex-col gap-1">
          <span className="flex items-center gap-2">
            <Badge variant={STATE_BADGE[entry.state]} size="sm">
              {t(`academy.advice.state.${entry.state}`)}
            </Badge>
            <span className="text-caption font-bold text-fg">
              {t(`academy.advice.action.${action}`)}
            </span>
          </span>
          {entry.reasons.length > 0 && (
            <ul
              id={action === "promote" ? promoteReasonsId : undefined}
              className="list-disc pl-4 text-caption text-fg-muted"
            >
              {entry.reasons.map((reason) => (
                <li key={reason}>{t(`academy.advice.${reason}`)}</li>
              ))}
            </ul>
          )}
        </li>
      ))}
    </ul>
  );
}
