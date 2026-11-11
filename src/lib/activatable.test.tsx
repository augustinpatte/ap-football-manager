import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { activatable } from "./activatable";

function Row({ onActivate }: { onActivate?: () => void }) {
  return (
    <div data-testid="row" {...activatable(onActivate)}>
      <input type="checkbox" aria-label="select" />
      Row text
    </div>
  );
}

describe("activatable", () => {
  /**
   * Given a row with an activation handler
   * When it is clicked
   * Then the handler runs once and the row is exposed as a focusable button.
   */
  it("makes a clickable row a focusable button", () => {
    const onActivate = vi.fn();
    render(<Row onActivate={onActivate} />);

    const row = screen.getByRole("button", { name: /row text/i });
    fireEvent.click(row);

    expect(onActivate).toHaveBeenCalledTimes(1);
    expect(row).toHaveAttribute("tabindex", "0");
  });

  /**
   * Given a focused row
   * When Enter or Space is pressed
   * Then the handler runs, and Space does not scroll the page.
   */
  it.each(["Enter", " "])("activates on the %j key", (key) => {
    const onActivate = vi.fn();
    render(<Row onActivate={onActivate} />);

    const notPrevented = fireEvent.keyDown(screen.getByTestId("row"), { key });

    expect(onActivate).toHaveBeenCalledTimes(1);
    expect(notPrevented).toBe(false);
  });

  /**
   * Given a focused row
   * When another key is pressed
   * Then nothing happens and the key keeps its default behaviour.
   */
  it("ignores other keys", () => {
    const onActivate = vi.fn();
    render(<Row onActivate={onActivate} />);

    const notPrevented = fireEvent.keyDown(screen.getByTestId("row"), { key: "a" });

    expect(onActivate).not.toHaveBeenCalled();
    expect(notPrevented).toBe(true);
  });

  /**
   * Given a control nested inside the row
   * When a key is pressed on that control
   * Then the row does not activate: the key belongs to the nested control.
   */
  it("does not steal keys from a nested control", () => {
    const onActivate = vi.fn();
    render(<Row onActivate={onActivate} />);

    fireEvent.keyDown(screen.getByRole("checkbox", { name: "select" }), { key: " " });

    expect(onActivate).not.toHaveBeenCalled();
  });

  /**
   * Given a checkbox nested inside the row
   * When the checkbox is clicked
   * Then the row does not activate: that click is the checkbox's.
   */
  it("ignores a click on a nested control", () => {
    const onActivate = vi.fn();
    render(<Row onActivate={onActivate} />);

    fireEvent.click(screen.getByRole("checkbox", { name: "select" }));

    expect(onActivate).not.toHaveBeenCalled();
  });

  /**
   * Given no handler (the screen offers no navigation)
   * When the row is rendered
   * Then it stays an inert element: no role, not focusable.
   */
  it("adds nothing without a handler", () => {
    render(<Row />);

    const row = screen.getByTestId("row");

    expect(row).not.toHaveAttribute("role");
    expect(row).not.toHaveAttribute("tabindex");
  });
});
