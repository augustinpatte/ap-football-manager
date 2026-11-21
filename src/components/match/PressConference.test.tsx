import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import type { GameStateData } from "../../store/gameStore";
import PressConference from "./PressConference";
import type { MatchSnapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === "match.pressSubmitFailed") return `FAILED: ${String(options?.reason)}`;
      return options ? `${key} ${JSON.stringify(options)}` : key;
    },
  }),
}));
vi.mock("../ui", () => ({
  Badge: ({ children }: { children: React.ReactNode }) => <span>{children}</span>,
  ThemeToggle: () => null,
  Button: ({ children, onClick }: { children: React.ReactNode; onClick?: () => void }) => (
    <button type="button" onClick={onClick}>
      {children}
    </button>
  ),
}));

const mockedInvoke = vi.mocked(invoke);

function fourTwoSnapshot(): MatchSnapshot {
  return {
    phase: "FullTime",
    home_score: 4,
    away_score: 2,
    home_team: { id: "home1", name: "Home FC", players: [{ id: "h1", name: "Hanna Striker" }] },
    away_team: { id: "away1", name: "Away FC", players: [{ id: "a1", name: "Alex Winger" }] },
    events: [
      {
        minute: 5,
        event_type: "Goal",
        side: "Home",
        zone: "",
        player_id: "h1",
        secondary_player_id: null,
      },
    ],
  } as unknown as MatchSnapshot;
}

function answerEveryQuestion(): void {
  for (let step = 0; step < 5; step += 1) {
    fireEvent.click(
      screen.getAllByRole("button").filter((b) => b.textContent?.includes("match.press"))[0],
    );
    fireEvent.click(screen.getByRole("button", { name: /match\.(nextQuestion|leaveConference)/ }));
  }
}

describe("PressConference", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  /**
   * Given a match that finished 4-2 for the home side the manager runs
   * When the conference opens and every answer is submitted
   * Then the result question and the submitted payload describe 4-2 against Away FC, with the
   *      home scorer as the focus player.
   */
  it("asks about and submits the 4-2 that was played", async () => {
    mockedInvoke.mockResolvedValue({ game: { id: "g" }, morale_delta: 1 });
    const onFinish = vi.fn();
    render(
      <PressConference
        snapshot={fourTwoSnapshot()}
        gameState={{} as GameStateData}
        userSide="Home"
        onFinish={onFinish}
      />,
    );

    expect(screen.getByText(/match\.press\.result\.questions\.win/)).toHaveTextContent(
      '{"userScore":4,"oppScore":2,"oppName":"Away FC"}',
    );

    answerEveryQuestion();

    await waitFor(() => expect(onFinish).toHaveBeenCalledTimes(1));
    const [, args] = mockedInvoke.mock.calls[0] as [
      string,
      { answers: { question_id: string; question_text: string; player_id?: string }[] },
    ];
    expect(args.answers.find((a) => a.question_id === "result")?.question_text).toContain(
      '"userScore":4,"oppScore":2',
    );
    expect(args.answers.find((a) => a.question_id === "player_focus")?.player_id).toBe("h1");
  });

  /**
   * Given the backend refuses the conference
   * When the last answer is submitted
   * Then the manager is told why and stays on the conference: it is not silently dismissed.
   */
  it("keeps the conference open and says why when the backend refuses it", async () => {
    mockedInvoke.mockRejectedValue("be.error.liveMatch.noCompletedMatch");
    const onFinish = vi.fn();
    render(
      <PressConference
        snapshot={fourTwoSnapshot()}
        gameState={{} as GameStateData}
        userSide="Home"
        onFinish={onFinish}
      />,
    );

    answerEveryQuestion();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("FAILED: be.error.liveMatch.noCompletedMatch");
    // The button that was pressed has just disabled itself: focus goes to the reason, not the page.
    expect(alert).toHaveFocus();
    expect(onFinish).not.toHaveBeenCalled();
  });

  /**
   * Given the first submission failed
   * When the manager tries again and the backend now accepts
   * Then the conference is submitted once more and then closes; leaving without recording is a
   *      separate, explicit choice.
   */
  it("lets the manager retry, or leave without recording on purpose", async () => {
    mockedInvoke.mockRejectedValueOnce("be.error.liveMatch.noCompletedMatch");
    mockedInvoke.mockResolvedValueOnce({ game: { id: "g" }, morale_delta: 0 });
    const onFinish = vi.fn();
    render(
      <PressConference
        snapshot={fourTwoSnapshot()}
        gameState={{} as GameStateData}
        userSide="Home"
        onFinish={onFinish}
      />,
    );
    answerEveryQuestion();
    await screen.findByRole("alert");

    fireEvent.click(screen.getByRole("button", { name: "match.pressRetry" }));

    await waitFor(() => expect(onFinish).toHaveBeenCalledTimes(1));
    expect(mockedInvoke).toHaveBeenCalledTimes(2);
    // The retry sends the very same answers, not a fresh or emptied set.
    expect(mockedInvoke.mock.calls[1]).toEqual(mockedInvoke.mock.calls[0]);
  });

  /**
   * Given the last answer has been submitted and the backend has not replied yet
   * When the manager presses the same button again
   * Then only one submission is sent.
   */
  it("sends the conference once while a submission is in flight", async () => {
    let finish: (value: unknown) => void = () => {};
    mockedInvoke.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    render(
      <PressConference
        snapshot={fourTwoSnapshot()}
        gameState={{} as GameStateData}
        userSide="Home"
        onFinish={vi.fn()}
      />,
    );
    for (let step = 0; step < 4; step += 1) {
      fireEvent.click(
        screen.getAllByRole("button").filter((b) => b.textContent?.includes("match.press"))[0],
      );
      fireEvent.click(screen.getByRole("button", { name: /match\.nextQuestion/ }));
    }
    fireEvent.click(
      screen.getAllByRole("button").filter((b) => b.textContent?.includes("match.press"))[0],
    );
    const leave = screen.getByRole("button", { name: /match\.leaveConference/ });

    fireEvent.click(leave);
    fireEvent.click(leave);

    expect(mockedInvoke).toHaveBeenCalledTimes(1);
    finish({ game: { id: "g" }, morale_delta: 0 });
  });

  /**
   * Given a conference the backend refuses because one was already held today
   * When the manager chooses Leave without recording
   * Then the conference closes, and only because they chose it.
   */
  it("leaves without recording only when the manager chooses to", async () => {
    mockedInvoke.mockRejectedValue("be.error.liveMatch.pressConferenceAlreadyHeld");
    const onFinish = vi.fn();
    render(
      <PressConference
        snapshot={fourTwoSnapshot()}
        gameState={{} as GameStateData}
        userSide="Home"
        onFinish={onFinish}
      />,
    );
    answerEveryQuestion();
    await screen.findByRole("alert");
    expect(onFinish).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "match.pressLeaveUnrecorded" }));

    expect(onFinish).toHaveBeenCalledTimes(1);
  });
});
