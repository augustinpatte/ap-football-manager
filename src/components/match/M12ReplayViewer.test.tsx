import {
  act,
  fireEvent,
  type RenderOptions,
  render as renderWithoutTheme,
  screen,
} from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../../context/ThemeContext";
import M12ReplayViewer from "./M12ReplayViewer";
import { buildEventsOnlyReplay, buildReplayFixture, MINUTE_MS } from "./m12Replay.fixtures";

// The theme provider reads the system preference.
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

/** The match shell carries the theme toggle, so every render has the provider (and keeps it on rerender). */
const render = (ui: ReactElement, options?: Omit<RenderOptions, "wrapper">) =>
  renderWithoutTheme(ui, { wrapper: ThemeProvider, ...options });

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

const canvasContext = {
  scale: vi.fn(),
  fillRect: vi.fn(),
  beginPath: vi.fn(),
  arc: vi.fn(),
  ellipse: vi.fn(),
  fill: vi.fn(),
  stroke: vi.fn(),
  strokeRect: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  fillText: vi.fn(),
};

/** Shirt labels drawn on the last paint: one per player on the pitch. */
function drawnPlayerLabels(): string[] {
  return canvasContext.fillText.mock.calls.slice(-30).map(([label]) => String(label));
}

beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = vi.fn(
    () => canvasContext,
  ) as unknown as typeof HTMLCanvasElement.prototype.getContext;
});

beforeEach(() => {
  // The viewer also assigns style strings onto this object; only the spies need clearing.
  Object.values(canvasContext).forEach((value) => {
    if (typeof value === "function") value.mockClear();
  });
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
    expect(
      screen.getByText("match.replayTeam.home 1 – 0 match.replayTeam.away"),
    ).toBeInTheDocument();
  });

  /** Given a replay paused away from kick-off, when its parent rerenders unchanged, then it keeps the selected time. */
  it("does not reset a sought replay to kickoff on an unchanged rerender", () => {
    const { rerender } = render(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    fireEvent.change(screen.getByRole("slider", { name: "match.replayTimeline" }), {
      target: { value: "1000" },
    });

    rerender(<M12ReplayViewer replayJson={replay} onFinish={vi.fn()} />);

    expect(
      screen.getByText("match.replayTeam.home 1 – 0 match.replayTeam.away"),
    ).toBeInTheDocument();
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

    expect(
      screen.getByText("match.replayTeam.home 1 – 0 match.replayTeam.away"),
    ).toBeInTheDocument();
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
    expect(screen.getByRole("alert")).toHaveTextContent("match.replayUnsupportedSchema");
  });

  afterEach(() => {
    vi.useRealTimers();
  });
});

describe("M12ReplayViewer with a squad-and-bench replay", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  const names = {
    "home-9": "Hugo Nine",
    "away-1": "Axel One",
    "home-2": "Hal Two",
    "home-12": "Ben Twelve",
  };

  /** Given a replay with benches, when it opens, then 22 players and the ball are painted and the scoreboard shows the teams. */
  it("paints exactly the 22 players on the pitch, the ball and the scoreboard", () => {
    const fixture = buildReplayFixture();
    render(<M12ReplayViewer replayJson={fixture.json} onFinish={vi.fn()} />);

    expect(
      screen.getByText("match.replayTeam.home 0 – 0 match.replayTeam.away"),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("match.replayPitch")).toBeInTheDocument();
    expect(canvasContext.fillText).toHaveBeenCalledTimes(22);
    expect(screen.getByTestId("replay-players-on-pitch")).toHaveTextContent(
      "match.replayPlayersOnPitch",
    );
  });

  /** Given a red card in the replay, when the timeline is moved past it, then 21 players are painted. */
  it("paints 21 players after a sending-off when seeking past it", () => {
    const fixture = buildReplayFixture();
    render(<M12ReplayViewer replayJson={fixture.json} onFinish={vi.fn()} />);
    canvasContext.fillText.mockClear();

    fireEvent.change(screen.getByRole("slider", { name: "match.replayTimeline" }), {
      target: { value: String(31 * MINUTE_MS) },
    });

    expect(drawnPlayerLabels()).toHaveLength(21);
  });

  /** Given the engine's internal event kinds in the replay, when seeking past them, then none is printed as the engine spelled it. */
  it("never prints an engine event kind as written", () => {
    const fixture = buildReplayFixture();
    render(<M12ReplayViewer replayJson={fixture.json} onFinish={vi.fn()} playerNames={names} />);

    fireEvent.change(screen.getByRole("slider", { name: "match.replayTimeline" }), {
      target: { value: String(61 * MINUTE_MS) },
    });

    expect(screen.queryByText(/shootout/i)).not.toBeInTheDocument();
    expect(screen.getByText("match.replayFeed.goal")).toBeInTheDocument();
  });

  /** Given uneven benches and explicit home ids, when it opens, then 11 players per side are painted. */
  it("uses the supplied home ids instead of splitting the roster in half", () => {
    const fixture = buildReplayFixture({ homeBench: 0, awayBench: 8 });
    render(
      <M12ReplayViewer
        replayJson={fixture.json}
        onFinish={vi.fn()}
        homePlayerIds={new Set(fixture.homeIds)}
      />,
    );
    expect(screen.getByTestId("replay-players-on-pitch")).toBeInTheDocument();
    // Labels restart at 1 for each side, so a correct split paints 1..11 twice.
    const labels = drawnPlayerLabels().sort((a, b) => Number(a) - Number(b));
    expect(labels).toEqual(
      Array.from({ length: 11 }, (_, index) => String(index + 1))
        .flatMap((label) => [label, label])
        .sort((a, b) => Number(a) - Number(b)),
    );
  });

  /** Given a result with events but no frames, when opened, then the viewer says so instead of showing an empty pitch. */
  it("explains a replay that has no player-movement data", () => {
    const back = vi.fn();
    render(<M12ReplayViewer replayJson={buildEventsOnlyReplay()} readOnly onFinish={back} />);

    expect(screen.getByRole("alert")).toHaveTextContent("match.replayNoFrames");
    expect(screen.queryByLabelText("match.replayPitch")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "match.backToSchedule" }));
    expect(back).toHaveBeenCalledOnce();
  });

  /** Given a read-only replay, when it plays to the end and the manager leaves, then only the close callback fires. */
  it("plays a historical replay to its end without asking to finalise anything", () => {
    vi.useFakeTimers();
    const close = vi.fn();
    const fixture = buildReplayFixture();
    render(<M12ReplayViewer replayJson={fixture.json} readOnly onFinish={close} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "4" } });

    act(() => {
      vi.advanceTimersByTime(60 * MINUTE_MS);
    });

    expect(screen.getByRole("slider", { name: "match.replayTimeline" })).toHaveValue(
      String(60 * MINUTE_MS),
    );
    expect(screen.queryByRole("button", { name: "match.goToResult" })).not.toBeInTheDocument();
    expect(close).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "match.backToSchedule" }));
    expect(close).toHaveBeenCalledOnce();
  });
});
