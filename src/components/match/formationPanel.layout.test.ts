import { describe, expect, it } from "vitest";
import { placeOnPitch } from "./formationPanel.helpers";
import { SHAPES, squad } from "./formationPanel.fixtures";

/** The footprint of one token in pixels: `w-[4.5rem]`, and a 32 px shirt circle above two 16 px lines. */
const TOKEN_W = 72;
const TOKEN_H = 64;
/** The pitch is 2:3 and 26 rem (416 px) wide in every supported window (960 px and up); rem scales with the UI. */
const PITCH_WIDTHS = [416];

function overlaps(widthPx: number, formation: string): string[] {
  const heightPx = widthPx * 1.5;
  const boxes = placeOnPitch(squad(formation), SHAPES).map(({ player, x, y }) => ({
    id: player.id,
    left: (x / 100) * widthPx - TOKEN_W / 2,
    top: (y / 100) * heightPx - TOKEN_H / 2,
  }));
  const clashes: string[] = [];
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i];
      const b = boxes[j];
      if (Math.abs(a.left - b.left) < TOKEN_W && Math.abs(a.top - b.top) < TOKEN_H) {
        clashes.push(`${a.id}/${b.id}`);
      }
    }
  }
  return clashes;
}

describe("the tactical pitch layout", () => {
  describe.each(SHAPES.map((shape) => shape.label))("%s", (formation) => {
    /**
     * Given the engine's own slots for the formation
     * When eleven tokens are laid out on the pitch at each supported width
     * Then no two tokens overlap and every token stays inside the pitch
     */
    it.each(PITCH_WIDTHS)("keeps_tokens_apart_at_a_pitch_%ipx_wide", (width) => {
      expect(overlaps(width, formation)).toEqual([]);
    });

    it("keeps_every_whole_token_inside_the_pitch_markings", () => {
      for (const { x, y } of placeOnPitch(squad(formation), SHAPES)) {
        // The whole footprint is inside the markings: the pitch draws its touchlines 4 % in from the
        // edge (viewBox 100 x 140), so a token's text is never drawn across a line at the border.
        const sideInset = 4 + (TOKEN_W / 2 / 416) * 100;
        const endInset = (4 / 140) * 100 + (TOKEN_H / 2 / 624) * 100;
        expect(x).toBeGreaterThanOrEqual(sideInset - 0.01);
        expect(x).toBeLessThanOrEqual(100 - sideInset + 0.01);
        expect(y).toBeGreaterThanOrEqual(endInset - 0.01);
        expect(y).toBeLessThanOrEqual(100 - endInset + 0.01);
      }
    });
  });
});
