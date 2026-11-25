import { describe, expect, it } from "vitest";
import type {
  FormationShape,
  MatchPlayerView,
  MatchTeamView,
} from "../../services/interactiveMatchService";
import {
  departedStatus,
  placeOnPitch,
  substitutionAllowance,
  toPitchPlayer,
} from "./formationPanel.helpers";
import { SHAPES, squad } from "./formationPanel.fixtures";

const byRole = (placed: ReturnType<typeof placeOnPitch>, role: string) =>
  placed.filter((p) => p.player.role === role);

describe("placeOnPitch", () => {
  /**
   * Given a team lined up in 4-4-2, the engine's order (right to left in each line)
   * When it is placed
   * Then the left back stands on the left of the pitch and the right back on the right, and the
   * goalkeeper is at the bottom and the strikers at the top
   */
  it("places_a_442_with_left_on_the_left_and_the_attack_at_the_top", () => {
    const placed = placeOnPitch(squad("4-4-2"), SHAPES);

    const [leftBack] = byRole(placed, "LB");
    const [rightBack] = byRole(placed, "RB");
    const [keeper] = byRole(placed, "GK");
    const strikers = byRole(placed, "ST");
    expect(placed).toHaveLength(11);
    expect(leftBack.x).toBeLessThan(rightBack.x);
    expect(keeper.y).toBeGreaterThan(leftBack.y);
    expect(strikers.every((s) => s.y < leftBack.y)).toBe(true);
  });

  /**
   * Given 3-5-2, whose wing-backs stand in midfield depth
   * When it is placed
   * Then the wing-backs are drawn level with the midfield, not on the back line: the shape is the
   * engine's, not a guess from the role's name
   */
  it("draws_the_wing_backs_of_a_352_at_midfield_depth", () => {
    const placed = placeOnPitch(squad("3-5-2"), SHAPES);

    const centreBackY = byRole(placed, "CB")[0].y;
    const [wingBack] = byRole(placed, "LWB");
    const [midfielder] = byRole(placed, "CM");
    expect(byRole(placed, "CB")).toHaveLength(3);
    expect(wingBack.y).toBeLessThan(centreBackY);
    expect(Math.abs(wingBack.y - midfielder.y)).toBeLessThan(centreBackY - wingBack.y);
  });

  /**
   * Given 4-3-3
   * When it is placed
   * Then the three forwards share the top line: both wingers outside the centre forward
   */
  it("puts_the_wingers_of_a_433_either_side_of_the_centre_forward", () => {
    const placed = placeOnPitch(squad("4-3-3"), SHAPES);

    const [leftWing] = byRole(placed, "LW");
    const [rightWing] = byRole(placed, "RW");
    const [striker] = byRole(placed, "ST");
    expect(leftWing.x).toBeLessThan(striker.x);
    expect(striker.x).toBeLessThan(rightWing.x);
  });

  /**
   * Given a team reduced to ten (a striker sent off, so the engine lists ten on the pitch)
   * When it is placed
   * Then ten players are drawn, each in the slot of his own role, and the empty slot stays empty
   */
  it("draws_ten_players_and_leaves_the_vacated_slot_empty", () => {
    const team = squad("4-4-2");
    const sentOff = team.on_pitch.filter((p) => p.role === "ST")[1];
    team.on_pitch = team.on_pitch.filter((p) => p !== sentOff);
    team.sent_off = [sentOff];

    const placed = placeOnPitch(team, SHAPES);

    expect(placed).toHaveLength(10);
    expect(byRole(placed, "ST")).toHaveLength(1);
    expect(placed.some((p) => p.player.id === sentOff.id)).toBe(false);
    const keys = placed.map((p) => `${p.x}/${p.y}`);
    expect(new Set(keys).size).toBe(10);
  });

  /**
   * Given a midfielder dismissed from the middle of the engine's list (so every later player moves up one)
   * When the team is placed
   * Then each survivor still stands in the slot of his OWN role: the left midfielder on the left of the
   * midfield line and both strikers at the top, not one slot off
   */
  it("keeps_each_survivor_in_his_own_slot_when_someone_in_the_middle_is_dismissed", () => {
    const team = squad("4-4-2");
    const dismissed = team.on_pitch.find((p) => p.role === "CM") as MatchPlayerView;
    team.on_pitch = team.on_pitch.filter((p) => p !== dismissed);
    team.sent_off = [dismissed];

    const placed = placeOnPitch(team, SHAPES);

    expect(placed).toHaveLength(10);
    const [leftMid] = byRole(placed, "LM");
    const [rightMid] = byRole(placed, "RM");
    expect(leftMid.x).toBeLessThan(rightMid.x);
    expect(leftMid.y).toBeCloseTo(rightMid.y, 0);
    const strikers = byRole(placed, "ST");
    expect(strikers).toHaveLength(2);
    expect(strikers.every((s) => s.y < leftMid.y)).toBe(true);
    expect(byRole(placed, "GK")[0].y).toBeGreaterThan(leftMid.y);
  });

  /**
   * Given a substitute listed out of the order of his slot (the engine lists a swapped player at the
   * vacated index, not at the end)
   * When the team is placed
   * Then he stands where his role says, not where his place in the list says
   */
  it("places_a_player_by_his_role_not_by_his_place_in_the_list", () => {
    const team = squad("4-4-2");
    [team.on_pitch[1], team.on_pitch[10]] = [team.on_pitch[10], team.on_pitch[1]];

    const placed = placeOnPitch(team, SHAPES);

    const [fullBack] = byRole(placed, "RB");
    const [striker] = byRole(placed, "ST");
    expect(fullBack.y).toBeGreaterThan(striker.y);
    expect(fullBack.x).toBeGreaterThan(50);
  });

  /**
   * Given two players of the same role (two centre backs)
   * When they are placed
   * Then each keeps the slot the engine's order gives him: the first listed takes the first slot
   */
  it("gives_players_of_one_role_the_slots_in_the_engines_order", () => {
    const placed = placeOnPitch(squad("4-4-2"), SHAPES);

    const [first, second] = byRole(placed, "CB");
    expect(first.player.id).toBe("H3");
    expect(second.player.id).toBe("H4");
    expect(first.x).not.toBe(second.x);
  });

  /**
   * Given a formation the view carries no shape for
   * When it is placed
   * Then every player is still drawn, on rows of their own that fit the pitch, never dropped
   */
  it("still_draws_everyone_when_the_formation_has_no_shape", () => {
    const placed = placeOnPitch(squad("4-4-2"), [] as FormationShape[]);

    expect(placed).toHaveLength(11);
    expect(new Set(placed.map((p) => `${p.x}/${p.y}`)).size).toBe(11);
  });

  /**
   * Given a player whose role fits no slot of the shape
   * When the team is placed
   * Then he is still drawn (never hidden) at a position no other player occupies
   */
  it("draws_a_player_whose_role_matches_no_slot", () => {
    const team = squad("4-4-2");
    team.on_pitch[10] = { ...team.on_pitch[10], role: "SS" };

    const placed = placeOnPitch(team, SHAPES);

    expect(placed).toHaveLength(11);
    expect(new Set(placed.map((p) => `${p.x}/${p.y}`)).size).toBe(11);
  });
});

describe("toPitchPlayer", () => {
  /**
   * Given an engine player at 63 % condition playing as a left back
   * When he is adapted for the shared pitch
   * Then the pitch gets his id, name, a 0 to 100 condition and a position group from his slot role
   */
  it("adapts_an_engine_player_to_the_shared_pitch_type", () => {
    const player: MatchPlayerView = {
      id: "p1",
      name: "Ada Back",
      shirt_number: 3,
      natural_position: "LB",
      role: "LB",
      condition: 0.63,
      minutes_played: 12,
      yellow_cards: 0,
      injured: false,
    };

    const adapted = toPitchPlayer(player);

    expect(adapted).toMatchObject({
      id: "p1",
      name: "Ada Back",
      condition: 63,
      position: "Defender",
    });
  });
});

describe("substitutionAllowance", () => {
  /**
   * Given five substitutions allowed, two used and one queued
   * When the allowance is read
   * Then two are still free: what the engine will accept is the allowance less what is queued
   */
  it("subtracts_the_queue_from_what_remains", () => {
    const team = squad("4-4-2");
    team.substitutions_used = 2;
    team.substitutions_remaining = 3;
    team.pending_substitutions = [{ off: "H5", on: "H12" }];

    expect(substitutionAllowance(team)).toEqual({ used: 2, max: 5, queued: 1, free: 2 });
  });

  /**
   * Given every substitution queued or used
   * When the allowance is read
   * Then none is free and the count never goes negative
   */
  it("never_goes_below_zero", () => {
    const team = squad("4-4-2");
    team.substitutions_used = 5;
    team.substitutions_remaining = 0;
    team.pending_substitutions = [{ off: "H5", on: "H12" }];

    expect(substitutionAllowance(team).free).toBe(0);
  });
});

describe("departedStatus", () => {
  const player = (overrides: Partial<MatchPlayerView> = {}): MatchPlayerView => ({
    id: "x",
    name: "X",
    shirt_number: 1,
    natural_position: "CM",
    role: null,
    condition: 0.5,
    minutes_played: 30,
    yellow_cards: 0,
    injured: false,
    ...overrides,
  });

  /**
   * Given players the engine lists as gone
   * When their status is read
   * Then a sent-off player is dismissed, an injured one is injured, and the rest were substituted
   */
  it("tells_dismissed_injured_and_substituted_apart", () => {
    const team: MatchTeamView = squad("4-4-2");
    const dismissed = player({ id: "d" });
    const hurt = player({ id: "h", injured: true });
    const replaced = player({ id: "r" });
    team.sent_off = [dismissed];
    team.used = [hurt, replaced];

    expect(departedStatus(team, dismissed)).toBe("sent_off");
    expect(departedStatus(team, hurt)).toBe("injured");
    expect(departedStatus(team, replaced)).toBe("substituted");
  });
});
