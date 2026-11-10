import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import {
  applyAssistantRecommendation,
  proposeStaffContract,
  releaseStaff,
  setDutyDelegation,
  setPlayerProtected,
} from "./staffService";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe("staffService", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("proposes a contract with the wage and length", async () => {
    const outcome = { response: { status: "accepted" } };
    mockedInvoke.mockResolvedValueOnce(outcome);

    await expect(proposeStaffContract("staff-1", 2000, 2)).resolves.toBe(outcome);
    expect(mockedInvoke).toHaveBeenCalledWith("propose_staff_contract", {
      staffId: "staff-1",
      wage: 2000,
      years: 2,
    });
  });

  it("sets a delegation with the confirmation flag", async () => {
    mockedInvoke.mockResolvedValueOnce({});
    await setDutyDelegation("Training", true, false);
    expect(mockedInvoke).toHaveBeenCalledWith("set_duty_delegation", {
      duty: "Training",
      delegated: true,
      confirmMajor: false,
    });
  });

  it("applies a recommendation by id", async () => {
    mockedInvoke.mockResolvedValueOnce({});
    await applyAssistantRecommendation("rec-1");
    expect(mockedInvoke).toHaveBeenCalledWith("apply_assistant_recommendation", {
      recommendationId: "rec-1",
    });
  });

  it("calls the release staff backend command", async () => {
    const response = { manager: { id: "manager-1" } };
    mockedInvoke.mockResolvedValueOnce(response);

    await expect(releaseStaff("staff-2")).resolves.toBe(response);
    expect(mockedInvoke).toHaveBeenCalledWith("release_staff", {
      staffId: "staff-2",
    });
  });

  it("tells the assistant a player is not to be sold", async () => {
    const game = { manager: {} };
    mockedInvoke.mockResolvedValueOnce(game);

    await expect(setPlayerProtected("p-9", true)).resolves.toBe(game);
    expect(mockedInvoke).toHaveBeenCalledWith("set_player_protected", {
      playerId: "p-9",
      protected: true,
    });
  });
});
