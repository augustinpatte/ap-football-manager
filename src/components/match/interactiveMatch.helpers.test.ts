import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import { changedTactics, matchMinute, rejectionMessage } from "./interactiveMatch.helpers";
import type { PlanView } from "../../services/interactiveMatchService";

// A translator that shows its key and parameters, so a test sees which words were chosen.
const t = ((key: string, params?: Record<string, string>) =>
  params ? `${key}|${Object.values(params).join("|")}` : key) as unknown as TFunction;

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
