import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import type { WorldDatabaseInfo } from "../../services/worldDatabaseService";
import WorldDatabaseStep from "./WorldDatabaseStep";

const listWorldDatabases = vi.fn();
const findFm24WorldSource = vi.fn();
const installFm24World = vi.fn();
vi.mock("../../services/worldDatabaseService", () => ({
  listWorldDatabases: () => listWorldDatabases(),
  findFm24WorldSource: () => findFm24WorldSource(),
  installFm24World: (path?: string) => installFm24World(path),
}));

let pickedFile: string | null = null;
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(pickedFile),
}));

beforeAll(async () => {
  await i18nReady;
  await i18n.changeLanguage("en");
});

const demo: WorldDatabaseInfo = {
  id: "random",
  name: "be.world.random.name",
  description: "",
  team_count: 440,
  player_count: 9680,
  history_mode: "generated",
  base_year: null,
  snapshot_date: null,
  source: "builtin",
  path: "",
  world_id: "",
  competitions: [],
  start_date: null,
};

const fm24: WorldDatabaseInfo = {
  ...demo,
  id: "file:/data/databases/apfm-fm24-2026.json",
  name: "AP Football Manager — FM24 2026/27",
  team_count: 240,
  player_count: 10137,
  history_mode: "hybrid",
  base_year: 2026,
  source: "user",
  path: "/data/databases/apfm-fm24-2026.json",
  world_id: "apfm-fm24-2026",
  competitions: ["Ligue 1", "Premier League"],
  start_date: "2026-06-01T00:00:00+00:00",
};

function renderStep(onNext = vi.fn()) {
  render(<WorldDatabaseStep onNext={onNext} onBack={vi.fn()} onClose={vi.fn()} />);
  return onNext;
}

describe("WorldDatabaseStep", () => {
  beforeEach(() => {
    listWorldDatabases.mockReset();
    findFm24WorldSource.mockReset();
    installFm24World.mockReset();
    pickedFile = null;
  });

  /** Given an installed FM24 world, when the step opens, then it is selected and its scope, start date and leagues are shown. */
  it("shows the installed FM24 world with its scope, start date and leagues", async () => {
    listWorldDatabases.mockResolvedValue([demo, fm24]);

    renderStep();

    expect(await screen.findByText("240 clubs · 2 leagues · 10137 players")).toBeInTheDocument();
    expect(screen.getByText("June 1, 2026")).toBeInTheDocument();
    expect(screen.getByText("Available leagues: 2")).toBeInTheDocument();
    expect(screen.getByText("Selected")).toBeInTheDocument();
    expect(screen.getByText("Local data")).toBeInTheDocument();
  });

  /** Given an installed FM24 world, when continuing, then the file world is handed on with its base year. */
  it("hands the FM24 file and its base year to the flow", async () => {
    listWorldDatabases.mockResolvedValue([demo, fm24]);
    const onNext = renderStep();
    await screen.findByText("Selected");

    fireEvent.click(screen.getByText("Continue"));

    expect(onNext).toHaveBeenCalledWith({
      kind: "file",
      path: "/data/databases/apfm-fm24-2026.json",
      baseYear: 2026,
    });
  });

  /** Given only the demo world, when the step opens, then it offers the install and keeps the demo selectable. */
  it("offers to install the FM24 world when it is absent and still allows the demo", async () => {
    listWorldDatabases.mockResolvedValue([demo]);
    const onNext = renderStep();

    expect(await screen.findByText("Install the FM24 world")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Continue"));

    expect(onNext).toHaveBeenCalledWith({ kind: "demo" });
  });

  /** Given a game that finds the world.db itself, when installing, then no file picker is needed and the new world is selected. */
  it("installs from the source the game found and selects the new world", async () => {
    listWorldDatabases.mockResolvedValueOnce([demo]).mockResolvedValue([demo, fm24]);
    findFm24WorldSource.mockResolvedValue("/data/world.db");
    installFm24World.mockResolvedValue({});
    renderStep();

    fireEvent.click(await screen.findByText("Install the FM24 world"));

    await waitFor(() => expect(installFm24World).toHaveBeenCalledWith("/data/world.db"));
    expect(await screen.findByText("Selected")).toBeInTheDocument();
  });

  /** Given no world.db anywhere and a cancelled picker, when installing, then nothing is installed. */
  it("installs nothing when the player cancels the file picker", async () => {
    listWorldDatabases.mockResolvedValue([demo]);
    findFm24WorldSource.mockResolvedValue(null);
    renderStep();

    fireEvent.click(await screen.findByText("Install the FM24 world"));

    await waitFor(() => expect(screen.getByText("Install the FM24 world")).toBeEnabled());
    expect(installFm24World).not.toHaveBeenCalled();
  });

  /** Given a backend refusal, when installing, then the player reads the translated reason. */
  it("shows the translated reason when the install is refused", async () => {
    listWorldDatabases.mockResolvedValue([demo]);
    findFm24WorldSource.mockResolvedValue("/data/world.db");
    installFm24World.mockRejectedValue("be.error.fm24.checksFailed");
    renderStep();

    fireEvent.click(await screen.findByText("Install the FM24 world"));

    expect(await screen.findByRole("alert")).toHaveTextContent("failed its integrity checks");
  });
});
