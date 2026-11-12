import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { GameStateData } from "../../store/gameStore";
import type {
  ExternalNegotiationOutcome,
  RegistryPage,
  RegistryPlayerDetail,
  RegistrySummary,
} from "../../services/registryService";
import {
  finalizeExternalTransfer,
  getExternalPlayer,
  negotiateExternalTransfer,
  searchExternalPlayers,
} from "../../services/registryService";
import GlobalMarketTab from "./GlobalMarketTab";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key} ${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

vi.mock("../../services/registryService", () => ({
  searchExternalPlayers: vi.fn(),
  getExternalPlayer: vi.fn(),
  negotiateExternalTransfer: vi.fn(),
  finalizeExternalTransfer: vi.fn(),
}));

const mockedSearch = vi.mocked(searchExternalPlayers);
const mockedDetail = vi.mocked(getExternalPlayer);
const mockedNegotiate = vi.mocked(negotiateExternalTransfer);
const mockedFinalize = vi.mocked(finalizeExternalTransfer);

const summary: RegistrySummary = {
  id: "fm-p-42",
  name: "Alex Striker",
  age: 24,
  nationality: "FR",
  position: "Striker",
  positionGroup: "forward",
  ovr: 78,
  potential: 86,
  club: "External United",
  ownership: "contracted",
  contractEnd: "2028-06-30",
  marketValue: 12_000_000,
  weeklyWage: 22_000,
  askingPrice: 20_000_000,
  status: "eligible",
};

const detail: RegistryPlayerDetail = {
  id: summary.id,
  name: summary.name,
  firstName: "Alex",
  lastName: "Striker",
  commonName: null,
  age: 24,
  birthDate: "2002-03-01",
  nation: "FR",
  nationality: "FR",
  position: "Striker",
  positionGroup: "forward",
  naturalPosition: "Striker",
  alternatePositions: [],
  footedness: "Right",
  weakFoot: 3,
  attributes: { pace: 80, shooting: 82, passing: 70 },
  ovr: 78,
  potential: 86,
  clubId: 900,
  clubName: "External United",
  clubNation: "FR",
  clubReputation: 640,
  ownership: "contracted",
  contractEnd: "2028-06-30",
  marketValue: 12_000_000,
  weeklyWage: 22_000,
  valueSource: "stored",
  wageSource: "stored",
  askingPrice: 20_000_000,
  status: "eligible",
  exclusionReason: null,
  fmUid: 1,
  fmUidStatus: "matched",
  sourceRowHash: "hash",
};

const agreed: ExternalNegotiationOutcome = {
  playerId: detail.id,
  freeAgent: false,
  status: "agreed",
  fee: 20_000_000,
  minimumFee: 18_000_000,
  clubDecision: "accepted",
  weeklyWage: 45_000,
  contractYears: 4,
  signingBonus: 500_000,
  contractEnd: "2030-06-30",
  playerVerdict: "accepts",
  playerScore: 70,
  mainObstacle: null,
  canAffordFee: true,
  wagePolicyAllows: true,
  canAffordBonus: true,
  windowOpen: true,
  blocker: null,
  fromClub: "External United",
};

const countered: ExternalNegotiationOutcome = {
  ...agreed,
  status: "club_countered",
  fee: 24_000_000,
  clubDecision: "countered",
};

const refused: ExternalNegotiationOutcome = {
  ...agreed,
  status: "player_refused",
  playerVerdict: "refuses",
  mainObstacle: "ClubBelowHisLevel",
};

const page = (items: RegistrySummary[], total = items.length): RegistryPage => ({
  items,
  total,
  page: 1,
  pageSize: 25,
  counts: { all: total, freeAgent: 0, contracted: total, incomplete: 0 },
});

const signedGame = {
  teams: [{ id: "team-1", name: "User FC" }],
  manager: { team_id: "team-1" },
} as unknown as GameStateData;

describe("GlobalMarketTab", () => {
  beforeEach(() => {
    mockedSearch.mockReset();
    mockedDetail.mockReset();
    mockedNegotiate.mockReset();
    mockedFinalize.mockReset();
    mockedSearch.mockResolvedValue(page([summary], 60));
  });

  it("asks the backend for the sort, direction, filter and page the controls describe", async () => {
    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);

    await screen.findByText("Alex Striker");
    expect(mockedSearch).toHaveBeenLastCalledWith(
      expect.objectContaining({ sort: "ovr", descending: true, page: 1, pageSize: 25 }),
    );

    fireEvent.change(screen.getByLabelText("globalMarket.sortBy"), {
      target: { value: "value" },
    });
    await waitFor(() =>
      expect(mockedSearch).toHaveBeenLastCalledWith(
        expect.objectContaining({ sort: "value", descending: true, page: 1 }),
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: /globalMarket\.descending/ }));
    await waitFor(() =>
      expect(mockedSearch).toHaveBeenLastCalledWith(
        expect.objectContaining({ sort: "value", descending: false }),
      ),
    );

    fireEvent.change(screen.getByLabelText("globalMarket.availabilityLabel"), {
      target: { value: "free_agent" },
    });
    await waitFor(() =>
      expect(mockedSearch).toHaveBeenLastCalledWith(
        expect.objectContaining({ availability: "free_agent", page: 1 }),
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.nextPage" }));
    await waitFor(() =>
      expect(mockedSearch).toHaveBeenLastCalledWith(
        expect.objectContaining({ availability: "free_agent", page: 2 }),
      ),
    );
  });

  it("shows an agreed profile and only moves the career once the manager confirms", async () => {
    const onGameUpdate = vi.fn();
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(agreed);
    mockedFinalize.mockResolvedValue({ game: signedGame, outcome: agreed });

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={onGameUpdate} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));

    await screen.findByText("globalMarket.negotiation");
    expect(screen.getByText("globalMarket.potential")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.agreed");
    expect(mockedNegotiate).toHaveBeenCalledWith("fm-p-42", {
      fee: null,
      weeklyWage: null,
      contractYears: null,
      signingBonus: null,
    });

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.sign" }));
    expect(screen.getByText("globalMarket.confirmTitle")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.confirm" }));

    await waitFor(() => expect(mockedFinalize).toHaveBeenCalledWith("fm-p-42", expect.any(Object)));
    expect(onGameUpdate).toHaveBeenCalledWith(signedGame);
    await screen.findByText(/globalMarket\.signed/);
  });

  it("does not sign a counter: accepting their terms submits the counter first", async () => {
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValueOnce(countered).mockResolvedValueOnce(agreed);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.club_countered");
    expect(screen.getByRole("button", { name: "globalMarket.sign" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.useTheirTerms" }));
    await screen.findByText("globalMarket.status.agreed");

    expect(mockedNegotiate).toHaveBeenLastCalledWith(
      "fm-p-42",
      expect.objectContaining({ fee: 24_000_000, weeklyWage: 45_000, contractYears: 4 }),
    );
    expect(screen.getByRole("button", { name: "globalMarket.sign" })).toBeEnabled();
  });

  it("shows a refusal with its reason and keeps the signing button closed", async () => {
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(refused);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.player_refused");
    expect(screen.getByText(/freeAgents\.reason\.ClubBelowHisLevel/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "globalMarket.sign" })).toBeDisabled();
  });
});
