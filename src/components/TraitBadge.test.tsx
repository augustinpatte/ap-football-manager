import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { TraitBadge, TraitList } from "./TraitBadge";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        "traits.Speedster.label": "Velocista",
        "traits.Speedster.desc": "Explosivo",
        "traits.HotHead.label": "Temperamental",
        "traits.HotHead.desc": "Juega al límite",
        "traits.Wonderkid.label": "Joya",
        "traits.Wonderkid.desc": "Talento especial",
        "common.attributes.pace": "Ritmo",
        "common.attributes.aggression": "Agresividad",
        "common.attributes.composure": "Compostura",
        "youthAcademy.age": "Edad",
        "youthAcademy.potential": "Potencial",
        "youthAcademy.growth": "Crecimiento",
      };

      return translations[key] ?? key;
    },
  }),
}));

describe("TraitBadge", () => {
  it("uses translated labels for minimum-threshold requirements in the tooltip", () => {
    render(<TraitBadge trait="Speedster" />);

    expect(screen.getByLabelText("Explosivo | Ritmo 85+")).toBeInTheDocument();
  });

  it("uses numeric operators instead of embedded English phrases", () => {
    render(<TraitBadge trait="HotHead" />);

    expect(
      screen.getByLabelText("Juega al límite | Agresividad 85+, Compostura < 50"),
    ).toBeInTheDocument();
  });

  it("uses translated non-attribute labels for wonderkid requirements", () => {
    render(<TraitBadge trait="Wonderkid" />);

    expect(
      screen.getByLabelText("Talento especial | Edad <= 20, Potencial 90+, Crecimiento 14+"),
    ).toBeInTheDocument();
  });
});

describe("TraitList", () => {
  /**
   * Given a player whose stored traits include Wonderkid, which the game awards only for an S-tier
   *       ceiling
   * When his traits are listed
   * Then the Wonderkid badge is left out, so no list reveals a bound on his hidden potential, and
   *      his other traits are still shown.
   */
  it("does not list a trait that is awarded from a hidden potential", () => {
    render(<TraitList traits={["Speedster", "Wonderkid"]} />);

    expect(screen.getByText("Velocista")).toBeInTheDocument();
    expect(screen.queryByText("Joya")).toBeNull();
    expect(screen.queryByLabelText(/Potencial/)).toBeNull();
  });

  /**
   * Given a player whose only trait is Wonderkid
   * When his traits are listed
   * Then nothing is rendered, and the "+N" overflow count does not count the hidden trait.
   */
  it("renders nothing for a list of only hidden traits, and counts none of them as overflow", () => {
    const { container, rerender } = render(<TraitList traits={["Wonderkid"]} />);
    expect(container).toBeEmptyDOMElement();

    rerender(<TraitList traits={["Wonderkid", "Speedster", "HotHead"]} max={1} />);
    expect(screen.getByText("+1")).toBeInTheDocument();
  });
});
