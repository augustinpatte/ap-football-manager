import { describe, expect, it } from "vitest";

import {
  FAST_FORWARD_PRESETS,
  addDays,
  daysBetween,
  isCalendarDate,
  stopCanResume,
} from "./useFastForward.helpers";

describe("fast-forward date helpers", () => {
  /** Given a date near a month or year end, when days are added, then the calendar rolls over. */
  it("adds days across month, year and leap-day boundaries", () => {
    expect(addDays("2033-01-30", 3)).toBe("2033-02-02");
    expect(addDays("2033-12-30", 3)).toBe("2034-01-02");
    expect(addDays("2036-02-28", 1)).toBe("2036-02-29");
    expect(addDays("2035-02-28", 1)).toBe("2035-03-01");
    expect(addDays("2033-07-01", 0)).toBe("2033-07-01");
  });

  /** Given two dates, when the gap is taken, then it is whole days whatever the clocks do. */
  it("counts whole days between dates, independent of daylight saving", () => {
    expect(daysBetween("2033-03-26", "2033-03-28")).toBe(2);
    expect(daysBetween("2033-10-29", "2033-10-31")).toBe(2);
    expect(daysBetween("2033-07-01", "2033-07-01")).toBe(0);
    expect(daysBetween("2033-07-02", "2033-07-01")).toBe(-1);
  });

  /** Given text from a field, when it is checked, then only a real calendar day passes. */
  it("accepts real calendar days only", () => {
    expect(isCalendarDate("2036-02-29")).toBe(true);
    expect(isCalendarDate("2035-02-29")).toBe(false);
    expect(isCalendarDate("2033-13-01")).toBe(false);
    expect(isCalendarDate("2033-7-1")).toBe(false);
    expect(isCalendarDate("")).toBe(false);
  });

  /** Given the quick picks, when read, then they are a week, a month and a quarter. */
  it("offers a week, a month and three months", () => {
    expect(FAST_FORWARD_PRESETS.map((preset) => preset.days)).toEqual([7, 30, 90]);
  });

  /** Given each way a run can end, when asked, then only the interrupted ones can be resumed. */
  it("tells which stops can be resumed", () => {
    expect(stopCanResume({ kind: "decisions", decisions: [] })).toBe(true);
    expect(stopCanResume({ kind: "cancelled" })).toBe(true);
    expect(stopCanResume({ kind: "manager_fired" })).toBe(false);
    expect(stopCanResume({ kind: "season_complete" })).toBe(false);
  });
});
