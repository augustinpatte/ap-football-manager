import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18nReady } from "../../i18n";
import { backend, reset, stubCanvas } from "./interactiveMatchTestBackend";
import { opened, tr, wall } from "./interactiveMatchTestScreen";

vi.mock(
  "../../services/sessionService",
  async () => (await import("./interactiveMatchTestBackend")).sessionMock,
);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("./interactiveMatchTestBackend")).windowMock,
);
vi.mock(
  "../../services/interactiveMatchService",
  async () => (await import("./interactiveMatchTestBackend")).serviceMock,
);

const pitchLabel = () => tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" });
const openFormationTab = () =>
  fireEvent.click(screen.getByRole("tab", { name: tr("match.formationPanel.tabFormation") }));
const bench = () => within(screen.getByRole("region", { name: tr("match.formationPanel.bench") }));

describe("the Formation & Substitutions tab of the live match", () => {
  beforeEach(async () => {
    await i18nReady;
    reset();
    stubCanvas();
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  /**
   * Given a managed match
   * When the screen opens
   * Then the live pitch is the first tab, the Formation & Substitutions tab sits beside the
   * list-based controls, and switching to it and back keeps the live pitch
   */
  it("offers_the_tab_beside_the_list_controls_and_a_way_back_to_the_live_pitch", async () => {
    await opened();
    expect(screen.getByRole("tab", { name: tr("match.formationPanel.tabLive") })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") })).toBeInTheDocument();

    openFormationTab();
    expect(
      screen.getByRole("heading", { name: tr("match.formationPanel.title") }),
    ).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: tr("match.selectPlayerOff") })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: pitchLabel() })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: tr("match.formationPanel.backToLive") }));
    expect(screen.getByRole("img", { name: pitchLabel() })).toBeInTheDocument();
  });

  /**
   * Given the engine's view of the manager's team
   * When the tab shows it
   * Then every player the engine has on the pitch is a button named after him, and the bench is the engine's
   */
  it("draws_the_engines_own_lineup_and_bench", async () => {
    await opened();
    openFormationTab();

    expect(screen.getByRole("button", { name: "H Keeper" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "H Back" })).toBeInTheDocument();
    expect(bench().getByRole("button", { name: /H Spare/ })).toBeInTheDocument();
  });

  /**
   * Given a substitution chosen on the pitch
   * When it is confirmed
   * Then the engine receives the Substitute order against the revision it had, the queue shows it only
   * once the engine has queued it, and cancelling sends the engine's own cancel order
   */
  it("queues_a_substitution_through_the_engine_and_cancels_it", async () => {
    await opened();
    openFormationTab();
    const revision = backend.state.revision;

    fireEvent.click(screen.getByRole("button", { name: "H Back" }));
    fireEvent.click(bench().getByRole("button", { name: /H Spare/ }));
    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.formationPanel.confirmNamed", { off: "H Back", on: "H Spare" }),
      }),
    );
    await wall(100);

    expect(backend.state.calls).toContain(`order:${revision}`);
    expect(backend.state.orders).toEqual([
      { Substitute: { player_off_id: "H2", player_on_id: "H9" } },
    ]);
    const queue = screen.getByRole("region", { name: tr("match.queuedSubstitutions") });
    expect(within(queue).getByText("H Back → H Spare")).toBeInTheDocument();
    expect(bench().getByRole("button", { name: /H Spare/ })).toBeDisabled();

    fireEvent.click(
      within(queue).getByRole("button", {
        name: tr("match.removeQueuedSubstitution", { off: "H Back", on: "H Spare" }),
      }),
    );
    await wall(100);
    expect(backend.state.orders).toContainEqual({ CancelSubstitution: { player_off_id: "H2" } });
    expect(screen.getByText(tr("match.formationPanel.queueEmpty"))).toBeInTheDocument();
  });

  /**
   * Given the engine moved on after the screen's view (a stale revision)
   * When the manager confirms a substitution
   * Then the engine's refusal is shown in words and nothing is queued on screen
   */
  it("shows_a_stale_revision_refusal_and_queues_nothing", async () => {
    await opened();
    openFormationTab();
    fireEvent.click(screen.getByRole("button", { name: "H Back" }));
    fireEvent.click(bench().getByRole("button", { name: /H Spare/ }));
    backend.state.revision += 3;

    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.formationPanel.confirmNamed", { off: "H Back", on: "H Spare" }),
      }),
    );
    await wall(100);

    expect(screen.getByRole("alert")).toHaveTextContent(
      tr("match.interactive.rejection.stale_revision"),
    );
    expect(backend.state.orders).toEqual([]);
    expect(screen.getByText(tr("match.formationPanel.queueEmpty"))).toBeInTheDocument();
  });

  /**
   * Given an engine that has no substitutions left
   * When the manager confirms one
   * Then the engine's refusal is shown and the queue stays empty
   */
  it("shows_the_engines_refusal_when_substitutions_are_exhausted", async () => {
    backend.state.rejectNext = "no_substitutions_left";
    await opened();
    openFormationTab();
    fireEvent.click(screen.getByRole("button", { name: "H Back" }));
    fireEvent.click(bench().getByRole("button", { name: /H Spare/ }));

    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.formationPanel.confirmNamed", { off: "H Back", on: "H Spare" }),
      }),
    );
    await wall(100);

    expect(screen.getByRole("alert")).toHaveTextContent(
      tr("match.interactive.rejection.no_substitutions_left"),
    );
    expect(screen.getByText(tr("match.formationPanel.queueEmpty"))).toBeInTheDocument();
  });

  /**
   * Given a match restored from a save with a substitution already queued
   * When the tab is opened
   * Then the queue shows it, the substitute is taken, and the same player cannot be taken off twice
   */
  it("shows_a_queued_substitution_restored_from_a_save", async () => {
    backend.state.pending = [{ off: "H2", on: "H9" }];
    await opened();
    openFormationTab();

    const queue = screen.getByRole("region", { name: tr("match.queuedSubstitutions") });
    expect(within(queue).getByText("H Back → H Spare")).toBeInTheDocument();
    expect(bench().getByRole("button", { name: /H Spare/ })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "H Back" }));
    expect(screen.getByText(tr("match.formationPanel.hint.queued"))).toBeInTheDocument();
  });

  /**
   * Given a formation change from the tab
   * When it is applied
   * Then it goes out as the ordinary Tactics order, and the engine's plan decides what shows
   */
  it("changes_formation_through_the_existing_tactics_order", async () => {
    await opened();
    openFormationTab();
    fireEvent.click(
      screen.getByRole("combobox", {
        name: tr("match.formationPanel.currentFormation", { formation: "4-4-2" }),
      }),
    );
    fireEvent.click(screen.getByRole("option", { name: "4-3-3" }));

    fireEvent.click(
      screen.getByRole("button", { name: tr("match.formationPanel.changeFormation") }),
    );
    await wall(100);

    expect(backend.state.orders).toEqual([{ Tactics: { formation: "4-3-3" } }]);
  });
});
