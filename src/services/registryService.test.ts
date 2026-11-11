import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import {
  getExternalPlayer,
  getExternalRegistryStatus,
  previewExternalRecruitment,
  recruitExternalPlayer,
  searchExternalPlayers,
  type RegistrySearchRequest,
} from "./registryService";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

const request: RegistrySearchRequest = {
  text: "free",
  nation: "FR",
  position: "forward",
  minAge: 18,
  maxAge: 30,
  club: "",
  availability: "all",
  sort: "value",
  descending: true,
  page: 2,
  pageSize: 25,
  includeIncomplete: false,
};

describe("registryService", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("sends one page request to the backend and returns the page", async () => {
    const page = { items: [], total: 0, page: 2, pageSize: 25, counts: {} };
    mockedInvoke.mockResolvedValueOnce(page);

    await expect(searchExternalPlayers(request)).resolves.toBe(page);
    expect(mockedInvoke).toHaveBeenCalledWith("search_external_players", { request });
  });

  it("reads one registry detail by source identity", async () => {
    mockedInvoke.mockResolvedValueOnce(null);

    await expect(getExternalPlayer("fm-p-42")).resolves.toBeNull();
    expect(mockedInvoke).toHaveBeenCalledWith("get_external_player", {
      playerId: "fm-p-42",
    });
  });

  it("previews and recruits through the two backend commands", async () => {
    const preview = { playerId: "fm-p-42", fee: 1000, blocker: null };
    const recruited = { game: {}, outcome: { playerId: "fm-p-42" } };
    mockedInvoke.mockResolvedValueOnce(preview).mockResolvedValueOnce(recruited);

    await expect(previewExternalRecruitment("fm-p-42")).resolves.toBe(preview);
    await expect(recruitExternalPlayer("fm-p-42")).resolves.toBe(recruited);

    expect(mockedInvoke).toHaveBeenNthCalledWith(1, "preview_external_recruitment", {
      playerId: "fm-p-42",
    });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, "recruit_external_player", {
      playerId: "fm-p-42",
    });
  });

  it("reports the registry coverage", async () => {
    const status = { installed: true, eligible: 68000 };
    mockedInvoke.mockResolvedValueOnce(status);

    await expect(getExternalRegistryStatus()).resolves.toBe(status);
    expect(mockedInvoke).toHaveBeenCalledWith("get_external_registry_status");
  });
});
