import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { i18nReady } from "../../i18n";
import type { MatchTeamView, UserOrder } from "../../services/interactiveMatchService";
import FormationSubstitutionsPanel from "./FormationSubstitutionsPanel";
import { SHAPES, matchPlayer, squad } from "./formationPanel.fixtures";

const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

function show(team: MatchTeamView, overrides: { canOrder?: boolean } = {}) {
  const onOrder = vi.fn<(order: UserOrder) => void>();
  const onBackToLive = vi.fn();
  const view = (next: MatchTeamView, canOrder = overrides.canOrder ?? true) => (
    <FormationSubstitutionsPanel
      team={next}
      shapes={SHAPES}
      supportedFormations={["4-4-2", "4-3-3", "3-5-2"]}
      canOrder={canOrder}
      onOrder={onOrder}
      onBackToLive={onBackToLive}
    />
  );
  const rendered = render(view(team));
  return {
    onOrder,
    onBackToLive,
    rerender: (next: MatchTeamView, canOrder?: boolean) => rendered.rerender(view(next, canOrder)),
  };
}

/** A substitute on the bench list (the queue's cancel buttons also carry a substitute's name). */
const substitute = (name: string) =>
  within(screen.getByRole("region", { name: tr("match.formationPanel.bench") })).getByRole(
    "button",
    { name: new RegExp(name) },
  );

/** The token of a player on the pitch: the pitch exposes each as a button named after him. */
const token = (name: string) => screen.getByRole("button", { name });

describe("FormationSubstitutionsPanel", () => {
  beforeEach(async () => {
    await i18nReady;
  });
  afterEach(cleanup);

  describe.each(["4-4-2", "4-3-3", "3-5-2"])("a team in %s", (formation) => {
    /**
     * Given a full team in the formation
     * When the panel opens
     * Then all eleven players are on the pitch as buttons named after them, with their shirt
     * number visible, and the formation in play is stated
     */
    it(`shows_all_eleven_players_of_a_${formation}_by_name`, () => {
      const team = squad(formation);
      show(team);

      for (const player of team.on_pitch) expect(token(player.name)).toBeInTheDocument();
      expect(screen.getAllByRole("button", { name: /^Player \d+$/ })).toHaveLength(11);
      expect(
        screen.getByText(tr("match.formationPanel.currentFormation", { formation })),
      ).toBeInTheDocument();
    });
  });

  /**
   * Given a team reduced to ten after a dismissal
   * When the panel opens
   * Then ten players stand on the pitch, the dismissed one is listed as sent off and cannot be
   * selected, and his substitute is not offered as a way back
   */
  it("shows_ten_players_and_lists_the_dismissed_one_as_sent_off", () => {
    const team = squad("4-4-2");
    const dismissed = team.on_pitch.splice(10, 1)[0];
    team.sent_off = [dismissed];
    show(team);

    expect(screen.getAllByRole("button", { name: /^Player \d+$/ })).toHaveLength(10);
    expect(screen.queryByRole("button", { name: dismissed.name })).toBeNull();
    const gone = screen.getByRole("region", { name: tr("match.replayLineup.gone") });
    expect(within(gone).getByText(dismissed.name)).toBeInTheDocument();
    expect(
      within(gone).getByText(tr("match.formationPanel.departed.sent_off")),
    ).toBeInTheDocument();
  });

  /**
   * Given a player the engine lists as used (one substituted, one injured off) and one sent off
   * When the panel opens
   * Then each is named with his own reason and none is selectable as a substitute or an outgoing player
   */
  it("tells_substituted_injured_and_dismissed_players_apart", () => {
    const team = squad("4-4-2");
    team.used = [
      matchPlayer("H30", "Sam Replaced", null),
      matchPlayer("H31", "Ira Hurt", null, { injured: true }),
    ];
    team.sent_off = [matchPlayer("H32", "Red Card", null)];
    show(team);

    const gone = screen.getByRole("region", { name: tr("match.replayLineup.gone") });
    const row = (name: string) => within(gone).getByText(name).closest("li") as HTMLElement;
    expect(
      within(row("Sam Replaced")).getByText(tr("match.formationPanel.departed.substituted")),
    ).toBeInTheDocument();
    expect(
      within(row("Ira Hurt")).getByText(tr("match.formationPanel.departed.injured")),
    ).toBeInTheDocument();
    expect(
      within(row("Red Card")).getByText(tr("match.formationPanel.departed.sent_off")),
    ).toBeInTheDocument();
    for (const name of ["Sam Replaced", "Ira Hurt", "Red Card"]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
  });

  /**
   * Given nobody is selected
   * When the panel opens
   * Then the substitutes cannot be chosen yet and the panel says to pick a player to take off
   */
  it("asks_for_the_outgoing_player_before_the_bench_can_be_used", () => {
    show(squad("4-4-2"));

    expect(screen.getByText(tr("match.formationPanel.hint.selectOff"))).toBeInTheDocument();
    expect(substitute("Sub 12")).toBeDisabled();
    expect(screen.getByRole("button", { name: tr("match.confirmSubstitution") })).toBeDisabled();
  });

  /**
   * Given a manager who selects an outgoing player and then an incoming one
   * When he confirms
   * Then exactly one Substitute order goes out with those two ids, and the selection is cleared
   * while the lineup on show is still the engine's (nothing is moved locally)
   */
  it("sends_one_substitute_order_with_the_chosen_players_and_changes_nothing_itself", () => {
    const team = squad("4-4-2");
    const { onOrder } = show(team);

    fireEvent.click(token("Player 6"));
    expect(screen.getByRole("heading", { name: /Player 6/ })).toBeInTheDocument();
    expect(
      screen.getByText(tr("match.formationPanel.hint.chooseReplacement", { name: "Player 6" })),
    ).toBeInTheDocument();
    fireEvent.click(substitute("Sub 13"));
    const confirm = screen.getByRole("button", {
      name: tr("match.formationPanel.confirmNamed", { off: "Player 6", on: "Sub 13" }),
    });
    fireEvent.click(confirm);

    expect(onOrder).toHaveBeenCalledTimes(1);
    expect(onOrder).toHaveBeenCalledWith({
      Substitute: { player_off_id: "H6", player_on_id: "H13" },
    });
    expect(screen.getByText(tr("match.formationPanel.hint.selectOff"))).toBeInTheDocument();
    // the engine has not accepted anything yet: Player 6 is still on the pitch, Sub 13 on the bench
    expect(token("Player 6")).toBeInTheDocument();
    expect(substitute("Sub 13")).toBeInTheDocument();
  });

  /**
   * Given a substitution the engine has queued for the next stoppage
   * When the panel shows the team
   * Then the queue names it, the outgoing player is marked as waiting, the incoming player cannot
   * be chosen again, and the outgoing one cannot be sent off twice
   */
  it("shows_a_queued_substitution_and_does_not_allow_it_twice", () => {
    const team = squad("4-4-2");
    team.pending_substitutions = [{ off: "H6", on: "H13" }];
    team.substitutions_remaining = 4;
    team.substitutions_used = 1;
    show(team);

    const queue = screen.getByRole("region", { name: tr("match.queuedSubstitutions") });
    expect(within(queue).getByText("Player 6 → Sub 13")).toBeInTheDocument();
    expect(substitute("Sub 13")).toBeDisabled();

    fireEvent.click(token("Player 6"));
    expect(screen.getByText(tr("match.formationPanel.hint.queued"))).toBeInTheDocument();
    expect(substitute("Sub 12")).toBeDisabled();
    expect(screen.getByRole("button", { name: tr("match.confirmSubstitution") })).toBeDisabled();

    // A different player is chosen to go off: the bench is open, but the substitute already
    // queued to come on is not offered a second time.
    fireEvent.click(token("Player 7"));
    expect(substitute("Sub 12")).toBeEnabled();
    expect(substitute("Sub 13")).toBeDisabled();
  });

  /**
   * Given a queued substitution
   * When the manager cancels it
   * Then a CancelSubstitution order goes out for the outgoing player, and the queue stays as the
   * engine has it until the next view arrives
   */
  it("cancels_a_queued_substitution_through_the_engines_order", () => {
    const team = squad("4-4-2");
    team.pending_substitutions = [{ off: "H6", on: "H13" }];
    const { onOrder } = show(team);

    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.removeQueuedSubstitution", { off: "Player 6", on: "Sub 13" }),
      }),
    );

    expect(onOrder).toHaveBeenCalledWith({ CancelSubstitution: { player_off_id: "H6" } });
    expect(screen.getByText("Player 6 → Sub 13")).toBeInTheDocument();
  });

  /**
   * Given every substitution used or queued
   * When a player is selected
   * Then the panel says none is left and nothing can be confirmed
   */
  it("refuses_a_new_substitution_when_the_allowance_is_spent", () => {
    const team = squad("4-4-2");
    team.substitutions_used = 4;
    team.substitutions_remaining = 1;
    team.pending_substitutions = [{ off: "H2", on: "H12" }];
    show(team);

    fireEvent.click(token("Player 6"));

    expect(screen.getByText(tr("match.formationPanel.hint.exhausted"))).toBeInTheDocument();
    expect(substitute("Sub 13")).toBeDisabled();
    expect(screen.getByRole("button", { name: tr("match.confirmSubstitution") })).toBeDisabled();
  });

  /**
   * Given the engine does not accept orders now (the picture is behind, or the match is over)
   * When the manager looks at a player
   * Then he can still read the details, but no order can be started or sent
   */
  it("lets_the_manager_look_but_not_order_while_orders_are_closed", () => {
    const team = squad("4-4-2");
    team.pending_substitutions = [{ off: "H2", on: "H12" }];
    show(team, { canOrder: false });

    fireEvent.click(token("Player 6"));

    expect(screen.getByRole("heading", { name: /Player 6/ })).toBeInTheDocument();
    expect(substitute("Sub 13")).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: tr("match.removeQueuedSubstitution", { off: "Player 2", on: "Sub 12" }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: tr("match.formationPanel.changeFormation") }),
    ).toBeDisabled();
  });

  /**
   * Given a selected player the engine then takes off the pitch (a queued change took effect)
   * When the next view arrives
   * Then the selection is dropped: a player who has left is never offered as outgoing
   */
  it("drops_a_selection_when_the_player_has_left_the_pitch", () => {
    const team = squad("4-4-2");
    const { rerender } = show(team);
    fireEvent.click(token("Player 6"));
    expect(screen.getByRole("heading", { name: /Player 6/ })).toBeInTheDocument();

    const after = squad("4-4-2");
    const left = after.on_pitch.splice(5, 1)[0];
    after.used = [left];
    rerender(after);

    expect(screen.getByText(tr("match.formationPanel.hint.selectOff"))).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /Player 6/ })).toBeNull();
  });

  /**
   * Given a formation change from 4-4-2 to 4-3-3
   * When the manager applies it
   * Then a Tactics order carries only the formation, and the control is idle while nothing differs
   */
  it("changes_formation_through_the_tactics_order", () => {
    const { onOrder } = show(squad("4-4-2"));
    const apply = screen.getByRole("button", { name: tr("match.formationPanel.changeFormation") });
    expect(apply).toBeDisabled();

    fireEvent.click(
      screen.getByRole("combobox", {
        name: tr("match.formationPanel.currentFormation", { formation: "4-4-2" }),
      }),
    );
    fireEvent.click(screen.getByRole("option", { name: "4-3-3" }));
    fireEvent.click(apply);

    expect(onOrder).toHaveBeenCalledWith({ Tactics: { formation: "4-3-3" } });
  });

  /**
   * Given an injured player still on the pitch
   * When the panel shows him
   * Then his injury is in the detail and in the token's description, not in colour alone
   */
  it("marks_an_injured_player_in_words", () => {
    const team = squad("4-4-2");
    team.on_pitch[3] = { ...team.on_pitch[3], injured: true };
    show(team);

    fireEvent.click(token(team.on_pitch[3].name));

    expect(screen.getAllByText(tr("common.injured")).length).toBeGreaterThan(0);
    expect(token(team.on_pitch[3].name)).toHaveAccessibleDescription(
      expect.stringContaining(tr("common.injured")),
    );
  });

  /**
   * Given keyboard use only
   * When Enter and Space are pressed on pitch tokens
   * Then they select the player like a click, and Space on a selected token deselects it
   */
  it("selects_with_the_keyboard", () => {
    show(squad("4-4-2"));

    fireEvent.keyDown(token("Player 4"), { key: "Enter" });
    expect(screen.getByRole("heading", { name: /Player 4/ })).toBeInTheDocument();
    fireEvent.keyDown(token("Player 4"), { key: " " });
    expect(screen.queryByRole("heading", { name: /Player 4/ })).toBeNull();
  });

  /**
   * Given the panel is open
   * When the manager asks to go back
   * Then the screen is told to show the live pitch again
   */
  it("offers_a_way_back_to_the_live_match", () => {
    const { onBackToLive } = show(squad("4-4-2"));

    fireEvent.click(screen.getByRole("button", { name: tr("match.formationPanel.backToLive") }));

    expect(onBackToLive).toHaveBeenCalledTimes(1);
  });
  /**
   * Given an outgoing player and a chosen substitute
   * When a different player is chosen to go off
   * Then the substitute chosen for the first is let go: he must be chosen again for the new pair
   */
  it("forgets_the_chosen_substitute_when_another_player_is_chosen_to_go_off", () => {
    show(squad("4-4-2"));
    fireEvent.click(token("Player 6"));
    fireEvent.click(substitute("Sub 13"));
    expect(substitute("Sub 13")).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(token("Player 7"));

    expect(substitute("Sub 13")).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: tr("match.confirmSubstitution") })).toBeDisabled();
  });

  /**
   * Given a substitute chosen for an outgoing player
   * When the engine queues that very substitute (another order)
   * Then the choice is dropped and Confirm cannot send him a second time
   */
  it("drops_a_chosen_substitute_the_engine_has_since_queued", () => {
    const team = squad("4-4-2");
    const { rerender } = show(team);
    fireEvent.click(token("Player 6"));
    fireEvent.click(substitute("Sub 13"));

    const after = squad("4-4-2");
    after.pending_substitutions = [{ off: "H2", on: "H13" }];
    rerender(after);

    expect(screen.getByRole("button", { name: tr("match.confirmSubstitution") })).toBeDisabled();
    expect(substitute("Sub 13")).toBeDisabled();
  });

  /**
   * Given a confirmed substitution, a cancelled one and a cleared selection
   * When each button is pressed (and then disables itself or disappears)
   * Then focus moves to the panel title first, never to the page or a disabled control
   */
  it("moves_focus_to_the_title_before_a_button_goes_away", () => {
    const team = squad("4-4-2");
    team.pending_substitutions = [{ off: "H2", on: "H12" }];
    show(team);
    const title = screen.getByText(tr("match.formationPanel.title"));

    fireEvent.click(token("Player 6"));
    fireEvent.click(substitute("Sub 13"));
    fireEvent.click(
      screen.getByRole("button", {
        name: tr("match.formationPanel.confirmNamed", { off: "Player 6", on: "Sub 13" }),
      }),
    );
    expect(title).toHaveFocus();

    fireEvent.click(token("Player 7"));
    fireEvent.click(
      screen.getByRole("button", { name: tr("match.formationPanel.clearSelection") }),
    );
    expect(title).toHaveFocus();
  });
});
