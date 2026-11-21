import type { ReactNode } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import MatchSimulation from "./MatchSimulation";

const navigateMock = vi.fn();
let gameState: Record<string, unknown>;

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../lib/errorDialog", () => ({ showError: vi.fn() }));
vi.mock("../utils/backendI18n", () => ({ resolveBackendKeyError: (_e: unknown, f: string) => f }));
vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateMock,
  useLocation: () => ({ state: null }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: "en" } }),
}));
vi.mock("../store/gameStore", () => ({
  useGameStore: () => ({ gameState, setGameState: vi.fn(), clearGame: vi.fn() }),
}));
vi.mock("../store/settingsStore", () => ({
  useSettingsStore: () => ({ settings: { match_speed: "normal" } }),
}));
vi.mock("../components/match/PreMatchSetup", () => ({
  default: ({ viewModeSlot }: { viewModeSlot?: ReactNode }) => (
    <div data-testid="prematch">{viewModeSlot}</div>
  ),
}));
vi.mock("../components/match/InteractiveMatchScreen", () => ({
  default: () => <div data-testid="interactive-screen" />,
}));
vi.mock("../components/match/M12ReplayViewer", () => ({
  default: ({ readOnly }: { readOnly?: boolean }) => (
    <div data-testid="replay-viewer" data-read-only={String(readOnly)} />
  ),
}));

const mockedInvoke = vi.mocked(invoke);

function snapshot(): Record<string, unknown> {
  const player = (id: string) => ({ id, name: id, position: "Goalkeeper", condition: 100 });
  return {
    phase: "PreKickOff",
    current_minute: 0,
    home_score: 0,
    away_score: 0,
    events: [],
    home_team: { id: "home1", name: "Home FC", players: [player("h1")] },
    away_team: { id: "away1", name: "Away FC", players: [player("a1")] },
    home_bench: [],
    away_bench: [],
    substitutions: [],
  };
}

describe("MatchSimulation viewing modes", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    navigateMock.mockReset();
    gameState = {
      clock: { current_date: "2026-08-01", start_date: "2026-08-01" },
      manager: { id: "m", team_id: "home1" },
      teams: [
        { id: "home1", name: "Home FC" },
        { id: "away1", name: "Away FC" },
      ],
      players: [],
      staff: [],
      competitions: [],
      league: null,
    };
  });

  /**
   * Given a prepared match
   * When the manager chooses Manage Match in 2D
   * Then the interactive screen opens, no replay is prepared, and nothing is simulated ahead.
   */
  it("opens the interactive screen, not a replay, for Manage Match in 2D", async () => {
    mockedInvoke.mockResolvedValueOnce(snapshot());
    render(<MatchSimulation />);

    fireEvent.click(await screen.findByRole("button", { name: "match.interactive.manage" }));

    expect(await screen.findByTestId("interactive-screen")).toBeInTheDocument();
    expect(screen.queryByTestId("replay-viewer")).not.toBeInTheDocument();
    expect(mockedInvoke).not.toHaveBeenCalledWith("prepare_spatial_replay");
    expect(mockedInvoke).not.toHaveBeenCalledWith("simulate_spatial_match");
  });

  /**
   * Given a prepared match
   * When the manager chooses Watch simulation
   * Then a replay of the already-simulated match opens, not the interactive screen, and it is not
   *      read-only because its result has not been applied yet.
   */
  it("opens a replay, never the interactive screen, for Watch simulation", async () => {
    mockedInvoke.mockResolvedValueOnce(snapshot()).mockResolvedValueOnce({ replay_json: "{}" });
    render(<MatchSimulation />);

    fireEvent.click(await screen.findByRole("button", { name: "match.watchWithM12" }));

    await waitFor(() => expect(screen.getByTestId("replay-viewer")).toBeInTheDocument());
    expect(screen.getByTestId("replay-viewer")).toHaveAttribute("data-read-only", "false");
    expect(screen.queryByTestId("interactive-screen")).not.toBeInTheDocument();
    expect(mockedInvoke).toHaveBeenCalledWith("prepare_spatial_replay");
  });
});
