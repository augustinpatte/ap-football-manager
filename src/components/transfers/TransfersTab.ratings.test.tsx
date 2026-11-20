import { render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { createInstance } from "i18next";
import { invoke } from "@tauri-apps/api/core";

import en from "../../i18n/locales/en.json";
import TransfersTab from "./TransfersTab";
import { createGameState, createPlayer, marketPageOf } from "./TransfersTab.testFixtures";
import type { RatingView } from "../../services/playerRatingService";

const translations = createInstance();
await translations.init({
  lng: "en",
  resources: { en: { translation: en } },
  interpolation: { escapeValue: false },
});
vi.mock("react-i18next", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-i18next")>()),
  useTranslation: () => ({ t: translations.t.bind(translations), i18n: translations }),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: vi.fn((path: string) => path),
  invoke: vi.fn(async () => ({})),
  isTauri: vi.fn(() => false),
}));
vi.mock("../../hooks/useRatingTiers", async () => {
  const { cannedRatingTiersModule } = await import("../../test-utils/ratingTiers");
  return cannedRatingTiersModule();
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

function answerMarketWith(rating: RatingView, ovr: number) {
  const player = createPlayer({
    id: "market-player",
    full_name: "Market Man",
    team_id: "team-2",
    transfer_listed: true,
    ovr,
  });
  const page = marketPageOf([player]);
  page.items[0] = { ...page.items[0], rating };
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "get_players_page") return page;
    return {};
  });
  return player;
}

function renderMarket(player: ReturnType<typeof createPlayer>) {
  return render(
    <TransfersTab
      gameState={createGameState([player])}
      onSelectPlayer={vi.fn()}
      onSelectTeam={vi.fn()}
      onGameUpdate={vi.fn()}
    />,
  );
}

/**
 * Given a listed player the club has not scouted, rated 73 in the game, whom the backend answers
 *   with his public standing
 * When the transfer market renders
 * Then his row says "Good" in the rating column and the number 73 is nowhere on screen, and the
 *   rating header explains the order
 */
it("shows an unscouted market player as a standing, never his rating", async () => {
  const player = answerMarketWith({ kind: "standing", standing: "good" }, 73);

  renderMarket(player);

  const row = (await screen.findByText("Market Man")).closest("tr") as HTMLElement;
  expect(within(row).getByText("Good")).toBeInTheDocument();
  expect(screen.queryByText("73")).not.toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: /OVR/ })).toHaveAttribute(
    "title",
    en.playerRating.sortHint,
  );
});

/**
 * Given the same player after a scout reported on him, but from before this season
 * When the transfer market renders
 * Then his row shows the scout's range marked as an old report, not his current rating
 */
it("shows a scouted market player's range marked old when the report is from an earlier season", async () => {
  const player = answerMarketWith(
    {
      kind: "estimate",
      low: 66,
      high: 72,
      confidence: "moderate",
      current: false,
      scouted_on: "2026-03-01",
    },
    81,
  );

  renderMarket(player);

  const row = (await screen.findByText("Market Man")).closest("tr") as HTMLElement;
  expect(within(row).getByText("66–72")).toBeInTheDocument();
  expect(within(row).getByText("Old report")).toBeInTheDocument();
  expect(screen.queryByText("81")).not.toBeInTheDocument();
});
