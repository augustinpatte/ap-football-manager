import { describe, expect, it } from "vitest";

import en from "../../i18n/locales/en.json";

describe("match mode wording (English)", () => {
  /**
   * Given the three ways to play a prepared match
   * When the manager reads their names and descriptions
   * Then managing says it is interactive and cannot be rewound, watching a simulation says it is a
   *      non-interactive replay that can be sought, and quick simulation says it is immediate.
   */
  it("tells managing, watching and quick simulation apart", () => {
    expect(en.match.interactive.manage).toBe("Manage Match in 2D");
    expect(en.match.watchWithM12).toBe("Watch simulation");
    expect(en.match.simulateWithM12).toBe("Quick simulation");

    expect(en.match.interactive.manageDesc).toMatch(/tactics/i);
    expect(en.match.interactive.manageDesc).toMatch(/substitutions/i);
    expect(en.match.interactive.manageDesc).toMatch(/cannot rewind/i);

    expect(en.match.viewMode.watchDesc).toMatch(/simulated first/i);
    expect(en.match.viewMode.watchDesc).toMatch(/timeline/i);
    expect(en.match.viewMode.watchDesc).toMatch(/cannot change tactics/i);

    expect(en.match.viewMode.quickDesc).toMatch(/instantly/i);
  });

  /**
   * Given the classic text-commentary match
   * When the manager reads its start button
   * Then it says it is the classic match, so it is not mistaken for the 2D one.
   */
  it("names the classic match as classic", () => {
    expect(en.match.startMatch).toMatch(/classic/i);
  });
});
