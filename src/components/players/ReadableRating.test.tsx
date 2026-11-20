import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { RatingView } from "../../services/playerRatingService";
import ReadableRating from "./ReadableRating";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

vi.mock("../../hooks/useRatingTiers", async () => {
  const { cannedRatingTiersModule } = await import("../../test-utils/ratingTiers");
  return cannedRatingTiersModule();
});

const ESTIMATE: RatingView = {
  kind: "estimate",
  low: 64,
  high: 70,
  confidence: "moderate",
  current: true,
  scouted_on: "2026-09-01",
};

describe("ReadableRating", () => {
  /**
   * Given a player the club knows exactly
   * When his rating is shown
   * Then the number is on screen with an accessible name that says the number
   */
  it("shows the number of a player the club knows", () => {
    render(<ReadableRating rating={{ kind: "exact", ovr: 77 }} />);

    expect(screen.getByText("77")).toBeInTheDocument();
    expect(screen.getByText('playerRating.a11y.exact:{"value":77}')).toBeInTheDocument();
  });

  /**
   * Given a current scout's estimate
   * When it is shown
   * Then it reads as a range, says it is an estimate with its confidence, and carries no old-report mark
   */
  it("shows a current estimate as a range with its confidence", () => {
    render(<ReadableRating rating={ESTIMATE} />);

    expect(screen.getByText("64–70")).toBeInTheDocument();
    expect(screen.getByText("playerRating.estimate")).toBeInTheDocument();
    expect(
      screen.getByText(
        'playerRating.a11y.estimate:{"low":64,"high":70,"confidence":"playerRating.confidence.moderate"}',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("playerRating.stale")).not.toBeInTheDocument();
  });

  /**
   * Given an estimate from a report written before this season
   * When it is shown
   * Then it still reads as that range, marked as an old report in text and for a screen reader
   */
  it("marks an estimate from an earlier season as old", () => {
    render(<ReadableRating rating={{ ...ESTIMATE, kind: "estimate", current: false }} />);

    expect(screen.getByText("64–70")).toBeInTheDocument();
    expect(screen.getByText("playerRating.stale")).toBeInTheDocument();
    expect(
      screen.getByText(
        'playerRating.a11y.estimateStale:{"low":64,"high":70,"confidence":"playerRating.confidence.moderate"}',
      ),
    ).toBeInTheDocument();
  });

  /**
   * Given a player known only by his public standing
   * When his rating is shown
   * Then the standing's name is on screen and no digit is, visible or in the accessible text
   */
  it("shows a standing as a word with no number anywhere", () => {
    const { container } = render(<ReadableRating rating={{ kind: "standing", standing: "good" }} />);

    expect(screen.getByText("playerRating.standing.good")).toBeInTheDocument();
    // The mocked translator echoes keys, and "a11y" is one: look for digits outside key names.
    expect((container.textContent ?? "").replaceAll("a11y", "")).not.toMatch(/\d/);
    expect(
      screen.getByText(
        'playerRating.a11y.standing:{"standing":"playerRating.standing.good"}',
      ),
    ).toBeInTheDocument();
  });

  /**
   * Given a rating that has not arrived, or could not be read
   * When it is shown
   * Then a dash stands in, announced as unavailable, and no number is invented
   */
  it("shows a dash while the rating is not available", () => {
    render(<ReadableRating rating={null} />);

    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("playerRating.unavailable")).toBeInTheDocument();
  });
});
