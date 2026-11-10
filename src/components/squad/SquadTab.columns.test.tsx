import { fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import { useSettingsStore } from "../../store/settingsStore";
import type { GameStateData } from "../../store/gameStore";
import { createPlayer, createSeasonContext, createTeam } from "../../test-utils/factories";
import SquadTab from "./SquadTab";

/**
 * The squad table's column chooser: which columns the player can hide, that the name stays,
 * that sorting follows what is visible, that the choice is saved, and that it reads in French.
 */

vi.mock("../../hooks/useContractWarningStages", () => ({
  useContractRiskLevels: () => () => "stable",
}));
vi.mock("../../hooks/useRatingTiers", async () => {
  const { cannedRatingTiersModule } = await import("../../test-utils/ratingTiers");
  return cannedRatingTiersModule();
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const OPTIONAL_EN = [
  "Jersey Number",
  "Pos",
  "Familiarity",
  "Traits",
  "Age",
  "Condition",
  "Morale",
  "OVR",
  "Contract",
];

function game(): GameStateData {
  const players = ["gk1", "d1", "d2", "m1", "f1"].map((id, index) =>
    createPlayer({
      id,
      match_name: id.toUpperCase(),
      full_name: `Player ${id}`,
      position: index === 0 ? "Goalkeeper" : "Defender",
      natural_position: index === 0 ? "Goalkeeper" : "Defender",
      team_id: "team-1",
      morale: 50 + index * 10,
      match_day_eligible: true,
    }),
  );
  return {
    season_context: createSeasonContext(),
    clock: { current_date: "2026-08-01", start_date: "2026-08-01" },
    manager: {
      display_name: "Test Manager",
      id: "manager-1",
      first_name: "Test",
      last_name: "Manager",
      date_of_birth: "1980-01-01",
      nationality: "GB",
      reputation: 50,
      satisfaction: 50,
      fan_approval: 50,
      team_id: "team-1",
      career_stats: {
        matches_managed: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        trophies: 0,
        best_finish: null,
      },
      career_history: [],
    },
    teams: [createTeam({ starting_xi_ids: players.map((player) => player.id) })],
    players,
    staff: [],
    messages: [],
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
  };
}

function renderSquad() {
  const state = game();
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "get_squad") return state.players;
    return undefined;
  });
  return render(
    <SquadTab
      gameState={state}
      managerId="manager-1"
      onSelectPlayer={vi.fn()}
      onGameUpdate={vi.fn()}
    />,
  );
}

const headerNames = () => screen.getAllByRole("columnheader").map((header) => header.textContent);
const savedHidden = () => {
  const saves = vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_settings");
  const last = saves[saves.length - 1]?.[1] as
    | { settings: { squad_hidden_columns: string[] } }
    | undefined;
  return last?.settings.squad_hidden_columns;
};

describe("the squad table's column chooser", () => {
  beforeEach(async () => {
    await i18nReady;
    await i18n.changeLanguage("en");
    vi.mocked(invoke).mockReset();
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, squad_hidden_columns: [] },
    }));
  });

  /**
   * Given a player with no stored choice,
   * when the squad opens and the chooser is expanded,
   * then every column shows, and the chooser offers each optional one, checked, but not the name.
   */
  it("shows every column by default and offers every optional one but the name", async () => {
    renderSquad();
    const toggle = await screen.findByRole("button", { name: "Columns" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(headerNames()).toEqual(expect.arrayContaining(["Name", "Age", "Morale"]));

    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const group = screen.getByRole("group", { name: "Columns" });
    const boxes = within(group).getAllByRole("checkbox");
    expect(boxes.map((box) => box.getAttribute("aria-label"))).toEqual(OPTIONAL_EN);
    expect(boxes.every((box) => (box as HTMLInputElement).checked)).toBe(true);
  });

  /**
   * Given the chooser open,
   * when the player unticks Age,
   * then the Age header and its cells go, the name stays, the other columns are untouched, and the
   *      choice is saved to the settings.
   */
  it("hides a column's header and cells and saves the choice", async () => {
    renderSquad();
    const cellsBefore = (await screen.findAllByRole("row"))[1].querySelectorAll("td").length;
    fireEvent.click(await screen.findByRole("button", { name: "Columns" }));

    fireEvent.click(screen.getByRole("checkbox", { name: "Age" }));

    expect(headerNames()).not.toContain("Age");
    expect(headerNames()).toContain("Name");
    expect(headerNames()).toContain("Morale");
    expect(screen.getAllByRole("row")[1].querySelectorAll("td").length).toBe(cellsBefore - 1);
    expect(savedHidden()).toEqual(["age"]);
  });

  /**
   * Given the chooser open,
   * when the player clicks a column's visible name rather than its box,
   * then the box toggles all the same: the label is tied to the control.
   */
  it("toggles a column from its visible name", async () => {
    renderSquad();
    fireEvent.click(await screen.findByRole("button", { name: "Columns" }));

    fireEvent.click(screen.getByText("Morale", { selector: "label" }));

    expect(headerNames()).not.toContain("Morale");
    expect(screen.getByRole("checkbox", { name: "Morale" })).not.toBeChecked();
  });

  /**
   * Given a choice already saved in the settings (a restart, or a second visit),
   * when the squad opens,
   * then that column is already hidden and its box is unticked: nothing has to be chosen twice.
   */
  it("opens with the saved choice applied", async () => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, squad_hidden_columns: ["morale", "traits"] },
    }));

    renderSquad();
    await screen.findByRole("button", { name: "Columns" });

    expect(headerNames()).not.toContain("Morale");
    expect(headerNames()).not.toContain("Traits");
    fireEvent.click(screen.getByRole("button", { name: "Columns" }));
    expect(screen.getByRole("checkbox", { name: "Morale" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Age" })).toBeChecked();
  });

  /**
   * Given a hidden column and a visible one,
   * when the player sorts by the visible one,
   * then the table sorts: hiding a column does not take sorting away from the others.
   */
  it("keeps sorting working on the columns that stay", async () => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, squad_hidden_columns: ["age"] },
    }));
    renderSquad();

    fireEvent.click(await screen.findByRole("button", { name: "Morale" }));
    // Higher morale first is the starting direction of that column.
    expect(screen.getByRole("columnheader", { name: "Morale" })).toHaveAttribute(
      "aria-sort",
      "descending",
    );
    fireEvent.click(screen.getByRole("button", { name: "Morale" }));

    expect(screen.getByRole("columnheader", { name: "Morale" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
  });

  /**
   * Given the table sorted by Age,
   * when the player hides Age,
   * then the order falls back to the default (by position) instead of staying set by a column
   *      nobody can see.
   */
  it("drops a sort on the column the player hides", async () => {
    renderSquad();
    fireEvent.click(await screen.findByRole("button", { name: "Age" }));
    expect(screen.getByRole("columnheader", { name: "Age" })).toHaveAttribute("aria-sort");
    fireEvent.click(screen.getByRole("button", { name: "Columns" }));

    fireEvent.click(screen.getByRole("checkbox", { name: "Age" }));

    expect(screen.getByRole("columnheader", { name: "Pos" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
  });

  /**
   * Given several hidden columns,
   * when the player chooses Show all columns,
   * then they all return and an empty list is saved.
   */
  it("brings every column back with Show all columns", async () => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, squad_hidden_columns: ["age", "morale"] },
    }));
    renderSquad();
    fireEvent.click(await screen.findByRole("button", { name: "Columns" }));

    fireEvent.click(screen.getByRole("button", { name: "Show all columns" }));

    expect(headerNames()).toEqual(expect.arrayContaining(["Age", "Morale"]));
    expect(savedHidden()).toEqual([]);
  });

  /**
   * Given a keyboard user,
   * when they tab to the chooser, open it, and close it with Escape,
   * then the toggle is a real button, the boxes are real checkboxes, and Escape closes the panel
   *      and puts focus back on the button.
   */
  it("works from the keyboard: real controls, and Escape closes and returns focus", async () => {
    renderSquad();
    const toggle = await screen.findByRole("button", { name: "Columns" });
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle.className).toContain("focus-visible:ring-focus");

    fireEvent.click(toggle);
    const box = screen.getByRole("checkbox", { name: "Age" });
    expect((box as HTMLInputElement).type).toBe("checkbox");
    box.focus();
    fireEvent.keyDown(box, { key: "Escape" });

    expect(screen.queryByRole("group", { name: "Columns" })).not.toBeInTheDocument();
    expect(toggle).toHaveFocus();
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  /**
   * Given the game in French,
   * when the chooser is opened,
   * then the button, the group, every column's name and the reset all read in French.
   */
  it("reads in French", async () => {
    await i18n.changeLanguage("fr");
    renderSquad();
    fireEvent.click(await screen.findByRole("button", { name: "Colonnes" }));

    const group = screen.getByRole("group", { name: "Colonnes" });
    const names = within(group)
      .getAllByRole("checkbox")
      .map((box) => box.getAttribute("aria-label"));
    expect(names).toEqual([
      "Numéro de Maillot",
      "Pos.",
      "Familiarité",
      "Attributs",
      "Âge",
      "Condition",
      "Moral",
      "GEN",
      "Contrat",
    ]);
    expect(
      screen.getByRole("button", { name: "Afficher toutes les colonnes" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^squad\./)).not.toBeInTheDocument();
  });
});
