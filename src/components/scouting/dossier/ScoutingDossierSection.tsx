import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { GameStateData } from "../../../store/gameStore";
import {
  shortlistPlayer,
  unshortlistPlayer,
  type ScoutingTargetView,
} from "../../../services/scoutingService";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../../utils/errorMessage";
import { Button, Card, CardBody, CardHeader } from "../../ui";
import CompareDialog from "./CompareDialog";
import { COMPARE_MAX, COMPARE_MIN, toggleCompared } from "./dossierFormat";
import TargetDetailDialog from "./TargetDetailDialog";
import TargetTable from "./TargetTable";
import type { useScoutingDossier } from "./useScoutingDossier";

interface ScoutingDossierSectionProps {
  /** The dossier the screen loaded (`useScoutingDossier`), shared with the search below it. */
  dossierState: ReturnType<typeof useScoutingDossier>;
  onGameUpdate: (state: GameStateData) => void;
  canScout: boolean;
  onScoutAgain: (playerId: string) => void;
  onBid?: (playerId: string) => void;
  onOfferContract?: (playerId: string) => void;
  onOpenProfile?: (playerId: string) => void;
}

/** The reports the club holds and the shortlist it follows, with the comparison between them. */
export default function ScoutingDossierSection({
  dossierState,
  onGameUpdate,
  canScout,
  onScoutAgain,
  onBid,
  onOfferContract,
  onOpenProfile,
}: ScoutingDossierSectionProps) {
  const { t } = useTranslation();
  const { dossier, loading, error, refresh } = dossierState;
  const [opened, setOpened] = useState<ScoutingTargetView | null>(null);
  const [compared, setCompared] = useState<string[]>([]);
  const [comparing, setComparing] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const everyTarget = useMemo(() => {
    const byId = new Map<string, ScoutingTargetView>();
    for (const view of [...(dossier?.reports ?? []), ...(dossier?.shortlist ?? [])]) {
      byId.set(view.player_id, view);
    }
    return byId;
  }, [dossier]);
  // Kept as last seen when a refresh no longer lists him (he was just taken off the shortlist), so
  // the dialog the manager is in does not vanish under them.
  const openedTarget = opened ? (everyTarget.get(opened.player_id) ?? opened) : null;
  const comparedTargets = compared
    .map((id) => everyTarget.get(id))
    .filter((view): view is ScoutingTargetView => view !== undefined);

  const toggleShortlist = async (target: ScoutingTargetView) => {
    setBusyId(target.player_id);
    setActionError(null);
    try {
      const updated = target.shortlisted
        ? await unshortlistPlayer(target.player_id)
        : await shortlistPlayer(target.player_id);
      onGameUpdate(updated);
      refresh();
    } catch (err) {
      setActionError(resolveTranslatedErrorMessage(getErrorMessage(err), t));
    } finally {
      setBusyId(null);
    }
  };

  const compareButton = (
    <Button
      size="sm"
      variant="outline"
      disabled={compared.length < COMPARE_MIN}
      onClick={() => setComparing(true)}
    >
      {t("scouting.dossier.compare", { count: compared.length })}
    </Button>
  );

  return (
    <>
      {actionError || error ? (
        <p role="alert" className="text-body font-bold text-danger">
          {actionError ?? error}
        </p>
      ) : null}

      <Card>
        <CardHeader action={compareButton}>{t("scouting.dossier.shortlistTitle")}</CardHeader>
        <CardBody>
          <p className="mb-3 text-body text-fg-muted">
            {t("scouting.dossier.shortlistHint", { max: COMPARE_MAX })}
          </p>
          {loading && !dossier ? (
            <p role="status" className="text-body text-fg-muted">
              {t("scouting.dossier.loading")}
            </p>
          ) : (dossier?.shortlist.length ?? 0) === 0 ? (
            <p className="text-body text-fg-subtle">{t("scouting.dossier.noShortlist")}</p>
          ) : (
            <TargetTable
              targets={dossier?.shortlist ?? []}
              compared={compared}
              onToggleCompared={(id) => setCompared((current) => toggleCompared(current, id))}
              onOpen={(target) => setOpened(target)}
              onToggleShortlist={(target) => void toggleShortlist(target)}
              busyId={busyId}
            />
          )}
        </CardBody>
      </Card>

      <Card>
        <CardHeader>{t("scouting.dossier.reportsTitle")}</CardHeader>
        <CardBody>
          <p className="mb-3 text-body text-fg-muted">{t("scouting.dossier.reportsHint")}</p>
          {loading && !dossier ? null : (dossier?.reports.length ?? 0) === 0 ? (
            <p className="text-body text-fg-subtle">{t("scouting.dossier.noReports")}</p>
          ) : (
            <TargetTable
              targets={dossier?.reports ?? []}
              compared={compared}
              onToggleCompared={(id) => setCompared((current) => toggleCompared(current, id))}
              onOpen={(target) => setOpened(target)}
              onToggleShortlist={(target) => void toggleShortlist(target)}
              busyId={busyId}
            />
          )}
        </CardBody>
      </Card>

      {openedTarget && (
        <TargetDetailDialog
          target={openedTarget}
          canScout={canScout}
          onClose={() => setOpened(null)}
          onToggleShortlist={(target) => void toggleShortlist(target)}
          onScoutAgain={onScoutAgain}
          onBid={
            onBid
              ? (playerId) => {
                  setOpened(null);
                  onBid(playerId);
                }
              : undefined
          }
          onOfferContract={
            onOfferContract
              ? (playerId) => {
                  setOpened(null);
                  onOfferContract(playerId);
                }
              : undefined
          }
          onOpenProfile={onOpenProfile}
        />
      )}
      {comparing && comparedTargets.length >= COMPARE_MIN && (
        <CompareDialog targets={comparedTargets} onClose={() => setComparing(false)} />
      )}
    </>
  );
}
