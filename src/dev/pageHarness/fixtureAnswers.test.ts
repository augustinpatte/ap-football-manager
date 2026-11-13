import { describe, expect, it } from "vitest";

import { readStyleGuideOptions } from "../styleGuide/styleGuideOptions";
import type { RegistrySearchRequest } from "../../services/registryService";
import { fixtureHandlers } from "./fixtureAnswers";
import { gameState, squad } from "./fixture.generated";
import { MARKET_TOTAL, marketPlayers } from "./globalMarketFixture";
import { UNANSWERED } from "./harnessIpc";

const handlers = fixtureHandlers(readStyleGuideOptions(""), "en");
const club = gameState.manager.team_id;

const search = {
  text: "",
  nation: null,
  position: "any",
  minAge: null,
  maxAge: null,
  club: "",
  availability: "all",
  sort: "ovr",
  descending: true,
  page: 1,
  pageSize: 25,
  includeIncomplete: false,
} satisfies RegistrySearchRequest;

describe("fixture answers", () => {
  /** Given the user's club, when its squad is asked for, then the fixture's squad comes back. */
  it("answers for the user's club", () => {
    expect(handlers.get_squad({ teamId: club })).toBe(squad);
  });

  /**
   * Given another club, when its squad, standing or finances are asked for, then the fixture
   * declines: it only holds the user's club, and must not pass it off as another's.
   */
  it("declines another club", () => {
    for (const command of ["get_squad", "get_club_league_standing", "get_finance_snapshot"]) {
      expect(handlers[command]({ teamId: "another-club" })).toBe(UNANSWERED);
    }
  });

  /** Given no team id, when the finances are asked for, then they are the user's club's, as in Rust. */
  it("treats a missing team id as the user's club for finances", () => {
    expect(handlers.get_finance_snapshot({ teamId: null })).not.toBe(UNANSWERED);
  });

  /**
   * Given player ids, one of them unknown, when contract stages are asked for, then only the
   * known players' stages come back, as Rust leaves an unknown id out.
   */
  it("answers contract stages only for the players asked about", () => {
    const [first] = squad;
    const stages = handlers.get_contract_warning_stages({ playerIds: [first.id, "nobody"] });
    expect(Object.keys(stages as object)).toEqual([first.id]);
  });

  /**
   * Given the dashboard's staff warnings, when they ask for fitness and rating tiers, then the
   * fixture answers so the shell can finish loading instead of leaving a call unanswered.
   */
  it("answers squad fitness and the rating tiers", () => {
    expect(handlers.get_squad_fitness({})).toMatchObject({
      players: squad.length,
      exhausted_count: 0,
    });
    const tiers = handlers.get_rating_tiers({}) as string[];
    expect(tiers).toHaveLength(100);
    expect(tiers[95]).toBe("S");
  });

  /**
   * Given a page request, when the market is searched, then the fixture's own registry answers,
   * with more results than one page so the paging controls can be exercised.
   */
  it("answers the global market from its own registry", () => {
    const first = handlers.search_external_players({ request: search }) as {
      items: unknown[];
      total: number;
    };
    expect(first.total).toBe(MARKET_TOTAL);
    expect(first.items).toHaveLength(25);

    const second = handlers.search_external_players({
      request: { ...search, page: 2 },
    }) as { items: unknown[]; page: number };
    expect(second.page).toBe(2);
    expect(second.items).toHaveLength(MARKET_TOTAL - 25);
  });

  /** Given a market id, when it is asked about, then its profile and terms come back. */
  it("answers a market player and his negotiation", () => {
    const player = marketPlayers[0];
    expect(handlers.get_external_player({ playerId: player.id })).toMatchObject({
      id: player.id,
      name: player.name,
    });
    expect(
      handlers.negotiate_external_transfer({ playerId: player.id, request: {} }),
    ).toMatchObject({ playerId: player.id, status: "agreed", blocker: null });
    expect(handlers.finalize_external_transfer({ playerId: player.id, request: {} })).toMatchObject(
      { game: gameState, outcome: { playerId: player.id } },
    );
  });

  /** Given an id outside the market, then the command declines instead of inventing a player. */
  it("declines a player the market does not hold", () => {
    expect(handlers.get_external_player({ playerId: "nobody" })).toBe(UNANSWERED);
    expect(handlers.negotiate_external_transfer({ playerId: "nobody", request: {} })).toBe(
      UNANSWERED,
    );
    expect(handlers.finalize_external_transfer({ playerId: "nobody", request: {} })).toBe(
      UNANSWERED,
    );
  });
});
