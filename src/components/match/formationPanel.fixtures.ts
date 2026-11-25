import real from "./liveMatch.real.generated.json";
import type {
  FormationShape,
  MatchPlayerView,
  MatchTeamView,
} from "../../services/interactiveMatchService";

/** The engine's own slots for every supported formation, as the real match fixture captured them. */
export const SHAPES: FormationShape[] = real.start.view.formation_shapes;

export function matchPlayer(
  id: string,
  name: string,
  role: string | null,
  overrides: Partial<MatchPlayerView> = {},
): MatchPlayerView {
  return {
    id,
    name,
    shirt_number: Number(id.replace(/\D/g, "")) || 0,
    natural_position: role ?? "CM",
    role,
    condition: 0.9,
    minutes_played: 30,
    yellow_cards: 0,
    injured: false,
    ...overrides,
  };
}

/**
 * A full team as the engine lists it: eleven on the pitch in the formation's slot order
 * (H1..H11), seven on the bench (H12..H18), five substitutions.
 */
export function squad(formation: string): MatchTeamView {
  const shape = SHAPES.find((s) => s.label === formation);
  const roles = shape?.slots.map((slot) => slot.role) ?? [];
  const benchRoles = ["GK", "CB", "RB", "CM", "AM", "LW", "ST"];
  return {
    side: "home",
    formation,
    tactics: { mentality: 50, pressing: 50, line_height: 50, width: 50, tempo: 50, directness: 50 },
    on_pitch: roles.map((role, i) => matchPlayer(`H${i + 1}`, `Player ${i + 1}`, role)),
    bench: benchRoles.map((role, i) =>
      matchPlayer(`H${i + 12}`, `Sub ${i + 12}`, null, {
        natural_position: role,
        minutes_played: 0,
      }),
    ),
    used: [],
    sent_off: [],
    substitutions_used: 0,
    substitutions_remaining: 5,
    windows_remaining: 3,
    pending_substitutions: [],
  };
}
