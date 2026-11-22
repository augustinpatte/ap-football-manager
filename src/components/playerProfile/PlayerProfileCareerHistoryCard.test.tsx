import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CareerEntry } from "../../store/gameStore";
import PlayerProfileCareerHistoryCard from "./PlayerProfileCareerHistoryCard";

const t = (key: string) => key;

const entry = (season: number, synthetic?: boolean): CareerEntry => ({
  season,
  team_id: "club",
  team_name: "Club FC",
  appearances: 20,
  goals: 2,
  assists: 1,
  ...(synthetic === undefined ? {} : { synthetic }),
});

describe("PlayerProfileCareerHistoryCard", () => {
  /** Given generated and played seasons, when the card renders, then only the generated ones are marked and the note explains why. */
  it("marks generated seasons as simulated and says they are not real records", () => {
    render(
      <PlayerProfileCareerHistoryCard career={[entry(2024, true), entry(2025, false)]} t={t} />,
    );

    expect(screen.getAllByText("playerProfile.careerSimulated")).toHaveLength(1);
    expect(screen.getByText("playerProfile.careerSimulatedNote")).toBeInTheDocument();
  });

  /** Given a career from a save that predates the flag, when the card renders, then nothing is marked. */
  it("does not mark entries of older saves", () => {
    render(<PlayerProfileCareerHistoryCard career={[entry(2024), entry(2025)]} t={t} />);

    expect(screen.queryByText("playerProfile.careerSimulated")).not.toBeInTheDocument();
    expect(screen.queryByText("playerProfile.careerSimulatedNote")).not.toBeInTheDocument();
  });

  /**
   * Given a real player whose earlier career is unavailable and who has no season in this game yet
   * When the card renders
   * Then it states that no verified record exists, instead of the empty "first season" line or a table.
   */
  it("states that no verified earlier career exists, rather than showing none silently", () => {
    render(<PlayerProfileCareerHistoryCard career={[]} earlierCareer="unavailable" t={t} />);

    expect(screen.getByText("playerProfile.careerUnavailable")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.noCareer")).not.toBeInTheDocument();
  });

  /**
   * Given a player whose earlier career is unavailable and a season he played in this game
   * When the card renders
   * Then the played season is listed and the unavailable note stays, so the table is not read as his whole career.
   */
  it("keeps the unavailable note beside seasons played in this game", () => {
    render(
      <PlayerProfileCareerHistoryCard
        career={[entry(2026, false)]}
        earlierCareer="unavailable"
        t={t}
      />,
    );

    expect(screen.getByText("2026/2027")).toBeInTheDocument();
    expect(screen.getByText("playerProfile.careerUnavailable")).toBeInTheDocument();
  });

  /**
   * Given a player of a generated world (or a save from before provenance)
   * When the card renders with no provenance
   * Then the empty state is the ordinary one and no unavailable note appears.
   */
  it("keeps the ordinary empty state for a generated career", () => {
    render(<PlayerProfileCareerHistoryCard career={[]} t={t} />);

    expect(screen.getByText("playerProfile.noCareer")).toBeInTheDocument();
    expect(screen.queryByText("playerProfile.careerUnavailable")).not.toBeInTheDocument();
  });
});
