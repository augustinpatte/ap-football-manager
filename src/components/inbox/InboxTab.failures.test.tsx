import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n, { i18nReady } from "../../i18n";
import type { GameStateData, MessageData } from "../../store/gameStore";
import { createSeasonContext, createTeam } from "../../test-utils/factories";
import InboxTab from "./InboxTab";

/**
 * When the backend refuses something the manager asked of the Inbox, they are told. These used to
 * be swallowed: the button did nothing and gave no reason.
 */

const backend = { refuse: false, messages: [] as MessageData[] };

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    if (command === "get_messages_page") return Promise.resolve(backend.messages);
    if (command === "get_archived_messages")
      return Promise.resolve({ messages: [], has_more: false });
    if (backend.refuse) return Promise.reject("be.error.noActiveGameSession");
    return Promise.reject(new Error(`unexpected command ${command}`));
  },
}));

function message(overrides: Partial<MessageData> = {}): MessageData {
  return {
    id: "m1",
    subject: "Board review",
    body: "Please note.",
    sender: "Board",
    sender_role: "Chairman",
    date: "2026-08-09",
    read: true,
    category: "General",
    priority: "Normal",
    actions: [],
    context: {},
    ...overrides,
  } as MessageData;
}

function game(messages: MessageData[]): GameStateData {
  return {
    season_context: createSeasonContext(),
    clock: { current_date: "2026-08-10T00:00:00Z", start_date: "2026-07-01T00:00:00Z" },
    manager: {
      display_name: "Jane Doe",
      id: "manager-1",
      first_name: "Jane",
      last_name: "Doe",
      date_of_birth: "1980-01-01",
      nationality: "GB",
      reputation: 50,
      satisfaction: 50,
      fan_approval: 50,
      team_id: "team-1",
      career_stats: {
        matches_managed: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        trophies: 0,
        best_finish: null,
      },
      career_history: [],
    },
    teams: [createTeam()],
    players: [],
    staff: [],
    messages,
    news: [],
    league: null,
    scouting_assignments: [],
    board_objectives: [],
  };
}

function renderInbox(messages: MessageData[], initialMessageId?: string) {
  backend.messages = messages;
  render(
    <InboxTab
      gameState={game(messages)}
      initialMessageId={initialMessageId}
      onGameUpdate={vi.fn()}
      onNavigate={vi.fn()}
    />,
  );
}

describe("the Inbox tells the manager when the backend refuses", () => {
  beforeEach(async () => {
    await i18nReady;
    await i18n.changeLanguage("en");
    backend.refuse = true;
  });

  /**
   * Given a message with an Acknowledge button and a backend that refuses it,
   * when the manager presses the button,
   * then an alert gives the backend's reason in the current language, and the button stays so
   * they can try again.
   */
  it("explains a refused message action", async () => {
    renderInbox(
      [
        message({
          actions: [
            { id: "ack", label: "Understood", action_type: "Acknowledge", resolved: false },
          ],
        }),
      ],
      "m1",
    );

    fireEvent.click(await screen.findByRole("button", { name: "Understood" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("No active game session");
    expect(screen.getByRole("button", { name: "Understood" })).toBeEnabled();
  });

  /**
   * Given unread mail and a backend that refuses Mark all read,
   * when the manager presses it,
   * then an alert says so; and when the next attempt starts, the old alert is gone.
   */
  it("explains a refused Mark all read, and clears the notice on the next attempt", async () => {
    renderInbox([message({ read: false })]);

    fireEvent.click(await screen.findByRole("button", { name: /mark all read/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No active game session");

    backend.refuse = false;
    fireEvent.click(screen.getByRole("button", { name: /mark all read/i }));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });
});
