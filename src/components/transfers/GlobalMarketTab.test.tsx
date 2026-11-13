import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

const page = (items: RegistrySummary[], total = items.length, pageNumber = 1): RegistryPage => ({
  items,
  total,
  page: pageNumber,
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

    fireEvent.click(screen.getByRole("combobox", { name: "globalMarket.sortBy" }));
    fireEvent.click(screen.getByRole("option", { name: "globalMarket.sort.value" }));
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

    fireEvent.click(screen.getByRole("combobox", { name: "globalMarket.availabilityLabel" }));
    fireEvent.click(screen.getByRole("option", { name: "globalMarket.availability.free_agent" }));
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

  it("resolves a blocker that carries parameters, so a full squad reads as a sentence", async () => {
    const blocked: ExternalNegotiationOutcome = {
      ...agreed,
      status: "blocked",
      blocker: "be.error.freeAgents.squadFull?count=30&max=30&release=1",
    };
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(blocked);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));

    await screen.findByText(/be\.error\.freeAgents\.squadFull \{.*"count":"30"/);
    expect(screen.queryByText(/squadFull\?count=/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "globalMarket.sign" })).toBeDisabled();
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

  it("names the table, marks the selected row by more than its tint, and keeps the caption current", async () => {
    mockedSearch.mockResolvedValue(page([summary], 60));
    mockedDetail.mockResolvedValue(detail);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    const table = await screen.findByRole("table", { name: /globalMarket\.tableCaption/ });
    expect(table).toHaveAccessibleName(/globalMarket\.tableCaption/);

    const row = screen.getByRole("row", { name: /Alex Striker/ });
    expect(row).not.toHaveAttribute("aria-current");

    fireEvent.click(screen.getByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");

    expect(row).toHaveAttribute("aria-current", "true");
    expect(within(row).getByText(/globalMarket\.selected/)).toBeInTheDocument();
  });

  it("moves focus to the profile on selection and back to the row when the profile closes", async () => {
    mockedDetail.mockResolvedValue(detail);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    const opener = await screen.findByRole("button", { name: /globalMarket\.openProfile/ });
    opener.focus();
    fireEvent.click(opener);

    const profile = await screen.findByRole("region", { name: "globalMarket.profile" });
    await waitFor(() => expect(profile).toHaveFocus());

    fireEvent.click(screen.getByRole("button", { name: "common.close" }));
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("has the negotiation live region in the page before an answer is put into it", async () => {
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(agreed);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");

    const status = screen.getByRole("status", { name: "globalMarket.negotiation" });
    expect(status).toBeEmptyDOMElement();

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.agreed");
    expect(status).not.toBeEmptyDOMElement();
    expect(status).toHaveTextContent(/globalMarket\.checkFee/);
  });

  it("explains why the signing control is closed until the terms are agreed", async () => {
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValueOnce(countered).mockResolvedValueOnce(agreed);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.signHintCheck");

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.club_countered");
    expect(screen.getByText("globalMarket.signHintCounter")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.useTheirTerms" }));
    await screen.findByText("globalMarket.status.agreed");
    expect(screen.queryByText("globalMarket.signHintCounter")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "globalMarket.sign" })).toBeEnabled();
  });

  it("keeps keyboard focus on a usable page control when the clicked one is disabled at the end", async () => {
    mockedSearch.mockImplementation(async (request) => page([summary], 26, request.page));
    mockedDetail.mockResolvedValue(detail);

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    await screen.findByRole("button", { name: /globalMarket\.openProfile/ });

    fireEvent.click(screen.getByRole("button", { name: "globalMarket.nextPage" }));
    const previous = await screen.findByRole("button", { name: "globalMarket.previousPage" });
    await waitFor(() => expect(previous).toHaveFocus());

    fireEvent.click(previous);
    const next = screen.getByRole("button", { name: "globalMarket.nextPage" });
    await waitFor(() => expect(next).toHaveFocus());
  });

  it("moves focus to the confirmation after the backend confirms a signing", async () => {
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(agreed);
    mockedFinalize.mockResolvedValue({ game: signedGame, outcome: agreed });

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.agreed");
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.sign" }));
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.confirm" }));

    const notice = screen.getByRole("status", { name: "globalMarket.title" });
    await screen.findByText(/globalMarket\.signed/);
    await waitFor(() => expect(notice).toHaveFocus());
  });

  it("does not announce success while the backend is still answering", async () => {
    let release!: (value: { game: GameStateData; outcome: ExternalNegotiationOutcome }) => void;
    mockedDetail.mockResolvedValue(detail);
    mockedNegotiate.mockResolvedValue(agreed);
    mockedFinalize.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.checkTerms" }));
    await screen.findByText("globalMarket.status.agreed");
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.sign" }));
    fireEvent.click(screen.getByRole("button", { name: "globalMarket.confirm" }));

    expect(screen.queryByText(/globalMarket\.signed/)).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("dialog")).getByRole("button", { name: "globalMarket.signing" }),
    ).toBeDisabled();

    await act(async () => {
      release({ game: signedGame, outcome: agreed });
    });
    await screen.findByText(/globalMarket\.signed/);
  });

  it("replaces an unknown backend key with a readable market error", async () => {
    mockedSearch.mockRejectedValue(new Error("be.error.not.a.real.key"));

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    const alert = await screen.findByRole("alert", { name: "globalMarket.title" });
    await waitFor(() => expect(alert).toHaveTextContent("globalMarket.errorGeneric"));
    expect(alert).not.toHaveTextContent("be.error");
  });

  it("keeps a long player name in the table and the profile without dropping it", async () => {
    const longName =
      "Alexandros Maximilian Konstantinos van der Berg-Whittington-Striker the Third";
    mockedSearch.mockResolvedValue(page([{ ...summary, name: longName }], 1));
    mockedDetail.mockResolvedValue({ ...detail, name: longName });

    render(<GlobalMarketTab gameState={{} as GameStateData} onGameUpdate={vi.fn()} />);
    expect(await screen.findByText(longName)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /globalMarket\.openProfile/ }));
    await screen.findByText("globalMarket.negotiation");
    expect(screen.getAllByText(longName).length).toBeGreaterThan(1);
  });
});
