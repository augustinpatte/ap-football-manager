/** An effect multiplier as a signed percentage against 1.0 (`1.1` reads `+10%`, `0.8` reads `-20%`). */
export function formatEffect(multiplier: number): string {
  const percent = Math.round((multiplier - 1) * 100);
  return `${percent > 0 ? "+" : ""}${percent}%`;
}
