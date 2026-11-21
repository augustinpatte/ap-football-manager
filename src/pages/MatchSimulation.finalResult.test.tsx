import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import MatchSimulation from "./MatchSimulation";

const setGameStateMock = vi.fn();
const navigateMock = vi.fn();
let locationState: unknown = null;
let gameStoreState: {
  gameState: Record<string, unknown> | null;
  setGameState: typeof setGameStateMock;
};

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../lib/errorDialog", () => ({ showError: vi.fn() }));
vi.mock("../utils/backendI18n", () => ({ resolveBackendKeyError: (_e: unknown, f: string) => f }));
vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateMock,
  useLocation: () => ({ state: locationState }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key,
    i18n: { language: "en" },
  }),
}));
vi.mock("../store/gameStore", () => ({ useGameStore: () => gameStoreState }));
vi.mock("../store/settingsStore", () => ({
  useSettingsStore: () => ({ settings: { match_speed: "normal" } }),
}));

type ScreenProps = {
  snapshot: { home_score: number; away_score: number; events: { event_type: string }[] };
  spatialResult?: { homeGoals: number; awayGoals: number; events: unknown[] } | null;
  onContinue?: () => void;
  onPressConference?: () => void;
};

vi.mock("../components/match/PreMatchSetup", () => ({
  default: ({ viewModeSlot }: { viewModeSlot?: React.ReactNode }) => (
    <div data-testid="prematch">{viewModeSlot}</div>
  ),
}));
vi.mock("../components/match/PostMatchScreen", () => ({
  // Reads its props the way the real screen does: the score and report events come from the
  // spatial result when there is one, else from the snapshot.
  default: ({ snapshot, spatialResult, onContinue }: ScreenProps) => (
    <div>
      <div data-testid="postmatch-report">
        {spatialResult?.homeGoals ?? snapshot.home_score}-
        {spatialResult?.awayGoals ?? snapshot.away_score}:
        {(spatialResult?.events ?? snapshot.events).length}
      </div>
      <button type="button" data-testid="postmatch-continue" onClick={onContinue}>
        Continue
      </button>
    </div>
  ),
}));
vi.mock("../components/match/InteractiveMatchScreen", () => ({
  default: ({ onFinished }: { onFinished: (response: unknown) => void }) => (
    <button
      type="button"
      data-testid="finish-managed"
      onClick={() => onFinished({ game: gameWithFixture(fourTwo) })}
    >
      Finish managed match
    </button>
  ),
}));
vi.mock("../components/match/RoundDigestScreen", () => ({
  default: ({ snapshot, onPressConference }: ScreenProps) => (
    <div>
      <div data-testid="digest-score">
        {snapshot.home_score}-{snapshot.away_score}
      </div>
      <button type="button" data-testid="digest-press" onClick={onPressConference}>
        Press
      </button>
    </div>
  ),
}));
vi.mock("../components/match/PressConference", () => ({
  default: ({ snapshot }: ScreenProps) => (
    <div>
      <div data-testid="press-score">
        {snapshot.home_score}-{snapshot.away_score}
      </div>
      <div data-testid="press-goals">
        {snapshot.events.filter((event) => event.event_type === "Goal").length}
      </div>
    </div>
  ),
}));

const mockedInvoke = vi.mocked(invoke);

function enginePlayer(id: string): Record<string, unknown> {
  return { id, name: id, position: "Goalkeeper", condition: 100 };
}

function preMatchSnapshot(): Record<string, unknown> {
  return {
    phase: "PreKickOff",
    current_minute: 0,
    home_score: 0,
    away_score: 0,
    events: [],
    home_team: { id: "home1", name: "Home FC", players: [enginePlayer("h1")] },
    away_team: { id: "away1", name: "Away FC", players: [enginePlayer("a1")] },
    home_bench: [],
    away_bench: [],
    substitutions: [],
  };
}

const fourTwo = {
  home_goals: 4,
  away_goals: 2,
  home_scorers: [
    { player_id: "h1", minute: 5 },
    { player_id: "h1", minute: 30 },
    { player_id: "h1", minute: 61 },
    { player_id: "h1", minute: 88 },
  ],
  away_scorers: [
    { player_id: "a1", minute: 12 },
    { player_id: "a1", minute: 70 },
  ],
};

function gameWithFixture(result: typeof fourTwo | null): Record<string, unknown> {
  const team = (id: string) => ({ id, name: id, colors: { primary: "#000", secondary: "#fff" } });
  return {
    clock: { current_date: "2026-08-01", start_date: "2026-08-01" },
    manager: { id: "mgr1", team_id: "home1" },
    teams: [team("home1"), team("away1")],
    players: [],
    staff: [],
    messages: [],
    news: [],
    competitions: [],
    league: {
      id: "league1",
      name: "League",
      standings: [],
      top_scorers: [],
      fixtures: [
        {
          id: "fix1",
          competition: "League",
          competition_id: "league1",
          home_team_id: "home1",
          away_team_id: "away1",
          date: "2026-08-01",
          status: result ? "Completed" : "Scheduled",
          result,
          round: 1,
          matchday: null,
        },
      ],
    },
  };
}

describe("MatchSimulation final result", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    setGameStateMock.mockReset();
    locationState = null;
    gameStoreState = { gameState: gameWithFixture(null), setGameState: setGameStateMock };
  });

  /**
   * Given a match that the M12 quick simulation finished 4-2, while the page was opened on the
   *       pre-match 0-0 snapshot
   * When the manager continues to the digest and the press conference
   * Then both are told 4-2 with the six goals, never the 0-0 the page started from.
   */
  it("tells the digest and the press conference the simulated 4-2, not the pre-match 0-0", async () => {
    mockedInvoke.mockResolvedValueOnce(preMatchSnapshot());
    render(<MatchSimulation />);
    const simulateButton = await screen.findByRole("button", { name: "match.simulateWithM12" });

    const finished = gameWithFixture(fourTwo);
    mockedInvoke.mockResolvedValueOnce({
      game: finished,
      home_goals: 4,
      away_goals: 2,
      events: [
        ...fourTwo.home_scorers.map((s) => ({
          minute: s.minute,
          event_type: "Goal",
          side: "Home",
          zone: "Box",
          player_id: s.player_id,
          secondary_player_id: null,
        })),
        ...fourTwo.away_scorers.map((s) => ({
          minute: s.minute,
          event_type: "Goal",
          side: "Away",
          zone: "Box",
          player_id: s.player_id,
          secondary_player_id: null,
        })),
      ],
    });
    // The store hands the finished game back, as it does in the app.
    setGameStateMock.mockImplementation((game: Record<string, unknown>) => {
      gameStoreState = { gameState: game, setGameState: setGameStateMock };
    });

    fireEvent.click(simulateButton);
    expect(await screen.findByTestId("postmatch-report")).toHaveTextContent("4-2:6");
    fireEvent.click(screen.getByTestId("postmatch-continue"));
    expect(await screen.findByTestId("digest-score")).toHaveTextContent("4-2");

    fireEvent.click(screen.getByTestId("digest-press"));
    await waitFor(() => expect(screen.getByTestId("press-score")).toHaveTextContent("4-2"));
    expect(screen.getByTestId("press-goals")).toHaveTextContent("6");
  });

  /**
   * Given a match the manager played through the interactive screen, whose finish returns no
   *       event list, while the career's fixture holds the 4-2 and its six scorers
   * When the post-match report, the digest and the press conference open
   * Then all three describe 4-2 with six goals, taken from the stored fixture.
   */
  it("tells every screen the stored 4-2 after a managed match", async () => {
    mockedInvoke.mockResolvedValueOnce(preMatchSnapshot());
    setGameStateMock.mockImplementation((game: Record<string, unknown>) => {
      gameStoreState = { gameState: game, setGameState: setGameStateMock };
    });
    render(<MatchSimulation />);
    fireEvent.click(await screen.findByRole("button", { name: "match.interactive.manage" }));

    fireEvent.click(await screen.findByTestId("finish-managed"));

    expect(await screen.findByTestId("postmatch-report")).toHaveTextContent("4-2:6");
    fireEvent.click(screen.getByTestId("postmatch-continue"));
    expect(await screen.findByTestId("digest-score")).toHaveTextContent("4-2");
    fireEvent.click(screen.getByTestId("digest-press"));
    await waitFor(() => expect(screen.getByTestId("press-score")).toHaveTextContent("4-2"));
    expect(screen.getByTestId("press-goals")).toHaveTextContent("6");
  });
});
