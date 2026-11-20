import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { createPlayer } from "./TransfersTab.testFixtures";
import { DealWorkspaceFacts } from "./PlayerDealWorkspace.layout";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../hooks/useRatingTiers", async () => {
  const { cannedRatingTiersModule } = await import("../../test-utils/ratingTiers");
  return cannedRatingTiersModule();
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

/**
 * Given a player the club has not scouted, rated 74 in the game
 * When the deal workspace shows his facts and the backend answers with his public standing
 * Then the OVR fact is the standing, and 74 is nowhere on screen
 */
it("shows the deal target's standing, not his rating, when he is unscouted", async () => {
  const player = createPlayer({ ovr: 74 });
  vi.mocked(invoke).mockResolvedValue({
    [player.id]: { kind: "standing", standing: "good" },
  });

  render(<DealWorkspaceFacts player={player} myTeam={null} />);

  expect(await screen.findByText("playerRating.standing.good")).toBeInTheDocument();
  expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_player_ratings", { playerIds: [player.id] });
  expect(screen.queryByText("74")).not.toBeInTheDocument();
});

/**
 * Given the rating has not arrived
 * When the facts render
 * Then no rating is shown (a dash is announced as unavailable) rather than the number in the game
 */
it("shows no rating until the backend has answered", () => {
  const player = createPlayer({ ovr: 74 });
  vi.mocked(invoke).mockReturnValue(new Promise(() => {}));

  render(<DealWorkspaceFacts player={player} myTeam={null} />);

  expect(screen.getByText("playerRating.unavailable")).toBeInTheDocument();
  expect(screen.queryByText("74")).not.toBeInTheDocument();
});
