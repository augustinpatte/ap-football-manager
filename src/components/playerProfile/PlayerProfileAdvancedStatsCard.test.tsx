import { render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import i18n, { changeAppLanguage, i18nReady } from "../../i18n";
import type { PlayerAdvancedStatsSummary } from "./PlayerProfile.helpers";
import PlayerProfileAdvancedStatsCard from "./PlayerProfileAdvancedStatsCard";

const SUMMARY: PlayerAdvancedStatsSummary = {
  percentileEligible: true,
  peerGroup: null,
  seasonRatingRated: false,
  detailRecorded: true,
  metrics: {
    shots: { total: 20, per90: 4, percentile: 2 },
    shotsOnTarget: { total: 10, per90: 2, percentile: null },
    passes: { completed: 80, attempted: 100, accuracy: 80, percentile: null },
    tacklesWon: { total: 9, per90: 1.8, percentile: null },
    interceptions: { total: 6, per90: 1.2, percentile: null },
    foulsCommitted: { total: 5, per90: 1, percentile: null },
  },
};

const DETAIL_NOT_RECORDED =
  "Detailed match statistics are not recorded for matches played in the background.";

describe("PlayerProfileAdvancedStatsCard percentiles", () => {
  beforeAll(async () => {
    await i18nReady;
  });

  afterEach(async () => {
    await changeAppLanguage("en");
  });

  /**
   * Given the game language is Portuguese and a shots percentile of 2
   * When the card renders
   * Then it reads "2.º", not "2nd"
   */
  it("writes the percentile as an ordinal in the game language", async () => {
    await changeAppLanguage("pt");

    render(<PlayerProfileAdvancedStatsCard summary={SUMMARY} t={(key) => key} />);

    expect(screen.getByText("2.º")).toBeInTheDocument();
    expect(screen.queryByText("2nd")).not.toBeInTheDocument();
  });

  /**
   * Given the game language is English
   * When the same card renders
   * Then it reads "2nd"
   */
  it("keeps English suffixes in English", async () => {
    await changeAppLanguage("en");

    render(<PlayerProfileAdvancedStatsCard summary={SUMMARY} t={(key) => key} />);

    expect(screen.getByText("2nd")).toBeInTheDocument();
  });

  /**
   * Given the backend names a striker's group as the second division
   * When the card renders
   * Then the screen says which group the percentiles compare with
   */
  it("states the league and position the percentiles are measured within", () => {
    render(
      <PlayerProfileAdvancedStatsCard
        summary={{
          ...SUMMARY,
          peerGroup: {
            competitionId: "div-2",
            competitionName: "Second Division",
            competitionNameKey: null,
            countryId: null,
            position: "Striker",
          },
        }}
        t={i18n.t}
      />,
    );

    expect(screen.getByText("Compared with: Striker in Second Division")).toBeInTheDocument();
  });

  /**
   * Given the backend gives no peer group (a player with no league)
   * When the card renders
   * Then no group line is shown
   */
  it("shows no group line without a peer group", () => {
    render(<PlayerProfileAdvancedStatsCard summary={SUMMARY} t={i18n.t} />);

    expect(screen.queryByText(/Compared with/)).not.toBeInTheDocument();
  });

  /**
   * Given a player of a club the game simulates in the background (no minute of his season had
   * its shots and passes recorded)
   * When the card renders
   * Then it says so and shows no number, not a row of zeros
   */
  it("shows nothing measured, and says why, when the detail was not recorded", () => {
    const unrecorded: PlayerAdvancedStatsSummary = {
      ...SUMMARY,
      detailRecorded: false,
      metrics: {
        shots: { total: 0, per90: null, percentile: null },
        shotsOnTarget: { total: 0, per90: null, percentile: null },
        passes: { completed: 0, attempted: 0, accuracy: null, percentile: null },
        tacklesWon: { total: 0, per90: null, percentile: null },
        interceptions: { total: 0, per90: null, percentile: null },
        foulsCommitted: { total: 0, per90: null, percentile: null },
      },
    };

    render(<PlayerProfileAdvancedStatsCard summary={unrecorded} t={(key) => key} />);

    expect(screen.getByText(DETAIL_NOT_RECORDED)).toBeInTheDocument();
    expect(screen.queryByText("0")).not.toBeInTheDocument();
    expect(screen.queryByText("0 / 0")).not.toBeInTheDocument();
  });

  /**
   * The control for the test above: with the detail recorded, the note is absent and the totals
   * show, so the previous test cannot pass for a card that hides everything.
   */
  it("shows the totals and no note when the detail was recorded", () => {
    render(<PlayerProfileAdvancedStatsCard summary={SUMMARY} t={(key) => key} />);

    expect(screen.queryByText(DETAIL_NOT_RECORDED)).not.toBeInTheDocument();
    expect(screen.getByText("20")).toBeInTheDocument();
  });
});
