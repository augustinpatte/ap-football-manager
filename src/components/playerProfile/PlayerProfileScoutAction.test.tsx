import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import PlayerProfileScoutAction from "./PlayerProfileScoutAction";
import type { StaffData } from "../../store/gameStore";
import type { ScoutAvailability } from "./PlayerProfile.scouting";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { free?: number; total?: number; active?: number }) => {
      if (key === "scouting.profileAvailability")
        return `${options?.free} free of ${options?.total}, ${options?.active} active`;
      if (key === "scouting.noScoutsHint") return "Hire a scout first";
      if (key === "scouting.scoutingInProgress") return "Scouting in progress";
      if (key === "scouting.scoutBtn") return "Scout";
      return key;
    },
  }),
}));

describe("PlayerProfileScoutAction", () => {
  const scout: StaffData = {
    id: "scout-1",
    first_name: "Sam",
    last_name: "Scout",
    display_name: "Sam Scout",
    date_of_birth: "1990-01-01",
    nationality: "ES",
    role: "Scout",
    attributes: {
      coaching: 0,
      judgingAbility: 14,
      judgingPotential: 15,
      physiotherapy: 0,
    },
    team_id: "team-1",
    specialization: null,
    wage: 1500,
    contract_end: "2027-06-30",
  };

  function buildAvailability(overrides: Partial<ScoutAvailability>): ScoutAvailability {
    return {
      scouts: [scout],
      availableScout: scout,
      alreadyScouting: false,
      allBusy: false,
      canScout: true,
      freeScoutCount: 1,
      activeAssignmentCount: 0,
      ...overrides,
    };
  }

  it("renders translated scout actions", () => {
    const onScout = vi.fn();

    render(
      <PlayerProfileScoutAction
        availability={buildAvailability({})}
        scoutStatus="idle"
        scoutError={null}
        onScout={onScout}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Scout/i }));

    expect(onScout).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /Scout/i })).toBeInTheDocument();
  });

  it("renders translated hints and progress states", () => {
    const { rerender } = render(
      <PlayerProfileScoutAction
        availability={buildAvailability({
          scouts: [],
          availableScout: null,
          canScout: false,
        })}
        scoutStatus="idle"
        scoutError={null}
        onScout={vi.fn()}
      />,
    );

    expect(screen.getByText("Hire a scout first")).toBeInTheDocument();

    rerender(
      <PlayerProfileScoutAction
        availability={buildAvailability({
          alreadyScouting: true,
          canScout: false,
        })}
        scoutStatus="sent"
        scoutError={null}
        onScout={vi.fn()}
      />,
    );

    expect(screen.getByText("Scouting in progress")).toBeInTheDocument();
  });

  /**
   * Given a club with two scouts, one of them already on an assignment
   * When the manager opens a player profile
   * Then the profile says how many scouts are free and how many assignments are running.
   */
  it("shows scout availability next to the button", () => {
    render(
      <PlayerProfileScoutAction
        availability={buildAvailability({
          scouts: [scout, { ...scout, id: "scout-2" }],
          freeScoutCount: 1,
          activeAssignmentCount: 1,
        })}
        scoutStatus="idle"
        scoutError={null}
        onScout={vi.fn()}
      />,
    );

    expect(screen.getByText("1 free of 2, 1 active")).toBeTruthy();
  });

  /**
   * Given every scout is busy
   * When the profile renders
   * Then the button is disabled and the reason is on screen, not just a greyed control.
   */
  it("explains a disabled button when every scout is busy", () => {
    render(
      <PlayerProfileScoutAction
        availability={buildAvailability({
          availableScout: null,
          allBusy: true,
          canScout: false,
          freeScoutCount: 0,
          activeAssignmentCount: 1,
        })}
        scoutStatus="idle"
        scoutError={null}
        onScout={vi.fn()}
      />,
    );

    expect((screen.getByRole("button", { name: "Scout" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(screen.getByText("0 free of 1, 1 active")).toBeTruthy();
  });
});
