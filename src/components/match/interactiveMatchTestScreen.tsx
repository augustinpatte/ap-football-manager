import { act, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import i18n from "../../i18n";
import InteractiveMatchScreen from "./InteractiveMatchScreen";

/** Lets `ms` of wall-clock time pass: animation frames run, and the engine fake answers between them. */
export async function wall(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

export function renderScreen(onFinished = vi.fn(), onExited = vi.fn()) {
  render(
    <InteractiveMatchScreen
      homeName="Reds"
      awayName="Blues"
      onFinished={onFinished}
      onExited={onExited}
    />,
  );
  return onFinished;
}

/** The app's own wording for a key, so a test never restates a translation. */
export const tr = (key: string, params?: Record<string, string | number>) => i18n.t(key, params);

export async function opened(onFinished = vi.fn()) {
  renderScreen(onFinished);
  await screen.findByRole("img", {
    name: tr("match.liveView.pitchLabel", { home: "Reds", away: "Blues" }),
  });
  await wall(50);
  return onFinished;
}
