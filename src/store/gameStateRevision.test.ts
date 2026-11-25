import { beforeEach, describe, expect, it, vi } from "vitest";

import { submitTransferCommand } from "../components/transfers/transferSubmission";
import { useGameStore } from "./gameStore";
import type { GameStateData } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const t = (key: string) => key;

/**
 * The smallest game the store can derive a session from. `cash` is the field a stale answer would
 * revert: it is how the tests tell which snapshot the screen is showing.
 */
function snapshot(
  careerToken: number | undefined,
  revision: number | undefined,
  cash: number,
): GameStateData {
  return {
    career_token: careerToken,
    state_revision: revision,
    clock: { current_date: "2026-08-01", start_date: "2026-08-01" },
    manager: { team_id: "team1", nationality: "GB" },
    teams: [{ id: "team1", name: "Club", finance: cash, nationality: "GB" }],
    players: [],
    staff: [],
    messages: [],
    news: [],
  } as unknown as GameStateData;
}

const shownCash = () => useGameStore.getState().gameState?.teams[0]?.finance;

describe("the game store and the order in which answers arrive", () => {
  beforeEach(() => {
    useGameStore.setState({ gameState: null, sessionState: null, hasActiveGame: true });
  });

  /**
   * Given the screen shows revision 7 of a career
   * When the answer that represents revision 6 of the same career arrives late
   * Then the screen keeps revision 7: an older answer never overwrites a newer state
   */
  it("ignores_an_older_snapshot_of_the_same_career", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 7, 700));

    setGameState(snapshot(4, 6, 600));

    expect(shownCash()).toBe(700);
    expect(useGameStore.getState().gameState?.state_revision).toBe(7);
  });

  /**
   * Given the screen shows revision 6
   * When revision 7 arrives
   * Then the screen moves forward
   */
  it("accepts_a_newer_snapshot_of_the_same_career", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 6, 600));

    setGameState(snapshot(4, 7, 700));

    expect(shownCash()).toBe(700);
  });

  /**
   * Given the screen shows revision 7
   * When a local edit of that very state is stored again (same revision)
   * Then it is kept: the screen's own patches carry the revision of the state they patch
   */
  it("accepts_a_snapshot_with_the_revision_already_shown", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 7, 700));

    setGameState(snapshot(4, 7, 750));

    expect(shownCash()).toBe(750);
  });

  /**
   * Given the screen shows revision 50 of career 4
   * When a new career (token 5) starts again at revision 1
   * Then it replaces the old one: revisions are only compared inside one career
   */
  it("never_compares_revisions_of_two_careers", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 50, 5000));

    setGameState(snapshot(5, 1, 100));

    expect(useGameStore.getState().gameState?.career_token).toBe(5);
    expect(shownCash()).toBe(100);
  });

  /**
   * Given the screen shows career 5
   * When a late answer from the replaced career 4 arrives, whatever its revision
   * Then it is dropped: a lower career token is an older career
   */
  it("drops_a_late_answer_from_a_replaced_career", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(5, 1, 100));

    setGameState(snapshot(4, 90, 9000));

    expect(useGameStore.getState().gameState?.career_token).toBe(5);
    expect(shownCash()).toBe(100);
  });

  /**
   * Given a snapshot with no stamp (an older backend, a hand-built fixture)
   * When it arrives before or after stamped ones
   * Then it is accepted: without ordering information nothing can be rejected
   */
  it("accepts_a_snapshot_that_carries_no_stamp", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 7, 700));

    setGameState(snapshot(undefined, undefined, 1));
    expect(shownCash()).toBe(1);

    setGameState(snapshot(4, 3, 3));
    expect(shownCash()).toBe(3);
  });

  /**
   * Given the screen shows a stamped snapshot
   * When one arrives with a career but no revision (a partial stamp)
   * Then it is accepted: with no ordering information for the pair nothing can be refused
   */
  it("accepts_a_snapshot_with_a_career_but_no_revision", () => {
    const { setGameState } = useGameStore.getState();
    setGameState(snapshot(4, 7, 700));

    setGameState(snapshot(4, undefined, 2));

    expect(shownCash()).toBe(2);
  });

  /**
   * Given two successful transfer commands A then B (the backend committed A as revision 6, B as 7)
   * When their answers reach the screen in the order B, A
   * Then the screen ends on B, and both answers were reported as done (neither was an error)
   */
  it("keeps_the_later_commit_when_two_transfer_answers_arrive_reversed", async () => {
    useGameStore.getState().setGameState(snapshot(4, 5, 500));
    const onGameUpdate = (game: GameStateData) => useGameStore.getState().setGameState(game);
    let answerA: (game: GameStateData) => void = () => {};
    let answerB: (game: GameStateData) => void = () => {};
    const sentA = new Promise<GameStateData>((resolve) => {
      answerA = resolve;
    });
    const sentB = new Promise<GameStateData>((resolve) => {
      answerB = resolve;
    });

    const a = submitTransferCommand(() => sentA, { t, onGameUpdate: undefined });
    const b = submitTransferCommand(() => sentB, { t, onGameUpdate: undefined });
    answerB(snapshot(4, 7, 700));
    const resultB = await b;
    if (resultB.kind === "done") onGameUpdate(resultB.value);
    answerA(snapshot(4, 6, 600));
    const resultA = await a;
    if (resultA.kind === "done") onGameUpdate(resultA.value);

    expect([resultA.kind, resultB.kind]).toEqual(["done", "done"]);
    expect(shownCash()).toBe(700);
  });
});
