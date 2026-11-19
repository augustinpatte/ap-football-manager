import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { TrainingOutlookData } from "../../services/trainingService";
import TrainingOutlookCard from "./TrainingOutlookCard";

// The suite's convention: `t` returns the key, with the parameters it was given.
vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: () => {} },
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params && Object.keys(params).length > 0 ? `${key} ${JSON.stringify(params)}` : key,
    i18n: { language: "en" },
  }),
}));

const outlook = (overrides: Partial<TrainingOutlookData> = {}): TrainingOutlookData => ({
  is_training_day: true,
  taper: null,
  session_intensity: "High",
  fixtures_this_week: 1,
  coaching_effect: 1.1,
  recovery_effect: 1.25,
  starters_condition: 82.4,
  delegated_to: null,
  proposal_waiting: false,
  ...overrides,
});

describe("TrainingOutlookCard", () => {
  /**
   * Given a plain training day with the manager in charge
   * When the card renders
   * Then it shows the session intensity, the staff effects and the starters' condition, and says
   *      the manager sets the plan.
   */
  it("shows the session, the staff effects and who sets the plan", () => {
    render(<TrainingOutlookCard outlook={outlook()} plannedIntensity="High" />);
    expect(
      screen.getByText('training.outlook.session {"intensity":"training.intensities.High.label"}'),
    ).toBeInTheDocument();
    expect(screen.getByText("+10%")).toBeInTheDocument();
    expect(screen.getByText("+25%")).toBeInTheDocument();
    expect(screen.getByText("82%")).toBeInTheDocument();
    expect(screen.getByText("training.outlook.setByYou")).toBeInTheDocument();
  });

  /**
   * Given a match two days away that eases a High plan to Medium
   * When the card renders
   * Then it says the session runs at Medium and that it was eased from High.
   */
  it("explains a near-match taper", () => {
    render(
      <TrainingOutlookCard
        outlook={outlook({ taper: "nearMatch", session_intensity: "Medium" })}
        plannedIntensity="High"
      />,
    );
    expect(
      screen.getByText(
        'training.outlook.session {"intensity":"training.intensities.Medium.label"}',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'training.outlook.taperNearMatch {"planned":"training.intensities.High.label"}',
      ),
    ).toBeInTheDocument();
  });

  /**
   * Given a congested week
   * When the card renders
   * Then it says how many matches make the week recovery work only.
   */
  it("explains a congested week", () => {
    render(
      <TrainingOutlookCard
        outlook={outlook({ taper: "congested", session_intensity: "Low", fixtures_this_week: 3 })}
        plannedIntensity="High"
      />,
    );
    expect(screen.getByText('training.outlook.taperCongested {"count":3}')).toBeInTheDocument();
  });

  /**
   * Given a rest day, training handed to a named coach, and a proposal waiting
   * When the card renders
   * Then it reports the rest day, names the coach with the post and rating, and points at the
   *      proposal.
   */
  it("names the delegate and the waiting proposal on a rest day", () => {
    render(
      <TrainingOutlookCard
        outlook={outlook({
          is_training_day: false,
          delegated_to: { name: "Ana Pires", post: "FitnessCoach", overall: 77 },
          proposal_waiting: true,
        })}
        plannedIntensity="Medium"
      />,
    );
    expect(screen.getByText("training.outlook.restDay")).toBeInTheDocument();
    expect(
      screen.getByText(
        'training.outlook.setByStaff {"name":"Ana Pires","post":"staff.posts.FitnessCoach","rating":77}',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("training.outlook.proposalWaiting")).toBeInTheDocument();
  });
});
