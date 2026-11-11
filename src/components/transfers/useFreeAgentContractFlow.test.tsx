import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { JSX } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FreeAgentTalkData, GameStateData, PlayerData, TeamData } from "../../store/gameStore";
import {
  acceptFreeAgentCounter,
  offerFreeAgentContract,
  previewFreeAgentContractImpact,
  withdrawFreeAgentOffer,
} from "../../services/freeAgentService";
import { resolveBackendError } from "../../utils/backendI18n";
import { useFreeAgentContractFlow } from "./useFreeAgentContractFlow";
import { createSeasonContext } from "../../test-utils/factories";

vi.mock("../../services/freeAgentService", () => ({
  acceptFreeAgentCounter: vi.fn(),
  offerFreeAgentContract: vi.fn(),
  previewFreeAgentContractImpact: vi.fn(),
  withdrawFreeAgentOffer: vi.fn(),
}));

vi.mock("../../utils/backendI18n", () => ({
  resolveBackendError: vi.fn((error: unknown) =>
    error instanceof Error ? error.message : String(error),
  ),
}));

const mockedOfferFreeAgentContract = vi.mocked(offerFreeAgentContract);
const mockedPreviewFreeAgentContractImpact = vi.mocked(previewFreeAgentContractImpact);
const mockedAcceptFreeAgentCounter = vi.mocked(acceptFreeAgentCounter);
const mockedWithdrawFreeAgentOffer = vi.mocked(withdrawFreeAgentOffer);
const mockedResolveBackendError = vi.mocked(resolveBackendError);

function createTeam(overrides: Partial<TeamData> = {}): TeamData {
  return {
    id: "team-1",
    name: "User FC",
    short_name: "USR",
    country: "England",
    city: "London",
    stadium_name: "User Ground",
    stadium_capacity: 25000,
    finance: 5000000,
    manager_id: "manager-1",
    reputation: 50,
    wage_budget: 50000,
    transfer_budget: 2000000,
    season_income: 0,
    season_expenses: 0,
    formation: "4-4-2",
    play_style: "Balanced",
    training_focus: "Physical",
    training_intensity: "Medium",
    training_schedule: "Balanced",
    founded_year: 1900,
    colors: { primary: "#111111", secondary: "#ffffff" },
    facilities: { training: 1, medical: 1, scouting: 1 },
    starting_xi_ids: [],
    match_roles: {
      captain: null,
      vice_captain: null,
      penalty_taker: null,
      free_kick_taker: null,
      corner_taker: null,
    },
    form: [],
    history: [],
    ...overrides,
  };
}

function createPlayer(overrides: Partial<PlayerData> = {}): PlayerData {
  return {
    id: "free-agent-1",
    match_name: "F. Agent",
    full_name: "Free Agent",
    date_of_birth: "2000-01-01",
    nationality: "England",
    position: "Forward",
    natural_position: "Forward",
    alternate_positions: [],
    training_focus: null,
    attributes: {
      pace: 60,
      stamina: 60,
      strength: 60,
      agility: 60,
      passing: 60,
      shooting: 60,
      tackling: 60,
      dribbling: 60,
      defending: 60,
      positioning: 60,
      vision: 60,
      decisions: 60,
      composure: 60,
      aggression: 60,
      teamwork: 60,
      leadership: 60,
      handling: 30,
      reflexes: 30,
      aerial: 60,
    },
    condition: 90,
    morale: 70,
    injury: null,
    team_id: null,
    retired: false,
    contract_end: null,
    wage: 0,
    market_value: 600000,
    stats: {
      appearances: 0,
      goals: 0,
      assists: 0,
      clean_sheets: 0,
      yellow_cards: 0,
      red_cards: 0,
      avg_rating: 0,
      minutes_played: 0,
    },
    career: [],
    transfer_listed: false,
    loan_listed: false,
    transfer_offers: [],
    traits: [],
    ...overrides,
  };
}

function createGameState(players: PlayerData[] = [createPlayer()]): GameStateData {
  return {
    season_context: createSeasonContext(),
    clock: {
      current_date: "2026-08-01T12:00:00Z",
      start_date: "2026-07-01T12:00:00Z",
    },
    manager: {
      display_name: "Jane Doe",
      id: "manager-1",
      first_name: "Jane",
      last_name: "Doe",
      date_of_birth: "1980-01-01",
      nationality: "England",
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
    teams: [createTeam()],
    players,
    staff: [],
    messages: [],
    news: [],
    league: {
      id: "league-1",
      name: "Premier Division",
      season: 1,
      fixtures: [],
      standings: [],
    },
    scouting_assignments: [],
    board_objectives: [],
  };
}

function HookHarness({
  gameState,
  target,
}: {
  gameState: GameStateData;
  target: PlayerData;
}): JSX.Element {
  const {
    contractWage,
    setContractWage,
    contractLength,
    setContractLength,
    steppingStoneAvailable,
    signingBonus,
    setSigningBonus,
    signingBonusError,
    contractSubmitDisabled,
    promiseSteppingStone,
    setPromiseSteppingStone,
    openFreeAgentContract,
    submitFreeAgentContract,
  } = useFreeAgentContractFlow({ gameState });

  return (
    <div>
      <button type="button" onClick={() => openFreeAgentContract(target)}>
        Open
      </button>
      <label htmlFor="wage">Wage</label>
      <input
        id="wage"
        value={contractWage}
        onChange={(event) => setContractWage(event.target.value)}
      />
      <label htmlFor="years">Years</label>
      <input
        id="years"
        value={contractLength}
        onChange={(event) => setContractLength(event.target.value)}
      />
      <label htmlFor="bonus">Bonus</label>
      <input id="bonus" value={signingBonus} onChange={(e) => setSigningBonus(e.target.value)} />
      <output aria-label="bonus-error">{signingBonusError ?? "none"}</output>
      <output aria-label="submit-disabled">{String(contractSubmitDisabled)}</output>
      {steppingStoneAvailable ? (
        <label>
          Promise
          <input
            type="checkbox"
            checked={promiseSteppingStone}
            onChange={(event) => setPromiseSteppingStone(event.target.checked)}
          />
        </label>
      ) : null}
      <button type="button" onClick={() => void submitFreeAgentContract()}>
        Submit
      </button>
    </div>
  );
}

describe("useFreeAgentContractFlow", () => {
  beforeEach(() => {
    mockedOfferFreeAgentContract.mockReset();
    mockedPreviewFreeAgentContractImpact.mockReset();
    mockedResolveBackendError.mockClear();
    mockedAcceptFreeAgentCounter.mockReset();
    mockedWithdrawFreeAgentOffer.mockReset();
    mockedPreviewFreeAgentContractImpact.mockResolvedValue({
      stepping_stone_available: false,
      interest: {
        verdict: "keen",
        main_obstacle: null,
        expected_wage: 4000,
        expected_years: 3,
        place_in_side: "starter",
        days_unattached: 0,
        min_response_days: 1,
        max_response_days: 3,
        rival_clubs: [],
        open_talk: null,
      },
      projection: {
        current_annual_wage_bill: 0,
        projected_annual_wage_bill: 4000,
        annual_wage_budget: 50000,
        annual_soft_cap: 55000,
        current_weekly_wage_spend: 0,
        projected_weekly_wage_spend: 4000,
        current_cash_runway_weeks: 40,
        projected_cash_runway_weeks: 30,
        currently_over_budget: false,
        policy_allows: true,
      },
    });
  });

  it("does not submit when the computed wage is invalid", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);

    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(mockedPreviewFreeAgentContractImpact).toHaveBeenCalledWith(
        target.id,
        3000,
        0,
        3,
        false,
      );
    });

    fireEvent.change(screen.getByLabelText("Wage"), {
      target: { value: "abc" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).not.toHaveBeenCalled();
    });
  });

  it("submits the contract offer with current values", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);
    mockedOfferFreeAgentContract.mockResolvedValue({
      outcome: "pending",
      game: gameState,
      talk: null,
      respond_on: "2026-08-03",
      reason: null,
      suggested_wage: null,
      suggested_years: null,
    });

    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.change(screen.getByLabelText("Wage"), {
      target: { value: "4000" },
    });
    fireEvent.change(screen.getByLabelText("Years"), {
      target: { value: "3" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).toHaveBeenCalledWith(target.id, 4000, 3, false, 0);
    });
  });

  /**
   * Given a player who would not value a stepping-stone promise
   * When the contract form opens
   * Then the promise option is not offered.
   */
  it("hides the promise option when the backend says the player has no use for it", async () => {
    const target = createPlayer();
    render(<HookHarness gameState={createGameState([target])} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(mockedPreviewFreeAgentContractImpact).toHaveBeenCalled();
    });
    expect(screen.queryByLabelText("Promise")).not.toBeInTheDocument();
  });

  /**
   * Given a player who would value a stepping-stone promise, and the manager ticks it
   * When the offer is submitted
   * Then the promise travels with the offer, and only because it was ticked.
   */
  it("sends the promise with the offer when the manager gives it", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);
    mockedPreviewFreeAgentContractImpact.mockResolvedValue({
      stepping_stone_available: true,
      interest: {
        verdict: "keen",
        main_obstacle: null,
        expected_wage: 4000,
        expected_years: 3,
        place_in_side: "starter",
        days_unattached: 0,
        min_response_days: 1,
        max_response_days: 3,
        rival_clubs: [],
        open_talk: null,
      },
      projection: {
        current_annual_wage_bill: 0,
        projected_annual_wage_bill: 4000,
        annual_wage_budget: 50000,
        annual_soft_cap: 55000,
        current_weekly_wage_spend: 0,
        projected_weekly_wage_spend: 4000,
        current_cash_runway_weeks: 40,
        projected_cash_runway_weeks: 30,
        currently_over_budget: false,
        policy_allows: true,
      },
    });
    mockedOfferFreeAgentContract.mockResolvedValue({
      outcome: "pending",
      game: gameState,
      talk: null,
      respond_on: "2026-08-03",
      reason: null,
      suggested_wage: null,
      suggested_years: null,
    });
    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.click(await screen.findByLabelText("Promise"));
    fireEvent.change(screen.getByLabelText("Wage"), { target: { value: "4000" } });
    fireEvent.change(screen.getByLabelText("Years"), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).toHaveBeenCalledWith(target.id, 4000, 3, true, 0);
    });
  });

  /**
   * Given a signing bonus typed into the contract form
   * When the offer is previewed and submitted
   * Then the backend is asked about, and sent, that bonus.
   */
  it("previews and sends the signing bonus", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);
    mockedOfferFreeAgentContract.mockResolvedValue({
      outcome: "pending",
      game: gameState,
      talk: null,
      respond_on: "2026-08-03",
      reason: null,
      suggested_wage: null,
      suggested_years: null,
    });
    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.change(screen.getByLabelText("Wage"), { target: { value: "4000" } });
    fireEvent.change(screen.getByLabelText("Years"), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText("Bonus"), { target: { value: "100000" } });

    await waitFor(() => {
      expect(mockedPreviewFreeAgentContractImpact).toHaveBeenCalledWith(
        target.id,
        4000,
        100000,
        3,
        false,
      );
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));
    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).toHaveBeenCalledWith(target.id, 4000, 3, false, 100000);
    });
  });

  /**
   * Given the backend says the club cannot pay the bonus
   * When the preview comes back
   * Then the reason is exposed and submitting is disabled.
   */
  it("disables the offer when the backend says the bonus cannot be paid", async () => {
    const target = createPlayer();
    mockedPreviewFreeAgentContractImpact.mockResolvedValue({
      stepping_stone_available: false,
      signing_bonus_error: "be.error.transfers.insufficientFunds",
      interest: {
        verdict: "keen",
        main_obstacle: null,
        expected_wage: 4000,
        expected_years: 3,
        place_in_side: "starter",
        days_unattached: 0,
        min_response_days: 1,
        max_response_days: 3,
        rival_clubs: [],
        open_talk: null,
      },
      projection: {
        current_annual_wage_bill: 0,
        projected_annual_wage_bill: 4000,
        annual_wage_budget: 50000,
        annual_soft_cap: 55000,
        current_weekly_wage_spend: 0,
        projected_weekly_wage_spend: 4000,
        current_cash_runway_weeks: 40,
        projected_cash_runway_weeks: 30,
        currently_over_budget: false,
        policy_allows: true,
      },
    });
    render(<HookHarness gameState={createGameState([target])} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.change(screen.getByLabelText("Bonus"), { target: { value: "9000000" } });

    await waitFor(() => {
      expect(screen.getByLabelText("bonus-error")).toHaveTextContent(
        "be.error.transfers.insufficientFunds",
      );
    });
    expect(screen.getByLabelText("submit-disabled")).toHaveTextContent("true");
  });

  /**
   * Given a bonus that is not a whole number of euros
   * When the manager submits
   * Then nothing is sent.
   */
  it("does not send a bonus that is not a whole amount", async () => {
    const target = createPlayer();
    render(<HookHarness gameState={createGameState([target])} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.change(screen.getByLabelText("Bonus"), { target: { value: "12.5" } });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).not.toHaveBeenCalled();
    });
  });

  it("does not submit offers longer than five years", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);

    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.change(screen.getByLabelText("Years"), {
      target: { value: "6" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedOfferFreeAgentContract).not.toHaveBeenCalled();
    });
  });

  it("derives default contract years from the current in-game date", async () => {
    const target = createPlayer({
      id: "veteran-free-agent",
      date_of_birth: "1994-12-15",
    });
    const gameState = createGameState([target]);

    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(screen.getByLabelText("Years")).toHaveValue("2");
    });
  });

  it("resolves backend errors before storing them", async () => {
    const target = createPlayer();
    const gameState = createGameState([target]);
    mockedOfferFreeAgentContract.mockRejectedValue(
      new Error("be.error.contracts.boardWagePolicy?budget=50000"),
    );
    mockedResolveBackendError.mockReturnValue("Board wage policy");

    render(<HookHarness gameState={gameState} target={target} />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    await waitFor(() => {
      expect(mockedResolveBackendError).toHaveBeenCalledWith(
        expect.objectContaining({
          message: "be.error.contracts.boardWagePolicy?budget=50000",
        }),
      );
    });
  });

  describe("negotiation state", () => {
    function talk(overrides: Partial<FreeAgentTalkData> = {}): FreeAgentTalkData {
      return {
        id: "fat:1",
        club_id: "team-1",
        status: "awaiting_response",
        weekly_wage: 4000,
        contract_years: 3,
        signing_bonus: 0,
        promise_stepping_stone: false,
        round: 1,
        opened_on: "2026-08-01",
        updated_on: "2026-08-01",
        respond_on: "2026-08-03",
        counter_wage: null,
        counter_years: null,
        counter_expires_on: null,
        closed_on: null,
        reason: null,
        made_by_manager: true,
        ...overrides,
      };
    }

    function withTalks(talks: FreeAgentTalkData[]): {
      target: PlayerData;
      gameState: GameStateData;
    } {
      const target = createPlayer({
        morale_core: { manager_trust: 50, free_agent_talks: talks },
      });
      return { target, gameState: createGameState([target]) };
    }

    function NegotiationHarness({
      gameState,
      target,
      onGameUpdate,
    }: {
      gameState: GameStateData;
      target: PlayerData;
      onGameUpdate?: (game: GameStateData) => void;
    }): JSX.Element {
      const flow = useFreeAgentContractFlow({ gameState, onGameUpdate });
      return (
        <div>
          <button type="button" onClick={() => flow.openFreeAgentContract(target)}>
            Open
          </button>
          <button type="button" onClick={() => void flow.submitFreeAgentContract()}>
            Submit
          </button>
          <button type="button" onClick={flow.talkPanel.onAcceptCounter}>
            Accept counter
          </button>
          <button type="button" onClick={flow.talkPanel.onWithdraw}>
            Withdraw
          </button>
          <output aria-label="wage">{flow.contractWage}</output>
          <output aria-label="years">{flow.contractLength}</output>
          <output aria-label="submit-disabled">{String(flow.contractSubmitDisabled)}</output>
          <output aria-label="error">{flow.contractError ?? "none"}</output>
          <output aria-label="rivals">
            {flow.talkPanel.rivals.map((rival) => rival.club_name).join(",")}
          </output>
        </div>
      );
    }

    /**
     * Given an offer the player has not answered
     * When the form opens and the manager submits again
     * Then submit is disabled and nothing is sent (no duplicate offer from a second click).
     */
    it("blocks a second offer while the first awaits an answer", async () => {
      const { target, gameState } = withTalks([talk()]);
      render(<NegotiationHarness gameState={gameState} target={target} />);

      fireEvent.click(screen.getByRole("button", { name: "Open" }));
      await waitFor(() => expect(mockedPreviewFreeAgentContractImpact).toHaveBeenCalled());
      fireEvent.click(screen.getByRole("button", { name: "Submit" }));

      expect(screen.getByLabelText("submit-disabled")).toHaveTextContent("true");
      expect(mockedOfferFreeAgentContract).not.toHaveBeenCalled();
    });

    /**
     * Given a counter-offer on the table
     * When the form opens
     * Then it starts from his terms, and accepting them publishes the game the backend returns.
     */
    it("starts from the counter-offer and accepts it through the backend", async () => {
      const onGameUpdate = vi.fn();
      const { target, gameState } = withTalks([
        talk({
          status: "counter_offered",
          respond_on: null,
          counter_wage: 5200,
          counter_years: 4,
          counter_expires_on: "2026-08-06",
        }),
      ]);
      mockedAcceptFreeAgentCounter.mockResolvedValue({
        outcome: "accepted",
        game: gameState,
        talk: null,
        respond_on: null,
        reason: null,
        suggested_wage: null,
        suggested_years: null,
      });
      render(
        <NegotiationHarness gameState={gameState} target={target} onGameUpdate={onGameUpdate} />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Open" }));
      expect(screen.getByLabelText("wage")).toHaveTextContent("5200");
      expect(screen.getByLabelText("years")).toHaveTextContent("4");
      fireEvent.click(screen.getByRole("button", { name: "Accept counter" }));

      await waitFor(() => expect(onGameUpdate).toHaveBeenCalledWith(gameState));
      expect(mockedAcceptFreeAgentCounter).toHaveBeenCalledWith(target.id);
    });

    /**
     * Given an open offer
     * When the manager withdraws and the backend refuses
     * Then the resolved error is shown and the game is not replaced.
     */
    it("shows the backend's error when a withdrawal is refused", async () => {
      const onGameUpdate = vi.fn();
      const { target, gameState } = withTalks([talk()]);
      mockedWithdrawFreeAgentOffer.mockRejectedValue(new Error("be.error.freeAgents.noOpenTalk"));
      mockedResolveBackendError.mockReturnValue("No open talk");
      render(
        <NegotiationHarness gameState={gameState} target={target} onGameUpdate={onGameUpdate} />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Open" }));
      fireEvent.click(screen.getByRole("button", { name: "Withdraw" }));

      await waitFor(() => expect(screen.getByLabelText("error")).toHaveTextContent("No open talk"));
      expect(onGameUpdate).not.toHaveBeenCalled();
    });

    /**
     * Given another club's offer open for the same player
     * When the form is open
     * Then that club is named as a rival, and a closed talk of ours does not block a new offer.
     */
    it("names rival clubs and lets a new offer follow an ended talk", async () => {
      const { target, gameState } = withTalks([
        talk({ id: "fat:rival", club_id: "team-9", made_by_manager: false }),
        talk({
          status: "rejected",
          respond_on: null,
          closed_on: "2026-08-02",
          reason: "wage_too_low",
        }),
      ]);
      gameState.teams.push({ ...gameState.teams[0], id: "team-9", name: "Rival FC" });
      render(<NegotiationHarness gameState={gameState} target={target} />);

      fireEvent.click(screen.getByRole("button", { name: "Open" }));
      await waitFor(() => expect(mockedPreviewFreeAgentContractImpact).toHaveBeenCalled());

      expect(screen.getByLabelText("rivals")).toHaveTextContent("Rival FC");
      expect(screen.getByLabelText("submit-disabled")).toHaveTextContent("false");
    });
  });
});
