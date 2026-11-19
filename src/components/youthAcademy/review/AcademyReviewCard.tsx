import { useState } from "react";
import { useTranslation } from "react-i18next";

import { formatDate } from "../../../lib/dateFormatting";
import { positionBadgeVariant } from "../../../lib/helpers";
import type { AcademyRow, AcademyVerdict } from "../../../services/academyService";
import { setPlayerSquadRole } from "../../../services/squadService";
import { toggleLoanList } from "../../../services/transfersService";
import type { GameStateData, PlayerSelectionOptions } from "../../../store/gameStore";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../../utils/errorMessage";
import { translatePositionAbbreviation } from "../../squad/SquadTab.helpers";
import { Badge, Button, Card, CardBody, CardHeader } from "../../ui";
import { useAcademyReview } from "./useAcademyReview";

interface AcademyReviewCardProps {
  /** The game the screen shows; a new one refreshes the review. */
  gameState: GameStateData | null;
  onGameUpdate?: (game: GameStateData) => void;
  onSelectPlayer?: (id: string, options?: PlayerSelectionOptions) => void;
}

const VERDICT_BADGE: Record<AcademyVerdict, "success" | "primary" | "neutral" | "danger"> = {
  ready_for_senior: "success",
  loan_candidate: "primary",
  keep_developing: "neutral",
  unlikely_to_progress: "danger",
};

const HEAD =
  "py-2 px-2 text-left text-caption font-heading font-bold uppercase tracking-label text-fg-muted";
const CELL = "py-2 px-2 text-body text-fg align-top";
const LINK =
  "text-left font-heading font-bold text-fg hover:text-positive rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page";

/** Where each academy player stands, in terms the manager can act on, with the reasons shown. */
export default function AcademyReviewCard({
  gameState,
  onGameUpdate,
  onSelectPlayer,
}: AcademyReviewCardProps) {
  const { t, i18n } = useTranslation();
  const { review, loading, error, refresh } = useAcademyReview(gameState);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const act = async (playerId: string, run: () => Promise<GameStateData>) => {
    setBusyId(playerId);
    setActionError(null);
    try {
      // Run first: an absent callback must not skip the action itself.
      const updated = await run();
      onGameUpdate?.(updated);
      refresh();
    } catch (err) {
      setActionError(resolveTranslatedErrorMessage(getErrorMessage(err), t));
    } finally {
      setBusyId(null);
    }
  };

  const renderActions = (row: AcademyRow) => (
    <div className="flex flex-wrap justify-end gap-2">
      <Button
        size="sm"
        disabled={busyId === row.player_id || row.promotion_blocked !== null || row.on_loan}
        title={row.promotion_blocked ? t("academy.promotionBlocked") : undefined}
        onClick={() => void act(row.player_id, () => setPlayerSquadRole(row.player_id, "Senior"))}
      >
        {t("youthAcademy.promoteToSeniorSquad")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={busyId === row.player_id || row.on_loan}
        onClick={() => void act(row.player_id, () => toggleLoanList(row.player_id))}
      >
        {row.loan_listed ? t("academy.unlistLoan") : t("academy.listLoan")}
      </Button>
      {onSelectPlayer && (
        <Button
          size="sm"
          variant="ghost"
          aria-label={`${t("academy.release")} ${row.full_name}`}
          onClick={() => onSelectPlayer(row.player_id, { openTermination: true })}
        >
          {t("academy.release")}
        </Button>
      )}
    </div>
  );

  return (
    <Card accent="primary">
      <CardHeader>{t("academy.reviewTitle")}</CardHeader>
      <CardBody className="flex flex-col gap-3">
        <p className="text-body text-fg-muted">{t("academy.reviewHint")}</p>
        {error || actionError ? (
          <p
            role="alert"
            className="text-caption font-heading font-bold uppercase tracking-label text-danger"
          >
            {actionError ?? error}
          </p>
        ) : null}
        {loading && !review ? (
          <p role="status" className="text-body text-fg-muted">
            {t("academy.loading")}
          </p>
        ) : null}
        {review && (
          <>
            <p className="text-body text-fg">
              {t("academy.capacity", { seniors: review.seniors, limit: review.senior_limit })} ·{" "}
              {t("academy.nextIntake", { count: review.next_intake })}
            </p>
            {review.rows.length === 0 ? (
              <p className="text-body text-fg-subtle">{t("youthAcademy.noYouthPlayers")}</p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-line">
                      <th className={HEAD}>{t("youthAcademy.player")}</th>
                      <th className={`${HEAD} text-center`}>{t("youthAcademy.age")}</th>
                      <th className={`${HEAD} text-center`}>{t("youthAcademy.ovr")}</th>
                      <th className={`${HEAD} text-center`}>{t("academy.ceiling")}</th>
                      <th className={`${HEAD} text-center`}>{t("academy.expectedNext")}</th>
                      <th className={`${HEAD} text-center`}>{t("academy.squadCover")}</th>
                      <th className={HEAD}>{t("academy.verdict")}</th>
                      <th className={`${HEAD} text-right`}>{t("scouting.action")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {review.rows.map((row) => (
                      <tr key={row.player_id} className="border-b border-line">
                        <td className={CELL}>
                          <button
                            type="button"
                            className={LINK}
                            onClick={() => onSelectPlayer?.(row.player_id)}
                          >
                            {row.full_name}
                          </button>
                          <div className="mt-1 flex items-center gap-2">
                            <Badge variant={positionBadgeVariant(row.natural_position)} size="sm">
                              {translatePositionAbbreviation(t, row.natural_position)}
                            </Badge>
                            {row.contract_end && (
                              <span className="text-caption text-fg-subtle">
                                {formatDate(row.contract_end, i18n.language)}
                              </span>
                            )}
                          </div>
                        </td>
                        <td className={`${CELL} text-center`}>{row.age}</td>
                        <td className={`${CELL} text-center tabular-nums`}>{row.ovr}</td>
                        <td className={`${CELL} text-center tabular-nums`}>{row.potential}</td>
                        <td className={`${CELL} text-center tabular-nums`}>
                          {row.expected_ovr_next_year}
                        </td>
                        <td className={`${CELL} text-center tabular-nums`}>
                          {row.squad_cover > 0 ? row.squad_cover : "—"}
                        </td>
                        <td className={CELL}>
                          <Badge variant={VERDICT_BADGE[row.verdict]} size="sm">
                            {t(`academy.verdicts.${row.verdict}`)}
                          </Badge>
                          <ul className="mt-1 list-disc pl-4 text-caption text-fg-muted">
                            {row.reasons.map((reason) => (
                              <li key={reason}>{t(`academy.reason.${reason}`)}</li>
                            ))}
                          </ul>
                        </td>
                        <td className={`${CELL} text-right`}>{renderActions(row)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <p className="text-caption text-fg-subtle">{t("academy.ceilingNote")}</p>
          </>
        )}
      </CardBody>
    </Card>
  );
}
