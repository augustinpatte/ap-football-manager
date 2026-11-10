import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import {
  acceptFreeAgentCounter,
  offerFreeAgentContract,
  previewFreeAgentContractImpact,
  withdrawFreeAgentOffer,
} from "./freeAgentService";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe("freeAgentService", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("submits a free-agent contract offer", async () => {
    const response = { outcome: "pending", game: { manager: { id: "manager-1" } } };
    mockedInvoke.mockResolvedValueOnce(response);

    await expect(offerFreeAgentContract("player-1", 4000, 3)).resolves.toBe(response);
    expect(mockedInvoke).toHaveBeenCalledWith("offer_free_agent_contract", {
      playerId: "player-1",
      weeklyWage: 4000,
      contractYears: 3,
      promiseSteppingStone: false,
      signingBonus: 0,
    });
  });

  it("submits the stepping-stone promise only when asked to", async () => {
    mockedInvoke.mockResolvedValueOnce({ outcome: "accepted" });

    await offerFreeAgentContract("player-1", 4000, 3, true);

    expect(mockedInvoke).toHaveBeenCalledWith("offer_free_agent_contract", {
      playerId: "player-1",
      weeklyWage: 4000,
      contractYears: 3,
      promiseSteppingStone: true,
      signingBonus: 0,
    });
  });

  it("previews a free-agent contract impact", async () => {
    const response = { projection: { projected_annual_wage_bill: 4000 } };
    mockedInvoke.mockResolvedValueOnce(response);

    await expect(previewFreeAgentContractImpact("player-1", 4000)).resolves.toBe(response);
    expect(mockedInvoke).toHaveBeenCalledWith("preview_free_agent_contract_impact", {
      playerId: "player-1",
      weeklyWage: 4000,
      signingBonus: 0,
      promiseSteppingStone: false,
    });
  });

  it("previews with the contract length and promise the manager is considering", async () => {
    mockedInvoke.mockResolvedValueOnce({});

    await previewFreeAgentContractImpact("player-1", 4000, 0, 3, true);

    expect(mockedInvoke).toHaveBeenCalledWith("preview_free_agent_contract_impact", {
      playerId: "player-1",
      weeklyWage: 4000,
      signingBonus: 0,
      contractYears: 3,
      promiseSteppingStone: true,
    });
  });

  it("accepts a counter-offer and withdraws an offer by player id", async () => {
    mockedInvoke.mockResolvedValue({ outcome: "accepted" });

    await acceptFreeAgentCounter("player-1");
    await withdrawFreeAgentOffer("player-1");

    expect(mockedInvoke).toHaveBeenCalledWith("accept_free_agent_counter", {
      playerId: "player-1",
    });
    expect(mockedInvoke).toHaveBeenCalledWith("withdraw_free_agent_offer", {
      playerId: "player-1",
    });
  });

  it("sends the signing bonus with the offer and the preview", async () => {
    mockedInvoke.mockResolvedValue({});

    await offerFreeAgentContract("player-1", 4000, 3, false, 150000);
    await previewFreeAgentContractImpact("player-1", 4000, 150000);

    expect(mockedInvoke).toHaveBeenCalledWith("offer_free_agent_contract", {
      playerId: "player-1",
      weeklyWage: 4000,
      contractYears: 3,
      promiseSteppingStone: false,
      signingBonus: 150000,
    });
    expect(mockedInvoke).toHaveBeenCalledWith("preview_free_agent_contract_impact", {
      playerId: "player-1",
      weeklyWage: 4000,
      signingBonus: 150000,
      promiseSteppingStone: false,
    });
  });
});
