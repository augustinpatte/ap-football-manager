import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import { AboutLicences } from "./AboutLicences";

const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrl(url),
}));

beforeAll(async () => {
  await i18nReady;
  await i18n.changeLanguage("en");
});

describe("AboutLicences", () => {
  /** Given the About section, when rendered, then the GPL-3.0 notices, the upstream credit and the modification notice are visible. */
  it("shows the product, the upstream credit, the modification notice and the warranty disclaimer", () => {
    render(<AboutLicences />);

    expect(screen.getByText("AP Football Manager")).toBeInTheDocument();
    expect(screen.getByText(/GNU General Public License version 3/)).toBeInTheDocument();
    expect(screen.getByText(/modified copy of OpenFoot Manager/)).toBeInTheDocument();
    expect(screen.getByText(/Changes made by the AP Football Manager project/)).toBeInTheDocument();
    expect(screen.getByText(/ABSOLUTELY NO WARRANTY/)).toBeInTheDocument();
  });

  /** Given the About section, when the licence is toggled, then the full GPL-3.0 text appears and hides again. */
  it("shows and hides the full licence text", () => {
    render(<AboutLicences />);

    fireEvent.click(screen.getByText("Show the licence text"));
    expect(screen.getByText(/GNU GENERAL PUBLIC LICENSE/)).toBeInTheDocument();

    fireEvent.click(screen.getByText("Hide the licence text"));
    expect(screen.queryByText(/GNU GENERAL PUBLIC LICENSE/)).toBeNull();
  });

  /** Given the upstream button, when clicked, then it opens the upstream repository and nothing else. */
  it("opens the upstream repository only when the player asks", () => {
    render(<AboutLicences />);
    expect(openUrl).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("Upstream source code"));

    expect(openUrl).toHaveBeenCalledWith("https://github.com/openfootmanager/openfootmanager");
  });
});
