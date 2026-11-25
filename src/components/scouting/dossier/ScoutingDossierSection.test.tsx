import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ScoutingDossierView, ScoutingTargetView } from "../../../services/scoutingService";
import type { GameStateData } from "../../../store/gameStore";
import ScoutingDossierSection from "./ScoutingDossierSection";
import { useScoutingDossier } from "./useScoutingDossier";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

function target(overrides: Partial<ScoutingTargetView> = {}): ScoutingTargetView {
  return {
    player_id: "p1",
    full_name: "Ada Prospect",
    age: 19,
    natural_position: "Striker",
    alternate_positions: [],
    nationality: "GB",
    team_id: "club-2",
    team_name: "Rival FC",
    rating: {
      kind: "estimate",
      low: 58,
      high: 66,
      confidence: "moderate",
      current: true,
      scouted_on: "2026-08-01",
    },
    market_value: 2_000_000,
    weekly_wage: 5000,
    contract_end: "2027-06-30",
    is_free_agent: false,
    transfer_listed: false,
    loan_listed: false,
    injured: false,
    status: "current",
    shortlisted: false,
    being_scouted: false,
    report: {
      scouted_on: "2026-08-01",
      scout_name: "Sam Scout",
      reports: 1,
      ability: { low: 58, high: 66 },
      potential: { low: 70, high: 84 },
      confidence: "moderate",
      strengths: ["pace"],
      weaknesses: ["aerial"],
    },
    outlook: "high_growth",
    squad_impact: "cover",
    fee_affordable: true,
    wage_permitted: true,
    risks: ["contractExpiring"],
    ...overrides,
  };
}

const SCOUTED = target();
const UNSCOUTED = target({
  player_id: "p2",
  full_name: "Bo Unscouted",
  status: "unscouted",
  report: null,
  rating: { kind: "standing", standing: "average" },
  outlook: "unclear",
  // The club cannot judge a fit it cannot measure.
  squad_impact: null,
  shortlisted: true,
  risks: ["unscouted"],
});

function Harness({
  onGameUpdate = vi.fn(),
  canScout = true,
  onScoutAgain = vi.fn(),
  onBid = vi.fn(),
}: {
  onGameUpdate?: (game: GameStateData) => void;
  canScout?: boolean;
  onScoutAgain?: (id: string) => void;
  onBid?: (id: string) => void;
}) {
  const dossierState = useScoutingDossier(1);
  return (
    <ScoutingDossierSection
      dossierState={dossierState}
      onGameUpdate={onGameUpdate}
      canScout={canScout}
      onScoutAgain={onScoutAgain}
      onBid={onBid}
      onOfferContract={vi.fn()}
      onOpenProfile={vi.fn()}
    />
  );
}

function answerDossier(view: ScoutingDossierView) {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_scouting_dossier") return view;
    if (command === "unshortlist_player" || command === "shortlist_player")
      return { updated: true };
    return undefined;
  });
}

describe("ScoutingDossierSection", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /**
   * Given a dossier with one scouted and one unscouted shortlisted player
   * When the section loads
   * Then the scouted player shows a range, the unscouted one shows unknown and never a number it
   *      has not earned.
   */
  it("shows ranges for scouted players and unknown for unscouted ones", async () => {
    answerDossier({ reports: [SCOUTED], shortlist: [UNSCOUTED] });

    render(<Harness />);

    // The rating column and the ability column both read the scout's range.
    expect((await screen.findAllByText("58–66")).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("70–84")).toBeInTheDocument();
    const unscoutedRow = screen.getByText("Bo Unscouted").closest("tr");
    expect(
      within(unscoutedRow as HTMLElement).getAllByText("scouting.dossier.unknown").length,
    ).toBeGreaterThanOrEqual(2);
    expect(within(unscoutedRow as HTMLElement).queryByText(/\d+–\d+/)).toBeNull();
  });

  /**
   * Given a shortlisted player nobody has scouted
   * When the section loads
   * Then the rating cell is his public standing, with no rating number, and the squad fit reads
   *      unknown rather than a verdict computed from a hidden rating
   */
  it("shows an unscouted target as a standing and an unknown squad fit", async () => {
    answerDossier({ reports: [], shortlist: [UNSCOUTED] });

    render(<Harness />);

    const row = (await screen.findByText("Bo Unscouted")).closest("tr") as HTMLElement;
    expect(within(row).getByText("playerRating.standing.average")).toBeInTheDocument();
    expect(within(row).queryByText("playerRating.estimate")).not.toBeInTheDocument();
    // Ability, potential and squad fit are all unknown.
    expect(within(row).getAllByText("scouting.dossier.unknown")).toHaveLength(3);
    expect(within(row).queryByText(/scouting\.dossier\.impact\./)).toBeNull();
  });

  /**
   * Given a loading dossier and then an empty one
   * When the section renders
   * Then a status line shows while loading, and the empty states explain how to start.
   */
  it("announces loading and explains the empty states", async () => {
    answerDossier({ reports: [], shortlist: [] });

    render(<Harness />);

    expect(screen.getByRole("status")).toHaveTextContent("scouting.dossier.loading");
    expect(await screen.findByText("scouting.dossier.noShortlist")).toBeInTheDocument();
    expect(screen.getByText("scouting.dossier.noReports")).toBeInTheDocument();
  });

  /**
   * Given a dossier that cannot be loaded
   * When the section renders
   * Then the failure is announced as an alert instead of an empty page.
   */
  it("reports a dossier that could not be loaded", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("boom");
    });

    render(<Harness />);
    expect(await screen.findByRole("alert")).toHaveTextContent("errors.unexpected");
    expect(screen.getByRole("alert")).not.toHaveTextContent("boom");
  });

  /**
   * Given a shortlisted player
   * When the manager removes him from the shortlist
   * Then the command is sent, the game is forwarded and the dossier is asked again.
   */
  it("removes a player from the shortlist and refreshes", async () => {
    answerDossier({ reports: [], shortlist: [UNSCOUTED] });
    const onGameUpdate = vi.fn();

    render(<Harness onGameUpdate={onGameUpdate} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `scouting.dossier.removeShortlistFor:${JSON.stringify({ player: "Bo Unscouted" })}`,
      }),
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("unshortlist_player", { playerId: "p2" });
      expect(onGameUpdate).toHaveBeenCalledWith({ updated: true });
    });
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([c]) => c === "get_scouting_dossier").length).toBe(2),
    );
  });

  /**
   * Given two targets
   * When one is ticked, then the second
   * Then Compare stays disabled until two are chosen, and the comparison shows both side by side
   *      with the unscouted player's estimate as unknown.
   */
  it("compares two targets side by side", async () => {
    answerDossier({ reports: [SCOUTED], shortlist: [UNSCOUTED] });

    render(<Harness />);
    const compare = await screen.findByRole("button", { name: /scouting\.dossier\.compare:/ });
    expect(compare).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: `scouting.dossier.selectToCompare:${JSON.stringify({ player: "Ada Prospect" })}`,
      }),
    );
    expect(compare).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: `scouting.dossier.selectToCompare:${JSON.stringify({ player: "Bo Unscouted" })}`,
      }),
    );
    expect(compare).toBeEnabled();
    fireEvent.click(compare);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("columnheader", { name: "Ada Prospect" })).toBeInTheDocument();
    expect(within(dialog).getByRole("columnheader", { name: "Bo Unscouted" })).toBeInTheDocument();
    expect(within(dialog).getAllByText("58–66").length).toBeGreaterThanOrEqual(1);
    expect(within(dialog).getAllByText("scouting.dossier.unknown").length).toBeGreaterThan(0);
    // The unscouted player is a standing here, never a rating a column could rank him by.
    expect(within(dialog).getByText("playerRating.standing.average")).toBeInTheDocument();
  });

  /**
   * Given a scouted target
   * When his details are opened
   * Then the report, finance and risks show, the bid action is offered for a player with a club,
   *      and scouting him again is refused while no scout is free.
   */
  it("opens a report with its actions and disables scouting when nobody is free", async () => {
    answerDossier({ reports: [SCOUTED], shortlist: [] });

    render(<Harness canScout={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Ada Prospect" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("scouting.risk.contractExpiring")).toBeInTheDocument();
    expect(within(dialog).getByText("scouting.dossier.outlook.high_growth")).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "scouting.dossier.makeBid" }),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "scouting.noScoutsFree" })).toBeDisabled();
  });

  /**
   * Given an open report with a bid action
   * When the manager chooses to make a bid
   * Then the report closes first (so two modals never stack and Escape closes only the bid) and the
   *      bid flow is told which player.
   */
  it("closes the report before handing over to the bid flow", async () => {
    answerDossier({ reports: [SCOUTED], shortlist: [] });
    const onBid = vi.fn();

    render(<Harness onBid={onBid} />);
    fireEvent.click(await screen.findByRole("button", { name: "Ada Prospect" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "scouting.dossier.makeBid" }));

    expect(onBid).toHaveBeenCalledWith("p1");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  /**
   * Given a report whose player is taken off the shortlist from inside his open report
   * When the dossier refreshes without him
   * Then the report stays open until the manager closes it.
   */
  it("keeps an open report open when its player drops off the lists", async () => {
    const view: ScoutingDossierView = { reports: [], shortlist: [UNSCOUTED] };
    answerDossier(view);

    render(<Harness />);
    fireEvent.click(await screen.findByRole("button", { name: "Bo Unscouted" }));
    const dialog = await screen.findByRole("dialog");
    answerDossier({ reports: [], shortlist: [] });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "scouting.dossier.removeShortlist" }),
    );

    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([c]) => c === "get_scouting_dossier").length).toBe(2),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  /**
   * Given four targets already chosen for the comparison
   * When the list is read
   * Then the unchosen ones cannot be ticked, so the limit is stated by the controls and not by a
   *      click that does nothing.
   */
  it("disables further choices at the comparison limit", async () => {
    const many = ["a", "b", "c", "d", "e"].map((id) =>
      target({ player_id: id, full_name: `Player ${id}`, shortlisted: true }),
    );
    answerDossier({ reports: [], shortlist: many });

    render(<Harness />);
    for (const id of ["a", "b", "c", "d"]) {
      fireEvent.click(
        await screen.findByRole("checkbox", {
          name: `scouting.dossier.selectToCompare:${JSON.stringify({ player: `Player ${id}` })}`,
        }),
      );
    }

    expect(
      screen.getByRole("checkbox", {
        name: `scouting.dossier.selectToCompare:${JSON.stringify({ player: "Player e" })}`,
      }),
    ).toBeDisabled();
  });
});
