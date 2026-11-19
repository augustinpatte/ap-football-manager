import { type MutableRefObject, useCallback, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { InteractiveView } from "../../services/interactiveMatchService";
import type { LiveDirector } from "./liveDirector";
import { drawPitch } from "./replayPitch";
import type { PitchPlayer } from "./m12Replay";

interface LivePitchProps {
  director: LiveDirector;
  roster: InteractiveView["roster"];
  /** Shirt number by player id, from the engine's own lineups. */
  shirtOf: Readonly<Record<string, number>>;
  homeName: string;
  awayName: string;
  /** The goal line a goal has just crossed, lit for a few seconds (0 or 105). */
  flashGoalX?: number;
  /** Set by this component to a function that redraws the pitch now (called every animation frame by the screen). */
  drawRef: MutableRefObject<(() => void) | null>;
  /** Changes whenever the screen's state does, so a paused pitch is redrawn when something else changed. */
  version: number;
}

/**
 * The pitch of a live match. It draws the director's picture on a canvas and never holds the
 * match in React state: while the match runs the screen calls the redraw function each animation
 * frame, so the clock label and the lists rerender a few times a second but the pitch moves at
 * the display's rate. When the match is paused nothing animates and nothing is redrawn until
 * something changes.
 */
export default function LivePitch({
  director,
  roster,
  shirtOf,
  homeName,
  awayName,
  flashGoalX,
  drawRef,
  version,
}: LivePitchProps) {
  const { t } = useTranslation();
  const canvas = useRef<HTMLCanvasElement>(null);
  const size = useRef({ width: 0, height: 0, ratio: 1 });

  // A roster index never changes meaning during a match, so the lookup is built once.
  const lookup = useMemo(
    () =>
      roster.map((entry) => ({
        id: entry.id,
        side: entry.side,
        label: shirtOf[entry.id] ?? 0,
      })),
    [roster, shirtOf],
  );

  const draw = useCallback(() => {
    const element = canvas.current;
    const sample = director.sample();
    if (!element || !sample) return;
    const rectangle = element.getBoundingClientRect();
    const width = rectangle.width || 960;
    const height = rectangle.height || 540;
    const ratio = window.devicePixelRatio || 1;
    const context = element.getContext("2d");
    if (!context) return;
    // Resizing a canvas clears it and reallocates its buffer, so only when the size really changed.
    if (
      size.current.width !== width ||
      size.current.height !== height ||
      size.current.ratio !== ratio
    ) {
      element.width = width * ratio;
      element.height = height * ratio;
      size.current = { width, height, ratio };
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    const onPitch: PitchPlayer[] = [];
    for (const player of sample.players) {
      const entry = lookup[player.rosterIndex];
      if (!entry) continue;
      onPitch.push({
        rosterIndex: player.rosterIndex,
        playerId: entry.id,
        side: entry.side,
        label: entry.label,
        x: player.x,
        y: player.y,
        heading: player.heading,
      });
    }
    drawPitch(context, width, height, {
      frame: {
        ball: sample.ball,
        possession: sample.holder === null ? null : (lookup[sample.holder]?.id ?? null),
      },
      onPitch,
      flashGoalX,
      homeAttacksPositiveX: sample.period === "first_half" || sample.period === "extra_time_first",
    });
  }, [director, lookup, flashGoalX]);

  useEffect(() => {
    drawRef.current = draw;
    draw();
    return () => {
      drawRef.current = null;
    };
  }, [draw, drawRef]);

  // Redraw when the screen's state changed while paused (a resize, a new frame from the engine).
  useEffect(() => {
    draw();
  }, [version, draw]);

  useEffect(() => {
    const element = canvas.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => draw());
    observer.observe(element);
    return () => observer.disconnect();
  }, [draw]);

  return (
    <canvas
      ref={canvas}
      role="img"
      aria-label={t("match.liveView.pitchLabel", { home: homeName, away: awayName })}
      className="aspect-[113/76] max-h-[70vh] w-full rounded bg-sunken"
    />
  );
}
