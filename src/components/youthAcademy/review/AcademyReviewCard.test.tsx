import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AcademyReview, AcademyRow } from "../../../services/academyService";
import AcademyReviewCard from "./AcademyReviewCard";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

const named = (key: string) => `${key}:${JSON.stringify({ player: "Kai Youth" })}`;
const PROMOTE = named("academy.promoteFor");
const LIST_LOAN = named("academy.listLoanFor");
const RELEASE = named("academy.releaseFor");

function row(overrides: Partial<AcademyRow> = {}): AcademyRow {
  return {
    player_id: "y1",
    full_name: "Kai Youth",
    age: 18,
    natural_position: "Midfielder",
    ovr: 58,
    potential: 74,
    expected_ovr_next_year: 62,
    verdict: "ready_for_senior",
    reasons: ["addsCover"],
    squad_cover: 55,
    seasons_left_in_academy: 4,
    injured: false,
    on_loan: false,
    loan_listed: false,
    contract_end: "2028-06-30",
    weekly_wage: 800,
    promotion_blocked: null,
    ...overrides,
  };
}

function review(rows: AcademyRow[], extra: Partial<AcademyReview> = {}): AcademyReview {
  return {
    rows,
    seniors: 24,
    senior_limit: 30,
    by_group: [1, 2, 2, 1],
    target_by_group: [1, 2, 2, 1],
    next_intake: 1,
    ...extra,
  };
}

function answer(value: AcademyReview | null, others: Record<string, unknown> = {}) {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_academy_review") return value;
    return others[command];
  });
}

describe("AcademyReviewCard", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /**
   * Given an academy with one player of each verdict
   * When the review loads
   * Then every verdict is named with its reasons, the capacity and next intake show, and potential
   *      is labelled as a ceiling.
   */
  it("names each verdict with its reasons", async () => {
    answer(
      review([
        row(),
        row({
          player_id: "y2",
          full_name: "Loan Lad",
          verdict: "loan_candidate",
          reasons: ["closeToSeniorLevel", "needsMatchMinutes"],
        }),
        row({
          player_id: "y3",
          full_name: "Slow Sam",
          verdict: "keep_developing",
          reasons: ["stillImproving"],
        }),
        row({
          player_id: "y4",
          full_name: "Last Lee",
          verdict: "unlikely_to_progress",
          reasons: ["ceilingBelowCover"],
        }),
      ]),
    );

    render(<AcademyReviewCard gameState={null} />);

    expect(await screen.findByText("academy.verdicts.ready_for_senior")).toBeInTheDocument();
    expect(screen.getByText("academy.verdicts.loan_candidate")).toBeInTheDocument();
    expect(screen.getByText("academy.verdicts.keep_developing")).toBeInTheDocument();
    expect(screen.getByText("academy.verdicts.unlikely_to_progress")).toBeInTheDocument();
    expect(screen.getByText("academy.reason.ceilingBelowCover")).toBeInTheDocument();
    expect(screen.getByText("academy.reason.needsMatchMinutes")).toBeInTheDocument();
    expect(screen.getByText(/academy\.capacity/)).toHaveTextContent('"seniors":24');
    expect(screen.getByText("academy.ceilingNote")).toBeInTheDocument();
  });

  /**
   * Given a ready youngster
   * When the manager promotes him
   * Then the squad-role command is sent for him as Senior, the game is forwarded and the review is
   *      asked again.
   */
  it("promotes a ready player through the existing squad command", async () => {
    answer(review([row()]), { set_player_squad_role: { promoted: true } });
    const onGameUpdate = vi.fn();

    render(<AcademyReviewCard gameState={null} onGameUpdate={onGameUpdate} />);
    fireEvent.click(await screen.findByRole("button", { name: PROMOTE }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_player_squad_role", {
        playerId: "y1",
        squadRole: "Senior",
      });
      expect(onGameUpdate).toHaveBeenCalledWith({ promoted: true });
    });
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([c]) => c === "get_academy_review").length).toBe(2),
    );
  });

  /**
   * Given a club with no senior place free
   * When the review loads
   * Then promotion is disabled for everyone rather than failing after the click.
   */
  it("disables promotion when no senior place is free", async () => {
    answer(review([row({ promotion_blocked: "be.error.squadFull" })], { seniors: 30 }));

    render(<AcademyReviewCard gameState={null} />);

    expect(await screen.findByRole("button", { name: PROMOTE })).toBeDisabled();
  });

  /**
   * Given a promotion the backend refuses
   * When the manager clicks Promote
   * Then the refusal is shown as an alert and nothing is forwarded.
   */
  it("shows a refused promotion", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_academy_review") return review([row()]);
      throw new Error("be.error.something");
    });
    const onGameUpdate = vi.fn();

    render(<AcademyReviewCard gameState={null} onGameUpdate={onGameUpdate} />);
    fireEvent.click(await screen.findByRole("button", { name: PROMOTE }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onGameUpdate).not.toHaveBeenCalled();
  });

  /**
   * Given an academy player
   * When the manager lists him for loan and then asks to release him
   * Then the loan list is toggled through the transfer command, and release opens his profile on
   *      the termination dialog rather than ending a contract on one click.
   */
  it("lists for loan and routes a release to the termination dialog", async () => {
    answer(review([row({ verdict: "loan_candidate", reasons: ["closeToSeniorLevel"] })]), {
      toggle_loan_list: { listed: true },
    });
    const onSelectPlayer = vi.fn();

    render(<AcademyReviewCard gameState={null} onSelectPlayer={onSelectPlayer} />);
    const table = await screen.findByRole("table");
    fireEvent.click(within(table).getByRole("button", { name: LIST_LOAN }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("toggle_loan_list", { playerId: "y1" }),
    );
    fireEvent.click(within(table).getByRole("button", { name: RELEASE }));
    expect(onSelectPlayer).toHaveBeenCalledWith("y1", { openTermination: true });
  });

  /**
   * Given a manager with no club
   * When the review loads
   * Then the card renders nothing but its frame and does not crash.
   */
  it("copes with no review", async () => {
    answer(null);

    render(<AcademyReviewCard gameState={null} />);

    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(screen.queryByRole("table")).toBeNull();
  });
});
