import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { resetMatchCommands } from "../components/match/matchCommands";
import MatchSimulation from "./MatchSimulation";

const navigateMock = vi.fn();
const setGameStateMock = vi.fn();
let locationState: unknown = null;
let gameStoreState: {
  gameState: Record<string, unknown> | null;
  setGameState: typeof setGameStateMock;
};

vi.mock("../components/match/matchCommands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../components/match/matchCommands")>();
  return { ...actual, resetMatchCommands: vi.fn(actual.resetMatchCommands) };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

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

vi.mock("../store/gameStore", () => ({
  useGameStore: () => gameStoreState,
}));

vi.mock("../store/settingsStore", () => ({
  useSettingsStore: () => ({
    settings: { match_speed: "normal" },
  }),
}));

vi.mock("../components/match/PreMatchSetup", () => ({
  default: ({
    snapshot,
    onStart,
  }: {
    snapshot: { home_team: { name: string } };
    onStart?: () => void;
  }) => (
    <div data-testid="prematch">
      {snapshot.home_team.name}
      <button type="button" data-testid="prematch-start" onClick={onStart}>
        Start
      </button>
    </div>
  ),
}));

vi.mock("../components/match/MatchLive", () => ({
  default: ({
    snapshot,
    preferredSpeed,
    onPreferredSpeedChange,
    onHalfTime,
    onFullTime,
  }: {
    snapshot: { home_team: { name: string } };
    preferredSpeed?: string;
    onPreferredSpeedChange?: (speed: string) => void;
    onHalfTime?: () => void;
    onFullTime?: () => void;
  }) => (
    <div data-testid="match-live-container" data-preferred-speed={preferredSpeed ?? "normal"}>
      {snapshot.home_team.name}
      <button type="button" data-testid="match-live" onClick={onFullTime}>
        Full Time
      </button>
      <button type="button" data-testid="match-trigger-halftime" onClick={onHalfTime}>
        Half Time
      </button>
      <button
        type="button"
        data-testid="match-trigger-speed-fast"
        onClick={() => onPreferredSpeedChange?.("fast")}
      >
        Fast
      </button>
    </div>
  ),
}));

vi.mock("../components/match/HalfTimeBreak", () => ({
  default: ({ onResume }: { onResume?: () => void }) => (
    <div data-testid="halftime">
      <button type="button" data-testid="halftime-resume" onClick={onResume}>
        Resume
      </button>
    </div>
  ),
}));

vi.mock("../components/match/PostMatchScreen", () => ({
  default: ({
    onContinue,
    onFinish,
    spatialResult,
  }: {
    onContinue?: () => void;
    onFinish?: () => void;
    spatialResult?: { homeGoals: number; awayGoals: number; events: unknown[] } | null;
  }) => (
    <div>
      {spatialResult && (
        <div data-testid="spatial-postmatch-result">
          {spatialResult.homeGoals}-{spatialResult.awayGoals}:{spatialResult.events.length}
        </div>
      )}
      <button type="button" data-testid="postmatch-continue" onClick={onContinue}>
        Continue
      </button>
      <button type="button" data-testid="postmatch-finish" onClick={onFinish}>
        Finish Match
      </button>
    </div>
  ),
}));

vi.mock("../components/match/M12ReplayViewer", () => ({
  default: ({
    onFinish,
    onBack,
  }: {
    onFinish: () => void | Promise<void>;
    onBack?: () => void;
  }) => (
    <div data-testid="m12-viewer">
      <button type="button" data-testid="m12-go-result" onClick={() => void onFinish()}>
        Go to result
      </button>
      <button type="button" data-testid="m12-back" onClick={onBack}>
        Back
      </button>
    </div>
  ),
}));

vi.mock("../components/match/RoundDigestScreen", () => ({
  default: ({
    roundSummary,
    currentFixture,
    onPressConference,
    onFinish,
  }: {
    roundSummary?: unknown;
    currentFixture?: { id: string } | null;
    onPressConference?: () => void;
    onFinish?: () => void;
  }) => (
    <div>
      <div data-testid="digest-round-summary">
        {roundSummary ? JSON.stringify(roundSummary) : "null"}
      </div>
      <div data-testid="digest-fixture">{currentFixture?.id ?? "none"}</div>
      <button type="button" data-testid="digest-press" onClick={onPressConference}>
        Press Conference
      </button>
      <button type="button" data-testid="digest-finish" onClick={onFinish}>
        Skip
      </button>
    </div>
  ),
}));

vi.mock("../components/match/PressConference", () => ({
  default: () => <div data-testid="press" />,
}));

const mockedInvoke = vi.mocked(invoke);

function makeEnginePlayer(
  overrides: Partial<Record<string, unknown>> = {},
): Record<string, unknown> {
  return {
    id: "p1",
    name: "Player One",
    position: "Goalkeeper",
    condition: 100,
    pace: 50,
    stamina: 50,
    strength: 50,
    agility: 50,
    passing: 50,
    shooting: 50,
    tackling: 50,
    dribbling: 50,
    defending: 50,
    positioning: 50,
    vision: 50,
    decisions: 50,
    composure: 50,
    aggression: 50,
    teamwork: 50,
    leadership: 50,
    handling: 50,
    reflexes: 50,
    aerial: 50,
    traits: [],
    ...overrides,
  };
}

function makeSnapshot(overrides: Partial<Record<string, unknown>> = {}): Record<string, unknown> {
  return {
    phase: "PreKickOff",
    current_minute: 0,
    home_score: 0,
    away_score: 0,
    possession: "Home",
    ball_zone: "Midfield",
    home_team: {
      id: "home1",
      name: "Home FC",
      formation: "4-4-2",
      play_style: "Balanced",
      players: [makeEnginePlayer({ id: "home-p1", name: "Home Keeper" })],
    },
    away_team: {
      id: "away1",
      name: "Away FC",
      formation: "4-4-2",
      play_style: "Balanced",
      players: [makeEnginePlayer({ id: "away-p1", name: "Away Keeper" })],
    },
    home_bench: [],
    away_bench: [],
    home_possession_pct: 50,
    away_possession_pct: 50,
    events: [],
    home_subs_made: 0,
    away_subs_made: 0,
    max_subs: 5,
    home_set_pieces: {
      free_kick_taker: null,
      corner_taker: null,
      penalty_taker: null,
      captain: null,
    },
    away_set_pieces: {
      free_kick_taker: null,
      corner_taker: null,
      penalty_taker: null,
      captain: null,
    },
    substitutions: [],
    allows_extra_time: false,
    home_yellows: {},
    away_yellows: {},
    sent_off: [],
    ...overrides,
  };
}

function makeGameState(): Record<string, unknown> {
  return {
    clock: {
      current_date: "2026-08-01",
      start_date: "2026-08-01",
    },
    manager: {
      id: "mgr1",
      first_name: "Test",
      last_name: "Manager",
      date_of_birth: "1980-01-01",
      nationality: "GB",
      reputation: 50,
      satisfaction: 50,
      fan_approval: 50,
      team_id: "home1",
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
    teams: [
      {
        id: "home1",
        name: "Home FC",
        short_name: "HOM",
        country: "England",
        city: "Home City",
        stadium_name: "Home Ground",
        stadium_capacity: 20000,
        finance: 1000000,
        manager_id: "mgr1",
        reputation: 50,
        wage_budget: 100000,
        transfer_budget: 500000,
        season_income: 0,
        season_expenses: 0,
        formation: "4-4-2",
        play_style: "Balanced",
        training_focus: "General",
        training_intensity: "Balanced",
        training_schedule: "Balanced",
        founded_year: 1900,
        colors: { primary: "#00ff00", secondary: "#ffffff" },
        starting_xi_ids: [],
        form: [],
        history: [],
      },
      {
        id: "away1",
        name: "Away FC",
        short_name: "AWY",
        country: "England",
        city: "Away City",
        stadium_name: "Away Ground",
        stadium_capacity: 20000,
        finance: 1000000,
        manager_id: null,
        reputation: 50,
        wage_budget: 100000,
        transfer_budget: 500000,
        season_income: 0,
        season_expenses: 0,
        formation: "4-4-2",
        play_style: "Balanced",
        training_focus: "General",
        training_intensity: "Balanced",
        training_schedule: "Balanced",
        founded_year: 1900,
        colors: { primary: "#0000ff", secondary: "#ffffff" },
        starting_xi_ids: [],
        form: [],
        history: [],
      },
    ],
    players: [],
    staff: [],
    messages: [],
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
  };
}

function makeGameStateWithFriendly(competition = "Friendly") {
  const base = makeGameState();
  return {
    ...base,
    league: {
      id: "league1",
      name: "Test League",
      fixtures: [
        {
          id: "fix1",
          competition,
          home_team_id: "home1",
          away_team_id: "away1",
          date: "2026-08-01",
          status: "Scheduled",
          result: null,
          round: 1,
          matchday: null,
        },
      ],
      standings: [],
      top_scorers: [],
    },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("MatchSimulation", (): void => {
  beforeEach(function resetState(): void {
    mockedInvoke.mockReset();
    navigateMock.mockReset();
    setGameStateMock.mockReset();
    locationState = null;
    gameStoreState = {
      gameState: makeGameState(),
      setGameState: setGameStateMock,
    };
  });

  /**
   * Given a match screen opens
   * When it mounts
   * Then the in-play command chain is started afresh, so a hung command from an earlier match
   *      cannot hold up this one.
   */
  it("starts a fresh command chain for each match screen", async (): Promise<void> => {
    mockedInvoke.mockResolvedValueOnce(makeSnapshot());

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(resetMatchCommands).toHaveBeenCalledTimes(1);
    });
  });

  it("renders the current live snapshot when get_match_snapshot succeeds", async (): Promise<void> => {
    mockedInvoke.mockResolvedValueOnce(makeSnapshot());

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenCalledWith("get_match_snapshot");
    });

    await waitFor((): void => {
      expect(screen.getByTestId("prematch")).toHaveTextContent("Home FC");
    });
  });

  /**
   * Given a prepared live fixture
   * When the manager chooses the M12 quick simulation
   * Then the spatial result returned by Tauri replaces the career state and
   *      opens the post-match flow without calling the OFM live finaliser.
   */
  it("finalizes a prepared fixture through the M12 command", async (): Promise<void> => {
    const finishedGame = makeGameState();
    mockedInvoke.mockResolvedValueOnce(makeSnapshot()).mockResolvedValueOnce({
      game: finishedGame,
      home_goals: 2,
      away_goals: 1,
      events: [],
    });

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(screen.getByRole("button", { name: "match.simulateWithM12" })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "match.simulateWithM12" }));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenLastCalledWith("simulate_spatial_match");
      expect(screen.getByTestId("postmatch-finish")).toBeInTheDocument();
    });
    expect(setGameStateMock).toHaveBeenCalledWith(finishedGame);
    expect(screen.getByTestId("spatial-postmatch-result")).toHaveTextContent("2-1:0");
  });

  /**
   * Given a prepared fixture and a slow M12 request
   * When the manager clicks simulation twice
   * Then only one production command is sent and the loading affordance blocks both buttons.
   */
  it("prevents duplicate M12 quick simulations while the result is loading", async (): Promise<void> => {
    const response = deferred<{
      game: Record<string, unknown>;
      home_goals: number;
      away_goals: number;
      events: [];
    }>();
    mockedInvoke
      .mockResolvedValueOnce(makeSnapshot())
      .mockImplementationOnce(() => response.promise);

    render(<MatchSimulation />);

    const quickSimulation = await screen.findByRole("button", {
      name: "match.simulateWithM12",
    });
    fireEvent.click(quickSimulation);
    fireEvent.click(quickSimulation);

    expect(mockedInvoke).toHaveBeenCalledTimes(2);
    expect(quickSimulation).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("dashboard.loading");

    response.resolve({ game: makeGameState(), home_goals: 3, away_goals: 2, events: [] });
    await waitFor(() => {
      expect(screen.getByTestId("spatial-postmatch-result")).toHaveTextContent("3-2:0");
    });
  });

  /**
   * Given M12 rejects before any finalisation
   * When the quick simulation is requested
   * Then the pre-match screen remains usable and exposes the translated failure.
   */
  it("keeps the fixture playable when M12 quick simulation fails", async (): Promise<void> => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      mockedInvoke
        .mockResolvedValueOnce(makeSnapshot())
        .mockRejectedValueOnce(new Error("engine failed"));
      render(<MatchSimulation />);

      const quickSimulation = await screen.findByRole("button", {
        name: "match.simulateWithM12",
      });
      fireEvent.click(quickSimulation);

      await waitFor(() => {
        expect(screen.getByRole("alert")).toHaveTextContent("match.m12SimulationFailed");
      });
      expect(quickSimulation).toBeEnabled();
      expect(screen.getByTestId("prematch")).toBeInTheDocument();
    } finally {
      errorSpy.mockRestore();
    }
  });

  /**
   * Given a prepared M12 replay
   * When the manager opens 2D then goes to the result twice
   * Then the exact prepared replay is finalised once and the M12 result reaches post-match.
   */
  it("opens M12 2D then finalizes the prepared replay exactly once", async (): Promise<void> => {
    const finalize = deferred<{
      game: Record<string, unknown>;
      home_goals: number;
      away_goals: number;
      events: [{ event_type: string }];
    }>();
    mockedInvoke
      .mockResolvedValueOnce(makeSnapshot())
      .mockResolvedValueOnce({ replay_json: '{"schema_version":1}' })
      .mockImplementationOnce(() => finalize.promise);
    render(<MatchSimulation />);

    fireEvent.click(await screen.findByRole("button", { name: "match.watchWithM12" }));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenLastCalledWith("prepare_spatial_replay");
      expect(screen.getByTestId("m12-viewer")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("m12-go-result"));
    fireEvent.click(screen.getByTestId("m12-go-result"));
    expect(mockedInvoke).toHaveBeenCalledTimes(3);

    finalize.resolve({
      game: makeGameState(),
      home_goals: 1,
      away_goals: 0,
      events: [{ event_type: "Goal" }],
    });
    await waitFor(() => {
      expect(screen.getByTestId("spatial-postmatch-result")).toHaveTextContent("1-0:1");
    });
  });

  it("restores the live match session when no snapshot exists but fixture index is provided", async (): Promise<void> => {
    const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      locationState = {
        fixtureIndex: 4,
        competitionId: "cup1",
        fixtureId: "cup-fix4",
        mode: "live",
        snapshot: makeSnapshot({
          allows_extra_time: true,
          home_team: {
            id: "home1",
            name: "Boot Snapshot FC",
            formation: "4-4-2",
            play_style: "Balanced",
            players: [makeEnginePlayer({ id: "boot-p1", name: "Boot Keeper" })],
          },
        }),
      };

      mockedInvoke.mockRejectedValueOnce(new Error("No active live match"));
      mockedInvoke.mockResolvedValueOnce(
        makeSnapshot({
          home_team: {
            id: "home1",
            name: "Restored FC",
            formation: "4-4-2",
            play_style: "Balanced",
            players: [makeEnginePlayer({ id: "restore-p1", name: "Restore Keeper" })],
          },
        }),
      );

      render(<MatchSimulation />);

      await waitFor((): void => {
        expect(mockedInvoke).toHaveBeenCalledWith("start_live_match", {
          allowsExtraTime: false,
          fixtureIndex: 4,
          competitionId: "cup1",
          fixtureId: "cup-fix4",
          mode: "live",
        });
      });

      expect(screen.getByTestId("prematch")).toHaveTextContent("Restored FC");
    } finally {
      consoleWarnSpy.mockRestore();
    }
  });

  it("moves spectators straight into the live match stage", async (): Promise<void> => {
    locationState = {
      mode: "spectator",
      snapshot: makeSnapshot(),
    };

    mockedInvoke.mockResolvedValueOnce(makeSnapshot());

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(screen.getByTestId("match-live-container")).toHaveTextContent("Home FC");
    });
  });

  it("navigates away from postmatch after the finalized game has been stored", async (): Promise<void> => {
    locationState = {
      mode: "spectator",
      snapshot: makeSnapshot(),
    };

    const finishedGame = makeGameState();
    mockedInvoke.mockResolvedValueOnce(makeSnapshot()).mockResolvedValueOnce({
      game: finishedGame,
      round_summary: {
        matchday: 1,
        is_complete: true,
        pending_fixture_count: 0,
        completed_results: [],
        standings_delta: [],
        notable_upset: null,
        top_scorer_delta: [],
      },
    });

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(screen.getByTestId("match-live-container")).toHaveTextContent("Home FC");
    });

    fireEvent.click(screen.getByTestId("match-live"));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenLastCalledWith("finish_live_match");
      expect(screen.getByTestId("postmatch-finish")).toBeInTheDocument();
    });

    expect(setGameStateMock).toHaveBeenCalledWith(finishedGame);

    fireEvent.click(screen.getByTestId("postmatch-finish"));

    await waitFor((): void => {
      expect(navigateMock).toHaveBeenCalledWith("/dashboard");
    });
  });

  it("finalizes the match on full time and passes the round summary into the digest screen", async (): Promise<void> => {
    locationState = {
      mode: "live",
      snapshot: makeSnapshot(),
    };

    const finishedGame = makeGameState();
    const roundSummary = {
      matchday: 1,
      is_complete: true,
      pending_fixture_count: 0,
      completed_results: [],
      standings_delta: [],
      notable_upset: null,
      top_scorer_delta: [],
    };
    mockedInvoke.mockResolvedValueOnce(makeSnapshot()).mockResolvedValueOnce({
      game: finishedGame,
      round_summary: roundSummary,
    });

    render(<MatchSimulation />);

    // Manager sees prematch; advance to live match
    await waitFor((): void => {
      expect(screen.getByTestId("prematch-start")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("prematch-start"));

    await waitFor((): void => {
      expect(screen.getByTestId("match-live-container")).toHaveTextContent("Home FC");
    });

    fireEvent.click(screen.getByTestId("match-live"));

    await waitFor((): void => {
      expect(mockedInvoke).toHaveBeenLastCalledWith("finish_live_match");
      expect(screen.getByTestId("postmatch-finish")).toBeInTheDocument();
    });

    expect(setGameStateMock).toHaveBeenCalledWith(finishedGame);

    // Manager clicks Continue → goes to digest
    fireEvent.click(screen.getByTestId("postmatch-continue"));

    await waitFor((): void => {
      expect(screen.getByTestId("digest-round-summary")).toBeInTheDocument();
    });

    expect(screen.getByTestId("digest-round-summary")).toHaveTextContent('"matchday":1');

    fireEvent.click(screen.getByTestId("digest-finish"));

    await waitFor((): void => {
      expect(navigateMock).toHaveBeenCalledWith("/dashboard");
    });
  });

  /**
   * Given a cup tie, which lives in the cup's competition and never in the league mirror, whose
   *       route carries its competition and fixture ids, while the league mirror holds a league
   *       match between other clubs at the route's index and the backend answers with a league
   *       round summary
   * When the match reaches the digest
   * Then the digest is given the cup tie as the match the user played, not that league match.
   */
  it("hands the digest the cup tie the route names, not the league match at its index", async (): Promise<void> => {
    const base = makeGameStateWithFriendly("League");
    const league = base.league;
    const otherLeagueMatch = {
      ...league.fixtures[0],
      id: "other-league-match",
      home_team_id: "x1",
      away_team_id: "y1",
    };
    const cupTie = { ...league.fixtures[0], id: "cup-tie", competition: "Cup" };
    locationState = {
      mode: "live",
      fixtureIndex: 0,
      competitionId: "cup1",
      fixtureId: "cup-tie",
      snapshot: makeSnapshot(),
    };
    gameStoreState = {
      gameState: {
        ...base,
        league: { ...league, fixtures: [otherLeagueMatch] },
        competitions: [
          { ...league, fixtures: [otherLeagueMatch] },
          { ...league, id: "cup1", name: "Cup", fixtures: [cupTie] },
        ],
      },
      setGameState: setGameStateMock,
    };
    mockedInvoke.mockResolvedValueOnce(makeSnapshot()).mockResolvedValueOnce({
      game: makeGameState(),
      round_summary: { matchday: 3, completed_results: [] },
    });

    render(<MatchSimulation />);

    await waitFor((): void => {
      expect(screen.getByTestId("prematch-start")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("prematch-start"));
    await waitFor((): void => {
      expect(screen.getByTestId("match-live")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("match-live"));
    await waitFor((): void => {
      expect(screen.getByTestId("postmatch-continue")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("postmatch-continue"));

    await waitFor((): void => {
      expect(screen.getByTestId("digest-fixture")).toHaveTextContent("cup-tie");
    });
  });

  it("preserves user-selected match speed from first half into second half", async (): Promise<void> => {
    const mockedInvoke = vi.mocked(invoke);
    mockedInvoke.mockResolvedValueOnce(makeSnapshot());

    render(<MatchSimulation />);

    // Advance to the first-half live match view.
    await waitFor((): void => {
      expect(screen.getByTestId("prematch-start")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("prematch-start"));

    await waitFor((): void => {
      expect(screen.getByTestId("match-live-container")).toBeInTheDocument();
    });

    // Initial preferred speed comes from settings ("normal").
    expect(screen.getByTestId("match-live-container")).toHaveAttribute(
      "data-preferred-speed",
      "normal",
    );

    // User selects fast speed during first half.
    fireEvent.click(screen.getByTestId("match-trigger-speed-fast"));

    // Trigger half-time transition.
    fireEvent.click(screen.getByTestId("match-trigger-halftime"));

    await waitFor((): void => {
      expect(screen.getByTestId("halftime")).toBeInTheDocument();
    });

    // Resume from half-time — advances to second_half stage.
    fireEvent.click(screen.getByTestId("halftime-resume"));

    // The remounted MatchLive for second half must carry the user's preferred speed.
    await waitFor((): void => {
      expect(screen.getByTestId("match-live-container")).toHaveAttribute(
        "data-preferred-speed",
        "fast",
      );
    });
  });
});
