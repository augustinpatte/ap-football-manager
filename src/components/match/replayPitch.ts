// Draws one frame of an M12 replay on a canvas. The pitch is 105 x 68 m, origin bottom-left, as the
// replay contract fixes it; everything drawn is a recorded position (players, ball) or a fixed
// marking of the field. A canvas cannot take a Tailwind class, so the turf and kit colours are
// named here once rather than scattered as literals through the drawing code.

import type { PitchPlayer, ReplayFrame } from "./m12Replay";

const PITCH_LENGTH = 105;
const PITCH_WIDTH = 68;
/** Room round the touchlines for the goals (2.4 m deep) and a margin. */
const MARGIN = 4;
const GOAL_WIDTH = 7.32;
const GOAL_DEPTH = 2.4;

const COLOURS = {
  surround: "#16301f",
  stripeLight: "#2f7340",
  stripeDark: "#2b6a3b",
  line: "#e9f3e5",
  net: "rgba(233, 243, 229, 0.45)",
  post: "#ffffff",
  home: "#2f7bf6",
  away: "#f2744a",
  label: "#ffffff",
  outline: "#0b1b10",
  holder: "#ffe14d",
  ball: "#ffffff",
  ballEdge: "#111111",
  flash: "rgba(255, 225, 77, 0.55)",
} as const;

export type PitchScene = {
  frame: ReplayFrame;
  onPitch: PitchPlayer[];
  /** The goal line a goal has just crossed (0 or 105), to highlight that goal. */
  flashGoalX?: number;
};

/** Pixel size of a player: readable at a glance, never smaller than a legible number. */
export function playerRadius(scale: number): number {
  return Math.max(9, scale * 1.55);
}

export function drawPitch(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  scene: PitchScene,
): void {
  const { frame, onPitch, flashGoalX } = scene;
  const scale = Math.min(width / (PITCH_LENGTH + 2 * MARGIN), height / (PITCH_WIDTH + 2 * MARGIN));
  const originX = (width - PITCH_LENGTH * scale) / 2;
  const originY = (height - PITCH_WIDTH * scale) / 2;
  const x = (value: number) => originX + value * scale;
  const y = (value: number) => originY + (PITCH_WIDTH - value) * scale;

  context.fillStyle = COLOURS.surround;
  context.fillRect(0, 0, width, height);
  for (let stripe = 0; stripe < 14; stripe += 1) {
    context.fillStyle = stripe % 2 ? COLOURS.stripeDark : COLOURS.stripeLight;
    context.fillRect(x(stripe * 7.5), y(PITCH_WIDTH), 7.6 * scale, PITCH_WIDTH * scale);
  }

  drawMarkings(context, scale, x, y);
  drawGoal(context, scale, x, y, 0, -1, flashGoalX === 0);
  drawGoal(context, scale, x, y, PITCH_LENGTH, 1, flashGoalX === PITCH_LENGTH);

  const radius = playerRadius(scale);
  for (const player of onPitch) {
    const holds = frame.possession === player.playerId;
    context.beginPath();
    context.arc(x(player.x), y(player.y), radius, 0, Math.PI * 2);
    context.fillStyle = player.side === "home" ? COLOURS.home : COLOURS.away;
    context.fill();
    context.lineWidth = holds ? Math.max(3, radius * 0.3) : Math.max(1.5, radius * 0.12);
    context.strokeStyle = holds ? COLOURS.holder : COLOURS.outline;
    context.stroke();
    context.fillStyle = COLOURS.label;
    context.font = `bold ${Math.round(radius * 1.05)}px sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(String(player.label), x(player.x), y(player.y) + 0.5);
  }

  // The ball is lifted by its height so a lofted pass reads as one.
  const lift = Math.max(0, frame.ball.z) * scale * 0.25;
  const ballRadius = Math.max(5, scale * 0.65);
  context.fillStyle = "rgba(0, 0, 0, 0.3)";
  context.beginPath();
  context.ellipse(
    x(frame.ball.x),
    y(frame.ball.y),
    ballRadius,
    ballRadius * 0.5,
    0,
    0,
    Math.PI * 2,
  );
  context.fill();
  context.fillStyle = COLOURS.ball;
  context.beginPath();
  context.arc(x(frame.ball.x), y(frame.ball.y) - lift, ballRadius, 0, Math.PI * 2);
  context.fill();
  context.lineWidth = 1.5;
  context.strokeStyle = COLOURS.ballEdge;
  context.stroke();
}

type ToPixels = (value: number) => number;

function drawMarkings(
  context: CanvasRenderingContext2D,
  scale: number,
  x: ToPixels,
  y: ToPixels,
): void {
  context.strokeStyle = COLOURS.line;
  context.lineWidth = Math.max(1.5, scale * 0.16);
  context.strokeRect(x(0), y(PITCH_WIDTH), PITCH_LENGTH * scale, PITCH_WIDTH * scale);
  const line = (x1: number, y1: number, x2: number, y2: number) => {
    context.beginPath();
    context.moveTo(x(x1), y(y1));
    context.lineTo(x(x2), y(y2));
    context.stroke();
  };
  const arc = (cx: number, cy: number, r: number, from: number, to: number) => {
    context.beginPath();
    context.arc(x(cx), y(cy), r * scale, from, to);
    context.stroke();
  };
  const spot = (cx: number, cy: number) => {
    context.beginPath();
    context.arc(x(cx), y(cy), Math.max(2, scale * 0.25), 0, Math.PI * 2);
    context.fillStyle = COLOURS.line;
    context.fill();
  };
  const middle = PITCH_WIDTH / 2;
  line(PITCH_LENGTH / 2, 0, PITCH_LENGTH / 2, PITCH_WIDTH);
  arc(PITCH_LENGTH / 2, middle, 9.15, 0, Math.PI * 2);
  spot(PITCH_LENGTH / 2, middle);

  // Penalty area 16.5 x 40.32 m, goal area 5.5 x 18.32 m, penalty spot 11 m, arc 9.15 m.
  for (const end of [0, PITCH_LENGTH]) {
    const inward = end === 0 ? 1 : -1;
    const box = (depth: number, span: number) => {
      const top = middle + span / 2;
      const bottom = middle - span / 2;
      line(end, top, end + inward * depth, top);
      line(end + inward * depth, top, end + inward * depth, bottom);
      line(end + inward * depth, bottom, end, bottom);
    };
    box(16.5, 40.32);
    box(5.5, 18.32);
    spot(end + inward * 11, middle);
    // Only the part of the penalty arc outside the box is drawn.
    const centre = end + inward * 11;
    const reach = Math.acos(5.5 / 9.15);
    if (end === 0) arc(centre, middle, 9.15, -reach, reach);
    else arc(centre, middle, 9.15, Math.PI - reach, Math.PI + reach);
  }
  for (const [cx, cy] of [
    [0, 0],
    [0, PITCH_WIDTH],
    [PITCH_LENGTH, 0],
    [PITCH_LENGTH, PITCH_WIDTH],
  ] as const) {
    const start = cx === 0 ? (cy === 0 ? 0 : -Math.PI / 2) : cy === 0 ? Math.PI / 2 : Math.PI;
    arc(cx, cy, 1, start, start + Math.PI / 2);
  }
}

function drawGoal(
  context: CanvasRenderingContext2D,
  scale: number,
  x: ToPixels,
  y: ToPixels,
  lineX: number,
  outward: 1 | -1,
  flash: boolean,
): void {
  const middle = PITCH_WIDTH / 2;
  const top = y(middle + GOAL_WIDTH / 2);
  const bottom = y(middle - GOAL_WIDTH / 2);
  const front = x(lineX);
  const back = x(lineX + outward * GOAL_DEPTH);
  const left = Math.min(front, back);
  const depth = Math.abs(back - front);

  if (flash) {
    context.fillStyle = COLOURS.flash;
    context.fillRect(left - scale, top - scale, depth + scale * 2, bottom - top + scale * 2);
  }
  context.fillStyle = COLOURS.net;
  context.fillRect(left, top, depth, bottom - top);
  context.strokeStyle = COLOURS.post;
  context.lineWidth = Math.max(2.5, scale * 0.3);
  context.beginPath();
  context.moveTo(front, top);
  context.lineTo(back, top);
  context.lineTo(back, bottom);
  context.lineTo(front, bottom);
  context.stroke();
}
