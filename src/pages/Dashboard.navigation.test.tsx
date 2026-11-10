import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../i18n";
import type { GameStateData, MessageData } from "../store/gameStore";
import type { LeagueData } from "../store/types";
import { createPlayer, createSeasonContext, createTeam } from "../test-utils/factories";
import Dashboard from "./Dashboard";

/**
 * The route from "something needs me" to "the screen that fixes it", through the real
 * `Dashboard.handleNavigate`: the real alert model, alert list, workspace and tab router. Only the
 * tab bodies, the sidebar and the header are stand-ins, so what is checked is which tab the
 * dashboard opens, not what the tab draws.
 */

const invokeMock = vi.fn();
let gameState: GameStateData;
let financeSnapshot: Record<string, unknown> | null = null;
let squadFitness: Record<string, unknown> | null = null;

vi.mock("react-router-dom", () => ({ useNavigate: () => vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested: vi.fn(async () => () => {}), destroy: vi.fn() }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../lib/extraTranslations", () => ({ applyExtraTranslations: vi.fn() }));
vi.mock("../store/gameStore", () => {
  const state = () => ({
    hasActiveGame: true,
    managerName: "Jane Doe",
    gameState,
    sessionState: null,
    setGameState: vi.fn(),
    setMessages: vi.fn(),
    clearGame: vi.fn(),
    isDirty: false,
    markClean: vi.fn(),
  });
  return { useGameStore: Object.assign(() => state(), { getState: state }) };
});
vi.mock("../store/settingsStore", () => {
  const getState = () => ({
    settings: { language: "en", default_match_mode: "live" },
    currency: { code: "EUR", symbol: "€", exchange_rate: 1 },
    supportedCurrencies: { EUR: { code: "EUR", symbol: "€", exchange_rate: 1 } },
    loaded: true,
  });
  return {
    useSettingsStore: Object.assign(() => ({ ...getState(), loadSettings: vi.fn() }), { getState }),
  };
});
vi.mock("../hooks/useAdvanceTime", () => ({
  useAdvanceTime: () => ({
    isAdvancing: false,
    showContinueMenu: false,
    setShowContinueMenu: vi.fn(),
    showMatchConfirm: false,
    setShowMatchConfirm: vi.fn(),
    matchMode: "live",
    setMatchMode: vi.fn(),
    blockerModal: null,
    setBlockerModal: vi.fn(),
    handleContinue: vi.fn(),
    handleConfirmMatch: vi.fn(),
    handleSkipToMatchDay: vi.fn(),
  }),
}));
vi.mock("../components/dashboard/DashboardSidebar", () => ({ default: () => null }));
vi.mock("../components/dashboard/DashboardHeader", () => ({ default: () => null }));
vi.mock("../components/home/HomeTab", () => ({ default: () => <p>Opened Home</p> }));
vi.mock("../components/squad/SquadTab", () => ({ default: () => <p>Opened Squad</p> }));
vi.mock("../components/tactics/TacticsTab", () => ({ default: () => <p>Opened Tactics</p> }));
vi.mock("../components/training/TrainingTab", () => ({ default: () => <p>Opened Training</p> }));
vi.mock("../components/schedule/ScheduleTab", () => ({ default: () => <p>Opened Schedule</p> }));
vi.mock("../components/finances/FinancesTab", () => ({ default: () => <p>Opened Finances</p> }));
vi.mock("../components/transfers/TransfersTab", () => ({
  default: () => <p>Opened Transfers</p>,
}));
vi.mock("../components/staff/StaffTab", () => ({ default: () => <p>Opened Staff</p> }));
vi.mock("../components/teamProfile", () => ({ default: () => <p>Opened Team profile</p> }));

const TODAY = "2026-07-10";

function elevenPlayers(count: number) {
  return Array.from({ length: count }, (_, index) =>
    createPlayer({
      id: `p${index + 1}`,
      full_name: `Player ${index + 1}`,
      match_name: `P${index + 1}`,
    }),
  );
}

function baseGame(): GameStateData {
  const players = elevenPlayers(11);
  return {
    season_context: createSeasonContext(),
    clock: { current_date: `${TODAY}T12:00:00Z`, start_date: "2026-07-01T12:00:00Z" },
    manager: {
      display_name: "Jane Doe",
      id: "manager-1",
      first_name: "Jane",
      last_name: "Doe",
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
    teams: [
      createTeam({ starting_xi_ids: players.map((player) => player.id) }),
      createTeam({ id: "team-2", name: "Beta FC", short_name: "BET", manager_id: "manager-2" }),
    ],
    players,
    staff: [],
    messages: [],
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
  };
}

function matchToday(): LeagueData {
  return {
    id: "league-1",
    name: "Test League",
    season: 2026,
    standings: [],
    fixtures: [
      {
        id: "fixture-1",
        matchday: 1,
        date: TODAY,
        home_team_id: "team-1",
        away_team_id: "team-2",
        competition: "League",
        competitive: true,
        advancing_team_id: null,
        status: "Scheduled",
        result: null,
      },
    ],
  } as unknown as LeagueData;
}

function message(overrides: Partial<MessageData>): MessageData {
  return {
    id: "msg-1",
    subject: "Something for you",
    body: "Have a look.",
    sender: "Assistant",
    sender_role: "Assistant Manager",
    date: "2026-07-09",
    read: true,
    category: "General",
    priority: "Normal",
    actions: [],
    context: {},
    ...overrides,
  } as MessageData;
}

beforeEach(async () => {
  await i18nReady;
  await i18n.changeLanguage("en");
  gameState = baseGame();
  financeSnapshot = null;
  squadFitness = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case "get_active_game":
        return gameState;
      case "get_finance_snapshot":
        return financeSnapshot === null ? null : { snapshot: financeSnapshot };
      case "get_squad_fitness":
        return squadFitness;
      case "get_messages_page":
        return gameState.messages;
      case "resolve_message_action":
        return { game: gameState };
      default:
        return null;
    }
  });
});

async function openAlert(name: RegExp) {
  render(<Dashboard />);
  await screen.findByText("Opened Home");
  fireEvent.click(await screen.findByRole("button", { name }));
}

describe("a dashboard alert opens the tab that fixes it", () => {
  /** Given staff proposals waiting, when their alert is clicked, then the Staff tab opens. */
  it("sends pending delegation proposals to Staff", async () => {
    gameState.delegation = {
      protected_players: [],
      proposals: [{ id: "prop-1" }],
    } as unknown as GameStateData["delegation"];

    await openAlert(/staff proposal\(s\) waiting/i);

    expect(await screen.findByText("Opened Staff")).toBeInTheDocument();
  });

  /** Given an injured player in the starting XI, when the alert is clicked, then Squad opens. */
  it("sends an injured starter to Squad", async () => {
    gameState.players[2].injury = { name: "Hamstring", days_remaining: 12 };

    await openAlert(/injured player\(s\) in Starting XI/i);

    expect(await screen.findByText("Opened Squad")).toBeInTheDocument();
  });

  /** Given a match today and a short eleven, when the match-day alert is clicked, then Squad opens. */
  it("sends a match-day lineup warning to Squad", async () => {
    gameState.players = elevenPlayers(10);
    gameState.teams[0].starting_xi_ids = gameState.players.map((player) => player.id);
    gameState.competitions = [matchToday()];

    await openAlert(/match today! set your starting xi/i);

    expect(await screen.findByText("Opened Squad")).toBeInTheDocument();
  });

  /** Given an urgent unread message, when the alert is clicked, then the Inbox opens. */
  it("sends an urgent unread message to the Inbox", async () => {
    gameState.messages = [message({ read: false, priority: "Urgent" })];

    await openAlert(/urgent message\(s\) unread/i);

    expect(await screen.findByRole("heading", { name: /inbox/i })).toBeInTheDocument();
  });

  /** Given exhausted players, when the alert is clicked, then Training opens. */
  it("sends exhausted players to Training", async () => {
    squadFitness = {
      players: 11,
      average_condition: 30,
      exhausted_count: 4,
      critical_count: 4,
      statuses: {},
    };

    await openAlert(/in critical condition/i);

    expect(await screen.findByText("Opened Training")).toBeInTheDocument();
  });

  /** Given the backend calls the cash runway critical, when the alert is clicked, then Finances opens. */
  it("sends a finance crisis to Finances", async () => {
    financeSnapshot = {
      annual_wage_bill: 0,
      weekly_wage_spend: 0,
      weekly_wage_budget: 50000,
      weekly_recurring_income: 0,
      weekly_sponsor_income: 0,
      projected_weekly_net: 0,
      cash_runway_weeks: 2,
      wage_budget_usage_percent: 10,
      currently_in_debt: false,
      currently_over_budget: false,
      wage_budget_status: "stable",
      runway_status: "critical",
      overall_status: "critical",
      marketing_campaign_cooldown_days_remaining: 0,
    };

    await openAlert(/finances critical/i);

    expect(await screen.findByText("Opened Finances")).toBeInTheDocument();
  });
});

/** The routes the Rust side puts in its messages' NavigateTo actions (`grep '?tab=' src-tauri`). */
const BACKEND_ROUTES: [route: string, opened: string, source: string][] = [
  ["/dashboard?tab=Transfers", "Opened Transfers", "an incoming transfer offer"],
  ["/dashboard?tab=Finances", "Opened Finances", "a finance warning"],
  ["/dashboard?tab=Training", "Opened Training", "a fitness warning"],
  ["/dashboard?tab=Tactics", "Opened Tactics", "a post-match tactics note"],
  ["/dashboard?tab=Schedule", "Opened Schedule", "a fixture change"],
  ["/dashboard?tab=Staff", "Opened Staff", "the assistant's advice"],
  ["/dashboard?tab=Squad", "Opened Squad", "the contract review"],
  ["/team/team-2", "Opened Team profile", "a club link"],
];

describe("an inbox message action opens the screen it names", () => {
  for (const [route, opened, source] of BACKEND_ROUTES) {
    /**
     * Given {source}'s message with a NavigateTo action to {route},
     * when the manager opens it in the Inbox and presses the action,
     * then the dashboard shows the screen the route names.
     */
    it(`takes ${source} (${route}) to the screen`, async () => {
      gameState.messages = [
        message({
          actions: [
            {
              id: "go",
              label: "Go there",
              action_type: { NavigateTo: { route } },
              resolved: false,
            },
          ],
        }),
      ];
      render(<Dashboard initialTab="Inbox" />);

      fireEvent.click(await screen.findByText("Something for you"));
      fireEvent.click(await screen.findByRole("button", { name: "Go there" }));

      expect(await screen.findByText(opened)).toBeInTheDocument();
    });
  }
});
