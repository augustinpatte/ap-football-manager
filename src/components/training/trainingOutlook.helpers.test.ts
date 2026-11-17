import { describe, expect, it } from "vitest";

import { formatEffect } from "./trainingOutlook.helpers";

describe("formatEffect", () => {
  /**
   * Given multipliers above, at and below 1.0
   * When they are formatted
   * Then the sign is explicit for a gain, absent at zero, and a minus for a penalty.
   */
  it("signs the change against an average staff", () => {
    expect(formatEffect(1.1)).toBe("+10%");
    expect(formatEffect(1)).toBe("0%");
    expect(formatEffect(0.8)).toBe("-20%");
    expect(formatEffect(1.375)).toBe("+38%");
  });
});
