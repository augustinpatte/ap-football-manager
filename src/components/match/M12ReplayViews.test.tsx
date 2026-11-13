import { fireEvent, render as renderWithoutTheme, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../../context/ThemeContext";
import M12ReplayViewer from "./M12ReplayViewer";
import { buildReplayFixture, MINUTE_MS } from "./m12Replay.fixtures";

// A `t` that shows its parameters, so a test can see which name or number reached the screen.
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params
        ? `${key}|${Object.entries(params)
            .map(([name, value]) => `${name}=${String(value)}`)
            .join(",")}`
        : key,
    i18n: { language: "en" },
  }),
}));

const canvasContext = new Proxy(
  {},
  {
    get: (_target, name) => (name === "canvas" ? undefined : vi.fn()),
    set: () => true,
  },
);

beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = vi.fn(
    () => canvasContext,
  ) as unknown as typeof HTMLCanvasElement.prototype.getContext;
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
});

const render = (ui: ReactElement) => renderWithoutTheme(ui, { wrapper: ThemeProvider });

const names = {
  "home-9": "Hugo Nine",
  "away-1": "Axel One",
  "home-2": "Hal Two",
  "home-12": "Ben Twelve",
};

/** The fixture's replay with the internal ids the engine really carries in place of club names. */
function replayWithClubIds(extra: Record<string, unknown> = {}): string {
  const fixture = buildReplayFixture();
  return JSON.stringify({
    ...fixture.document,
    home_team: "FM-C-1072",
    away_team: "FM-C-528",
    ...extra,
  });
}

const timeline = () => screen.getByRole("slider", { name: "match.replayTimeline" });
const seekToMinute = (minute: number, secondsOver = 0) =>
  fireEvent.change(timeline(), {
    target: { value: String(minute * MINUTE_MS + secondsOver * 1000) },
  });

describe("M12ReplayViewer club names", () => {
  /**
   * Given a replay that carries the engine's internal club ids
   * When the game knows those clubs
   * Then the scoreboard shows their names, matched whatever the id's case, and no id.
   */
  it("shows the clubs' real names, never the ids", () => {
    render(
      <M12ReplayViewer
        replayJson={replayWithClubIds()}
        onFinish={vi.fn()}
        teams={[
          { id: "fm-c-1072", name: "Albacete Balompié" },
          { id: "FM-C-528", name: "Bayern München" },
        ]}
      />,
    );
    expect(screen.getByText("Albacete Balompié 0 – 0 Bayern München")).toBeInTheDocument();
    expect(screen.queryByText(/FM-C/i)).not.toBeInTheDocument();
  });

  /**
   * Given the same replay and a game that has no such clubs
   * When it opens
   * Then a controlled fallback is shown, still not the id.
   */
  it("falls back to Home and Away when a club's name is missing", () => {
    render(<M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} teams={[]} />);
    expect(
      screen.getByText("match.replayTeam.home 0 – 0 match.replayTeam.away"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/FM-C/i)).not.toBeInTheDocument();
  });
});

describe("M12ReplayViewer events", () => {
  /**
   * Given a goal at 10'
   * When the replay is a few seconds past it
   * Then the goal is announced with the scorer's name and minute, in the feed too.
   */
  it("announces a goal with the scorer's name", () => {
    render(
      <M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} playerNames={names} />,
    );
    seekToMinute(10, 3);

    expect(screen.getByRole("status")).toHaveTextContent(
      "match.replayGoalBanner|player=Hugo Nine,minute=10",
    );
    expect(screen.getByText(/match\.replayFeed\.goal\|player=Hugo Nine/)).toBeInTheDocument();
  });

  /**
   * Given a player the game cannot name
   * When an event is about him
   * Then the feed says "a player", not his id.
   */
  it("never shows a player id", () => {
    render(<M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} />);
    seekToMinute(60);

    expect(screen.queryByText(/home-9|away-1|home-12/)).not.toBeInTheDocument();
    expect(screen.getAllByText(/match\.replayUnknownPlayer/).length).toBeGreaterThan(0);
  });
});

describe("M12ReplayViewer synchronized views", () => {
  const tab = (id: string) => screen.getByRole("tab", { name: `match.replayTab.${id}` });

  /**
   * Given a replay paused at 60'
   * When the manager moves through every view
   * Then the clock, the score and the slider are exactly what they were: one match, one state.
   */
  it("changing view does not change the clock, the score or the match", () => {
    render(
      <M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} playerNames={names} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    seekToMinute(60);
    const scoreboard = screen.getByText(/1 – 0/).textContent;
    const clock = timeline().getAttribute("value") ?? (timeline() as HTMLInputElement).value;

    for (const id of ["condition", "stats", "lineup", "pitch"]) {
      fireEvent.click(tab(id));
      expect(screen.getByText(/1 – 0/).textContent).toBe(scoreboard);
      expect((timeline() as HTMLInputElement).value).toBe(clock);
    }
  });

  /**
   * Given a goal, a red card and a substitution before 60'
   * When the statistics view is opened
   * Then it counts what happened up to that minute and names the minute.
   */
  it("shows statistics up to the current minute", () => {
    render(
      <M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} playerNames={names} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    seekToMinute(60);
    fireEvent.click(tab("stats"));

    const panel = screen.getByRole("tabpanel", { name: "match.replayTab.stats" });
    expect(within(panel).getByText("match.replayStats.title|minute=60")).toBeInTheDocument();
    const goalsRow = within(panel).getByText("match.replayStats.goals").closest("tr");
    expect(goalsRow).toHaveTextContent("1");
    const redRow = within(panel).getByText("match.replayStats.redCards").closest("tr");
    expect(redRow).toHaveTextContent("1");
  });

  /**
   * Given a replay without recorded conditions
   * When the condition view is opened
   * Then it says so instead of showing made-up values.
   */
  it("says plainly when conditions were not recorded", () => {
    render(<M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} />);
    fireEvent.click(tab("condition"));
    expect(screen.getByText("match.replayCondition.none")).toBeInTheDocument();
  });

  /**
   * Given a replay that recorded conditions
   * When the condition view is opened
   * Then each player on the pitch is listed with his recorded value.
   */
  it("lists each player's recorded condition", () => {
    const fixture = buildReplayFixture();
    const frames = (fixture.document.frames as Array<Record<string, unknown>>).map((frame) => ({
      ...frame,
      extras: {
        phase: [0, 0],
        agents: [
          { id: "home-9", cond: 87, act: 0 },
          { id: "away-1", cond: 59, act: 0 },
        ],
      },
    }));
    render(
      <M12ReplayViewer
        replayJson={replayWithClubIds({ frames })}
        onFinish={vi.fn()}
        playerNames={names}
      />,
    );
    fireEvent.click(tab("condition"));

    expect(screen.getByText("Hugo Nine")).toBeInTheDocument();
    expect(screen.getByText("87")).toBeInTheDocument();
    expect(screen.getByText("Axel One")).toBeInTheDocument();
    expect(screen.getByText("59")).toBeInTheDocument();
  });

  /**
   * Given the line-up view after a sending-off and a substitution
   * When it is opened
   * Then it lists who left and why, and says that nothing here changes the match.
   */
  it("shows who has left the match and that the view is a replay", () => {
    render(
      <M12ReplayViewer replayJson={replayWithClubIds()} onFinish={vi.fn()} playerNames={names} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "match.pauseReplay" }));
    seekToMinute(60);
    fireEvent.click(tab("lineup"));

    const panel = screen.getByRole("tabpanel", { name: "match.replayTab.lineup" });
    expect(within(panel).getByText("match.replayLineup.replayOnly")).toBeInTheDocument();
    expect(within(panel).getByText(/Axel One/)).toHaveTextContent("match.replayLineup.sentOff");
    expect(within(panel).getByText(/Hal Two/)).toHaveTextContent("match.replayLineup.substituted");
  });
});
