import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import {
  changedTactics,
  eventLine,
  matchMinute,
  rejectionMessage,
} from "./interactiveMatch.helpers";
import type { MatchKeyEvent, PlanView } from "../../services/interactiveMatchService";

// A translator that shows its key and parameters, so a test sees which words were chosen.
const t = ((key: string, params?: Record<string, string>) =>
  params ? `${key}|${Object.values(params).join("|")}` : key) as unknown as TFunction;

const ctx = {
  names: { p1: "Ada", p2: "Bo" },
  teamNames: { home: "Reds", away: "Blues" },
  regulationMinutes: 90,
  t,
};

const plan: PlanView = {
  style: "Balanced",
  pressing: "Medium",
  tempo: "Direct",
  formation: "4-4-2",
};

describe("matchMinute", () => {
  /** Given an engine clock of 45:30, then the match is in minute 45. */
  it("reads the minute from the engine's clock", () => {
    expect(matchMinute(45 * 60_000 + 30_000)).toBe(45);
  });
});

describe("eventLine", () => {
  const goal: MatchKeyEvent = {
    id: 1,
    t_ms: 12 * 60_000,
    type: "goal",
    side: "home",
    scorer: "p1",
  };

  /** Given a goal by the home side, then the line names the minute, the team and the scorer. */
  it("describes a goal with the replay's own words", () => {
    expect(eventLine(goal, ctx)).toBe("12' Reds: match.replayFeed.goal|Ada");
  });

  /** Given each card colour, then each has its own wording. */
  it("distinguishes yellow, second yellow and red", () => {
    const card = (colour: MatchKeyEvent["colour"]): MatchKeyEvent => ({
      id: 2,
      t_ms: 60_000,
      type: "card",
      side: "away",
      player: "p2",
      colour,
    });
    expect(eventLine(card("yellow"), ctx)).toContain("match.replayFeed.yellowCard");
    expect(eventLine(card("second_yellow"), ctx)).toContain("match.replayFeed.secondYellow");
    expect(eventLine(card("red"), ctx)).toContain("match.replayFeed.redCard");
  });

  /** Given a substitution, then the incoming player is the subject and the outgoing the other. */
  it("puts the incoming player first in a substitution", () => {
    const sub: MatchKeyEvent = {
      id: 3,
      t_ms: 70 * 60_000,
      type: "substitution",
      side: "home",
      player_off: "p1",
      player_on: "p2",
    };
    expect(eventLine(sub, ctx)).toBe("70' Reds: match.replayFeed.substitution|Bo|Ada");
  });

  /** Given football detail nobody reads, then it is not a line. */
  it("drops events a person does not need", () => {
    expect(eventLine({ id: 4, t_ms: 1000, type: "pass_attempt" }, ctx)).toBeNull();
  });

  /** Given the end of the first half and the end of regulation, then only the first is a line. */
  it("labels the half-time break but leaves full time to the match end", () => {
    expect(eventLine({ id: 5, t_ms: 46 * 60_000, type: "period_end" }, ctx)).toContain("halfTime");
    expect(eventLine({ id: 6, t_ms: 91 * 60_000, type: "period_end" }, ctx)).toBeNull();
    expect(eventLine({ id: 7, t_ms: 91 * 60_000, type: "match_end" }, ctx)).toContain("fullTime");
  });
});

describe("rejectionMessage", () => {
  /** Given a code the screen knows, then its own text; an unknown code, the generic one. */
  it("falls back to the generic refusal for a code it does not know", () => {
    expect(rejectionMessage("no_window_left", t)).toBe(
      "match.interactive.rejection.no_window_left",
    );
    expect(rejectionMessage("something_new", t)).toBe("match.interactive.rejection.other");
  });
});

describe("changedTactics", () => {
  /** Given a draft equal to the plan, then there is nothing to send. */
  it("is null when nothing moved", () => {
    expect(changedTactics(plan, { ...plan }, ["4-4-2", "4-3-3"])).toBeNull();
  });

  /** Given only pressing and formation moved, then only those two are sent. */
  it("sends only what moved", () => {
    expect(
      changedTactics(plan, { ...plan, pressing: "Aggressive", formation: "4-3-3" }, [
        "4-4-2",
        "4-3-3",
      ]),
    ).toEqual({ pressing: "Aggressive", formation: "4-3-3" });
  });

  /** Given a formation the engine cannot build, then it is never sent. */
  it("never offers an unsupported formation to the engine", () => {
    expect(changedTactics(plan, { ...plan, formation: "2-3-5" }, ["4-4-2"])).toBeNull();
  });
});
