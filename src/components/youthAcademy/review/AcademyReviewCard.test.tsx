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
    ceiling: {
      kind: "estimate",
      low: 68,
      high: 79,
      confidence: "moderate",
      current: true,
      scouted_on: "2026-09-15",
    },
    verdict: "ready_for_senior",
    reasons: ["addsCover"],
    squad_cover: 55,
    seasons_left_in_academy: 4,
    injured: false,
    on_loan: false,
    loan_listed: false,
    contract_end: "2028-06-30",
    weekly_wage: 800,
    training_focus: null,
    promotion_blocked: null,
    advice: {
      promote: { state: "recommended", reasons: ["promote.ready"] },
      loan: { state: "available", reasons: ["loan.promoteInstead"] },
      keep: { state: "available", reasons: [] },
      renew: null,
    },
    ...overrides,
  };
}

function review(rows: AcademyRow[], extra: Partial<AcademyReview> = {}): AcademyReview {
  return {
    rows,
    summary: {
      coaching: {
        rating: 55,
        has_staff: true,
        confidence: "moderate",
        band_width: 12,
        progression_bonus_pct: 2,
      },
      average_ceiling: 74,
      high_ceiling_count: 0,
    },
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
   * Then promotion is disabled rather than failing after the click, and the reason is on screen
   *      and tied to the button.
   */
  it("disables promotion when no senior place is free and says why", async () => {
    answer(
      review(
        [
          row({
            promotion_blocked: "be.error.squadFull",
            advice: {
              promote: { state: "blocked", reasons: ["promote.noPlace"] },
              loan: { state: "available", reasons: ["loan.promoteInstead"] },
              keep: { state: "available", reasons: [] },
              renew: null,
            },
          }),
        ],
        { seniors: 30 },
      ),
    );

    render(<AcademyReviewCard gameState={null} />);

    const promote = await screen.findByRole("button", { name: PROMOTE });
    expect(promote).toBeDisabled();
    expect(screen.getByText("academy.advice.promote.noPlace")).toBeInTheDocument();
    expect(promote).toHaveAccessibleDescription("academy.advice.promote.noPlace");
  });

  /**
   * Given a youngster whose ceiling the youth staff assessed as a band
   * When the review loads
   * Then the band is shown as an estimate with its confidence, his rating is exact, and no
   *      expected-rating column or bare potential figure is on the screen.
   */
  it("shows the ceiling as an assessed band, never a number", async () => {
    answer(review([row()]));

    render(<AcademyReviewCard gameState={null} />);

    const table = await screen.findByRole("table");
    expect(within(table).getByText("68–79")).toBeInTheDocument();
    expect(within(table).getByText(/playerRating\.a11y\.potentialEstimate/)).toHaveTextContent(
      '"low":68',
    );
    expect(within(table).queryByText("academy.expectedNext")).toBeNull();
    expect(within(table).getByText("58")).toBeInTheDocument();
  });

  /**
   * Given a club whose youth department reads ceilings to a stated width
   * When the review loads
   * Then the coaching note explains the width and the progression bonus.
   */
  it("explains how well the youth staff can read a ceiling", async () => {
    answer(review([row()]));

    render(<AcademyReviewCard gameState={null} />);

    expect(await screen.findByText(/academy\.coachingSummary/)).toHaveTextContent('"width":12');
    expect(screen.getByText(/academy\.coachingBonus/)).toHaveTextContent('"percent":2');
  });

  /**
   * Given an academy player and the manager's choice of an individual focus
   * When he picks Technical
   * Then the existing training command is sent for that player and the review is asked again.
   */
  it("sets an individual training focus through the training command", async () => {
    answer(review([row()]), { set_player_training_focus: { updated: true } });

    render(<AcademyReviewCard gameState={null} />);
    fireEvent.click(
      await screen.findByRole("combobox", {
        name: `academy.trainingFocusFor:${JSON.stringify({ player: "Kai Youth" })}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("option", { name: "training.focuses.Technical.label" }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_player_training_focus", {
        playerId: "y1",
        focus: "Technical",
      }),
    );
  });

  /**
   * Given a youngster whose contract ends soon, and one whose contract runs on
   * When the review loads
   * Then only the first offers a renewal, which opens his profile on the renewal dialog.
   */
  it("offers a renewal only when the contract is ending", async () => {
    answer(
      review([
        row({
          advice: {
            promote: { state: "available", reasons: ["promote.notNeeded"] },
            loan: { state: "available", reasons: ["loan.notNeeded"] },
            keep: { state: "recommended", reasons: ["keep.developing"] },
            renew: { state: "recommended", reasons: ["renew.endsSoon"] },
          },
        }),
        row({ player_id: "y2", full_name: "Steady Sam" }),
      ]),
    );
    const onSelectPlayer = vi.fn();

    render(<AcademyReviewCard gameState={null} onSelectPlayer={onSelectPlayer} />);
    const renew = await screen.findByRole("button", {
      name: `academy.renewFor:${JSON.stringify({ player: "Kai Youth" })}`,
    });
    fireEvent.click(renew);

    expect(onSelectPlayer).toHaveBeenCalledWith("y1", { openRenewal: true });
    expect(screen.getAllByRole("button", { name: /academy\.renewFor/ })).toHaveLength(1);
  });

  /**
   * Given a youngster out on loan
   * When the review loads
   * Then promotion and the loan list are disabled and the reasons say he is out.
   */
  it("disables promotion and the loan list for a player out on loan", async () => {
    answer(
      review([
        row({
          on_loan: true,
          advice: {
            promote: { state: "blocked", reasons: ["promote.onLoan"] },
            loan: { state: "blocked", reasons: ["loan.alreadyOut"] },
            keep: { state: "available", reasons: [] },
            renew: null,
          },
        }),
      ]),
    );

    render(<AcademyReviewCard gameState={null} />);

    expect(await screen.findByRole("button", { name: PROMOTE })).toBeDisabled();
    expect(screen.getByRole("button", { name: LIST_LOAN })).toBeDisabled();
    expect(screen.getByText("academy.advice.loan.alreadyOut")).toBeInTheDocument();
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
