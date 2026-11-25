import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PlayerData, TransferOfferData } from "../../store/gameStore";
import { TransferOfferRecord } from "./TransfersPlayerOffers";
import type { TransfersTabController } from "./useTransfersTabController";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

describe("TransferOfferRecord", () => {
  /**
   * Given a pending offer of €941,250 for a player
   * When the offer row is shown in the offers list
   * Then the fee is exact, matching the same offer in its history.
   */
  it("shows the offered fee exactly", () => {
    const state = {
      gameState: { teams: [{ id: "buyer", name: "Buyer FC" }] },
      t: (key: string) => key,
      userTeamId: "mine",
      handleRespondOffer: vi.fn(),
      openCounterNegotiation: vi.fn(),
    } as unknown as Pick<
      TransfersTabController,
      | "gameState"
      | "t"
      | "userTeamId"
      | "handleRespondOffer"
      | "openCounterNegotiation"
      | "departureBlockers"
    >;
    const offer = {
      id: "o1",
      from_team_id: "buyer",
      fee: 941250,
      status: "Pending",
    } as unknown as TransferOfferData;

    render(
      <TransferOfferRecord state={state} offer={offer} player={{ team_id: "x" } as PlayerData} />,
    );

    expect(screen.getByText(/€941,250/)).toBeInTheDocument();
  });
});

describe("TransferOfferRecord with a player the squad floor holds", () => {
  const baseState = (departureBlockers: Record<string, string>) =>
    ({
      gameState: { teams: [{ id: "buyer", name: "Buyer FC" }] },
      t: (key: string) => key,
      userTeamId: "mine",
      handleRespondOffer: vi.fn(),
      openCounterNegotiation: vi.fn(),
      departureBlockers,
    }) as unknown as Pick<
      TransfersTabController,
      | "gameState"
      | "t"
      | "userTeamId"
      | "handleRespondOffer"
      | "openCounterNegotiation"
      | "departureBlockers"
    >;
  const offer = {
    id: "o1",
    from_team_id: "buyer",
    fee: 941250,
    status: "Pending",
  } as unknown as TransferOfferData;
  const player = { id: "gk", team_id: "mine" } as unknown as PlayerData;

  /**
   * Given an offer for a player the club cannot spare
   * When the offer row is shown
   * Then accepting and countering are closed, declining is not, and the reason is on the row
   */
  it("closes_accept_and_counter_but_not_decline_and_says_why", () => {
    const state = baseState({
      gk: "be.error.squadFloor.wouldLeaveShort?group=common.positionGroups.Goalkeeper",
    });
    render(<TransferOfferRecord state={state} offer={offer} player={player} />);

    expect(screen.getByTitle("transfers.acceptOffer")).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByTitle("transfers.acceptOffer")).toHaveAccessibleDescription();
    expect(screen.getByRole("button", { name: "transfers.counterOffer" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByTitle("transfers.rejectOffer")).toBeEnabled();
    // Closed means closed: a click on the blocked accept does nothing.
    fireEvent.click(screen.getByTitle("transfers.acceptOffer"));
    expect(state.handleRespondOffer).not.toHaveBeenCalled();
    expect(screen.getByRole("note")).toBeInTheDocument();
  });

  /**
   * Given the same offer for a player the club can spare
   * When the offer row is shown
   * Then nothing is closed and there is no note
   */
  it("leaves_everything_open_for_a_player_who_can_leave", () => {
    render(<TransferOfferRecord state={baseState({})} offer={offer} player={player} />);

    expect(screen.getByTitle("transfers.acceptOffer")).not.toHaveAttribute("aria-disabled");
    expect(screen.getByRole("button", { name: "transfers.counterOffer" })).not.toHaveAttribute(
      "aria-disabled",
    );
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
  });
});
