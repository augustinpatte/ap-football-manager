import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";

import type { PlayerData, TeamData, TransferOfferData } from "../../store/gameStore";
import {
  formatDateFull,
  formatExactMoney,
  formatVal,
  getTeamName,
  positionBadgeVariant,
} from "../../lib/helpers";
import type {
  TransferBidProjectionData,
  TransferNegotiationResponseData,
} from "../../services/transfersService";
import NegotiationFeedbackPanel, {
  type NegotiationFeedbackPanelData,
} from "../NegotiationFeedbackPanel";
import { Badge, Checkbox, Dialog, Select } from "../ui";
import {
  MAX_INSTALMENTS,
  MAX_PLAYER_CONTRACT_YEARS,
  MAX_SELL_ON_PCT,
  parseBidTerms,
  type BidTermsInput,
} from "./bidTerms";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import { naturalPositionOf } from "../../lib/positions";
import TransferNegotiationHistory from "./TransferNegotiationHistory";
import { playerHasPendingRegistration } from "./TransfersTab.model";

export interface TransferBidFormProps {
  bidTarget: PlayerData;
  teams: TeamData[];
  bidAmount: string;
  onBidAmountChange: (value: string) => void;
  /** What the manager attaches to the fee: instalments, a sell-on clause, a bonus, a promise. */
  bidTerms?: BidTermsInput;
  onBidTermsChange?: (terms: BidTermsInput) => void;
  myTeam: TeamData | null;
  bidFee: number | null;
  bidProjection: TransferBidProjectionData["projection"] | null;
  bidFeedback: NegotiationFeedbackPanelData | null;
  activeBidOffer: TransferOfferData | null;
  hasExistingOffer: boolean;
  /** The day broken-off talks can resume: when set it replaces the bid form. */
  talksResumeOn?: string | null;
  bidResult: TransferNegotiationResponseData["decision"] | "error" | null;
  /** The translated reason a bid failed. Rendered in place of the bare word "error". */
  bidError?: string | null;
  bidLoading: boolean;
  bidSubmitDisabled: boolean;
  blockingTitle?: string | null;
  blockingDetail?: string | null;
  showPlayerSummary?: boolean;
  onSubmit: () => void;
  onClose: () => void;
}

type TransferBidModalProps = TransferBidFormProps;

export function TransferBidForm({
  bidTarget,
  teams,
  bidAmount,
  onBidAmountChange,
  bidTerms,
  onBidTermsChange,
  myTeam,
  bidFee,
  bidProjection,
  bidFeedback,
  activeBidOffer,
  hasExistingOffer,
  talksResumeOn = null,
  bidResult,
  bidError,
  bidLoading,
  bidSubmitDisabled,
  blockingTitle = null,
  blockingDetail = null,
  showPlayerSummary = true,
  onSubmit,
  onClose,
}: TransferBidFormProps) {
  const { t, i18n } = useTranslation();
  const titleId = `transfer-bid-modal-title-${bidTarget.id}`;
  // A player with an agreed deal awaiting registration is off the market — the
  // engine refuses a new bid. Surface it here so reaching this form from a
  // profile / squad / scouting action explains itself instead of dead-ending.
  const pendingRegistration = playerHasPendingRegistration(bidTarget);
  // The form says what is wrong with the terms rather than leaving Submit dead.
  const termsInvalid = bidTerms ? !parseBidTerms(bidTerms).valid : false;
  const outlook = bidProjection?.player_outlook ?? null;
  // When the player is the one holding the deal up, the feedback carries the terms he asks for.
  const demandedWage = Number(bidFeedback?.params?.wage ?? "");
  const demandedYears = Number(bidFeedback?.params?.years ?? "");
  const playerAsks =
    bidFeedback?.headline_key === "transfers.transferFeedbackPlayerWantsMoreHeadline" &&
    demandedWage > 0 &&
    demandedYears > 0;

  return (
    <>
      <h3
        id={titleId}
        className="text-sm font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-3"
      >
        {t("transfers.makeBid")}
      </h3>

      {showPlayerSummary ? (
        <div className="flex items-center gap-3 mb-4">
          <Badge variant={positionBadgeVariant(naturalPositionOf(bidTarget))} size="sm">
            {translatePositionAbbreviation(t, naturalPositionOf(bidTarget))}
          </Badge>
          <div>
            <p className="font-semibold text-sm text-gray-800 dark:text-gray-200">
              {bidTarget.full_name}
            </p>
            <p className="text-xs text-gray-400">
              {getTeamName(teams, bidTarget.team_id)} •{" "}
              {t("transfers.playerValue", {
                value: formatVal(bidTarget.market_value),
              })}
            </p>
          </div>
        </div>
      ) : null}
      {blockingTitle ? (
        <div
          role="alert"
          className="mb-4 flex gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-red-700 dark:border-red-500/30 dark:bg-red-500/10 dark:text-red-200"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="text-xs">
            <p className="font-heading font-bold uppercase tracking-wider">{blockingTitle}</p>
            {blockingDetail ? <p className="mt-1">{blockingDetail}</p> : null}
          </div>
        </div>
      ) : null}
      {pendingRegistration ? (
        <div
          role="alert"
          className="mb-4 flex gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <p className="text-xs">{t("transfers.bidBlockedPendingRegistration")}</p>
        </div>
      ) : null}
      {hasExistingOffer ? (
        <p className="text-xs text-gray-500 dark:text-gray-400 mb-3">
          {t("transfers.resumeNegotiationHint")}
        </p>
      ) : null}
      {talksResumeOn ? (
        <p role="status" className="text-xs text-amber-700 dark:text-amber-200 mb-3">
          {t("transfers.talksResumeOn", { date: formatDateFull(talksResumeOn, i18n.language) })}
        </p>
      ) : (
        <>
          <label
            htmlFor="bid-amount"
            className="text-xs font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-1 block"
          >
            {t("transfers.bidAmount")}
          </label>
          <input
            id="bid-amount"
            type="number"
            step="0.1"
            min="0"
            value={bidAmount}
            onChange={(event) => onBidAmountChange(event.target.value)}
            className="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-navy-700 border border-gray-200 dark:border-navy-600 text-sm text-gray-800 dark:text-gray-200 mb-3 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
          />
          {bidTerms && onBidTermsChange ? (
            <fieldset
              className="mb-3 rounded-lg border border-line bg-sunken p-3"
              disabled={bidLoading || bidResult === "accepted"}
            >
              <legend className="px-1 text-caption font-heading font-bold uppercase tracking-label text-fg-muted">
                {t("transfers.terms.title")}
              </legend>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label
                    htmlFor="bid-instalments"
                    className="text-caption text-fg-muted block mb-1"
                  >
                    {t("transfers.terms.instalments")}
                  </label>
                  <Select
                    id="bid-instalments"
                    selectSize="sm"
                    fullWidth
                    value={bidTerms.instalments}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, instalments: event.target.value })
                    }
                  >
                    {Array.from({ length: MAX_INSTALMENTS }, (_, index) => (
                      <option key={index + 1} value={String(index + 1)}>
                        {index === 0
                          ? t("transfers.terms.instalmentsSingle")
                          : t("transfers.terms.instalmentsCount", { count: index + 1 })}
                      </option>
                    ))}
                  </Select>
                </div>
                <div>
                  <label htmlFor="bid-sell-on" className="text-caption text-fg-muted block mb-1">
                    {t("transfers.terms.sellOn")}
                  </label>
                  <input
                    id="bid-sell-on"
                    type="number"
                    min="0"
                    max={MAX_SELL_ON_PCT}
                    step="1"
                    value={bidTerms.sellOnPct}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, sellOnPct: event.target.value })
                    }
                    aria-invalid={termsInvalid}
                    aria-describedby={termsInvalid ? "bid-terms-error" : undefined}
                    className="w-full px-3 py-2 rounded-lg bg-panel border border-line text-body text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-sunken disabled:opacity-50"
                  />
                </div>
                <div className="col-span-2">
                  <label htmlFor="bid-bonus" className="text-caption text-fg-muted block mb-1">
                    {t("transfers.terms.signingBonus")}
                  </label>
                  <input
                    id="bid-bonus"
                    type="number"
                    min="0"
                    step="0.05"
                    value={bidTerms.signingBonusMillions}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, signingBonusMillions: event.target.value })
                    }
                    aria-invalid={termsInvalid}
                    aria-describedby={termsInvalid ? "bid-terms-error" : undefined}
                    className="w-full px-3 py-2 rounded-lg bg-panel border border-line text-body text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-sunken disabled:opacity-50"
                  />
                </div>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-3">
                <div>
                  <label
                    htmlFor="bid-player-wage"
                    className="text-caption text-fg-muted block mb-1"
                  >
                    {t("transfers.playerTerms.wage")}
                  </label>
                  <input
                    id="bid-player-wage"
                    type="number"
                    min="0"
                    step="100"
                    placeholder={outlook ? String(outlook.expected_wage) : undefined}
                    value={bidTerms.playerWage}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, playerWage: event.target.value })
                    }
                    aria-invalid={termsInvalid}
                    aria-describedby={termsInvalid ? "bid-terms-error" : undefined}
                    className="w-full px-3 py-2 rounded-lg bg-panel border border-line text-body text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-sunken disabled:opacity-50"
                  />
                </div>
                <div>
                  <label
                    htmlFor="bid-player-years"
                    className="text-caption text-fg-muted block mb-1"
                  >
                    {t("transfers.playerTerms.years")}
                  </label>
                  <input
                    id="bid-player-years"
                    type="number"
                    min="0"
                    max={MAX_PLAYER_CONTRACT_YEARS}
                    step="1"
                    placeholder={outlook ? String(outlook.expected_years) : undefined}
                    value={bidTerms.playerYears}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, playerYears: event.target.value })
                    }
                    aria-invalid={termsInvalid}
                    aria-describedby={termsInvalid ? "bid-terms-error" : undefined}
                    className="w-full px-3 py-2 rounded-lg bg-panel border border-line text-body text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-sunken disabled:opacity-50"
                  />
                </div>
              </div>
              {outlook ? (
                <p role="status" className="mt-2 text-caption text-fg-muted">
                  {t(
                    outlook.interest === "keen"
                      ? "transfers.playerTerms.outlookKeen"
                      : outlook.interest === "hesitant"
                        ? "transfers.playerTerms.outlookHesitant"
                        : "transfers.playerTerms.outlookUnlikely",
                  )}{" "}
                  {t("transfers.playerTerms.expects", {
                    wage: formatExactMoney(outlook.expected_wage),
                    years: outlook.expected_years,
                  })}
                </p>
              ) : null}
              {playerAsks ? (
                <button
                  type="button"
                  onClick={() =>
                    onBidTermsChange({
                      ...bidTerms,
                      playerWage: String(demandedWage),
                      playerYears: String(demandedYears),
                    })
                  }
                  className="mt-2 rounded-lg bg-primary-700 px-3 py-1.5 text-caption font-heading font-bold uppercase tracking-label text-white hover:bg-primary-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-sunken"
                >
                  {t("transfers.playerTerms.meetTerms")}
                </button>
              ) : null}
              {bidProjection?.stepping_stone_available ? (
                <div className="mt-3 flex items-start gap-3">
                  <Checkbox
                    id="bid-promise"
                    checked={bidTerms.promise}
                    onChange={(event) =>
                      onBidTermsChange({ ...bidTerms, promise: event.target.checked })
                    }
                    aria-describedby="bid-promise-hint"
                  />
                  <div>
                    <label htmlFor="bid-promise" className="text-body font-semibold text-fg">
                      {t("transfers.promise.label")}
                    </label>
                    <p id="bid-promise-hint" className="text-caption text-fg-muted">
                      {t("transfers.promise.bidHint")}
                    </p>
                  </div>
                </div>
              ) : null}
              {termsInvalid ? (
                <p id="bid-terms-error" role="alert" className="mt-2 text-caption text-danger">
                  {t("transfers.terms.invalid", {
                    instalments: MAX_INSTALMENTS,
                    sellOn: MAX_SELL_ON_PCT,
                  })}
                </p>
              ) : (
                <p className="mt-2 text-caption text-fg-muted">
                  {t("transfers.terms.hint")} {t("transfers.playerTerms.hint")}
                </p>
              )}
            </fieldset>
          ) : null}
          {myTeam && bidFee !== null && bidProjection ? (
            <div className="rounded-lg border border-gray-200 dark:border-navy-700 bg-white/70 dark:bg-navy-900/40 p-3 mb-3 space-y-2">
              <p className="text-[11px] font-heading font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400">
                {t("transfers.bidImpactTitle")}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t(
                  bidProjection.pending_registration_date
                    ? "transfers.bidImpactTransferBudgetDeferred"
                    : "transfers.bidImpactTransferBudget",
                  {
                    before: formatExactMoney(bidProjection.transfer_budget_before),
                    after: formatExactMoney(bidProjection.transfer_budget_after),
                    date: bidProjection.pending_registration_date ?? "",
                  },
                )}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t(
                  bidProjection.pending_registration_date
                    ? "transfers.bidImpactBalanceDeferred"
                    : "transfers.bidImpactBalance",
                  {
                    before: formatExactMoney(bidProjection.finance_before),
                    after: formatExactMoney(bidProjection.finance_after),
                    date: bidProjection.pending_registration_date ?? "",
                  },
                )}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t("transfers.bidImpactWageBill", {
                  before: formatExactMoney(bidProjection.current_weekly_wage_spend),
                  after: formatExactMoney(bidProjection.projected_weekly_wage_spend),
                })}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t("transfers.bidImpactWeeklyWageBudget", {
                  budget: formatExactMoney(bidProjection.weekly_wage_budget),
                })}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t("transfers.bidImpactIncomingWage", {
                  wage: formatExactMoney(bidProjection.incoming_player_weekly_wage),
                })}
              </p>
              <p className="text-xs text-gray-600 dark:text-gray-300">
                {t("transfers.bidImpactWagePressure", {
                  percent: bidProjection.projected_wage_budget_usage_pct,
                })}
              </p>
              {(bidProjection.later_instalments ?? 0) > 0 ? (
                <p className="text-caption text-fg-muted">
                  {t("transfers.terms.projectionInstalments", {
                    now: formatExactMoney(bidProjection.cash_due_on_completion ?? 0),
                    count: bidProjection.later_instalments ?? 0,
                    amount: formatExactMoney(bidProjection.later_instalment_amount ?? 0),
                  })}
                </p>
              ) : null}
              {(bidProjection.signing_bonus ?? 0) > 0 ? (
                <p className="text-caption text-fg-muted">
                  {t("transfers.terms.projectionBonus", {
                    amount: formatExactMoney(bidProjection.signing_bonus ?? 0),
                  })}
                </p>
              ) : null}
              {bidProjection.exceeds_transfer_budget ? (
                <p className="text-xs text-red-600 dark:text-red-300">
                  {t("transfers.bidImpactOverTransferBudget")}
                </p>
              ) : null}
              {bidProjection.blocked_by_wage_policy ? (
                <p className="text-xs text-red-600 dark:text-red-300">
                  {t("transfers.bidImpactOverWagePolicy")}
                </p>
              ) : null}
              {bidProjection.exceeds_finance ? (
                <p className="text-xs text-red-600 dark:text-red-300">
                  {t("transfers.bidImpactOverBalance")}
                </p>
              ) : null}
            </div>
          ) : null}
        </>
      )}
      <NegotiationFeedbackPanel
        feedback={bidFeedback}
        titleKey="transfers.negotiationPulse"
        roundKey="transfers.negotiationRound"
        patienceKey="transfers.negotiationPatience"
        tensionKey="transfers.negotiationTension"
        className="mb-3"
      />
      <TransferNegotiationHistory offer={activeBidOffer} mode="outgoing" />
      {bidResult ? (
        <div
          className={`text-xs font-heading font-bold uppercase tracking-wider mb-3 ${bidResult === "accepted" ? "text-green-500" : bidResult === "rejected" ? "text-red-600 dark:text-red-300" : "text-amber-500"}`}
        >
          {bidResult === "accepted"
            ? t("transfers.bidAccepted")
            : bidResult === "rejected"
              ? t("transfers.bidRejected")
              : bidResult === "counter_offer"
                ? t("transfers.bidCountered")
                : bidResult === "player_talks"
                  ? t("transfers.bidPlayerTalks")
                  : bidError}
        </div>
      ) : null}
      <div className="flex gap-2">
        {talksResumeOn ? null : (
          <button
            type="button"
            onClick={onSubmit}
            disabled={bidSubmitDisabled || pendingRegistration}
            className="flex-1 py-2 bg-primary-700 hover:bg-primary-800 text-white rounded-lg font-heading font-bold text-sm uppercase tracking-wider transition-colors disabled:opacity-50"
          >
            {bidLoading
              ? t("transfers.submitting")
              : bidResult === "player_talks"
                ? t("transfers.submitPlayerTerms")
                : t("transfers.submitBid")}
          </button>
        )}
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

export default function TransferBidModal(props: TransferBidModalProps) {
  const titleId = `transfer-bid-modal-title-${props.bidTarget.id}`;

  return (
    <Dialog
      labelledBy={titleId}
      onClose={props.onClose}
      dismissOnBackdrop
      className="w-full max-w-sm rounded-xl p-6"
    >
      <TransferBidForm {...props} />
    </Dialog>
  );
}
