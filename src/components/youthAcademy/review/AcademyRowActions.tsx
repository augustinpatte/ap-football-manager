import { useTranslation } from "react-i18next";

import { TRAINING_FOCUS_IDS } from "../../../lib/trainingFocus";
import type { AcademyRow } from "../../../services/academyService";
import type { PlayerSelectionOptions } from "../../../store/gameStore";
import { Button, Select } from "../../ui";

interface AcademyRowActionsProps {
  row: AcademyRow;
  busy: boolean;
  /** The id of the list that explains why promotion is blocked or advised against. */
  promoteReasonsId: string;
  onPromote: () => void;
  onToggleLoanList: () => void;
  onSetTrainingFocus: (focus: string | null) => void;
  onSelectPlayer?: (id: string, options?: PlayerSelectionOptions) => void;
}

/**
 * Every action the backend supports on an academy player, each one enabled exactly when the review's
 * advice says the game would allow it: promote, loan-list, an individual training focus, a new
 * contract (the player's own renewal flow), and release (his termination flow).
 */
export default function AcademyRowActions({
  row,
  busy,
  promoteReasonsId,
  onPromote,
  onToggleLoanList,
  onSetTrainingFocus,
  onSelectPlayer,
}: AcademyRowActionsProps) {
  const { t } = useTranslation();
  const promoteBlocked = row.advice.promote.state === "blocked";
  const loanBlocked = row.advice.loan.state === "blocked";

  return (
    <div className="flex flex-wrap justify-end gap-2">
      <Button
        size="sm"
        disabled={busy || promoteBlocked}
        aria-describedby={row.advice.promote.reasons.length > 0 ? promoteReasonsId : undefined}
        aria-label={t("academy.promoteFor", { player: row.full_name })}
        onClick={onPromote}
      >
        {t("youthAcademy.promoteToSeniorSquad")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={busy || loanBlocked}
        aria-label={t(row.loan_listed ? "academy.unlistLoanFor" : "academy.listLoanFor", {
          player: row.full_name,
        })}
        onClick={onToggleLoanList}
      >
        {row.loan_listed ? t("academy.unlistLoan") : t("academy.listLoan")}
      </Button>
      {row.advice.renew && onSelectPlayer && (
        <Button
          size="sm"
          variant="outline"
          aria-label={t("academy.renewFor", { player: row.full_name })}
          onClick={() => onSelectPlayer(row.player_id, { openRenewal: true })}
        >
          {t("academy.renew")}
        </Button>
      )}
      <Select
        value={row.training_focus ?? ""}
        onChange={(event) => onSetTrainingFocus(event.target.value || null)}
        disabled={busy}
        variant="muted"
        selectSize="xs"
        aria-label={t("academy.trainingFocusFor", { player: row.full_name })}
        title={t("academy.trainingFocus")}
        wrapperClassName="w-full max-w-32"
      >
        <option value="">{t("training.groups.teamDefault")}</option>
        {TRAINING_FOCUS_IDS.map((focusId) => (
          <option key={focusId} value={focusId}>
            {t(`training.focuses.${focusId}.label`)}
          </option>
        ))}
      </Select>
      {onSelectPlayer && (
        <Button
          size="sm"
          variant="ghost"
          aria-label={t("academy.releaseFor", { player: row.full_name })}
          onClick={() => onSelectPlayer(row.player_id, { openTermination: true })}
        >
          {t("academy.release")}
        </Button>
      )}
    </div>
  );
}
