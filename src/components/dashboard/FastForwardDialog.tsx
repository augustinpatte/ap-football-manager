import type { JSX } from "react";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button, DatePicker, Dialog } from "../ui";
import type { FastForwardState } from "../../hooks/useFastForward";
import { FAST_FORWARD_PRESETS, addDays, stopCanResume } from "../../hooks/useFastForward.helpers";
import type {
  FastForwardDecisionKind,
  FastForwardPolicy,
  FastForwardResponse,
  FastForwardStop,
} from "../../services/fastForwardService";
import { resolveBackendError } from "../../utils/backendI18n";

const POLICIES: { value: FastForwardPolicy; label: string; description: string }[] = [
  {
    value: "stop_for_decisions",
    label: "fastForward.policy.decisions",
    description: "fastForward.policy.decisionsDesc",
  },
  {
    value: "stop_at_next_match",
    label: "fastForward.policy.nextMatch",
    description: "fastForward.policy.nextMatchDesc",
  },
  {
    value: "to_target_date",
    label: "fastForward.policy.target",
    description: "fastForward.policy.targetDesc",
  },
];

const PRESET_LABELS = {
  week: "fastForward.preset.week",
  month: "fastForward.preset.month",
  threeMonths: "fastForward.preset.threeMonths",
} as const;

const STOP_TITLES: Record<FastForwardStop["kind"], string> = {
  target_reached: "fastForward.result.targetReached",
  next_match: "fastForward.result.nextMatch",
  decisions: "fastForward.result.decisions",
  season_complete: "fastForward.result.seasonComplete",
  manager_fired: "fastForward.result.managerFired",
  cancelled: "fastForward.result.cancelled",
  day_limit: "fastForward.result.dayLimit",
  error: "fastForward.result.error",
};

const DECISION_LABELS: Record<FastForwardDecisionKind, string> = {
  blocker: "fastForward.decision.blocker",
  delegation_proposal: "fastForward.decision.delegationProposal",
  incoming_transfer_offer: "fastForward.decision.incomingTransferOffer",
  incoming_loan_offer: "fastForward.decision.incomingLoanOffer",
  transfer_deadline: "fastForward.decision.transferDeadline",
};

const FIELD_FOCUS =
  "focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-panel";

function ConfigureView({ state, onClose }: { state: FastForwardState; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <>
      <h2
        id="fast-forward-title"
        className="font-heading text-title font-bold uppercase tracking-label text-fg"
      >
        {t("fastForward.title")}
      </h2>

      <div className="mt-4">
        <p id="fast-forward-target" className="text-caption font-medium text-fg-muted">
          {t("fastForward.targetLabel")}
        </p>
        <div className="mt-1">
          <DatePicker
            value={state.target}
            onChange={state.setTarget}
            labelledBy="fast-forward-target"
            error={!state.targetIsValid}
          />
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          {FAST_FORWARD_PRESETS.map((preset) => (
            <Button
              key={preset.id}
              variant="outline"
              size="sm"
              onClick={() => state.setTarget(addDays(state.today, preset.days))}
            >
              {t(PRESET_LABELS[preset.id])}
            </Button>
          ))}
        </div>
        <p
          role={state.targetIsValid ? undefined : "alert"}
          className={`mt-2 text-caption ${state.targetIsValid ? "text-fg-muted" : "text-danger"}`}
        >
          {state.targetIsValid
            ? t("fastForward.daysAhead", { days: state.daysAhead })
            : t("fastForward.invalidTarget")}
        </p>
      </div>

      <fieldset className="mt-4">
        <legend className="text-caption font-medium text-fg-muted">
          {t("fastForward.policyLabel")}
        </legend>
        <div className="mt-1 flex flex-col gap-2">
          {POLICIES.map((policy) => (
            <label
              key={policy.value}
              className="flex cursor-pointer items-start gap-3 rounded-lg border border-line px-3 py-2 hover:bg-hover has-[:checked]:border-primary-500 has-[:checked]:bg-hover"
            >
              <input
                type="radio"
                name="fast-forward-policy"
                value={policy.value}
                checked={state.policy === policy.value}
                onChange={() => state.setPolicy(policy.value)}
                className={`mt-1 ${FIELD_FOCUS}`}
              />
              <span>
                <span className="block text-body font-medium text-fg">{t(policy.label)}</span>
                <span className="block text-caption text-fg-muted">{t(policy.description)}</span>
              </span>
            </label>
          ))}
        </div>
      </fieldset>

      {state.error ? (
        <p role="alert" className="mt-3 text-caption text-danger">
          {resolveBackendError(state.error)}
        </p>
      ) : null}

      <div className="mt-6 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => void state.start()} disabled={!state.targetIsValid}>
          {t("fastForward.start")}
        </Button>
      </div>
    </>
  );
}

function RunningView({ state }: { state: FastForwardState }) {
  const { t } = useTranslation();
  const progress = state.progress;
  const done = progress?.days_done ?? 0;
  const total = progress?.days_total ?? 0;
  const percent = total > 0 ? Math.round((done / total) * 100) : 0;
  return (
    <>
      <h2
        id="fast-forward-title"
        className="flex items-center gap-2 font-heading text-title font-bold uppercase tracking-label text-fg"
      >
        <Loader2 className="h-5 w-5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        {t("fastForward.title")}
      </h2>
      <div className="mt-4" aria-live="polite">
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          aria-label={t("fastForward.title")}
          className="h-2.5 overflow-hidden rounded-full bg-line"
        >
          <div className="h-full rounded-full bg-primary-500" style={{ width: `${percent}%` }} />
        </div>
        <p className="mt-2 text-body text-fg">
          {t("fastForward.progressDays", { done, total })}
          {progress ? ` · ${progress.current_date}` : ""}
        </p>
        <p className="text-caption text-fg-muted">
          {t("fastForward.progressMatches", { matches: progress?.user_matches_played ?? 0 })}
        </p>
      </div>
      <div className="mt-6 flex justify-end">
        <Button variant="outline" onClick={state.cancel} disabled={state.isCancelling}>
          {state.isCancelling ? t("fastForward.stopping") : t("fastForward.stop")}
        </Button>
      </div>
    </>
  );
}

function DecisionList({
  title,
  decisions,
}: {
  title: string;
  decisions: { kind: FastForwardDecisionKind; id: string; date?: string }[];
}) {
  const { t } = useTranslation();
  if (decisions.length === 0) return null;
  return (
    <div className="mt-3">
      <p className="text-caption font-medium text-fg-muted">{title}</p>
      <ul className="mt-1 flex flex-col gap-1">
        {decisions.map((decision) => (
          <li key={`${decision.kind}-${decision.id}`} className="text-body text-fg">
            {t(DECISION_LABELS[decision.kind])}
            {decision.date ? ` · ${decision.date}` : ""}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ResultBody({ response }: { response: FastForwardResponse }) {
  const { t } = useTranslation();
  const { progress, stop } = response;
  return (
    <>
      <p className="mt-2 text-body text-fg-muted">
        {t("fastForward.summary", {
          days: progress.days_done,
          from: progress.start_date,
          to: progress.current_date,
        })}
      </p>
      <p className="text-caption text-fg-muted">
        {t("fastForward.progressMatches", { matches: progress.user_matches_played })}
      </p>
      {progress.seasons_rolled_over > 0 ? (
        <p className="text-caption text-fg-muted">
          {t("fastForward.seasonsRolled", { seasons: progress.seasons_rolled_over })}
        </p>
      ) : null}
      {stop.kind === "decisions" ? (
        <DecisionList title={t("fastForward.decisionsTitle")} decisions={stop.decisions} />
      ) : null}
      <DecisionList
        title={t("fastForward.passedTitle")}
        decisions={response.passed_decisions.map((passed) => ({
          kind: passed.decision.kind,
          id: passed.decision.id,
          date: passed.date,
        }))}
      />
      {stop.kind === "error" ? (
        <p role="alert" className="mt-3 text-caption text-danger">
          {resolveBackendError(stop.error)}
        </p>
      ) : null}
    </>
  );
}

function ResultView({ state, onClose }: { state: FastForwardState; onClose: () => void }) {
  const { t } = useTranslation();
  const response = state.result;
  if (!response) return null;
  const stop = response.stop;
  return (
    <>
      <h2
        id="fast-forward-title"
        className="font-heading text-title font-bold uppercase tracking-label text-fg"
      >
        {t(STOP_TITLES[stop.kind], { date: response.progress.current_date })}
      </h2>
      <ResultBody response={response} />
      <div className="mt-6 flex justify-end gap-2">
        {stopCanResume(stop) ? (
          <Button variant="outline" onClick={() => void state.resume()}>
            {t("fastForward.resume")}
          </Button>
        ) : null}
        <Button onClick={onClose}>{t("common.close")}</Button>
      </div>
    </>
  );
}

export default function FastForwardDialog({
  state,
}: {
  state: FastForwardState;
}): JSX.Element | null {
  if (state.phase === "closed") return null;
  return (
    <Dialog
      labelledBy="fast-forward-title"
      onClose={state.close}
      busy={state.phase === "running"}
      className="mx-4 w-full max-w-lg rounded-2xl p-6"
    >
      {state.phase === "configure" ? <ConfigureView state={state} onClose={state.close} /> : null}
      {state.phase === "running" ? <RunningView state={state} /> : null}
      {state.phase === "result" ? <ResultView state={state} onClose={state.close} /> : null}
    </Dialog>
  );
}
