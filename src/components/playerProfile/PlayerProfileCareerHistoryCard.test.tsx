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
});
