import type { FastForwardStop } from "../services/fastForwardService";

const DAY_MS = 86_400_000;

function toUtcMs(isoDate: string): number {
  const [year, month, day] = isoDate.slice(0, 10).split("-").map(Number);
  return Date.UTC(year, month - 1, day);
}

/** `isoDate` plus `days`, as `YYYY-MM-DD`. Calendar arithmetic in UTC: no local-time day shifts. */
export function addDays(isoDate: string, days: number): string {
  return new Date(toUtcMs(isoDate) + days * DAY_MS).toISOString().slice(0, 10);
}

/** Whole days from `from` to `to` (negative when `to` is earlier). */
export function daysBetween(from: string, to: string): number {
  return Math.round((toUtcMs(to) - toUtcMs(from)) / DAY_MS);
}

/** A well-formed `YYYY-MM-DD` that names a real calendar day. */
export function isCalendarDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  return addDays(value, 0) === value;
}

export const FAST_FORWARD_PRESETS = [
  { id: "week", days: 7 },
  { id: "month", days: 30 },
  { id: "threeMonths", days: 90 },
] as const;

/** Whether carrying on with the same request makes sense after this stop. */
export function stopCanResume(stop: FastForwardStop): boolean {
  return (
    stop.kind === "decisions" ||
    stop.kind === "cancelled" ||
    stop.kind === "day_limit" ||
    stop.kind === "error"
  );
}
