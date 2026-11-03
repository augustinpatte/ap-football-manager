import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { GameStateData } from "../../store/gameStore";
import {
  type AssistantRecommendation,
  type DelegationLogEntry,
  type Duty,
  type StaffOverview,
  applyAssistantRecommendation,
  getAssistantRecommendations,
  resolveDelegationProposal,
  setDelegationLimits,
  setDelegationMode,
  setDutyDelegation,
} from "../../services/staffService";
import { resolveBackendError } from "../../utils/backendI18n";
import { formatExactMoney, formatWeeklyWage } from "../../lib/helpers";
import { Badge, Button, Card, CardBody, Checkbox, ProgressBar } from "../ui";
import DelegationModeCard from "./DelegationModeCard";

interface StaffManagementPanelProps {
  overview: StaffOverview | null;
  onGameUpdate: (state: GameStateData) => void;
}

const DUTIES: Duty[] = [
  "Training",
  "MatchPreparation",
  "Scouting",
  "Youth",
  "StaffRecruitment",
  "StaffRenewals",
  "Lineup",
  "PlayerRenewals",
  "PlayerRecruitment",
  "Tactics",
];

/** Backend params carry raw enum names; the ones that name a post or a position are translated. */
function useParams() {
  const { t } = useTranslation();
  return (params: [string, string][]): Record<string, string> =>
    Object.fromEntries(
      params.map(([key, value]) => [
        key,
        key === "post"
          ? t(`staff.posts.${value}`, { defaultValue: value })
          : key === "position"
            ? t(`common.positions.${value}`, { defaultValue: value })
            : key === "focus"
              ? t(`training.focuses.${value}.label`, { defaultValue: value })
              : key === "intensity"
                ? t(`training.intensities.${value}.label`, { defaultValue: value })
                : key === "style"
                  ? t(`common.playStyles.${value}`, { defaultValue: value })
                  : key === "fee" && Number.isFinite(Number(value))
                    ? formatExactMoney(Number(value))
                    : value,
      ]),
    );
}

export default function StaffManagementPanel({
  overview,
  onGameUpdate,
}: StaffManagementPanelProps) {
  const { t } = useTranslation();
  const named = useParams();
  const [recs, setRecs] = useState<AssistantRecommendation[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const proposalsCount = overview?.delegation.proposals.length ?? 0;
  const refreshRecs = useCallback(() => {
    getAssistantRecommendations()
      .then(setRecs)
      .catch(() => setRecs([]));
  }, []);
  // Recommendations are recomputed from the game; a resolved proposal or a new day changes them.
  useEffect(() => {
    refreshRecs();
  }, [refreshRecs, proposalsCount, overview?.has_assistant]);

  const run = async (id: string, action: () => Promise<GameStateData>) => {
    setBusy(id);
    setError("");
    try {
      onGameUpdate(await action());
      refreshRecs();
    } catch (err) {
      setError(resolveBackendError(err));
    } finally {
      setBusy(null);
    }
  };

  if (!overview) return null;
  const { budget, delegation, posts } = overview;
  const usedPct = budget.budget > 0 ? Math.min(100, (budget.weekly_bill / budget.budget) * 100) : 0;
  const logTitle = (entry: DelegationLogEntry) => {
    // A resolved proposal is logged as "<proposal key>.accepted"; it reads as the proposal itself.
    const key = entry.title_key.replace(/\.accepted$/, "");
    return t(key, named(entry.params));
  };

  return (
    <div className="grid gap-4">
      {error ? (
        <p role="alert" className="text-body text-danger">
          {error}
        </p>
      ) : null}

      <Card>
        <CardBody>
          <h3 className="font-heading text-lead uppercase">{t("staff.mgmt.budget")}</h3>
          <p className="mt-1 text-body text-fg-muted">
            {t("staff.mgmt.budgetUsed", {
              bill: formatWeeklyWage(budget.weekly_bill, "compact"),
              budget: formatWeeklyWage(budget.budget, "compact"),
            })}
          </p>
          <ProgressBar value={usedPct} variant={usedPct >= 90 ? "accent" : "primary"} size="sm" />
          <p className="mt-1 text-caption text-fg-muted">
            {t("staff.mgmt.headroom", { amount: formatWeeklyWage(budget.headroom, "compact") })}
          </p>
          <ul className="mt-3 grid gap-1 sm:grid-cols-2">
            {posts.map((status) => (
              <li key={status.post} className="flex items-center justify-between text-body">
                <span>{t(`staff.posts.${status.post}`)}</span>
                <span className="flex items-center gap-2">
                  <span className="tabular-nums text-fg-muted">
                    {status.count}/{status.limit}
                  </span>
                  {status.count < status.required ? (
                    <Badge variant="danger" size="sm">
                      {t("staff.mgmt.vacant")}
                    </Badge>
                  ) : null}
                </span>
              </li>
            ))}
          </ul>
        </CardBody>
      </Card>

      <DelegationModeCard
        mode={overview.delegation_mode}
        limits={delegation.limits}
        hasAssistant={overview.has_assistant}
        busy={busy !== null}
        onModeChange={(mode) => void run("mode", () => setDelegationMode(mode))}
        onLimitsSave={(limits) => void run("limits", () => setDelegationLimits(limits))}
      />

      <Card>
        <CardBody>
          <h3 className="font-heading text-lead uppercase">{t("staff.delegation.title")}</h3>
          <p className="mt-1 text-body text-fg-muted">
            {overview.has_assistant
              ? t("staff.delegation.intro")
              : t("staff.delegation.noAssistant")}
          </p>
          <ul className="mt-3 grid gap-2">
            {DUTIES.map((duty) => {
              const setting = delegation.settings.find((s) => s.duty === duty);
              const delegated = setting?.delegated ?? false;
              const confirm = setting?.confirm_major ?? true;
              return (
                <li
                  key={duty}
                  data-testid={`duty-${duty}`}
                  className="flex flex-wrap items-center justify-between gap-2"
                >
                  <span className="text-body">{t(`staff.delegation.duties.${duty}`)}</span>
                  <span className="flex items-center gap-3">
                    {delegated ? (
                      <span className="flex items-center gap-2 text-caption text-fg-muted">
                        <Checkbox
                          checked={confirm}
                          aria-label={t("staff.delegation.confirmMajor")}
                          onChange={(event) =>
                            void run(`confirm-${duty}`, () =>
                              setDutyDelegation(duty, true, event.target.checked),
                            )
                          }
                        />
                        {t("staff.delegation.confirmMajor")}
                      </span>
                    ) : null}
                    <Button
                      size="sm"
                      variant={delegated ? "outline" : "primary"}
                      disabled={busy !== null || (!delegated && !overview.has_assistant)}
                      onClick={() =>
                        void run(`duty-${duty}`, () => setDutyDelegation(duty, !delegated, confirm))
                      }
                    >
                      {t(delegated ? "staff.delegation.takeBack" : "staff.delegation.delegate")}
                    </Button>
                  </span>
                </li>
              );
            })}
          </ul>
        </CardBody>
      </Card>

      <Card>
        <CardBody>
          <h3 className="font-heading text-lead uppercase">{t("staff.delegation.proposals")}</h3>
          {delegation.proposals.length === 0 ? (
            <p className="mt-1 text-body text-fg-muted">{t("staff.delegation.noProposals")}</p>
          ) : (
            <ul className="mt-2 grid gap-3">
              {delegation.proposals.map((proposal) => (
                <li
                  key={proposal.id}
                  data-testid={`proposal-${proposal.id}`}
                  className="grid gap-1"
                >
                  <p className="text-body font-medium">
                    {t(proposal.title_key, named(proposal.params))}
                  </p>
                  <p className="text-caption text-fg-muted">
                    {t(proposal.reason_key, named(proposal.params))}
                  </p>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      disabled={busy !== null}
                      onClick={() =>
                        void run(proposal.id, () => resolveDelegationProposal(proposal.id, true))
                      }
                    >
                      {t("staff.delegation.accept")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy !== null}
                      onClick={() =>
                        void run(proposal.id, () => resolveDelegationProposal(proposal.id, false))
                      }
                    >
                      {t("staff.delegation.decline")}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          <h4 className="mt-4 text-body font-medium">{t("staff.delegation.log")}</h4>
          {delegation.log.length === 0 ? (
            <p className="text-caption text-fg-muted">{t("staff.delegation.noLog")}</p>
          ) : (
            <ul className="mt-1 grid gap-1 text-caption text-fg-muted">
              {[...delegation.log]
                .reverse()
                .slice(0, 8)
                .map((entry, index) => (
                  <li key={`${entry.date}-${index}`}>
                    {entry.date.slice(0, 10)} · {logTitle(entry)}
                    {entry.outcome === "executed" ? null : (
                      <>
                        {" · "}
                        <span>{t(`staff.delegation.outcome.${entry.outcome}`)}</span>
                      </>
                    )}
                  </li>
                ))}
            </ul>
          )}
        </CardBody>
      </Card>

      <Card>
        <CardBody>
          <h3 className="font-heading text-lead uppercase">{t("staff.assistant.title")}</h3>
          {recs.length === 0 ? (
            <p className="mt-1 text-body text-fg-muted">{t("staff.assistant.none")}</p>
          ) : (
            <ul className="mt-2 grid gap-3">
              {recs.map((rec) => (
                <li key={rec.id} data-testid={`rec-${rec.id}`} className="grid gap-1">
                  <p className="flex items-center gap-2 text-body font-medium">
                    <Badge
                      variant={
                        rec.severity >= 3 ? "danger" : rec.severity === 2 ? "primary" : "neutral"
                      }
                      size="sm"
                    >
                      {t(`staff.assistant.severity.${rec.severity}`)}
                    </Badge>
                    {t(rec.title_key, named(rec.params))}
                  </p>
                  <p className="text-caption text-fg-muted">
                    {t(rec.reason_key, named(rec.params))}
                  </p>
                  <div>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={busy !== null}
                      onClick={() => void run(rec.id, () => applyAssistantRecommendation(rec.id))}
                    >
                      {t("staff.assistant.apply")}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </CardBody>
      </Card>
    </div>
  );
}
