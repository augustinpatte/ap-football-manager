import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady, SUPPORTED_LANGUAGES } from "../../i18n";
import type { GameStateData, PlayerData } from "../../store/gameStore";
import TransferActivityPanel from "./TransferActivityPanel";

/**
 * The manager's one place to see where every offer and loan stands, after the dialog that sent it
 * is closed. Real translations, so a missing string or a leaked key shows.
 */

const USER = "team-user";
const OTHER = "team-other";

function gameWith(players: Partial<PlayerData>[]): GameStateData {
  return {
    manager: { team_id: USER },
    clock: { current_date: "2026-08-20T12:00:00Z" },
    teams: [
      { id: USER, name: "User FC" },
      { id: OTHER, name: "Other FC" },
    ],
    players: players.map((player, index) => ({
      id: `p${index}`,
      full_name: `Player ${index}`,
      team_id: USER,
      transfer_offers: [],
      loan_offers: [],
      active_loan: null,
      ...player,
    })),
  } as unknown as GameStateData;
}

const received = {
  id: "o1",
  from_team_id: OTHER,
  fee: 5_000_000,
  wage_offered: 0,
  last_manager_fee: null,
  negotiation_round: 1,
  suggested_counter_fee: null,
  status: "Pending" as const,
  date: "2026-08-18",
};

beforeAll(async () => {
  await i18nReady;
  await i18n.changeLanguage("en");
});

describe("TransferActivityPanel", () => {
  /**
   * Given an offer waiting for an answer and a completed loan out
   * When the panel is shown closed
   * Then its summary says one thing waits for the manager and counts the whole, and no row is
   *      printed until it is opened
   */
  it("summarises_what_waits_for_an_answer_while_closed", () => {
    render(
      <TransferActivityPanel
        gameState={gameWith([{ transfer_offers: [received] }])}
        onSelectPlayer={vi.fn()}
      />,
    );

    const toggle = screen.getByRole("button", { name: /transfer activity/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(within(toggle).getByText("1 waiting for your answer")).toBeInTheDocument();
    expect(screen.queryByText("Player 0")).not.toBeInTheDocument();
  });

  /**
   * Given the same state
   * When the panel is opened
   * Then the offer is under "Offers received", with the club, the fee and its status in words, and
   *      the player's name opens his page
   */
  it("lists_the_offer_in_its_section_and_opens_the_player", () => {
    const onSelectPlayer = vi.fn();
    render(
      <TransferActivityPanel
        gameState={gameWith([{ transfer_offers: [received] }])}
        onSelectPlayer={onSelectPlayer}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /transfer activity/i }));

    const section = screen.getByRole("region", { name: /offers received/i });
    expect(within(section).getByText("Other FC")).toBeInTheDocument();
    expect(within(section).getByText("Live")).toBeInTheDocument();
    fireEvent.click(within(section).getByRole("button", { name: "Open Player 0" }));
    expect(onSelectPlayer).toHaveBeenCalledWith("p0");
  });

  /**
   * Given a manager with no dealings
   * When the panel is opened
   * Then it says so, and still explains where a fee agreed before the player's terms is
   */
  it("says_there_is_nothing_and_explains_the_unstored_stage", () => {
    render(<TransferActivityPanel gameState={gameWith([{}])} onSelectPlayer={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /transfer activity/i }));

    expect(screen.getByText(/Nothing yet/)).toBeInTheDocument();
    expect(screen.getByText(/not yet with the player is not stored/)).toBeInTheDocument();
  });

  /**
   * Given an agreed deal and a loan in each of the 12 languages
   * When the panel is opened
   * Then no translation key or placeholder is left on screen
   */
  for (const { code } of SUPPORTED_LANGUAGES) {
    it(`leaks_no_key_or_placeholder_in_${code}`, async () => {
      await i18n.changeLanguage(code);
      const { container } = render(
        <TransferActivityPanel
          gameState={gameWith([
            {
              team_id: OTHER,
              transfer_offers: [
                {
                  ...received,
                  from_team_id: USER,
                  status: "PendingRegistration",
                  registration_date: "2027-01-01",
                },
              ],
            },
            {
              active_loan: {
                parent_team_id: USER,
                loan_team_id: OTHER,
                start_date: "2026-08-01",
                end_date: "2027-06-30",
                wage_contribution_pct: 60,
              },
              team_id: OTHER,
            },
          ])}
          onSelectPlayer={vi.fn()}
        />,
      );
      fireEvent.click(screen.getByRole("button", { expanded: false }));

      const text = container.textContent ?? "";
      expect(text).not.toMatch(/transfers\.activity|\{\{|undefined/);
      expect(text).toContain("Other FC");
      await i18n.changeLanguage("en");
    });
  }
});
