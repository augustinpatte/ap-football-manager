import type { TFunction } from "i18next";
import type {
  InteractiveView,
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
