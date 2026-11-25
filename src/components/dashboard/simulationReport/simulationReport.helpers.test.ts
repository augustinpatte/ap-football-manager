import { describe, expect, it } from "vitest";
import type { FastForwardResponse } from "../../../services/fastForwardService";
import { buildTimeline, isPartialRun, omitted, reportClubs } from "./simulationReport.helpers";
import { busyClub, emptyClub, listed } from "./reportFixtures";

const response = (clubs: ReturnType<typeof emptyClub>[], daysTotal: number): FastForwardResponse =>
  ({
    progress: { days_total: daysTotal, days_done: clubs.reduce((n, c) => n + c.days_played, 0) },
    report: { clubs },
  }) as unknown as FastForwardResponse;

describe("buildTimeline", () => {
  /**
   * Given a club report with a match, transfers, a contract, an injury, delegation entries and events
   * When the timeline is built
   * Then every dated item appears once, oldest first, the club's own items before the assistant's on a shared day
   */
  it("merges_every_dated_item_oldest_first", () => {
    const timeline = buildTimeline(busyClub());

    expect(timeline.map((entry) => [entry.date, entry.source])).toEqual([
      ["2026-08-01", "event"],
      ["2026-08-02", "injury"],
      ["2026-08-03", "transfer"],
      ["2026-08-04", "delegation"],
      ["2026-08-05", "match"],
      ["2026-08-05", "contract"],
      ["2026-08-06", "delegation"],
      ["2026-08-07", "transfer"],
      ["2026-08-08", "delegation"],
      ["2026-08-09", "event"],
    ]);
    expect(new Set(timeline.map((entry) => entry.id)).size).toBe(timeline.length);
  });

  /**
   * Given a club with nothing to report
   * When the timeline is built
   * Then it is empty: nothing is invented to fill the days
   */
  it("is_empty_for_an_interval_with_nothing_in_it", () => {
    expect(buildTimeline(emptyClub())).toEqual([]);
  });
});

describe("omitted", () => {
  /**
   * Given a list the backend capped at 2 items of 5
   * When the omitted count is read
   * Then it is the difference, and never negative
   */
  it("counts_what_the_backend_left_out", () => {
    expect(omitted(listed([1, 2], 5))).toBe(3);
    expect(omitted(listed([1, 2], 2))).toBe(0);
    expect(omitted(listed([1, 2, 3], 2))).toBe(0);
  });
});

describe("isPartialRun", () => {
  /**
   * Given a run asked for 90 days that played 12
   * When it is checked
   * Then it is partial; one that played all 90 is not
   */
  it("is_true_only_when_fewer_days_were_played_than_asked_for", () => {
    expect(isPartialRun(response([emptyClub({ days_played: 12 })], 90))).toBe(true);
    expect(isPartialRun(response([emptyClub({ days_played: 90 })], 90))).toBe(false);
  });

  /**
   * Given a manager dismissed on day 30 of 90
   * When it is checked
   * Then it is partial: the report covers 30 days
   */
  it("counts_the_days_of_every_club_stretch", () => {
    const stretches = [
      emptyClub({ days_played: 30, ended: "dismissed" }),
      emptyClub({ club_id: "blues", days_played: 20 }),
    ];
    expect(isPartialRun(response(stretches, 90))).toBe(true);
    expect(isPartialRun(response(stretches, 50))).toBe(false);
  });
});

describe("isPartialRun without a club", () => {
  /**
   * Given a manager who held no club during the run
   * When it is checked
   * Then it is not partial: there was nothing to cover
   */
  it("is_false_when_no_club_was_managed", () => {
    expect(isPartialRun(response([], 90))).toBe(false);
  });
});

describe("reportClubs", () => {
  /**
   * Given a response from a backend with no report
   * When the clubs are read
   * Then there are none, and nothing throws
   */
  it("treats_a_missing_report_as_no_clubs", () => {
    expect(reportClubs({ progress: {} } as unknown as FastForwardResponse)).toEqual([]);
  });
});
