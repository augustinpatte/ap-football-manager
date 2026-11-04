import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import i18n, { i18nReady } from "../../i18n";
import { SuspensionBadge } from "./SuspensionBadge";

let previous: string;
beforeAll(async () => {
  await i18nReady;
  previous = i18n.language;
});
afterAll(async () => {
  await i18n.changeLanguage(previous);
});

describe("SuspensionBadge", () => {
  /**
   * Given a player with two matches still to miss,
   * When the badge is drawn in English and in German,
   * Then it says so in each, with the count in its accessible name.
   */
  it("names the suspension and the matches still to miss in the language of the game", async () => {
    await i18n.changeLanguage("en");
    const { unmount } = render(<SuspensionBadge matches={2} />);
    expect(screen.getByRole("img", { name: "Suspended — matches still to miss: 2" })).toBeInTheDocument();
    unmount();
    await i18n.changeLanguage("de");
    render(<SuspensionBadge matches={2} />);
    expect(screen.getByRole("img", { name: "Gesperrt — noch zu verpassende Spiele: 2" })).toBeInTheDocument();
  });
});
