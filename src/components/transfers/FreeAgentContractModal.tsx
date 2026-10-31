import { useTranslation } from "react-i18next";

import type { PlayerData, TeamData } from "../../store/gameStore";
import type { FreeAgentContractProjection } from "../../services/freeAgentService";
import type { NegotiationFeedbackPanelData } from "../NegotiationFeedbackPanel";
import NegotiationFeedbackPanel from "../NegotiationFeedbackPanel";
import { Badge, Checkbox, Dialog } from "../ui";
import { formatExactMoney, formatVal, getTeamName, positionBadgeVariant } from "../../lib/helpers";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import { naturalPositionOf } from "../../lib/positions";
import { resolveTranslatedErrorMessage } from "../../utils/errorMessage";

const MAX_CONTRACT_YEARS = 5;

export interface FreeAgentContractFormProps {
  player: PlayerData;
  teams: TeamData[];
  wage: string;
  onWageChange: (value: string) => void;
  contractLength: string;
  onContractLengthChange: (value: string) => void;
  /** Paid on signing, in whole euros, on top of the wage. */
  signingBonus?: string;
  onSigningBonusChange?: (value: string) => void;
  /** A translation key: why that bonus cannot be paid. */
  signingBonusError?: string | null;
  /** The player would value the promise; the option is shown only then. */
  steppingStoneAvailable?: boolean;
  promiseSteppingStone?: boolean;
  onPromiseSteppingStoneChange?: (value: boolean) => void;
  projection: FreeAgentContractProjection | null;
  feedback: NegotiationFeedbackPanelData | null | undefined;
  statusMessage: string | null;
  statusClassName: string;
  submitting: boolean;
  submitDisabled: boolean;
  showPlayerSummary?: boolean;
  onSubmit: () => void;
  onClose: () => void;
}

type FreeAgentContractModalProps = FreeAgentContractFormProps;

export function FreeAgentContractForm({
  player,
  teams,
  wage,
  onWageChange,
  contractLength,
  onContractLengthChange,
  signingBonus,
  onSigningBonusChange,
  signingBonusError = null,
  steppingStoneAvailable = false,
  promiseSteppingStone = false,
  onPromiseSteppingStoneChange,
  projection,
  feedback,
  statusMessage,
  statusClassName,
  submitting,
  submitDisabled,
  showPlayerSummary = true,
  onSubmit,
  onClose,
}: FreeAgentContractFormProps) {
  const { t } = useTranslation();
  const titleId = `free-agent-contract-title-${player.id}`;

  // Cash runway is a week count, or null/undefined when cash-flow is positive
  // (no finite runway → "stable"). Shared by the before/after readouts.
  const formatRunway = (weeks: number | null | undefined): string =>
    weeks === null || weeks === undefined
      ? t("finances.runwayStable")
      : t("finances.runwayWeeks", { count: weeks });

  return (
    <>
      <h3
        id={titleId}
        className="text-sm font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-3"
      >
        {t("transfers.offerContract")}
      </h3>
      {showPlayerSummary ? (
        <div className="flex items-center gap-3 mb-4">
          <Badge variant={positionBadgeVariant(naturalPositionOf(player))} size="sm">
            {translatePositionAbbreviation(t, naturalPositionOf(player))}
          </Badge>
          <div>
            <p className="font-semibold text-sm text-gray-800 dark:text-gray-200">
              {player.full_name}
            </p>
            <p className="text-xs text-gray-400">
              {player.team_id ? getTeamName(teams, player.team_id) : t("common.freeAgent")} •{" "}
              {t("transfers.playerValue", {
                value: formatVal(player.market_value),
              })}
            </p>
          </div>
        </div>
      ) : null}

      <label
        htmlFor="free-agent-wage"
        className="text-xs font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-1 block"
      >
        {t("playerProfile.renewalWage")}
      </label>
      <input
        id="free-agent-wage"
        type="number"
        min="0"
        step="1000"
        value={wage}
        onChange={(event) => onWageChange(event.target.value)}
        className="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-navy-700 border border-gray-200 dark:border-navy-600 text-sm text-gray-800 dark:text-gray-200 mb-3 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
      />

      <label
        htmlFor="free-agent-years"
        className="text-xs font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-1 block"
      >
        {t("playerProfile.renewalLength")}
      </label>
      <input
        id="free-agent-years"
        type="number"
        min="1"
        max={String(MAX_CONTRACT_YEARS)}
        step="1"
        value={contractLength}
        onChange={(event) => onContractLengthChange(event.target.value)}
        className="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-navy-700 border border-gray-200 dark:border-navy-600 text-sm text-gray-800 dark:text-gray-200 mb-3 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
      />

      {signingBonus !== undefined && onSigningBonusChange ? (
        <>
          <label
            htmlFor="free-agent-bonus"
            className="text-caption font-heading font-bold uppercase tracking-label text-fg-muted mb-1 block"
          >
            {t("transfers.terms.signingBonusEuros")}
          </label>
          <input
            id="free-agent-bonus"
            type="number"
            min="0"
            step="10000"
            value={signingBonus}
            onChange={(event) => onSigningBonusChange(event.target.value)}
            aria-invalid={signingBonusError !== null}
            aria-describedby={signingBonusError ? "free-agent-bonus-error" : undefined}
            className="w-full px-3 py-2 rounded-lg bg-panel border border-line text-body text-fg mb-1 focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
          />
          {signingBonusError ? (
            <p id="free-agent-bonus-error" role="alert" className="mb-2 text-caption text-danger">
              {resolveTranslatedErrorMessage(signingBonusError, t)}
            </p>
          ) : (
            <p className="mb-3 text-caption text-fg-muted">{t("transfers.terms.bonusHint")}</p>
          )}
        </>
      ) : null}

      {steppingStoneAvailable && onPromiseSteppingStoneChange ? (
        <div className="mb-3 flex items-start gap-3 rounded-lg border border-line bg-sunken p-3">
          <Checkbox
            id="free-agent-stepping-stone"
            checked={promiseSteppingStone}
            onChange={(event) => onPromiseSteppingStoneChange(event.target.checked)}
            disabled={submitting}
            aria-describedby="free-agent-stepping-stone-hint"
          />
          <div>
            <label htmlFor="free-agent-stepping-stone" className="text-body font-semibold text-fg">
              {t("transfers.promise.label")}
            </label>
            <p id="free-agent-stepping-stone-hint" className="text-caption text-fg-muted">
              {t("transfers.promise.hint")}
            </p>
          </div>
        </div>
      ) : null}

      {projection ? (
        <div className="rounded-lg border border-gray-200 dark:border-navy-700 bg-white/70 dark:bg-navy-900/40 p-3 mb-3 space-y-2">
          <p className="text-[11px] font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400">
            {t("playerProfile.renewalProjectionTitle")}
          </p>
          <p className="text-xs text-gray-600 dark:text-gray-300">
            {t("playerProfile.renewalProjectionWageBill", {
              before: formatExactMoney(projection.current_weekly_wage_spend),
              after: formatExactMoney(projection.projected_weekly_wage_spend),
            })}
          </p>
          <p className="text-xs text-gray-600 dark:text-gray-300">
            {t("playerProfile.renewalProjectionBudgetUsage", {
              before: Math.round(
                (projection.current_annual_wage_bill / Math.max(projection.annual_wage_budget, 1)) *
                  100,
              ),
              after: Math.round(
                (projection.projected_annual_wage_bill /
                  Math.max(projection.annual_wage_budget, 1)) *
                  100,
              ),
            })}
          </p>
          <p className="text-xs text-gray-600 dark:text-gray-300">
            {t("playerProfile.renewalProjectionRunway", {
              before: formatRunway(projection.current_cash_runway_weeks),
              after: formatRunway(projection.projected_cash_runway_weeks),
            })}
          </p>
          {!projection.policy_allows ? (
            <p className="text-xs text-red-600 dark:text-red-300">
              {t("playerProfile.renewalBudgetWarning")}
            </p>
          ) : null}
        </div>
      ) : null}

      <NegotiationFeedbackPanel
        feedback={feedback ?? null}
        titleKey="playerProfile.renewalConversationTitle"
        roundKey="playerProfile.renewalRound"
        patienceKey="playerProfile.renewalPatience"
        tensionKey="playerProfile.renewalTension"
        className="mb-3"
      />

      {statusMessage ? (
        <div
          className={`text-xs font-heading font-bold uppercase tracking-wider mb-3 ${statusClassName}`}
        >
          {statusMessage}
        </div>
      ) : null}

      <div className="flex gap-2">
        <button
          type="button"
          onClick={onSubmit}
          disabled={submitDisabled}
          className="flex-1 py-2 bg-primary-700 hover:bg-primary-800 text-white rounded-lg font-heading font-bold text-sm uppercase tracking-wider transition-colors disabled:opacity-50"
        >
          {submitting ? t("transfers.submitting") : t("playerProfile.renewalSubmit")}
        </button>
        <button
          type="button"
          onClick={onClose}
          className="px-4 py-2 bg-gray-200 dark:bg-navy-700 text-gray-600 dark:text-gray-300 rounded-lg font-heading font-bold text-sm uppercase tracking-wider hover:bg-gray-300 dark:hover:bg-navy-600 transition-colors"
        >
          {t("transfers.close")}
        </button>
      </div>
    </>
  );
}

export default function FreeAgentContractModal(props: FreeAgentContractModalProps) {
  const titleId = `free-agent-contract-title-${props.player.id}`;

  return (
    <Dialog
      labelledBy={titleId}
      onClose={props.onClose}
      dismissOnBackdrop
      className="w-full max-w-sm rounded-xl p-6"
    >
      <FreeAgentContractForm {...props} />
    </Dialog>
  );
}
