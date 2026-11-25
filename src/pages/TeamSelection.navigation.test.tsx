import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../store/gameStore";
import { useGameStore } from "../store/gameStore";
import { resetCareerSessionForTests } from "../services/careerSession";
import { ThemeProvider } from "../context/ThemeContext";
import TeamSelection from "./TeamSelection";

/**
 * How team selection behaves when it is opened with nothing in the store: a reloaded webview, or a
 * route reached before the store was filled. It must ask the backend before it gives up, it must
 * never navigate while rendering, and it must not treat a failed question as "no career".
 */

const navigateMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("react-router-dom", () => ({ useNavigate: () => navigateMock }));
vi.mock("../services/portraitService", () => ({ prewarmManagerSquadPortraits: vi.fn() }));
vi.mock("../store/gameStore", () => ({ useGameStore: vi.fn() }));
vi.mock("../lib/extraTranslations", () => ({ applyExtraTranslations: vi.fn() }));

const translate = vi.hoisted(
  () => (key: string, params?: Record<string, string | number>) =>
    params?.defaultValue !== undefined ? String(params.defaultValue) : key,
);
vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: () => {} },
  useTranslation: () => ({ t: translate, i18n: { language: "en" } }),
}));

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

const invokeMock = vi.mocked(invoke);
const setGameState = vi.fn();
const setGameActive = vi.fn();

function emptyStore() {
  vi.mocked(useGameStore).mockReturnValue({
    gameState: null,
    hasActiveGame: false,
    setGameState,
    setGameActive,
  } as unknown as ReturnType<typeof useGameStore>);
}

function renderScreen() {
  return render(
    <ThemeProvider>
      <TeamSelection />
    </ThemeProvider>,
  );
}

describe("TeamSelection with an empty store", () => {
  let consoleError: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    navigateMock.mockReset();
    setGameState.mockReset();
    setGameActive.mockReset();
    invokeMock.mockReset();
    resetCareerSessionForTests();
    emptyStore();
    consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => consoleError.mockRestore());

  /**
   * Given the store is empty and the backend holds no career
   * When team selection opens
   * Then it waits for the answer, then leaves for the menu once, and never during render
   */
  it("leaves_for_the_menu_once_and_only_after_the_backend_says_there_is_no_career", async () => {
    let answer!: (error: string) => void;
    invokeMock.mockImplementation(
      () => new Promise((_resolve, reject) => (answer = (e) => reject(e))),
    );

    renderScreen();
    expect(navigateMock).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toBeInTheDocument();

    await act(async () => answer("be.error.noActiveGameSession"));

    expect(navigateMock).toHaveBeenCalledTimes(1);
    expect(navigateMock).toHaveBeenCalledWith("/", { replace: true });
    expect(consoleError).not.toHaveBeenCalled();
  });

  /**
   * Given the store is empty but the backend holds a career awaiting its first club
   * When team selection opens
   * Then the store is filled from the backend and the player stays on the screen
   */
  it("fills_the_store_from_the_backend_instead_of_bouncing_to_the_menu", async () => {
    const game = { manager: { display_name: "Sam Boss" }, extra_translations: {} };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_active_game") return game;
      if (command === "get_career_stage") return "awaitingClub";
      return null;
    });

    renderScreen();
    await act(async () => {});

    expect(setGameState).toHaveBeenCalledWith(game as unknown as GameStateData);
    expect(setGameActive).toHaveBeenCalledWith(true, "Sam Boss");
    expect(navigateMock).not.toHaveBeenCalledWith("/", expect.anything());
  });

  /**
   * Given the backend cannot answer for a reason that is not "no career"
   * When team selection opens
   * Then the player is not thrown to the menu; an alert offers a retry
   */
  it("shows_an_alert_not_a_redirect_when_the_lookup_itself_fails", async () => {
    invokeMock.mockRejectedValue(new Error("database is locked"));

    renderScreen();
    await act(async () => {});

    expect(navigateMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "career.loadFailed.retry" })).toBeInTheDocument();
  });
});
