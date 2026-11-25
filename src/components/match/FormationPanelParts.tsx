import { useTranslation } from "react-i18next";
import type {
  MatchPlayerView,
  MatchTeamView,
  PendingSubstitution,
} from "../../services/interactiveMatchService";
import { Badge, Button, ProgressBar } from "../ui";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import { type DepartedStatus, departedStatus } from "./formationPanel.helpers";

/** What a player can be doing in the match, in the words the panel uses for it. */
export function usePlayerStatus() {
  const { t } = useTranslation();
  return (team: MatchTeamView, player: MatchPlayerView): string | null => {
    const queued = team.pending_substitutions.find((p) => p.off === player.id);
    if (queued) return t("match.formationPanel.status.awaitingStoppage");
    if (player.injured) return t("common.injured");
    return null;
  };
}

const DEPARTED_VARIANT: Record<DepartedStatus, "danger" | "neutral"> = {
  sent_off: "danger",
  injured: "danger",
  substituted: "neutral",
};

/** Who has left the match and why: dismissed, injured or substituted, never mixed up. */
export function DepartedList({ team }: { team: MatchTeamView }) {
  const { t } = useTranslation();
  const gone = [...team.sent_off, ...team.used];
  if (gone.length === 0) return null;
  return (
    <section aria-labelledby="formation-panel-gone">
      <h3
        id="formation-panel-gone"
        className="text-caption font-bold uppercase tracking-label text-fg-muted"
      >
        {t("match.replayLineup.gone")}
      </h3>
      <ul className="mt-1 flex flex-col gap-1">
        {gone.map((player) => {
          const status = departedStatus(team, player);
          return (
            <li key={player.id} className="flex items-center justify-between gap-2 text-body">
              <span className="min-w-0 truncate text-fg-muted">{player.name}</span>
              <Badge variant={DEPARTED_VARIANT[status]}>
                {t(`match.formationPanel.departed.${status}`)}
              </Badge>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

interface QueueProps {
  pending: PendingSubstitution[];
  nameOf: (id: string) => string;
  disabled: boolean;
  onCancel: (offId: string) => void;
}

/** Substitutions the engine has accepted and will make at the next stoppage, each one cancellable. */
export function QueuedSubstitutions({ pending, nameOf, disabled, onCancel }: QueueProps) {
  const { t } = useTranslation();
  return (
    <section aria-labelledby="formation-panel-queue">
      <h3
        id="formation-panel-queue"
        className="text-caption font-bold uppercase tracking-label text-fg-muted"
      >
        {t("match.queuedSubstitutions")}
      </h3>
      {pending.length === 0 ? (
        <p className="mt-1 text-body text-fg-muted">{t("match.formationPanel.queueEmpty")}</p>
      ) : (
        <ul className="mt-1 flex flex-col gap-1">
          {pending.map((p) => (
            <li key={p.off} className="flex items-center justify-between gap-2 text-body text-fg">
              <span className="min-w-0 truncate">
                {nameOf(p.off)} → {nameOf(p.on)}
              </span>
              <Button
                size="sm"
                variant="ghost"
                disabled={disabled}
                onClick={() => onCancel(p.off)}
                aria-label={t("match.removeQueuedSubstitution", {
                  off: nameOf(p.off),
                  on: nameOf(p.on),
                })}
              >
                {t("match.formationPanel.cancelQueued")}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <p className="mt-1 text-caption text-fg-muted">{t("match.interactive.pendingNote")}</p>
    </section>
  );
}

interface DetailProps {
  player: MatchPlayerView;
  status: string | null;
  /** Who comes on for him at the next stoppage, when a substitution is queued. */
  replacementName?: string;
}

/** One player as the engine has him now: slot, condition, minutes, cards, injury. */
export function PlayerDetail({ player, status, replacementName }: DetailProps) {
  const { t } = useTranslation();
  const condition = Math.round(player.condition * 100);
  return (
    <div className="flex flex-col gap-2">
      <h3 className="font-heading text-heading text-fg">
        {player.shirt_number > 0 && <span className="text-fg-muted">#{player.shirt_number} </span>}
        {player.name}
      </h3>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-body">
        <dt className="text-fg-muted">{t("match.formationPanel.detail.position")}</dt>
        <dd className="text-fg">
          {translatePositionAbbreviation(t, player.role ?? player.natural_position)}
          {player.role && player.role !== player.natural_position && (
            <span className="text-fg-muted">
              {" "}
              (
              {t("match.formationPanel.detail.natural", {
                position: translatePositionAbbreviation(t, player.natural_position),
              })}
              )
            </span>
          )}
        </dd>
        <dt className="text-fg-muted">{t("match.replayCondition.title")}</dt>
        <dd className="flex items-center gap-2 text-fg">
          <ProgressBar value={condition} className="flex-1" />
          <span className="w-9 text-right">{condition}%</span>
        </dd>
        <dt className="text-fg-muted">{t("match.formationPanel.detail.minutes")}</dt>
        <dd className="text-fg">{Math.round(player.minutes_played)}</dd>
        <dt className="text-fg-muted">{t("match.yellowCards")}</dt>
        <dd className="text-fg">{player.yellow_cards}</dd>
      </dl>
      {status && (
        <p className="flex flex-wrap gap-2 text-body text-fg">
          <Badge variant={player.injured ? "danger" : "accent"}>{status}</Badge>
        </p>
      )}
      {replacementName && (
        <p className="text-body text-fg-muted">
          {t("match.formationPanel.detail.replacedBy", { name: replacementName })}
        </p>
      )}
    </div>
  );
}
