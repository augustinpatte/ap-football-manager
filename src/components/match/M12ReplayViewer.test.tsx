import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import M12ReplayViewer from "./M12ReplayViewer";

const replay = JSON.stringify({
  schema_version: 1,
  home_team: "Home",
  away_team: "Away",
  roster: Array.from({ length: 22 }, (_, index) => `p-${index}`),
  events: [{ t_ms: 1000, type: "goal", scorer: "p-1" }],
  frames: [
    {
      t_ms: 0,
      period: "first_half",
      ball: { x: 52, y: 34, z: 0 },
      score: { home: 0, away: 0 },
      players: Array.from({ length: 22 }, (_, index) => ({
        x: index,
        y: index,
        heading: 0,
        on_pitch: true,
      })),
    },
    {
      t_ms: 1000,
      period: "first_half",
      ball: { x: 54, y: 34, z: 0 },
      score: { home: 1, away: 0 },
      players: Array.from({ length: 22 }, (_, index) => ({
        x: index + 1,
        y: index,
        heading: 0,
        on_pitch: true,
      })),
    },
  ],
});

beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
    scale: vi.fn(),
    fillRect: vi.fn(),
    beginPath: vi.fn(),
    arc: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    strokeRect: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    fillText: vi.fn(),
  })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
});

describe("M12ReplayViewer", () => {
  /** Given a stored replay, when the manager pauses and seeks, then replay state changes without a backend call. */
  it("pauses and seeks a stored replay locally", () => {
    render(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    expect(screen.getByRole("button", { name: "match.playReplay" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("slider", { name: "match.replayTimeline" }), {
      target: { value: "1000" },
    });
    expect(screen.getByText("Home 1 – 0 Away")).toBeInTheDocument();
  });

  /** Given a replay paused away from kick-off, when its parent rerenders unchanged, then it keeps the selected time. */
  it("does not reset a sought replay to kickoff on an unchanged rerender", () => {
    const { rerender } = render(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    fireEvent.change(screen.getByRole("slider", { name: "match.replayTimeline" }), {
      target: { value: "1000" },
    });

    rerender(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);

    expect(screen.getByText("Home 1 – 0 Away")).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "match.replayTimeline" })).toHaveValue("1000");
  });

  /** Given a replay, when the manager selects 4× speed and resumes it, then frames advance at that selected speed. */
  it("uses the selected playback speed", () => {
    vi.useFakeTimers();
    render(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "match.playReplay" }));

    act(() => {
      vi.advanceTimersByTime(300);
    });

    expect(screen.getByText("Home 1 – 0 Away")).toBeInTheDocument();
  });

  /** Given a historical replay, when the manager returns, then no finalisation is requested. */
  it("returns from a read-only replay", () => {
    const back = vi.fn();
    render(<M12ReplayViewer replayJson={replay} readOnly onFinish={back} />);
    fireEvent.click(screen.getByRole("button", { name: "match.backToSchedule" }));
    expect(back).toHaveBeenCalledOnce();
  });

  /** Given malformed replay data, when the viewer opens it, then it rejects it without attempting a simulation. */
  it("rejects malformed replay data", () => {
    const back = vi.fn();
    render(<M12ReplayViewer replayJson="{" onFinish={back} readOnly />);
    expect(screen.getByRole("alert")).toHaveTextContent("match.replayUnavailable");
    fireEvent.click(screen.getByRole("button", { name: "match.backToSchedule" }));
    expect(back).toHaveBeenCalledOnce();
  });

  /** Given a replay from an unsupported M12 schema, when it opens, then the viewer refuses it without playback. */
  it("rejects an incompatible replay schema", () => {
    render(
      <M12ReplayViewer
        replayJson={replay.replace('"schema_version":1', '"schema_version":2')}
        onFinish={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("match.replayUnavailable");
  });

  afterEach(() => {
    vi.useRealTimers();
  });
});
