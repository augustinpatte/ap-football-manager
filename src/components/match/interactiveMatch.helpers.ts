import type { TFunction } from "i18next";
import type {
  InteractiveView,
  MatchKeyEvent,
  MatchSide,
  PlanView,
  TacticalChange,
} from "../../services/interactiveMatchService";

/** The match minute a clock reading is in (the engine's clock, never a screen timer). */
export function matchMinute(clockMs: number): number {
  return Math.floor(clockMs / 60_000);
}

/** Every player of both teams by id, from the engine's own lists. */
export function playerNames(view: InteractiveView): Record<string, string> {
  const names: Record<string, string> = {};
  for (const team of [view.session.home, view.session.away]) {
    for (const list of [team.on_pitch, team.bench, team.used, team.sent_off]) {
      for (const player of list) names[player.id] = player.name;
    }
  }
  return names;
}

const REJECTION_CODES = new Set([
  "stale_revision",
  "no_substitutions_left",
  "no_window_left",
  "goalkeeper_mismatch",
  "already_played",
  "not_on_pitch",
  "not_on_bench",
  "duplicate_request",
  "not_pending",
  "match_finished",
  "formation_needs_eleven_players",
  "unauthorized",
]);

/** The translated reason the engine gave for refusing an order. */
export function rejectionMessage(code: string, t: TFunction): string {
  const key = REJECTION_CODES.has(code) ? code : "other";
  return t(`match.interactive.rejection.${key}`);
}

interface EventContext {
  names: Record<string, string>;
  teamNames: Record<MatchSide, string>;
  regulationMinutes: number;
  t: TFunction;
}

/**
 * One line of the match feed for an event of the canonical stream, or `null` for the many
 * events a person does not need to read (passes, tackles, possession changes, ...). The words
 * are the replay viewer's: one vocabulary for the same events.
 */
export function eventLine(event: MatchKeyEvent, ctx: EventContext): string | null {
  const { names, teamNames, regulationMinutes, t } = ctx;
  const who = (id?: string) => (id ? (names[id] ?? t("match.replayUnknownPlayer")) : "");
  const team = event.side ? `${teamNames[event.side]}: ` : "";
  const minute = `${matchMinute(event.t_ms)}' `;
  switch (event.type) {
    case "goal":
      return minute + team + t("match.replayFeed.goal", { player: who(event.scorer) });
    case "card": {
      const key =
        event.colour === "red"
          ? "redCard"
          : event.colour === "second_yellow"
            ? "secondYellow"
            : "yellowCard";
      return minute + team + t(`match.replayFeed.${key}`, { player: who(event.player) });
    }
    case "substitution":
      return (
        minute +
        team +
        t("match.replayFeed.substitution", {
          player: who(event.player_on),
          other: who(event.player_off),
        })
      );
    case "injury":
      return minute + team + t("match.replayFeed.injury", { player: who(event.player) });
    case "tactical_change":
      return minute + team + t("match.interactive.eventTactical");
    case "period_end":
      // the end of regulation is reported by the match end itself
      return event.t_ms < regulationMinutes * 60_000
        ? minute + t("match.replayFeed.halfTime")
        : null;
    case "match_end":
      return minute + t("match.replayFeed.fullTime");
    default:
      return null;
  }
}

/** Only the fields the manager actually moved; `null` when nothing differs from the plan. */
export function changedTactics(
  plan: PlanView,
  draft: PlanView,
  supportedFormations: string[],
): TacticalChange | null {
  const change: TacticalChange = {};
  if (draft.style !== plan.style) change.style = draft.style;
  if (draft.pressing !== plan.pressing) change.pressing = draft.pressing;
  if (draft.tempo !== plan.tempo) change.tempo = draft.tempo;
  if (draft.formation !== plan.formation && supportedFormations.includes(draft.formation)) {
    change.formation = draft.formation;
  }
  return Object.keys(change).length > 0 ? change : null;
}
