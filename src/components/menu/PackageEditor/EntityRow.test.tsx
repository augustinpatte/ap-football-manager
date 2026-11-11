import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { EntityRow } from "./shared";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

function renderRow(overrides: Partial<Parameters<typeof EntityRow>[0]> = {}) {
  const handlers = { onClick: vi.fn(), onEdit: vi.fn(), onDelete: vi.fn() };
  render(
    <EntityRow
      title="Alpha FC"
      subtitle="alpha"
      editLabel="Edit"
      deleteLabel="Delete"
      {...handlers}
      {...overrides}
    />,
  );
  return handlers;
}

describe("EntityRow", () => {
  /**
   * Given a selectable row
   * When its title is clicked
   * Then the row is selected, once, and nothing is edited.
   */
  it("selects the row from its title button", () => {
    const { onClick, onEdit } = renderRow();

    fireEvent.click(screen.getByRole("button", { name: /Alpha FC/ }));

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(onEdit).not.toHaveBeenCalled();
  });

  /**
   * Given a selectable row
   * When Edit is clicked
   * Then it edits and does not also select the row.
   */
  it("edits without selecting", () => {
    const { onClick, onEdit } = renderRow();

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));

    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onClick).not.toHaveBeenCalled();
  });

  /**
   * Given a row waiting for delete confirmation
   * When it is looked at
   * Then the title is no longer a select button: confirming or cancelling is the only choice.
   */
  it("stops being selectable while a delete is awaiting confirmation", () => {
    const { onClick } = renderRow();

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(screen.queryByRole("button", { name: /Alpha FC/ })).toBeNull();
    expect(onClick).not.toHaveBeenCalled();
  });

  /**
   * Given a row with no selection handler
   * When it renders
   * Then the title is plain text, not a dead button.
   */
  it("renders a plain title when the row is not selectable", () => {
    renderRow({ onClick: undefined });

    expect(screen.queryByRole("button", { name: /Alpha FC/ })).toBeNull();
    expect(screen.getByText("Alpha FC")).toBeInTheDocument();
  });
});
