import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { MessageData } from "../../store/gameStore";
import InboxMessageListPane from "./InboxMessageListPane";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const message: MessageData = {
  id: "m1",
  subject: "Board meeting",
  body: "",
  sender: "Chairman",
  sender_role: "Board",
  date: "2026-08-01",
  read: false,
  category: "Board",
  priority: "Normal",
  actions: [],
  context: { team_id: null, player_id: null, fixture_id: null, match_result: null },
};

function renderPane(overrides: Partial<Parameters<typeof InboxMessageListPane>[0]> = {}) {
  const props = {
    bulkSelectionEnabled: false,
    filteredMessages: [message],
    hasSelectedMessage: false,
    language: "en",
    selectedMessageId: null,
    selectedMessageIds: [],
    onSelectMessage: vi.fn(),
    onToggleMessageSelection: vi.fn(),
    onRequestDeleteMessage: vi.fn(),
    onRequestMarkMessageRead: vi.fn(),
    ...overrides,
  };
  render(<InboxMessageListPane {...props} />);
  return props;
}

describe("InboxMessageListPane rows", () => {
  /**
   * Given a message in the list
   * When a keyboard user focuses its row and presses Enter, then Space
   * Then the message is opened each time.
   */
  it("opens a message from the keyboard", () => {
    const { onSelectMessage } = renderPane();
    const row = screen.getByTestId("inbox-row-m1");

    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });

    expect(row).toHaveAttribute("tabindex", "0");
    expect(onSelectMessage).toHaveBeenCalledTimes(2);
    expect(onSelectMessage).toHaveBeenCalledWith("m1");
  });

  /**
   * Given bulk selection is on
   * When the row's checkbox is clicked
   * Then it toggles the selection and does not also open the message.
   */
  it("toggles a checkbox without opening the message", () => {
    const { onSelectMessage, onToggleMessageSelection } = renderPane({
      bulkSelectionEnabled: true,
    });

    fireEvent.click(screen.getByTestId("inbox-select-message-m1"));

    expect(onToggleMessageSelection).toHaveBeenCalledWith("m1");
    expect(onSelectMessage).not.toHaveBeenCalled();
  });
});
