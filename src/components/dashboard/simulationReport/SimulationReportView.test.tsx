import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../../i18n";
import type { FastForwardResponse } from "../../../services/fastForwardService";
import { formatExactMoney } from "../../../lib/helpers";
import { useGameStore } from "../../../store/gameStore";
import type { ReportNavigation } from "./ReportLines";
import SimulationReportView from "./SimulationReportView";
import { busyClub, emptyClub, listed } from "./reportFixtures";

const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

function response(
  clubs: ReturnType<typeof emptyClub>[],
  extra: Partial<FastForwardResponse> = {},
): FastForwardResponse {
  return {
    progress: {
      start_date: "2026-08-01",
      current_date: "2026-08-11",
      target_date: "2026-08-11",
      days_done: 10,
      days_total: 10,
      user_matches_played: 1,
      seasons_rolled_over: 0,
    },
    stop: { kind: "target_reached" },
    season_summaries: [],
    passed_decisions: [],
    report: { clubs },
    game: {} as FastForwardResponse["game"],
    results: [],
    ...extra,
  };
}

function nav(): ReportNavigation {
  return {
    openPlayer: vi.fn(),
    openClub: vi.fn(),
    openInboxMessage: vi.fn(),
    openSchedule: vi.fn(),
  };
}

function show(r: FastForwardResponse, navigation?: ReportNavigation) {
  const onBack = vi.fn();
  render(<SimulationReportView response={r} onBack={onBack} backLabel="Back" nav={navigation} />);
  return { onBack };
}

const openTab = (name: string) => fireEvent.click(screen.getByRole("tab", { name }));
const tab = (key: string) => tr(`fastForward.report.tab.${key}`);

describe("SimulationReportView", () => {
  beforeEach(async () => {
    await i18nReady;
  });
  afterEach(() => {
    cleanup();
    useGameStore.setState({ gameState: null });
  });

  /**
   * Given a report with one of everything
   * When it opens
   * Then the summary states each section's own totals, the money formatted as the game formats it,
   * and the important events
   */
  it("summarises_each_section_with_the_reports_own_totals", () => {
    show(response([busyClub()]));

    const matches = screen.getByRole("region", { name: tab("matches") });
    expect(
      within(matches).getByText(tr("fastForward.report.stat.won")).nextSibling,
    ).toHaveTextContent("1");
    const transfers = screen.getByRole("region", { name: tab("transfers") });
    expect(within(transfers).getByText(formatExactMoney(2_000_000))).toBeInTheDocument();
    expect(within(transfers).getByText(formatExactMoney(500_000))).toBeInTheDocument();
    const finances = screen.getByRole("region", { name: tab("finances") });
    expect(within(finances).getByText(formatExactMoney(-400_000))).toBeInTheDocument();
    const events = screen.getByRole("region", { name: tr("fastForward.report.eventsTitle") });
    expect(within(events).getAllByRole("listitem")).toHaveLength(2);
  });

  /**
   * Given transfers, one whose fee the journal never paid
   * When the Transfers tab opens
   * Then each line names player, counterparty and fee, the contract shows the wage before and after,
   * and the unpaid fee is flagged rather than shown as fine
   */
  it("lists_transfers_with_fee_counterparty_and_the_contract_terms", () => {
    show(response([busyClub()]));
    openTab(tab("transfers"));

    const bought = screen.getByText("Ben Bought").closest("li") as HTMLElement;
    expect(bought).toHaveTextContent("Blues FC");
    expect(bought).toHaveTextContent(formatExactMoney(2_000_000));
    expect(bought).toHaveTextContent(tr("fastForward.report.feeNotInJournal"));
    const sold = screen.getByText("Sam Sold").closest("li") as HTMLElement;
    expect(sold).toHaveTextContent(formatExactMoney(500_000));
    expect(sold).not.toHaveTextContent(tr("fastForward.report.feeNotInJournal"));
    const renewal = screen.getByText("Ria Renewed").closest("li") as HTMLElement;
    expect(renewal).toHaveTextContent("→");
  });

  /**
   * Given a finances section that reconciles, and one with money the journal does not explain
   * When the Finances tab opens
   * Then the first says so and the second raises an alert with the amount
   */
  it("says_whether_the_cash_journal_explains_the_change", () => {
    show(response([busyClub()]));
    openTab(tab("finances"));
    expect(screen.getByText(tr("fastForward.report.reconciled"))).toBeInTheDocument();
    expect(screen.getByRole("table")).toHaveTextContent(
      tr("fastForward.report.cashKind.transfer_fee_out"),
    );
    cleanup();

    const broken = busyClub();
    broken.finances.unexplained = 777;
    broken.finances.detail_complete = false;
    show(response([broken]));
    openTab(tab("finances"));
    expect(screen.getByRole("alert")).toHaveTextContent(formatExactMoney(777));
    expect(screen.getByText(tr("fastForward.report.folded"))).toBeInTheDocument();
  });

  /**
   * Given a squad report with an arrival, a departure and an injury
   * When the Squad tab opens
   * Then each carries the ledger's cause (or none), and the injury says whether he recovered
   */
  it("lists_arrivals_departures_and_injuries_with_their_causes", () => {
    const club = busyClub();
    club.squad.arrivals.items.push({
      player_id: "p-mystery",
      player_name: "Max Mystery",
      cause: null,
      date: null,
      counterparty_name: null,
      fee: null,
    });
    club.squad.arrivals.total += 1;
    show(response([club]));
    openTab(tab("squad"));

    expect(screen.getByText("Max Mystery").closest("li")).toHaveTextContent(
      tr("fastForward.report.cause.none"),
    );
    expect(screen.getByText("Ben Bought").closest("li")).toHaveTextContent(
      tr("fastForward.report.cause.permanent_transfer"),
    );
    expect(screen.getByText("Ivy Hurt").closest("li")).toHaveTextContent(
      tr("fastForward.report.injury.stillInjured"),
    );
  });

  /**
   * Given delegation entries of two outcomes
   * When the Delegation tab opens
   * Then each reads in the game's own words for its key, with its outcome beside it
   */
  it("shows_what_the_assistant_did_and_what_became_of_it", () => {
    show(response([busyClub()]));
    openTab(tab("delegation"));

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent(tr("fastForward.report.delegationOutcome.executed"));
    expect(rows[1]).toHaveTextContent(tr("fastForward.report.delegationOutcome.declined"));
  });

  /**
   * Given a report with dated items of every kind
   * When the Timeline tab opens
   * Then they are listed oldest first and labelled by kind
   */
  it("lists_the_timeline_oldest_first", () => {
    show(response([busyClub()]));
    openTab(tab("timeline"));

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(10);
    expect(rows[0]).toHaveTextContent(tr("fastForward.report.source.event"));
    expect(rows[9]).toHaveTextContent(tr("fastForward.report.source.event"));
    expect(rows[2]).toHaveTextContent(tr("fastForward.report.source.transfer"));
  });

  /**
   * Given an interval with nothing in it
   * When any list tab opens
   * Then it says there is nothing to report instead of showing an empty box
   */
  it("says_so_when_a_section_has_nothing", () => {
    show(response([emptyClub()]));
    openTab(tab("matches"));
    expect(screen.getByText(tr("fastForward.report.nothing"))).toBeInTheDocument();
    openTab(tab("timeline"));
    expect(screen.getByText(tr("fastForward.report.nothing"))).toBeInTheDocument();
  });

  /**
   * Given a long list the backend capped
   * When the tab opens
   * Then the exact number left out is stated
   */
  it("states_how_many_items_the_backend_left_out", () => {
    const club = busyClub();
    club.matches.fixtures = listed(club.matches.fixtures.items, 42);
    show(response([club]));
    openTab(tab("matches"));

    expect(screen.getByText(tr("fastForward.report.more", { count: 41 }))).toBeInTheDocument();
  });

  /**
   * Given a run asked for 10 days that played 4, and a run that ended in an error
   * When the report opens
   * Then the partial coverage is stated, and the error is announced
   */
  it("states_partial_coverage_and_an_error_stop", () => {
    show(
      response([emptyClub({ days_played: 4 })], {
        stop: { kind: "error", error: "be.error.fastForward.careerChanged" },
      }),
    );

    expect(
      screen.getByText(tr("fastForward.report.partial", { played: 4, asked: 10 })),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  /**
   * Given a manager with no club, and a backend that sent no report
   * When the report opens
   * Then each says what it is, and nothing throws
   */
  it("handles_no_club_and_no_report", () => {
    show(response([]));
    expect(screen.getByText(tr("fastForward.report.noClub"))).toBeInTheDocument();
    cleanup();

    show(response([], { report: undefined }));
    expect(screen.getByText(tr("fastForward.report.unavailable"))).toBeInTheDocument();
  });

  /**
   * Given a manager dismissed from one club who then managed another
   * When the report opens
   * Then each club has its own tab and its own days, and the dismissal is stated for the first
   */
  it("separates_the_clubs_the_manager_held", () => {
    const first = busyClub();
    first.ended = "dismissed";
    first.days_played = 6;
    const second = emptyClub({ club_id: "blues", club_name: "Blues FC", days_played: 4 });
    show(response([first, second]));

    expect(screen.getByText(tr("fastForward.report.ended.dismissed"))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Blues FC" }));

    expect(screen.getByText(tr("fastForward.report.ended.continues"))).toBeInTheDocument();
    expect(
      screen.getByText(
        tr("fastForward.report.period", { from: "", until: "", days: 4 })
          .replace(/^.*·/, "")
          .trim(),
        { exact: false },
      ),
    ).toBeInTheDocument();
  });

  /**
   * Given a navigation handler
   * When a player, a club and a board message are clicked
   * Then each opens its own record by id; without a handler names are plain text
   */
  it("opens_records_by_id_when_it_can_and_is_plain_text_when_it_cannot", () => {
    const navigation = nav();
    show(response([busyClub()]), navigation);
    openTab(tab("transfers"));

    const bought = screen.getByText("Ben Bought").closest("li") as HTMLElement;
    fireEvent.click(
      within(bought).getByRole("button", {
        name: tr("fastForward.report.openRecord", { name: "Ben Bought" }),
      }),
    );
    fireEvent.click(
      within(bought).getByRole("button", {
        name: tr("fastForward.report.openRecord", { name: "Blues FC" }),
      }),
    );
    expect(navigation.openPlayer).toHaveBeenCalledWith("p-bought");
    expect(navigation.openClub).toHaveBeenCalledWith("blues");

    openTab(tab("timeline"));
    fireEvent.click(screen.getByRole("button", { name: tr("fastForward.report.openMessage") }));
    expect(navigation.openInboxMessage).toHaveBeenCalledWith("msg-1");
    cleanup();

    show(response([busyClub()]));
    openTab(tab("transfers"));
    expect(
      screen.queryByRole("button", {
        name: tr("fastForward.report.openRecord", { name: "Ben Bought" }),
      }),
    ).toBeNull();
  });

  /**
   * Given the game on screen
   * When the report is opened, browsed and left repeatedly
   * Then the game store is exactly as it was: reading a report mutates nothing
   */
  it("never_changes_the_game_when_it_is_browsed", () => {
    useGameStore.setState({
      gameState: { career_token: 1, state_revision: 3, competitions: [] } as unknown as ReturnType<
        typeof useGameStore.getState
      >["gameState"],
    });
    const before = useGameStore.getState();
    const snapshot = { gameState: before.gameState, isDirty: before.isDirty };
    for (let i = 0; i < 3; i++) {
      const { onBack } = show(response([busyClub()]));
      for (const name of [
        "matches",
        "transfers",
        "finances",
        "squad",
        "delegation",
        "timeline",
        "summary",
      ]) {
        openTab(tab(name));
      }
      fireEvent.click(screen.getByRole("button", { name: "Back" }));
      expect(onBack).toHaveBeenCalledTimes(1);
      cleanup();
    }

    expect(useGameStore.getState().gameState).toBe(snapshot.gameState);
    expect(useGameStore.getState().isDirty).toBe(snapshot.isDirty);
  });
  /**
   * Given every outcome the backend can log, including the camelCase one the manager is left with
   * When the Delegation tab opens
   * Then each outcome is a sentence in the player's language, never a raw translation key
   */
  it("words_every_delegation_outcome_never_as_a_raw_key", () => {
    show(response([busyClub()]));
    openTab(tab("delegation"));

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[2]).toHaveTextContent(tr("fastForward.report.delegationOutcome.leftToManager"));
    for (const row of rows) expect(row.textContent).not.toContain("fastForward.report");
  });

  /**
   * Given an injury the backend stores as a translation key
   * When the Squad tab opens
   * Then the player reads the translated injury, not the key
   */
  it("translates_the_injury_the_way_the_rest_of_the_game_does", () => {
    show(response([busyClub()]));
    openTab(tab("squad"));

    const row = screen.getByText("Ivy Hurt").closest("li") as HTMLElement;
    expect(row).toHaveTextContent(tr("common.injuries.twistedAnkle"));
    expect(row.textContent).not.toContain("common.injuries");
  });

  /**
   * Given a purchase for a fee, a sale for nothing and a free signing
   * When the Transfers tab opens
   * Then the fee is its own labelled clause (so no language has to put "free" into a price slot), and
   * a free signing carries no fee clause at all
   */
  it("shows_the_fee_as_its_own_clause", () => {
    const club = busyClub();
    club.transfers.moves.items.push(
      { ...club.transfers.moves.items[0], player_id: "p-gift", player_name: "Gus Gift", fee: 0 },
      {
        ...club.transfers.moves.items[0],
        kind: "free_signing",
        player_id: "p-free",
        player_name: "Fay Free",
        fee: null,
        fee_in_journal: null,
      },
    );
    show(response([club]));
    openTab(tab("transfers"));

    const feeClause = (fee: string) => tr("fastForward.report.feeLabel", { fee });
    expect(screen.getByText("Ben Bought").closest("li")).toHaveTextContent(
      feeClause(formatExactMoney(2_000_000)),
    );
    expect(screen.getByText("Gus Gift").closest("li")).toHaveTextContent(
      feeClause(tr("fastForward.report.noFee")),
    );
    expect(screen.getByText("Fay Free").closest("li")?.textContent).not.toContain(
      tr("fastForward.report.feeLabel", { fee: "" }).trim(),
    );
  });

  /**
   * Given a competition whose name is a template needing its nation and season
   * When a match of it is listed
   * Then the name is worded the way the game words it (from the competition in the game), not as an unresolved placeholder
   */
  it("words_a_templated_competition_name_from_the_game", () => {
    useGameStore.setState({
      gameState: {
        competitions: [
          {
            id: "league",
            name: "Test League",
            name_key: "tournaments.competitions.firstDivision",
            season: 2026,
            country_id: "ENG",
          },
        ],
      } as unknown as ReturnType<typeof useGameStore.getState>["gameState"],
    });
    show(response([busyClub()]));
    openTab(tab("matches"));

    const row = screen.getByText("Blues FC").closest("li") as HTMLElement;
    expect(row.textContent).not.toContain("{{");
    expect(row).toHaveTextContent(tr("nations.eng"));
    useGameStore.setState({ gameState: null });
  });

  /**
   * Given two clubs managed in one run
   * When the report opens
   * Then every tab points at a panel that exists, and the title takes focus so the new screen is announced
   */
  it("keeps_its_tab_references_valid_and_focuses_its_title", () => {
    show(response([busyClub(), emptyClub({ club_id: "blues", club_name: "Blues FC" })]));

    for (const tabElement of screen.getAllByRole("tab")) {
      const controlled = tabElement.getAttribute("aria-controls");
      expect(controlled && document.getElementById(controlled)).toBeTruthy();
    }
    expect(screen.getByRole("heading", { name: tr("fastForward.report.title") })).toHaveFocus();
  });
});
