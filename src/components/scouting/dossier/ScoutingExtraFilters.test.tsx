import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PlayersPageQuery } from "../../../services/playersService";
import ScoutingExtraFilters from "./ScoutingExtraFilters";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: "en" } }),
}));

const QUERY = {
  scope: "scoutable",
  page: 3,
} as unknown as PlayersPageQuery;

describe("ScoutingExtraFilters", () => {
  /**
   * Given the search on page three
   * When the manager ticks "contract ends within a year" and sets a potential floor
   * Then each change patches the query and sends the search back to page one, and a floor out of
   *      range is held to 1–99 while an emptied field clears it.
   */
  it("patches the query and returns to the first page", () => {
    const onChange = vi.fn();
    render(<ScoutingExtraFilters query={QUERY} onChange={onChange} />);

    fireEvent.click(screen.getByLabelText("scouting.filters.contractExpiring"));
    expect(onChange).toHaveBeenLastCalledWith({ contract_expiring: true, page: 1 });

    const potential = screen.getByLabelText("scouting.filters.potentialMin");
    fireEvent.change(potential, { target: { value: "150" } });
    expect(onChange).toHaveBeenLastCalledWith({ potential_min: 99, page: 1 });
  });

  /**
   * Given a stored potential floor
   * When the manager empties the field
   * Then the floor is cleared rather than set to zero.
   */
  it("clears the floor when the field is emptied", () => {
    const onChange = vi.fn();
    render(<ScoutingExtraFilters query={{ ...QUERY, potential_min: 72 }} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("scouting.filters.potentialMin"), {
      target: { value: "" },
    });

    expect(onChange).toHaveBeenLastCalledWith({ potential_min: null, page: 1 });
  });

  /**
   * Given a stored potential floor
   * When the filters render
   * Then the field shows it and says it applies to scouted players only.
   */
  it("shows the stored floor and its caveat", () => {
    render(<ScoutingExtraFilters query={{ ...QUERY, potential_min: 72 }} onChange={vi.fn()} />);
    expect(screen.getByLabelText("scouting.filters.potentialMin")).toHaveValue(72);
    expect(screen.getByText("scouting.filters.potentialHint")).toBeInTheDocument();
  });
});
