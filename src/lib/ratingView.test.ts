import { describe, expect, it } from "vitest";

import { ratingFigure } from "./ratingView";

describe("ratingFigure", () => {
  /**
   * Given each way a rating can be read, and none yet
   * When the short figure is asked
   * Then a known player is his number, a reported one the scout's range, and everyone else a dash
   */
  it("is a number, a range or a dash, and never a number nobody has earned", () => {
    expect(ratingFigure({ kind: "exact", ovr: 77 })).toBe("77");
    expect(
      ratingFigure({
        kind: "estimate",
        low: 64,
        high: 70,
        confidence: "high",
        current: true,
        scouted_on: "2026-09-01",
      }),
    ).toBe("64–70");
    expect(ratingFigure({ kind: "standing", standing: "elite" })).toBe("—");
    expect(ratingFigure(null)).toBe("—");
    expect(ratingFigure(undefined)).toBe("—");
  });
});
