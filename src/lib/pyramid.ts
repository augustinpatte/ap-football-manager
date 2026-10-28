import type { LeagueData } from "../store/gameStore";
import type { BoundaryRuleData } from "../store/types";

export interface PromotionRelegationZones {
  /** Places that go up without a play-off: the top N of the table. */
  promotionSlots: number;
  /** Places that go down without a barrage: the bottom N of the table. */
  relegationSlots: number;
  /** 1-based ranks that play the promotion play-off (or the barrage against the division above). */
  promotionPlayoff?: { from: number; to: number } | null;
  /** 1-based rank that plays the relegation barrage against a club of the division below. */
  barrageRank?: number | null;
}

const NO_ZONES: PromotionRelegationZones = { promotionSlots: 0, relegationSlots: 0 };

export type ZoneKind = "promotion" | "playoff" | "barrage" | "relegation" | null;

/** Which zone the row at `index` (0-based) of a table of `total` clubs falls in. */
export function zoneOfRow(zones: PromotionRelegationZones, index: number, total: number): ZoneKind {
  if (index < zones.promotionSlots) return "promotion";
  if (zones.relegationSlots > 0 && index >= total - zones.relegationSlots) return "relegation";
  const rank = index + 1;
  if (zones.promotionPlayoff && rank >= zones.promotionPlayoff.from && rank <= zones.promotionPlayoff.to) {
    return "playoff";
  }
  if (zones.barrageRank != null && rank === zones.barrageRank) return "barrage";
  return null;
}

/** The authored rule for the boundary below `division`, in force for its season. */
export function boundaryRuleFor(division: LeagueData): BoundaryRuleData | null {
  const entries = division.rules?.ladder?.boundaries ?? [];
  const inForce = entries
    .filter((entry) => entry.from_season <= division.season)
    .sort((a, b) => b.from_season - a.from_season)[0];
  return inForce?.rule ?? null;
}

/** Mirrors the backend's relegation_count: ~one slot per five clubs in the smaller division, at least one. */
function swapCount(upperSize: number, lowerSize: number): number {
  return Math.max(1, Math.floor(Math.min(upperSize, lowerSize) / 5));
}

function isLeagueTable(competition: LeagueData): boolean {
  return !competition.rules || competition.rules.format === "LeagueTable";
}

/**
 * A rung of a domestic pyramid — mirrors `is_ladder_tier` in
 * `end_of_season/berths.rs`. A cup or a regional side-competition is not a tier
 * even when it is scored as a table and belongs to a country, and the backend
 * will never promote into or out of one. `kind` and `scope` are optional on the
 * wire and default the way the Rust types do.
 */
function isLadderTier(competition: LeagueData): boolean {
  return (
    (competition.scope ?? "Domestic") === "Domestic" &&
    (competition.kind ?? "League") === "League" &&
    isLeagueTable(competition)
  );
}

/**
 * Two tiers of a pyramid hold different clubs — mirrors `tiers_share_clubs`.
 * A split-season country plays its Apertura and Clausura over one division at
 * consecutive priorities, so by rank alone they look like neighbouring tiers;
 * drawing zones between them promises movement between two halves of the same
 * division, which the backend refuses to perform.
 */
function shareClubs(left: LeagueData, right: LeagueData): boolean {
  const leftClubs = new Set(left.participant_ids ?? []);
  return (right.participant_ids ?? []).some((club) => leftClubs.has(club));
}

function divisionSize(competition: LeagueData): number {
  return competition.participant_ids?.length ?? competition.standings.length;
}

/**
 * Promotion/relegation zone sizes for a division's standings table, derived
 * from its neighbours in the country pyramid (league competitions sharing a
 * country, ordered by priority — lower priority is the higher tier).
 */
export function getPromotionRelegationZones(
  competitions: LeagueData[],
  competition: LeagueData,
): PromotionRelegationZones {
  if (!competition.country_id || !isLadderTier(competition)) {
    return NO_ZONES;
  }

  const myPriority = competition.priority ?? 0;
  const siblings = competitions.filter(
    (other) =>
      other.id !== competition.id &&
      other.country_id === competition.country_id &&
      isLadderTier(other),
  );
  const ladder = [competition, ...siblings];

  // The backend evaluates `tiers_share_clubs` across the whole run and refuses
  // the run when any pair overlaps — not just the pairs involving this
  // division. Dropping only the neighbours that overlap *this* one left the
  // rest looking like a clean ladder: a first division disjoint from both tiers
  // below it still drew zones while those two shared clubs with each other, and
  // the rollover exchanged nothing.
  const overlapping = ladder.some((tier, index) =>
    ladder.slice(index + 1).some((other) => shareClubs(tier, other)),
  );
  if (overlapping) {
    return NO_ZONES;
  }

  // The backend refuses a whole ladder run whose tiers do not rank distinctly —
  // equal priorities are peers, and which of them is "above" would be decided by
  // list order. Mirror that refusal rather than promising movement the rollover
  // will not make. Note this is about *equal* numbers only: the backend sorts a
  // country's tiers and chains neighbours by rank order, so a gap between 0 and
  // 2 still makes two adjacent divisions.
  const ranks = new Set(ladder.map((other) => other.priority ?? 0));
  if (ranks.size < ladder.length) {
    return NO_ZONES;
  }

  const above = siblings
    .filter((other) => (other.priority ?? 0) < myPriority)
    .sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0))[0];
  const below = siblings
    .filter((other) => (other.priority ?? 0) > myPriority)
    .sort((a, b) => (a.priority ?? 0) - (b.priority ?? 0))[0];

  // An authored boundary says exactly what moves; without one the generic swap applies.
  const ruleAbove = above ? boundaryRuleFor(above) : null;
  const ruleBelow = below ? boundaryRuleFor(competition) : null;
  const promotionPlayoff = ruleAbove?.promotion_playoff
    ? { from: ruleAbove.promotion_playoff.from_rank, to: ruleAbove.promotion_playoff.to_rank }
    : ruleAbove?.barrage
      ? { from: ruleAbove.barrage.lower_from_rank, to: ruleAbove.barrage.lower_to_rank }
      : null;
  const zones: PromotionRelegationZones = {
    promotionSlots: above
      ? ruleAbove
        ? ruleAbove.promoted_direct
        : swapCount(divisionSize(above), divisionSize(competition))
      : 0,
    relegationSlots: below
      ? ruleBelow
        ? ruleBelow.relegated_direct
        : swapCount(divisionSize(competition), divisionSize(below))
      : 0,
  };
  if (promotionPlayoff) zones.promotionPlayoff = promotionPlayoff;
  if (ruleBelow?.barrage) zones.barrageRank = ruleBelow.barrage.upper_rank;
  return zones;
}
