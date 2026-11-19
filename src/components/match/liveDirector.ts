// The director of a live 2D match: it holds the frames and events the engine has produced, moves the
// picture through them at the chosen speed and highlight mode, and says what the manager may do.
//
// The engine runs a little ahead of the picture (a few seconds in the full match, about a minute in
// the highlight modes) and the picture never gets ahead of the engine. That is the whole honesty
// of the screen: an order is only accepted when the picture has caught up with the engine, so the
// manager always acts on the moment he is looking at, and an accepted order only ever changes
// frames that have not been produced yet.
//
// Plain TypeScript with no React in it, so the behaviour is tested by driving it with a fake
// engine and a fake clock.

import type {
  AdvanceSnapshot,
  ClockView,
  InteractiveSnapshot,
  InteractiveView,
  MatchKeyEvent,
  StopReason,
} from "../../services/interactiveMatchService";
import type { StatsSource } from "./replayDerived";
import {
  FrameStore,
  type HighlightMode,
  type PlaybackSpeed,
  type SampledFrame,
  type Span,
  clockAt,
  clockLabel,
  highlightSpans,
  paceStep,
  sampleAt,
} from "./livePlayback";

export type DirectorStatus = "loading" | "playing" | "pausing" | "paused" | "break" | "finished";

/** Match seconds per wall second while the picture catches up with the engine for an order. */
const CATCH_UP_RATE = 32;
/** How often the screen is told the clock moved while the picture runs (the pitch itself is redrawn every frame). */
const NOTIFY_EVERY_MS = 250;
/** The picture is "at the head" when it is within a frame of it. */
const AT_HEAD_TOLERANCE_MS = 1;

export interface DirectorSnapshot {
  /** Changes on every notification; a cheap key for memoising what is derived from the rest. */
  version: number;
  status: DirectorStatus;
  mode: HighlightMode;
  speed: PlaybackSpeed;
  playheadMs: number;
  /** The last instant the engine has produced. */
  headMs: number;
  score: { home: number; away: number };
  /** The stopwatch reading at the picture's instant, `45+2'` in added time. */
  clock: string;
  frameState: SampledFrame["state"] | null;
  stoppage: string | null;
  period: SampledFrame["period"] | null;
  /** The team that last had the ball, at the picture's instant. */
  possessionSide: SampledFrame["possessionSide"];
  /** The engine's events up to the picture's instant, in order. */
  events: readonly MatchKeyEvent[];
  /** The engine's view of the match as of its head (lineups, bench, conditions, plans). */
  view: InteractiveView | null;
  /** The picture is at the engine's head and the match is not running: orders may be given. */
  canOrder: boolean;
  /** The picture is waiting for the engine to produce more. */
  buffering: boolean;
  /** Frames the screen refused as malformed; nonzero means the engine's data was incomplete. */
  rejectedFrames: number;
}

export class LiveDirector {
  readonly store = new FrameStore();
  private events: MatchKeyEvent[] = [];
  private clocks: ClockView[] = [];
  /** Who held the ball in each frame, for the possession figure: the same frames, by player id. */
  private possession: { t_ms: number; possession: string | null }[] = [];
  private homeIds: ReadonlySet<string> = new Set();
  private view: InteractiveView | null = null;
  private cursor = 0;
  private lastStop: StopReason | null = null;
  private streamEnded = false;
  private spans: Span[] | null = null;
  private playhead = 0;
  private mode: HighlightMode = "full";
  private speed: PlaybackSpeed = 2;
  private wantPlaying = false;
  private catchingUp = false;
  private waiting = false;
  private fetching = false;
  private sinceNotifyMs = 0;
  private version = 0;
  private snapshot: DirectorSnapshot;
  private readonly listeners = new Set<() => void>();

  constructor() {
    this.snapshot = this.build();
  }

  // ---- subscription (useSyncExternalStore)

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly getSnapshot = (): DirectorSnapshot => this.snapshot;

  private notify(): void {
    this.sinceNotifyMs = 0;
    this.snapshot = this.build();
    for (const listener of this.listeners) listener();
  }

  // ---- what the engine produced

  /** The event cursor to ask the engine from next time. */
  get eventCursor(): number {
    return this.cursor;
  }

  private takeView(snapshot: InteractiveSnapshot): void {
    this.view = snapshot.view;
    this.cursor = snapshot.next_event_cursor;
    const lastId = this.events[this.events.length - 1]?.id ?? -1;
    for (const event of snapshot.events) {
      if (event.id > lastId) this.events.push(event);
    }
    const clock = snapshot.view.clock;
    const known = this.clocks.findIndex((c) => c.period_start_ms === clock.period_start_ms);
    if (known >= 0) this.clocks[known] = clock;
    else this.clocks.push(clock);
  }

  /** Takes a reply that did not advance the match (an order, a read, the assistant switch). */
  ingestSnapshot(snapshot: InteractiveSnapshot): void {
    this.takeView(snapshot);
    this.notify();
  }

  /** Takes the stretch of play the engine just produced. */
  ingestAdvance(reply: AdvanceSnapshot): void {
    const before = this.store.length;
    this.store.append(reply.frames);
    this.takeView(reply.snapshot);
    const roster = reply.snapshot.view.roster;
    if (this.homeIds.size === 0) {
      this.homeIds = new Set(roster.filter((r) => r.side === "home").map((r) => r.id));
    }
    for (let i = before; i < this.store.length; i += 1) {
      const frame = this.store.frames[i];
      this.possession.push({
        t_ms: frame.t_ms,
        possession: frame.holder === null ? null : (roster[frame.holder]?.id ?? null),
      });
    }
    this.lastStop = reply.stop;
    this.streamEnded = reply.stop === "period_break" || reply.stop === "finished";
    this.refreshSpans();
    if (this.store.length > 0 && this.playhead < this.store.startMs) {
      this.playhead = this.store.startMs;
    }
    this.notify();
  }

  private refreshSpans(): void {
    this.spans = highlightSpans(this.mode, this.events, this.store);
  }

  // ---- what the manager asks for

  setMode(mode: HighlightMode): void {
    if (mode === this.mode) return;
    this.mode = mode;
    this.refreshSpans();
    this.notify();
  }

  setSpeed(speed: PlaybackSpeed): void {
    this.speed = speed;
    this.notify();
  }

  play(): void {
    if (this.lastStop === "finished" || this.store.length === 0) return;
    if (this.lastStop === "period_break") {
      // Only once the picture has reached the break may the next half be asked for.
      if (!this.atHead()) return;
      this.streamEnded = false;
    }
    this.wantPlaying = true;
    this.catchingUp = false;
    this.notify();
  }

  /** Stops asking for more play; the picture runs on to the engine's head so an order can be given there. */
  pause(): void {
    this.wantPlaying = false;
    this.catchingUp = !this.atHead();
    this.notify();
  }

  /** After the engine was played on without the picture (skip ahead): show where it got to. */
  skipToHead(): void {
    this.playhead = this.store.headMs;
    this.wantPlaying = false;
    this.catchingUp = false;
    this.notify();
  }

  /** A failed call: stop asking for more and let the picture settle at what exists. */
  halt(): void {
    this.pause();
  }

  // ---- time

  private atHead(): boolean {
    return this.playhead >= this.store.headMs - AT_HEAD_TOLERANCE_MS;
  }

  /** Moves the picture by `wallMs` of wall-clock time. Cheap enough to call every animation frame. */
  tick(wallMs: number): void {
    const running = this.wantPlaying || this.catchingUp;
    if (!running || this.store.length === 0) return;
    const result = paceStep({
      t: this.playhead,
      wallMs,
      rate: this.catchingUp ? CATCH_UP_RATE : this.speed,
      spans: this.spans,
      headMs: this.store.headMs,
      streamEnded: this.streamEnded,
    });
    this.playhead = result.t;
    this.waiting = result.waiting;
    if (this.catchingUp && (result.waiting || this.atHead())) {
      // Nothing worth showing is left before the head: go to it so the order is given there.
      this.playhead = this.store.headMs;
      this.catchingUp = false;
      this.notify();
      return;
    }
    if (this.wantPlaying && this.streamEnded && this.atHead()) {
      // The half (or the match) is over and shown: stop asking for play.
      this.wantPlaying = false;
      this.notify();
      return;
    }
    this.sinceNotifyMs += wallMs;
    if (this.sinceNotifyMs >= NOTIFY_EVERY_MS) this.notify();
  }

  /** Whether the engine should be asked for another stretch now. */
  needsMore(): boolean {
    if (!this.wantPlaying || this.streamEnded || this.fetching) return false;
    if (this.lastStop === "finished") return false;
    return this.store.headMs - this.playhead < this.lookaheadMs();
  }

  /** How many simulated seconds to ask for: more at a higher speed, so a request is not made every frame. */
  chunkSeconds(): number {
    if (this.mode !== "full") return 60;
    return Math.min(60, Math.max(10, this.speed * 5));
  }

  private lookaheadMs(): number {
    return this.mode !== "full" ? 90_000 : this.chunkSeconds() * 800;
  }

  /**
   * Asks the engine for the next stretch if one is due, through `advance` (which reports a failure
   * itself and returns `null`). At most one request is in flight.
   */
  async pump(advance: (seconds: number) => Promise<AdvanceSnapshot | null>): Promise<void> {
    if (!this.needsMore()) return;
    this.fetching = true;
    try {
      const reply = await advance(this.chunkSeconds());
      if (reply) this.ingestAdvance(reply);
      else this.halt();
    } finally {
      this.fetching = false;
    }
  }

  // ---- reading

  /** What the statistics are counted from, and which players are the home side's. */
  statsSource(): { source: StatsSource; homeIds: ReadonlySet<string> } {
    const roster = this.view?.roster.map((r) => r.id) ?? [];
    return {
      source: { events: this.events, roster, frames: this.possession },
      homeIds: this.homeIds,
    };
  }

  /** The picture's instant now; the snapshot's copy is only refreshed a few times a second. */
  get positionMs(): number {
    return this.playhead;
  }

  /** The world at the picture's instant, for drawing. */
  sample(): SampledFrame | null {
    return sampleAt(this.store, this.playhead);
  }

  /** The stopwatch reading for an instant of the match, in the period it falls in. */
  labelAt(t: number): string {
    const clock = clockAt(this.clocks, t);
    return clock ? clockLabel(t, clock) : "";
  }

  private status(): DirectorStatus {
    if (this.store.length === 0) return "loading";
    const atHead = this.atHead();
    if (this.lastStop === "finished" && atHead) return "finished";
    if (this.lastStop === "period_break" && atHead && !this.wantPlaying) return "break";
    if (this.wantPlaying) return "playing";
    if (this.catchingUp) return "pausing";
    return "paused";
  }

  private build(): DirectorSnapshot {
    this.version += 1;
    const sample = this.store.length > 0 ? sampleAt(this.store, this.playhead) : null;
    const floor = this.store.frames[this.store.indexAtOrBefore(this.playhead)]?.t_ms ?? -1;
    const status = this.status();
    const visible = this.events.filter((event) => event.t_ms <= floor);
    return {
      version: this.version,
      status,
      mode: this.mode,
      speed: this.speed,
      playheadMs: this.playhead,
      headMs: this.store.headMs,
      score: sample?.score ?? { home: 0, away: 0 },
      clock: sample ? this.labelAt(this.playhead) : "",
      frameState: sample?.state ?? null,
      stoppage: sample?.stoppage ?? null,
      period: sample?.period ?? null,
      possessionSide: sample?.possessionSide ?? null,
      events: visible,
      view: this.view,
      canOrder:
        this.view !== null &&
        this.view.user_side !== null &&
        (status === "paused" || status === "break") &&
        !this.view.session.finished,
      buffering: this.wantPlaying && this.waiting && !this.streamEnded,
      rejectedFrames: this.store.rejected,
    };
  }
}
