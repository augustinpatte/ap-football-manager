import { describe, expect, it } from "vitest";

import { richTokenScale } from "./FormationPitch";

/**
 * How much a rich token shrinks so two rows never overlap on a pitch this tall. The layout itself is
 * measured in a browser (docs/appendix/pitch-overlap-probe); this is the rule it measured.
 */
describe("richTokenScale", () => {
  /**
   * Given a tall pitch with room between rows
   * When the scale is asked for
   * Then tokens are drawn at full size
   */
  it("leaves_tokens_at_full_size_when_the_pitch_is_tall_enough", () => {
    expect(richTokenScale(728)).toBe(1);
    expect(richTokenScale(600)).toBe(1);
  });

  /**
   * Given the pitch heights the pre-match screen produced at 300 and 360 px widths
   * When the scale is asked for
   * Then tokens shrink in proportion to the height, never below what the rows can hold
   */
  it("shrinks_tokens_in_proportion_on_a_small_pitch", () => {
    expect(richTokenScale(420)).toBeCloseTo(0.727, 2);
    expect(richTokenScale(504)).toBeCloseTo(0.872, 2);
    expect(richTokenScale(420)).toBeLessThan(richTokenScale(504));
  });

  /**
   * Given a pitch that has not been measured yet, or a nonsense height
   * When the scale is asked for
   * Then it is 1, so a token is never hidden by a bad reading
   */
  it("defaults_to_full_size_for_an_unmeasured_pitch", () => {
    expect(richTokenScale(0)).toBe(1);
    expect(richTokenScale(Number.NaN)).toBe(1);
    expect(richTokenScale(-5)).toBe(1);
  });
});
