import { describe, expect, it } from "vitest";

import { COMPARE_MAX, formatRange, toggleCompared } from "./dossierFormat";

describe("dossier formatting", () => {
  /**
   * Given a range and a degenerate range and no range
   * When they are formatted
   * Then a range shows both ends, a point shows once, and an unjudged value is null (never zero).
   */
  it("shows a range, not a point, and nothing when nobody judged", () => {
    expect(formatRange({ low: 62, high: 70 })).toBe("62–70");
    expect(formatRange({ low: 70, high: 70 })).toBe("70");
    expect(formatRange(null)).toBeNull();
  });

  /**
   * Given a comparison already holding the most players it can
   * When one more is chosen, and an existing one is chosen again
   * Then the extra is refused and the existing one is removed.
   */
  it("holds at most four players and toggles off a chosen one", () => {
    const full = ["a", "b", "c", "d"];
    expect(full).toHaveLength(COMPARE_MAX);
    expect(toggleCompared(full, "e")).toEqual(full);
    expect(toggleCompared(full, "b")).toEqual(["a", "c", "d"]);
    expect(toggleCompared(["a"], "b")).toEqual(["a", "b"]);
  });
});
