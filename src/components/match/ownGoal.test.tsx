import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import i18n, { i18nReady } from "../../i18n";
import { getCommentary } from "./commentary";
import { renderScorers } from "./PostMatchHelpers";
import { EventFeed } from "./MatchPanels";
import type { EnginePlayerData, MatchEvent, MatchSnapshot } from "./types";

const makePlayer = (id: string, name: string): EnginePlayerData =>
  ({ id, name, position: "DF" }) as unknown as EnginePlayerData;

// Marquinhos plays for Away; his own goal is credited to Home.
const snapshot = (events: MatchEvent[]): MatchSnapshot =>
  ({
    home_team: { id: "h", name: "Home FC", players: [makePlayer("p1", "Haaland")] },
    away_team: { id: "a", name: "Away FC", players: [makePlayer("p3", "Marquinhos")] },
    home_bench: [],
    away_bench: [],
    events,
  }) as unknown as MatchSnapshot;

const ownGoal: MatchEvent = {
  minute: 63,
  event_type: "Goal",
  side: "Home",
  zone: "Midfield",
  player_id: null,
  secondary_player_id: null,
  detail: { OwnGoal: { by: "p3" } },
};

let previousLanguage: string;

beforeAll(async () => {
  await i18nReady;
  previousLanguage = i18n.language;
  await i18n.changeLanguage("en");
});

afterAll(async () => {
  await i18n.changeLanguage(previousLanguage);
});

describe("an own goal", () => {
  /**
   * Given a goal credited to Home that Away's defender put into his own net,
   * When the feed asks for celebration commentary,
   * Then there is none: the lines would name a scorer that does not exist.
   */
  it("has no celebration commentary", () => {
    expect(getCommentary(ownGoal, snapshot([ownGoal]), i18n.t.bind(i18n))).toBeNull();
  });

  /**
   * Given the same goal,
   * When the post-match scorer list is drawn for the side it counts for,
   * Then it names the player who put it in, as an own goal, under the beneficiary.
   */
  it("is listed under the side it counts for, with the player who scored it", () => {
    const snap = snapshot([ownGoal]);
    render(<div>{renderScorers(snap, [ownGoal], "Home", i18n.t.bind(i18n))}</div>);
    expect(screen.getByText("Own goal: Marquinhos")).toBeInTheDocument();
    expect(screen.getByText("Home FC")).toBeInTheDocument();
  });

  /**
   * Given an ordinary goal next to the own goal,
   * When the scorer list is drawn,
   * Then the ordinary one is still the scorer's name alone.
   */
  it("does not change how an ordinary goal is listed", () => {
    const ordinary: MatchEvent = { ...ownGoal, minute: 10, player_id: "p1", detail: null };
    const snap = snapshot([ordinary, ownGoal]);
    render(<div>{renderScorers(snap, [ordinary, ownGoal], "Home", i18n.t.bind(i18n))}</div>);
    expect(screen.getByText("Haaland")).toBeInTheDocument();
    expect(screen.getByText("Own goal: Marquinhos")).toBeInTheDocument();
  });

  /**
   * Given the live event feed showing the own goal,
   * When it is drawn,
   * Then the line names the player who put it in, for the side it counts for.
   */
  it("is named in the event feed", () => {
    const snap = snapshot([ownGoal]);
    render(<EventFeed events={[ownGoal]} snapshot={snap} feedRef={{ current: null }} />);
    expect(screen.getByText("Own goal: Marquinhos")).toBeInTheDocument();
    expect(screen.getByText("Home FC")).toBeInTheDocument();
  });
});
