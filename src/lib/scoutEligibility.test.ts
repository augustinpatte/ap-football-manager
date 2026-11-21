import { describe, expect, it } from "vitest";

import type { StaffData } from "../store/gameStore";
import type { StaffPost } from "../store/types";
import { isFieldScout, teamFieldScouts } from "./scoutEligibility";

function staff(overrides: Partial<StaffData> = {}): StaffData {
  return {
    id: "s1",
    first_name: "Sam",
    last_name: "Scout",
    display_name: "Sam Scout",
    date_of_birth: "1985-01-01",
    nationality: "GB",
    role: "Scout",
    attributes: { coaching: 20, judgingAbility: 65, judgingPotential: 70, physiotherapy: 10 },
    team_id: "team-1",
    specialization: null,
    wage: 1000,
    contract_end: "2027-06-30",
    ...overrides,
  };
}

describe("isFieldScout", () => {
  /**
   * Given an FM24 import whose recruitment analyst is stored with role Scout and post Analyst
   * When the manager looks for someone to send on a scouting trip
   * Then the analyst is not eligible — the backend rejects that post with staffMemberNotScout.
   */
  it("rejects an analyst even though the role is Scout", () => {
    expect(isFieldScout(staff({ role: "Scout", post: "Analyst" }))).toBe(false);
  });

  /**
   * Given the post/role combinations an FM24 import can write
   * When eligibility is decided
   * Then only the Scout role with a non-analyst seat is eligible, matching ofm_core scouting.rs.
   */
  it.each<[StaffData["role"], StaffPost | undefined, boolean]>([
    ["Scout", undefined, true], // generated staff: post is derived, not serialized
    ["Scout", "Scout", true],
    ["Scout", "HeadOfScouting", true],
    ["Scout", "Analyst", false],
    ["Coach", "FirstTeamCoach", false],
    ["Coach", "HeadOfYouth", false],
    ["Physio", "HeadOfMedical", false],
    ["AssistantManager", "AssistantManager", false],
  ])("role %s post %s -> %s", (role, post, expected) => {
    expect(isFieldScout(staff({ role, post }))).toBe(expected);
  });
});

describe("teamFieldScouts", () => {
  /**
   * Given a club with an analyst, a scout and a head of scouting, and another club's scout
   * When the club's scouts are listed
   * Then the analyst and the other club's scout are left out.
   */
  it("keeps only this club's eligible scouts", () => {
    const roster = [
      staff({ id: "analyst", post: "Analyst" }),
      staff({ id: "scout", post: "Scout" }),
      staff({ id: "head", post: "HeadOfScouting" }),
      staff({ id: "other", team_id: "team-2" }),
    ];
    expect(teamFieldScouts(roster, "team-1").map((s) => s.id)).toEqual(["scout", "head"]);
  });
});
