// The live director on the engine's real payloads around half time, a manager's orders and full time:
// `liveMatch.break.generated.json` was written by the backend test `write_break_fixture` from a real
// M12 match (frames trimmed to the last few of each stretch, everything else exactly as the IPC
// serialises it). The kickoff fixture covers the first three minutes only; the shapes that matter
// for a break (the clock after added time, a queued substitution, the second period's clock, the
// finished flag) first occur here.

import { describe, expect, it } from "vitest";
import type {
  AdvanceSnapshot,
  InteractiveSnapshot,
  OrderSnapshot,
} from "../../services/interactiveMatchService";
import fixture from "./liveMatch.break.generated.json";
import { LiveDirector } from "./liveDirector";

const halftime = fixture.halftime as unknown as AdvanceSnapshot;
const tactics = fixture.tactics as unknown as OrderSnapshot;
const substitution = fixture.substitution as unknown as OrderSnapshot;
const secondHalf = fixture.second_half as unknown as AdvanceSnapshot;
const fullTime = fixture.full_time as unknown as AdvanceSnapshot;

/** A director that has been handed what a screen taken over at half time would have. */
function atHalfTime(): LiveDirector {
  const director = new LiveDirector();
  director.ingestAdvance(halftime);
  director.skipToHead();
  return director;
}

describe("the live director on real half-time, order and full-time payloads", () => {
  /** Given the real stop at half time, Then the director is at the break, with orders open and the stopwatch in first-half added time. */
  it("reads a real half-time stop", () => {
    const snapshot = atHalfTime().getSnapshot();
    expect(snapshot.status).toBe("break");
    expect(snapshot.canOrder).toBe(true);
    expect(snapshot.period).toBe("first_half");
    expect(snapshot.view?.session.phase).toBe("half_time");
    // nominal time is 45 minutes; added time, if any, reads "45+n'", never "46'"
    expect(snapshot.clock).toMatch(/^45(\+\d+)?'$/);
    expect(snapshot.events.length).toBeGreaterThan(0);
    expect(snapshot.rejectedFrames).toBe(0);
  });

  /** Given the real replies to a tactics order and to a substitution at half time, Then they are accepted, carry the engine's new revision, and the substitution is waiting in the view. */
  it("takes the engine's real answers to orders at half time", () => {
    const director = atHalfTime();
    const before = director.getSnapshot().view?.session.revision ?? 0;
    expect(tactics.receipt.accepted).toBe(true);
    expect(tactics.receipt.revision).toBeGreaterThan(before);
    director.ingestSnapshot(tactics.snapshot);
    expect(director.getSnapshot().view?.home_plan.style).toBe("Attacking");

    expect(substitution.receipt.accepted).toBe(true);
    expect(substitution.receipt.revision).toBeGreaterThan(tactics.receipt.revision);
    director.ingestSnapshot(substitution.snapshot as InteractiveSnapshot);
    const waiting = director.getSnapshot().view?.session.home.pending_substitutions ?? [];
    expect(waiting).toHaveLength(1);
    // an order does not move the picture or the engine
    expect(director.getSnapshot().status).toBe("break");
    expect(director.getSnapshot().playheadMs).toBe(director.store.headMs);
  });

  /** Given the second half is asked for, Then the picture runs on from the break through the engine's frames, and the stopwatch of both halves reads in its own period. */
  it("plays on into the second half and reads each half's clock", () => {
    const director = atHalfTime();
    director.ingestSnapshot(substitution.snapshot as InteractiveSnapshot);
    const breakMs = director.store.headMs;
    director.play();
    director.ingestAdvance(secondHalf);
    director.tick(16);
    const snapshot = director.getSnapshot();
    expect(snapshot.status).toBe("playing");
    expect(director.store.headMs).toBeGreaterThan(breakMs);
    expect(director.store.rejected).toBe(0);
    // the first half's last minute and the second half's first minute, by the engine's own clocks
    // (the break instant itself is the second half's kickoff, so the last first-half frame is read)
    expect(director.labelAt(breakMs - 500)).toMatch(/^45(\+\d+)?'$/);
    expect(director.labelAt(director.store.headMs)).toMatch(/^4[6-9]'$/);
  });

  /** Given the real full-time reply, Then the match is finished at its head, orders are closed, and the score on the pitch is the view's score. */
  it("reads a real full time", () => {
    const director = atHalfTime();
    director.ingestAdvance(secondHalf);
    director.ingestAdvance(fullTime);
    director.skipToHead();
    const snapshot = director.getSnapshot();
    expect(fullTime.stop).toBe("finished");
    expect(snapshot.status).toBe("finished");
    expect(snapshot.canOrder).toBe(false);
    expect(snapshot.clock).toMatch(/^90(\+\d+)?'$/);
    const session = fullTime.snapshot.view.session;
    expect(snapshot.score).toEqual({ home: session.score_home, away: session.score_away });
  });
});
