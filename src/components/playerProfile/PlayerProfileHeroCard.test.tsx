import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ratingTierClass } from "../../lib/ratingTier";
import type { RatingView } from "../../services/playerRatingService";
import { createPlayer } from "../../test-utils/factories";
import PlayerProfileHeroCard from "./PlayerProfileHeroCard";

vi.mock("../../hooks/useRatingTiers", async () => {
  const { cannedRatingTiersModule } = await import("../../test-utils/ratingTiers");
  return cannedRatingTiersModule();
});

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

function t(key: string): string {
  return key === "common.ovr" ? "OVR" : key;
}

function renderHero(face?: string) {
  const player = createPlayer({ ovr: 77, media: face ? { face } : undefined });

  render(
    <PlayerProfileHeroCard
      player={player}
      rating={{ kind: "exact", ovr: 77 }}
      formKnown
      primaryPosition="Midfielder"
      age={22}
      teamName="Brussels FC"
      footednessLabel="Right"
      weakFootValue={4}
      language="en"
      isOwnClub
      scoutAvailability={{
        scouts: [],
        availableScout: null,
        alreadyScouting: false,
        allBusy: false,
        canScout: false,
      }}
      scoutStatus="idle"
      scoutError={null}
      onScout={() => {}}
      t={t}
    />,
  );
}

describe("PlayerProfileHeroCard", () => {
  /**
   * Given a player whose face picture is shown in the hero
   * When his profile opens
   * Then his OVR is still on screen, labelled, and not hidden behind the picture
   */
  it("shows the OVR of a player who has a face picture", () => {
    renderHero("assets/faces/fraser.png");

    expect(screen.getByRole("img", { name: "John Smith" })).toBeInTheDocument();
    expect(screen.getByText("OVR")).toBeInTheDocument();
    expect(screen.getByText("77")).toBeInTheDocument();
  });

  /**
   * Given a player with no face picture
   * When his profile opens
   * Then his OVR appears once, in its own labelled tile, and the avatar does not repeat it
   */
  it("shows the OVR once for a player without a face picture", () => {
    renderHero();

    expect(screen.getByText("OVR")).toBeInTheDocument();
    expect(screen.getAllByText("77")).toHaveLength(1);
  });
});

function renderHeroAtOvr(ovr: number) {
  return render(
    <PlayerProfileHeroCard
      player={createPlayer()}
      rating={{ kind: "exact", ovr }}
      formKnown
      primaryPosition="Forward"
      age={25}
      teamName="Club"
      footednessLabel="Right"
      weakFootValue={3}
      language="en"
      isOwnClub
      scoutAvailability={{
        scouts: [],
        availableScout: null,
        alreadyScouting: false,
        allBusy: false,
        canScout: false,
      }}
      scoutStatus="idle"
      scoutError={null}
      onScout={vi.fn()}
      t={(key: string) => key}
    />,
  );
}

describe("PlayerProfileHeroCard rating tiers", () => {
  /**
   * Given a player rated 76 and another rated 77, either side of the A-tier cut-off
   * When the profile hero shows the rating in its avatar frame
   * Then the frame takes the tint of the backend tier (B and A).
   */
  it("tints the rating frame by its backend tier", () => {
    const frameOf = (ovr: string) => screen.getByText(ovr).closest(".rounded-2xl");

    const { unmount } = renderHeroAtOvr(76);
    expect(frameOf("76")).toHaveClass(...ratingTierClass("B", "hero").split(" "));
    unmount();

    renderHeroAtOvr(77);
    expect(frameOf("77")).toHaveClass(...ratingTierClass("A", "hero").split(" "));
  });
});

function renderHeroWith(rating: RatingView | null) {
  return render(
    <PlayerProfileHeroCard
      player={createPlayer({ condition: 43, morale: 38 })}
      rating={rating}
      formKnown={rating?.kind === "exact"}
      primaryPosition="Forward"
      age={25}
      teamName="Club"
      footednessLabel="Right"
      weakFootValue={3}
      language="en"
      isOwnClub={false}
      scoutAvailability={{
        scouts: [],
        availableScout: null,
        alreadyScouting: false,
        allBusy: false,
        canScout: false,
      }}
      scoutStatus="idle"
      scoutError={null}
      onScout={vi.fn()}
      t={(key: string) => key}
    />,
  );
}

describe("PlayerProfileHeroCard for a player the club has not scouted", () => {
  /**
   * Given an external player known only by his public standing
   * When his profile hero renders
   * Then the OVR tile says the standing, no number stands for his rating, and his condition and
   *      morale (what a scout reports) are not shown either
   */
  it("shows a standing, never a rating, condition or morale", () => {
    renderHeroWith({ kind: "standing", standing: "strong" });

    expect(screen.getByText("playerRating.standing.strong")).toBeInTheDocument();
    expect(screen.queryByText("43%")).not.toBeInTheDocument();
    expect(screen.queryByText("38%")).not.toBeInTheDocument();
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(2);
    // The dashes are spoken as "not known", not read as a lone dash.
    expect(screen.getAllByText("playerRating.notKnown").length).toBeGreaterThanOrEqual(2);
  });

  /**
   * Given a player a scout reported on, from before this season
   * When his profile hero renders
   * Then the tile shows the scout's range marked as an old estimate, not a single figure
   */
  it("shows an old scout's range as an estimate marked old", () => {
    renderHeroWith({
      kind: "estimate",
      low: 70,
      high: 78,
      confidence: "low",
      current: false,
      scouted_on: "2026-03-01",
    });

    expect(screen.getByText("70–78")).toBeInTheDocument();
    expect(screen.getByText("playerRating.stale")).toBeInTheDocument();
  });

  /**
   * Given a rating that has not arrived yet
   * When his profile hero renders
   * Then a dash stands in the tile, and no number is invented
   */
  it("shows a dash while the rating is on its way", () => {
    renderHeroWith(null);

    expect(screen.getByText("playerRating.unavailable")).toBeInTheDocument();
  });

  /**
   * Given a player the club knows exactly (its own)
   * When his profile hero renders
   * Then his condition and morale are shown as before
   */
  it("still shows the condition and morale of a player the club knows", () => {
    renderHeroWith({ kind: "exact", ovr: 70 });

    expect(screen.getAllByText("43%").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("38%").length).toBeGreaterThanOrEqual(1);
  });
});
