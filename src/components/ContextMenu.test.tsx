import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import ContextMenu, { type ContextMenuItem } from "./ContextMenu";

function renderMenu(items: ContextMenuItem[]) {
  render(
    <div>
      <ContextMenu items={items}>
        <div data-testid="row">Row</div>
      </ContextMenu>
      <p data-testid="elsewhere">Elsewhere</p>
    </div>,
  );
  fireEvent.contextMenu(screen.getByTestId("row"), { clientX: 10, clientY: 10 });
}

describe("ContextMenu", () => {
  /**
   * Given a right-clicked row with an action
   * When the action is chosen
   * Then it runs once and the menu closes.
   */
  it("runs an item and closes", () => {
    const onClick = vi.fn();
    renderMenu([{ label: "Open", onClick }]);

    fireEvent.click(screen.getByRole("menuitem", { name: "Open" }));

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  /**
   * Given an open menu with a label row and a divider
   * When the label row (not an action) is clicked
   * Then the menu stays open: a click inside the menu is the menu's own.
   */
  it("stays open when a non-action part of the menu is clicked", () => {
    renderMenu([
      { label: "Heading", type: "label" },
      { label: "", divider: true },
      { label: "Open" },
    ]);

    fireEvent.click(screen.getByText("Heading"));

    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  /**
   * Given an open menu
   * When the page is clicked elsewhere
   * Then the menu closes.
   */
  it("closes on a click outside", () => {
    renderMenu([{ label: "Open" }]);

    fireEvent.click(screen.getByTestId("elsewhere"));

    expect(screen.queryByRole("menu")).toBeNull();
  });

  /**
   * Given an open menu
   * When Escape is pressed
   * Then the menu closes.
   */
  it("closes on Escape", () => {
    renderMenu([{ label: "Open" }]);

    fireEvent.keyDown(window, { key: "Escape" });

    expect(screen.queryByRole("menu")).toBeNull();
  });
});
