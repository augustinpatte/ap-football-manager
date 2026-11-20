import { Save } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui";
import type { MatchSaveStatus } from "./useMatchSave";

interface MatchSaveControlsProps {
  status: MatchSaveStatus;
  /** Another action (finishing the match, a save already running) holds the match. */
  disabled: boolean;
  onSave: () => void;
  onSaveAndExit: () => void;
}

/** Save the career in place, or save it and go back to the menu, from inside a live match. */
export function MatchSaveControls({
  status,
  disabled,
  onSave,
  onSaveAndExit,
}: MatchSaveControlsProps) {
  const { t } = useTranslation();
  const saving = status === "saving";
  const exiting = status === "exiting";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          icon={<Save aria-hidden="true" />}
          onClick={onSave}
          disabled={disabled || saving || exiting}
          aria-busy={saving}
        >
          {saving ? t("match.interactive.saving") : t("match.interactive.saveGame")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={onSaveAndExit}
          disabled={disabled || saving || exiting}
          aria-busy={exiting}
        >
          {exiting ? t("match.interactive.exiting") : t("match.interactive.saveExit")}
        </Button>
      </div>
      {/* Only while there is something to say: the screen keeps a single standing live region. */}
      {status === "saved" && (
        <span role="status" className="text-caption text-fg-muted">
          {t("match.interactive.saved")}
        </span>
      )}
    </div>
  );
}
