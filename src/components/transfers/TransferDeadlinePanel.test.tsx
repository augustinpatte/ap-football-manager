import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { GameStateData } from "../../store/gameStore";
import {
  getTransferDeadlineSummary,
  type TransferDeadlineSummaryData,
} from "../../services/transfersService";
import TransferDeadlinePanel, { shouldShowDeadlinePanel } from "./TransferDeadlinePanel";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options ? `${key} ${JSON.stringify(options)}` : key,
    i18n: { language: "en" },
  }),
}));
vi.mock("../../services/transfersService", () => ({
  getTransferDeadlineSummary: vi.fn(),
}));
vi.mock("../../utils/backendI18n", () => ({
  resolveBackendError: (error: unknown) => `resolved:${String(error)}`,
}));

const mockedSummary = vi.mocked(getTransferDeadlineSummary);
const gameState = {} as GameStateData;

function summary(
  overrides: Partial<TransferDeadlineSummaryData> = {},
): TransferDeadlineSummaryData {
  return {
    window_open: true,
    phase: "deadline_day",
    days_remaining: 0,
    closes_on: "2026-09-01",
    urgent: true,
    items: [],
    urgent_response_count: 0,
    recent_deals: [],
    ...overrides,
  };
}

describe("TransferDeadlinePanel", () => {
  beforeEach(() => {
    mockedSummary.mockReset();
  });

  /**
   * Given an unanswered bid on deadline day
   * When the panel loads
   * Then the closing day, the count to answer and the bid itself are all on screen.
   */
  it("surfaces the deadline and the offer that needs an answer", async () => {
    mockedSummary.mockResolvedValue(
      summary({
        urgent_response_count: 1,
        items: [
          {
            kind: "transfer",
            state: "incoming_offer",
            player_id: "p1",
            player_name: "Pat Prospect",
            counterparty_team_id: "t2",
            counterparty_team_name: "Big Club",
            offer_id: "o1",
            fee: 900000,
            needs_response: true,
          },
        ],
      }),
    );

    render(<TransferDeadlinePanel gameState={gameState} onSelectPlayer={vi.fn()} />);

    expect(await screen.findByText("season.windowClosesToday")).toBeInTheDocument();
    expect(screen.getByText(/transfers.deadline.needsResponse/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pat Prospect" })).toBeInTheDocument();
    expect(screen.getByText("transfers.deadline.state.incoming_offer")).toBeInTheDocument();
  });

  /**
   * Given a window that is open but nowhere near closing, with nothing pending
   * When the panel loads
   * Then it stays out of the way.
   */
  it("renders nothing early in the window", async () => {
    mockedSummary.mockResolvedValue(summary({ phase: "mid", days_remaining: 20, urgent: false }));

    const { container } = render(
      <TransferDeadlinePanel gameState={gameState} onSelectPlayer={vi.fn()} />,
    );

    await waitFor(() => expect(mockedSummary).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  /**
   * Given the backend refuses to answer
   * When the panel loads
   * Then the failure is shown as an alert rather than swallowed.
   */
  it("shows an actionable error when the summary cannot be loaded", async () => {
    mockedSummary.mockRejectedValue("be.error.noActiveGameSession");

    render(<TransferDeadlinePanel gameState={gameState} onSelectPlayer={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "resolved:be.error.noActiveGameSession",
    );
  });

  /**
   * Given a mid-window summary that nevertheless has an unanswered offer, and a closed window
   * Then the panel shows for the former and never for the latter.
   */
  it("decides when it is worth showing", () => {
    expect(shouldShowDeadlinePanel(summary({ phase: "mid", urgent_response_count: 1 }))).toBe(true);
    expect(shouldShowDeadlinePanel(summary({ phase: "mid", urgent_response_count: 0 }))).toBe(
      false,
    );
    expect(shouldShowDeadlinePanel(summary({ window_open: false }))).toBe(false);
  });
});
