import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Badge, Button, Card, CardBody, CardHeader, ProgressBar, Select } from "../ui";
import type {
  MatchPlayerView,
  MatchTeamView,
  PlanView,
  PlayStyleName,
  PressingName,
  TacticalChange,
  TempoName,
} from "../../services/interactiveMatchService";
import { changedTactics } from "./interactiveMatch.helpers";

/** A caption tied to its control by id, so the control is named for assistive technology. */
export function Field({ label, children }: { label: string; children: (id: string) => ReactNode }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-caption text-fg-muted">
        {label}
      </label>
      {children(id)}
    </div>
  );
}

const PLAY_STYLES: PlayStyleName[] = [
  "Defensive",
  "Counter",
  "Balanced",
  "Possession",
  "HighPress",
  "Attacking",
];
const PRESSING: PressingName[] = ["Passive", "Medium", "Aggressive"];
const TEMPOS: TempoName[] = ["Patient", "Direct"];

function PlayerRow({ player }: { player: MatchPlayerView }) {
  const { t } = useTranslation();
  const condition = Math.round(player.condition * 100);
  return (
    <li className="flex h-row items-center gap-2 text-body">
      <span className="w-8 shrink-0 text-caption text-fg-muted">{player.role ?? ""}</span>
      <span className="min-w-0 flex-1 truncate text-fg">{player.name}</span>
      {player.yellow_cards > 0 && <Badge variant="accent">{player.yellow_cards}</Badge>}
      {player.injured && <Badge variant="danger">+</Badge>}
      <span className="flex w-24 shrink-0 items-center gap-2">
        <span className="sr-only">{t("match.replayCondition.title")}</span>
        <ProgressBar value={condition} className="flex-1" />
        <span className="w-7 text-right text-caption text-fg-muted">{condition}</span>
      </span>
    </li>
  );
}

/** One team as the engine has it now: who is on the pitch, with condition, and who has left. */
export function TeamLineup({ name, team }: { name: string; team: MatchTeamView }) {
  const { t } = useTranslation();
  const gone = [...team.used, ...team.sent_off];
  return (
    <Card>
      <CardHeader>
        <span className="font-heading text-heading uppercase tracking-label text-fg">{name}</span>
        <span className="ml-2 text-caption text-fg-muted">{team.formation}</span>
      </CardHeader>
      <CardBody>
        <h3 className="text-caption font-bold uppercase tracking-label text-fg-muted">
          {t("match.replayLineup.onPitch")}
        </h3>
        <ul>
          {team.on_pitch.map((p) => (
            <PlayerRow key={p.id} player={p} />
          ))}
        </ul>
        {gone.length > 0 && (
          <>
            <h3 className="mt-2 text-caption font-bold uppercase tracking-label text-fg-muted">
              {t("match.replayLineup.gone")}
            </h3>
            <ul className="text-body text-fg-muted">
              {gone.map((p) => (
                <li key={p.id}>
                  {p.name} ·{" "}
                  {team.sent_off.some((s) => s.id === p.id)
                    ? t("match.replayLineup.sentOff")
                    : t("match.replayLineup.substituted")}
                </li>
              ))}
            </ul>
          </>
        )}
      </CardBody>
    </Card>
  );
}

interface TacticsPanelProps {
  plan: PlanView;
  supportedFormations: string[];
  disabled: boolean;
  onApply: (change: TacticalChange) => void;
}

/** Mentality, pressing, tempo and formation, in the game's own words; only changes are sent. */
export function TacticsPanel({ plan, supportedFormations, disabled, onApply }: TacticsPanelProps) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<PlanView>(plan);
  const [synced, setSynced] = useState(plan);
  if (synced !== plan) {
    // the plan changed underneath (the assistant, or an order just accepted): follow it
    setSynced(plan);
    setDraft(plan);
  }
  const change = changedTactics(plan, draft, supportedFormations);
  return (
    <Card>
      <CardHeader>
        <span className="font-heading text-heading uppercase tracking-label text-fg">
          {t("match.teamControls")}
        </span>
      </CardHeader>
      <CardBody>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("match.playStyle")}>
            {(id) => (
              <Select
                id={id}
                value={draft.style}
                onChange={(e) => setDraft({ ...draft, style: e.target.value as PlayStyleName })}
                disabled={disabled}
              >
                {PLAY_STYLES.map((s) => (
                  <option key={s} value={s}>
                    {t(`common.playStyles.${s}`)}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t("tactics.phaseSettings.pressingIntensity")}>
            {(id) => (
              <Select
                id={id}
                value={draft.pressing}
                onChange={(e) => setDraft({ ...draft, pressing: e.target.value as PressingName })}
                disabled={disabled}
              >
                {PRESSING.map((p) => (
                  <option key={p} value={p}>
                    {t(`tactics.phaseSettings.pressingIntensity_${p}`)}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t("tactics.phaseSettings.tempo")}>
            {(id) => (
              <Select
                id={id}
                value={draft.tempo}
                onChange={(e) => setDraft({ ...draft, tempo: e.target.value as TempoName })}
                disabled={disabled}
              >
                {TEMPOS.map((p) => (
                  <option key={p} value={p}>
                    {t(`tactics.phaseSettings.tempo_${p}`)}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t("match.formation")}>
            {(id) => (
              <Select
                id={id}
                value={draft.formation}
                onChange={(e) => setDraft({ ...draft, formation: e.target.value })}
                disabled={disabled}
              >
                {[...new Set([plan.formation, ...supportedFormations])].map((f) => (
                  <option key={f} value={f} disabled={!supportedFormations.includes(f)}>
                    {f}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        <Button
          size="sm"
          className="mt-3"
          disabled={disabled || change === null}
          onClick={() => change && onApply(change)}
        >
          {t("match.interactive.applyTactics")}
        </Button>
      </CardBody>
    </Card>
  );
}

interface SubstitutionPanelProps {
  team: MatchTeamView;
  names: Record<string, string>;
  disabled: boolean;
  onSubstitute: (off: string, on: string) => void;
  onCancel: (off: string) => void;
}

/** Pick who goes off and who comes on; the engine decides when, and refuses what is illegal. */
export function SubstitutionPanel({
  team,
  names,
  disabled,
  onSubstitute,
  onCancel,
}: SubstitutionPanelProps) {
  const { t } = useTranslation();
  const [off, setOff] = useState("");
  const [on, setOn] = useState("");
  const queuedOff = new Set(team.pending_substitutions.map((p) => p.off));
  const queuedOn = new Set(team.pending_substitutions.map((p) => p.on));
  const max = team.substitutions_used + team.substitutions_remaining;
  return (
    <Card>
      <CardHeader>
        <span className="font-heading text-heading uppercase tracking-label text-fg">
          {t("match.substitutionsTitle")}
        </span>
        <span className="ml-2 text-caption text-fg-muted">
          {t("match.subsUsed", { used: team.substitutions_used, max })}
        </span>
      </CardHeader>
      <CardBody>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("match.selectPlayerOff")}>
            {(id) => (
              <Select
                id={id}
                value={off}
                onChange={(e) => setOff(e.target.value)}
                disabled={disabled}
              >
                <option value="" />
                {team.on_pitch
                  .filter((p) => !queuedOff.has(p.id))
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </Select>
            )}
          </Field>
          <Field label={t("match.selectReplacement")}>
            {(id) => (
              <Select
                id={id}
                value={on}
                onChange={(e) => setOn(e.target.value)}
                disabled={disabled}
              >
                <option value="" />
                {team.bench
                  .filter((p) => !queuedOn.has(p.id))
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </Select>
            )}
          </Field>
        </div>
        <Button
          size="sm"
          className="mt-3"
          disabled={disabled || !off || !on}
          onClick={() => {
            onSubstitute(off, on);
            setOff("");
            setOn("");
          }}
        >
          {t("match.confirmSubstitution")}
        </Button>
        <p className="mt-2 text-caption text-fg-muted">{t("match.interactive.pendingNote")}</p>
        {team.pending_substitutions.length > 0 && (
          <>
            <h3 className="mt-2 text-caption font-bold uppercase tracking-label text-fg-muted">
              {t("match.queuedSubstitutions")}
            </h3>
            <ul>
              {team.pending_substitutions.map((p) => (
                <li key={p.off} className="flex items-center justify-between text-body text-fg">
                  <span>
                    {names[p.off] ?? p.off} → {names[p.on] ?? p.on}
                  </span>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => onCancel(p.off)}
                    aria-label={t("match.removeQueuedSubstitution", {
                      off: names[p.off] ?? p.off,
                      on: names[p.on] ?? p.on,
                    })}
                  >
                    ×
                  </Button>
                </li>
              ))}
            </ul>
          </>
        )}
      </CardBody>
    </Card>
  );
}

/** The match's key events, newest first. */
export function EventFeed({ lines }: { lines: string[] }) {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader>
        <span className="font-heading text-heading uppercase tracking-label text-fg">
          {t("match.keyEvents")}
        </span>
      </CardHeader>
      <CardBody>
        {lines.length === 0 ? (
          <p className="text-body text-fg-muted">{t("match.noEventsYet")}</p>
        ) : (
          <ol className="flex max-h-64 flex-col-reverse gap-1 overflow-y-auto text-body text-fg">
            {lines.map((line, i) => (
              <li key={`${i}-${line}`}>{line}</li>
            ))}
          </ol>
        )}
      </CardBody>
    </Card>
  );
}
