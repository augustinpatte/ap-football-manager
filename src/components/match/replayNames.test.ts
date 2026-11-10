import { describe, expect, it } from "vitest";

import { resolveReplayTeamName } from "./replayNames";

const teams = [
  { id: "fm-c-1072", name: "Albacete Balompié" },
  { id: "fm-c-528", name: "Bayern München" },
  { id: "fm-c-9", name: "   " },
];

describe("resolveReplayTeamName", () => {
  /**
   * Given a replay that carries a club id in the game's own spelling
   * When its name is resolved
   * Then it is the club's name.
   */
  it("finds the club by its id", () => {
    expect(resolveReplayTeamName("fm-c-528", teams)).toBe("Bayern München");
  });

  /**
   * Given the same id written in capitals (FM-C-1072)
   * When its name is resolved
   * Then the club is still found.
   */
  it("accepts the id in another case", () => {
    expect(resolveReplayTeamName("FM-C-1072", teams)).toBe("Albacete Balompié");
  });

  /**
   * Given an id no club has, and a club whose name is blank
   * When their names are resolved
   * Then there is none: the caller shows its fallback, never the internal id.
   */
  it("reports no name rather than inventing one", () => {
    expect(resolveReplayTeamName("FM-C-404", teams)).toBeNull();
    expect(resolveReplayTeamName("fm-c-9", teams)).toBeNull();
  });
});
