import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import PlayerProfileOutsideOwnerBanner from "./PlayerProfileOutsideOwnerBanner";

const t = (key: string, options?: Record<string, unknown>) =>
  options && "owner" in options ? `${key}:${String(options.owner)}>${String(options.team)}` : key;

describe("PlayerProfileOutsideOwnerBanner", () => {
  /**
   * Given a player the save has owned by a club outside the simulated leagues and playing for a club of the world
   * When the banner renders
   * Then the owner is named under "contract owner", the club he plays for under "playing for", and
   * a note says no return is scheduled because the owner is not simulated.
   */
  it("names the outside owner and explains that no return is scheduled", () => {
    render(<PlayerProfileOutsideOwnerBanner ownerName="Outside FC" teamName="Inside FC" t={t} />);

    const owner = screen.getByText("playerProfile.loanOwnerLabel").parentElement as HTMLElement;
    const playing = screen.getByText("playerProfile.loanPlayingForLabel")
      .parentElement as HTMLElement;
    expect(within(owner).getByText("Outside FC")).toBeInTheDocument();
    expect(within(playing).getByText("Inside FC")).toBeInTheDocument();
    expect(
      screen.getByText("playerProfile.outsideOwnerNote:Outside FC>Inside FC"),
    ).toBeInTheDocument();
  });
});
