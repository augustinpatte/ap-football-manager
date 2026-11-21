import type { StaffData } from "../store/types";

/**
 * Whether this person can be sent on a scouting job.
 *
 * Mirrors `resolve_user_scout` in `ofm_core/src/scouting.rs`: the Scout role, but not the
 * Analyst post. FM24 imports store recruitment analysts with `role: "Scout"` and only `post`
 * tells them apart, so the role alone over-counts. `post` is not serialized for generated
 * staff (it is derived from the role), which is why a missing `post` means an ordinary scout.
 */
export function isFieldScout(member: Pick<StaffData, "role" | "post">): boolean {
  return member.role === "Scout" && member.post !== "Analyst";
}

/** The club's staff who can take a scouting assignment. */
export function teamFieldScouts(staff: StaffData[], teamId: string | null): StaffData[] {
  return staff.filter((member) => isFieldScout(member) && member.team_id === teamId);
}
