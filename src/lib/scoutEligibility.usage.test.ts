import { describe, expect, it } from "vitest";

const SOURCES = import.meta.glob(["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("who may be sent scouting", () => {
  /**
   * Given an FM24 recruitment analyst, filed under the Scout role but refused by the backend
   * When any screen picks the people it can send on a scouting job
   * Then it asks the shared predicate: a bare comparison of the role with "Scout" anywhere else
   *      is the bug again, on a surface no other test reaches.
   */
  it("compares the Scout role only inside the shared eligibility helper", () => {
    const offenders = Object.entries(SOURCES)
      .filter(([path]) => !path.endsWith("/scoutEligibility.ts"))
      .filter(([, source]) => /\brole\s*[!=]==\s*["']Scout["']/.test(source))
      .map(([path]) => path);

    expect(Object.keys(SOURCES).length).toBeGreaterThan(100);
    expect(offenders).toEqual([]);
  });
});
