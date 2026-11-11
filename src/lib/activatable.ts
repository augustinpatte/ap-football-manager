import type { KeyboardEvent, MouseEvent } from "react";

/**
 * Focus ring for an activatable row. Inset because rows usually sit in an `overflow-hidden`
 * card that would clip an outer ring.
 */
export const ACTIVATABLE_FOCUS_CLASS =
  "focus:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-focus";

/** A control inside the row: a click on it is that control's, not the row's. */
const NESTED_CONTROL = "button, a, input, select, textarea, label, summary";

interface ActivatableProps {
  role: "button";
  tabIndex: 0;
  onClick: (event: MouseEvent<HTMLElement>) => void;
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}

/**
 * Props that make a non-`<button>` row or card operable by keyboard as well as by mouse.
 *
 * Use it where the clickable area is a layout block that cannot be a `<button>` (it holds
 * headings, flags or a nested control). Returns nothing without a handler, so a screen that
 * offers no navigation renders an inert element rather than a dead tab stop.
 */
export function activatable(onActivate: (() => void) | undefined): Partial<ActivatableProps> {
  if (!onActivate) return {};
  return {
    role: "button",
    tabIndex: 0,
    onClick: (event: MouseEvent<HTMLElement>) => {
      const nested = (event.target as Element).closest(NESTED_CONTROL);
      if (nested && nested !== event.currentTarget) return;
      onActivate();
    },
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      // A key pressed on a nested control (a checkbox, a link) belongs to that control.
      if (event.target !== event.currentTarget) return;
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      onActivate();
    },
  };
}
