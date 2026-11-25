import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady, SUPPORTED_LANGUAGES } from "../../i18n";
import type { GameStateData } from "../../store/gameStore";
import type { FixtureSummary, UserFixture, UserSchedule } from "../../services/scheduleService";
import MyMatchesView from "./MyMatchesView";

/**
 * The manager's default calendar: every competition his club plays in, in date order, so a
 * Champions League night is on it without choosing the Champions League. Real translations.
 */

function entry(
  competitionId: string,
  competitionName: string,
  id: string,
  date: string,
  overrides: Omit<Partial<UserFixture>, "fixture"> & { fixture?: Partial<FixtureSummary> } = {},
): UserFixture {
  const { fixture: fixtureOverrides, ...rest } = overrides;
  return {
    fixture: {
      id,
      matchday: 1,
      date,
      home_team_id: "psg",
      home_team_name: "PSG",
      away_team_id: `opp-${id}`,
      away_team_name: `Opponent ${id}`,
      competition: "League",
      competition_id: competitionId,
      status: "Scheduled",
      result: null,
      ...fixtureOverrides,
    },
    competition_name: competitionName,
    is_home: true,
    is_next: false,
    ...rest,
  };
}

const schedule: UserSchedule = {
  today: "2026-09-10",
  fixtures: [
    entry("ligue-1", "Ligue 1", "L1", "2026-08-30", {
      fixture: { status: "Completed", result: { home_goals: 2, away_goals: 1 } },
    }),
    entry("ucl", "UEFA Champions League", "E1", "2026-09-16", {
      is_next: true,
      is_home: false,
      fixture: { home_team_name: "Opponent E1", away_team_name: "PSG" },
    }),
    entry("ligue-1", "Ligue 1", "L2", "2026-09-20"),
    entry("coupe", "Coupe de France", "C1", "2026-10-02"),
  ],
};

const gameState = { competitions: [] } as unknown as GameStateData;

beforeAll(async () => {
  await i18nReady;
  await i18n.changeLanguage("en");
});

function show(overrides: Partial<React.ComponentProps<typeof MyMatchesView>> = {}) {
  const onOpenReplay = vi.fn().mockResolvedValue(undefined);
  render(
    <MyMatchesView
      schedule={schedule}
      gameState={gameState}
      onOpenReplay={onOpenReplay}
      replayLoadingFixture={null}
      t={i18n.t.bind(i18n)}
      {...overrides}
    />,
  );
  return { onOpenReplay };
}

describe("MyMatchesView", () => {
  /**
   * Given a club in the league, the cup and the Champions League
   * When the default view is shown
   * Then all of its matches are listed in date order, the Champions League one among them without
   *      any selection
   */
  it("lists_every_competition_in_date_order_without_choosing_one", () => {
    show();

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(4);
    expect(within(rows[1]).getByText("UEFA Champions League")).toBeInTheDocument();
    expect(within(rows[1]).getByText(/Opponent E1/)).toBeInTheDocument();
    expect(within(rows[1]).getByText("(Away)")).toBeInTheDocument();
    expect(within(rows[0]).getByText("2 – 1")).toBeInTheDocument();
  });

  /**
   * Given a next match in Europe
   * When the view is shown
   * Then it alone carries the next-match badge
   */
  it("marks_the_next_match_across_competitions", () => {
    show();

    expect(screen.getAllByText("Next match")).toHaveLength(1);
    expect(within(screen.getAllByRole("listitem")[1]).getByText("Next match")).toBeInTheDocument();
  });

  /**
   * Given the list
   * When a competition is chosen in the filter, and then "all" again
   * Then only its matches show, then all return
   */
  it("filters_by_competition_and_back", () => {
    show();

    fireEvent.click(screen.getByRole("combobox", { name: "Competition" }));
    fireEvent.click(screen.getByRole("option", { name: "Ligue 1" }));
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.queryByText("UEFA Champions League")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("combobox", { name: "Competition" }));
    fireEvent.click(screen.getByRole("option", { name: "All competitions" }));
    expect(screen.getAllByRole("listitem")).toHaveLength(4);
  });

  /**
   * Given a played match in the Champions League
   * When its review is opened
   * Then the replay is asked for with that competition's id and that fixture's id
   */
  it("opens_a_replay_with_the_exact_competition_and_fixture_ids", () => {
    const played: UserSchedule = {
      today: "2026-09-10",
      fixtures: [
        entry("ucl", "UEFA Champions League", "E9", "2026-09-01", {
          fixture: { status: "Completed", result: { home_goals: 1, away_goals: 1 } },
        }),
      ],
    };
    const { onOpenReplay } = show({ schedule: played });

    fireEvent.click(screen.getByRole("button", { name: "Replay in 2D" }));

    expect(onOpenReplay).toHaveBeenCalledWith("ucl", "E9");
  });

  /**
   * Given a match moved from its first date
   * When the view is shown
   * Then the row says where it was first set
   */
  it("names_the_original_date_of_a_postponed_match", () => {
    const moved: UserSchedule = {
      today: "2026-09-10",
      fixtures: [
        entry("ligue-1", "Ligue 1", "L7", "2026-09-30", {
          fixture: { rescheduled_from: "2026-09-27" },
        }),
      ],
    };
    show({ schedule: moved });

    expect(screen.getByTestId("moved-from-L7")).toBeInTheDocument();
  });

  /**
   * Given no schedule yet, and one with no matches
   * When shown
   * Then it says it is loading, then that there are none
   */
  it("says_when_it_is_loading_and_when_there_is_nothing", () => {
    const { unmount } = render(
      <MyMatchesView
        schedule={null}
        gameState={gameState}
        onOpenReplay={vi.fn()}
        replayLoadingFixture={null}
        t={i18n.t.bind(i18n)}
      />,
    );
    expect(screen.getByText(i18n.t("dashboard.loading"))).toBeInTheDocument();
    unmount();

    show({ schedule: { today: "2026-09-10", fixtures: [] } });
    expect(screen.getByText("No matches found for your club.")).toBeInTheDocument();
  });

  for (const { code } of SUPPORTED_LANGUAGES) {
    /** Given any shipped language, when the list is shown, then no key or placeholder is left. */
    it(`leaks_no_key_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      const { container } = render(
        <MyMatchesView
          schedule={schedule}
          gameState={gameState}
          onOpenReplay={vi.fn()}
          replayLoadingFixture={null}
          t={i18n.t.bind(i18n)}
        />,
      );
      expect(container.textContent).not.toMatch(/schedule\.myMatches|\{\{|undefined/);
      await i18n.changeLanguage("en");
    });
  }
});
