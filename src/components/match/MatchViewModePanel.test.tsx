import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import MatchViewModePanel from "./MatchViewModePanel";

describe("MatchViewModePanel", () => {
  /** Given the preparation screen, when the panel renders, then each way to play is named and explained. */
  it("explains live, 2D and quick simulation side by side", () => {
    render(
      <MatchViewModePanel
        busy={null}
        error={null}
        onWatchIn2d={vi.fn()}
        onQuickSimulation={vi.fn()}
      />,
    );
    expect(screen.getByRole("heading", { name: "match.viewMode.heading" })).toBeInTheDocument();
    expect(screen.getByText("match.viewMode.liveDesc")).toBeInTheDocument();
    expect(screen.getByText("match.viewMode.watchDesc")).toBeInTheDocument();
    expect(screen.getByText("match.viewMode.quickDesc")).toBeInTheDocument();
    // Start Match is the header's own button there: the panel must not add a second one.
    expect(screen.queryByRole("button", { name: "match.startMatch" })).not.toBeInTheDocument();
  });

  /** Given a screen without its own start button, when onPlayLive is given, then the live option is a button too. */
  it("adds the live button for spectators", () => {
    const live = vi.fn();
    render(
      <MatchViewModePanel
        busy={null}
        error={null}
        onPlayLive={live}
        onWatchIn2d={vi.fn()}
        onQuickSimulation={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "match.startMatch" }));
    expect(live).toHaveBeenCalledOnce();
  });

  /** Given a request in flight, when the panel renders, then every option is disabled and the wait is announced. */
  it("disables every option while one is running", () => {
    render(
      <MatchViewModePanel
        busy="preparing"
        error={null}
        onPlayLive={vi.fn()}
        onWatchIn2d={vi.fn()}
        onQuickSimulation={vi.fn()}
      />,
    );
    for (const name of ["match.startMatch", "match.watchWithM12", "match.simulateWithM12"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  /** Given a failed attempt, when the panel renders, then the translated error is announced as an alert. */
  it("announces a failure", () => {
    render(
      <MatchViewModePanel
        busy={null}
        error="boom"
        onWatchIn2d={vi.fn()}
        onQuickSimulation={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("boom");
  });
});
