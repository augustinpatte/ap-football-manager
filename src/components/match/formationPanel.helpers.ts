import type {
  FormationShape,
  MatchPlayerView,
  MatchTeamView,
} from "../../services/interactiveMatchService";
import { normalisePosition } from "../../lib/positions";
import type { EnginePlayerData } from "./types";

/** A player at a point of the pitch, as a share (0 to 100) of its width and height. */
export interface PlacedPlayer {
  player: MatchPlayerView;
  /** From the left edge. */
  x: number;
  /** From the top edge: the attack is at the top, the goalkeeper at the bottom. */
  y: number;
}

/** The engine's own extremes of slot depth (goalkeeper, deepest striker) in metres. */
const DEPTH_FROM_M = 3;
const DEPTH_TO_M = 74;
/** The pitch rows the extremes land on, so a token never touches the touchline or the goal line. */
const BOTTOM_PCT = 88;
const TOP_PCT = 12;
const PITCH_WIDTH_M = 68;
// A token is 15 % of the pitch's width: its centre stays far enough in that the whole token, name
// included, is inside the touchline and not drawn over it.
const SIDE_MARGIN_PCT = 13;
const STRAY_PER_ROW = 4;

function slotPoint(depth: number, lateral: number): { x: number; y: number } {
  const depthShare = (depth - DEPTH_FROM_M) / (DEPTH_TO_M - DEPTH_FROM_M);
  // The engine's lateral axis runs from the right touchline to the left one; the screen reads left to right.
  const x = 100 - (lateral / PITCH_WIDTH_M) * 100;
  return {
    x: Math.min(100 - SIDE_MARGIN_PCT, Math.max(SIDE_MARGIN_PCT, x)),
    y: BOTTOM_PCT - depthShare * (BOTTOM_PCT - TOP_PCT),
  };
}

/**
 * Where each player of the pitch stands. The engine says which role every player holds
 * (`role`) and the shape of the team's formation says where each role stands, so a player is
 * matched to a slot of his own role, taking slots of one role in the order the engine lists
 * them. A team reduced to ten leaves the vacated slot empty; a player whose role fits no slot
 * (or a formation with no shape) is still drawn, on a row of his own, never dropped.
 */
export function placeOnPitch(team: MatchTeamView, shapes: FormationShape[]): PlacedPlayer[] {
  const slots = shapes.find((shape) => shape.label === team.formation)?.slots ?? [];
  const taken = new Set<number>();
  const placement = new Map<string, { x: number; y: number }>();
  const unplaced: MatchPlayerView[] = [];

  for (const player of team.on_pitch) {
    const index = slots.findIndex((slot, i) => !taken.has(i) && slot.role === player.role);
    if (index === -1) {
      unplaced.push(player);
      continue;
    }
    taken.add(index);
    placement.set(player.id, slotPoint(slots[index].depth, slots[index].lateral));
  }
  // A player no slot names takes a slot nobody holds before he is parked on a row of his own.
  const stray: MatchPlayerView[] = [];
  for (const player of unplaced) {
    const index = slots.findIndex((_, i) => !taken.has(i));
    if (index === -1) {
      stray.push(player);
      continue;
    }
    taken.add(index);
    placement.set(player.id, slotPoint(slots[index].depth, slots[index].lateral));
  }
  // Rows of at most four (a token is 17 % of the pitch's width), so a team with no shape still fits.
  stray.forEach((player, i) => {
    const row = Math.floor(i / STRAY_PER_ROW);
    const inRow = Math.min(STRAY_PER_ROW, stray.length - row * STRAY_PER_ROW);
    placement.set(player.id, {
      x: (((i % STRAY_PER_ROW) + 1) / (inRow + 1)) * 100,
      y: 30 + row * 20,
    });
  });

  const points = team.on_pitch.map((player) => ({
    player,
    ...(placement.get(player.id) ?? { x: 50, y: 50 }),
  }));
  return keepApart(points);
}

/**
 * The pitch as the panel draws it at its widest, in pixels, and the footprint of one token. Sizes
 * are in rem on the page, so a larger UI scale grows all of them together and the proportions hold.
 */
const PITCH_W_PX = 416;
const PITCH_H_PX = 624;
const TOKEN_W_PX = 72;
const TOKEN_H_PX = 64;
const SIDE_INSET_PX = 0.04 * PITCH_W_PX;
const END_INSET_PX = (4 / 140) * PITCH_H_PX;

/**
 * The engine's slots are metres apart; a token is not. Where two slots sit closer than a token
 * (a deep midfielder between two central ones in 4-5-1), the pair is pushed apart along the axis
 * with the smaller overlap, the same way for the same input, until no two tokens touch.
 */
function keepApart(points: PlacedPlayer[]): PlacedPlayer[] {
  const px = points.map(({ x, y }) => ({ x: (x / 100) * PITCH_W_PX, y: (y / 100) * PITCH_H_PX }));
  // Inside the markings: the pitch draws its lines this far in (4 % of a 100 x 140 drawing).
  const clamp = (value: number, size: number, limit: number, inset: number) =>
    Math.min(limit - inset - size / 2, Math.max(inset + size / 2, value));
  for (let pass = 0; pass < 24; pass++) {
    let moved = false;
    for (let i = 0; i < px.length; i++) {
      for (let j = i + 1; j < px.length; j++) {
        const dx = px[j].x - px[i].x;
        const dy = px[j].y - px[i].y;
        const overlapX = TOKEN_W_PX - Math.abs(dx);
        const overlapY = TOKEN_H_PX - Math.abs(dy);
        if (overlapX <= 0 || overlapY <= 0) continue;
        moved = true;
        // Strictly less: a tie is settled along the pitch, where there is more room.
        if (overlapX < overlapY) {
          const push = (overlapX / 2 + 0.5) * (dx < 0 ? -1 : 1);
          px[i].x = clamp(px[i].x - push, TOKEN_W_PX, PITCH_W_PX, SIDE_INSET_PX);
          px[j].x = clamp(px[j].x + push, TOKEN_W_PX, PITCH_W_PX, SIDE_INSET_PX);
        } else {
          const push = (overlapY / 2 + 0.5) * (dy < 0 ? -1 : 1);
          px[i].y = clamp(px[i].y - push, TOKEN_H_PX, PITCH_H_PX, END_INSET_PX);
          px[j].y = clamp(px[j].y + push, TOKEN_H_PX, PITCH_H_PX, END_INSET_PX);
        }
      }
    }
    if (!moved) break;
  }
  return points.map((point, i) => ({
    player: point.player,
    x: (px[i].x / PITCH_W_PX) * 100,
    y: (px[i].y / PITCH_H_PX) * 100,
  }));
}

/**
 * The engine's player as the shared pitch component reads one. The pitch needs an identity, a
 * name, a condition on a 0 to 100 scale and a position group; the match engine owns every other
 * attribute and the live match never needs them here.
 */
export function toPitchPlayer(player: MatchPlayerView): EnginePlayerData {
  const slotRole = player.role ?? player.natural_position;
  return {
    id: player.id,
    name: player.name,
    position: normalisePosition(slotRole),
    condition: Math.round(player.condition * 100),
    role: slotRole,
    ovr: 0,
    pace: 0,
    stamina: 0,
    strength: 0,
    agility: 0,
    passing: 0,
    shooting: 0,
    tackling: 0,
    dribbling: 0,
    defending: 0,
    positioning: 0,
    vision: 0,
    decisions: 0,
    composure: 0,
    aggression: 0,
    teamwork: 0,
    leadership: 0,
    handling: 0,
    reflexes: 0,
    aerial: 0,
    traits: [],
  };
}

export interface SubstitutionAllowance {
  used: number;
  max: number;
  queued: number;
  /** What the engine will still accept: the allowance less what is already queued. */
  free: number;
}

export function substitutionAllowance(team: MatchTeamView): SubstitutionAllowance {
  const queued = team.pending_substitutions.length;
  return {
    used: team.substitutions_used,
    max: team.substitutions_used + team.substitutions_remaining,
    queued,
    free: Math.max(0, team.substitutions_remaining - queued),
  };
}

export type DepartedStatus = "sent_off" | "injured" | "substituted";

/** Why a player is no longer on the pitch or the bench, from the engine's own lists. */
export function departedStatus(team: MatchTeamView, player: MatchPlayerView): DepartedStatus {
  if (team.sent_off.some((gone) => gone.id === player.id)) return "sent_off";
  return player.injured ? "injured" : "substituted";
}
