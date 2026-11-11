// Representative M12 replay documents (schema_version 1) for tests. They are hand-built to the
// shape `spatial_match_output.rs` validates; they are NOT output of the real M12 engine, and no
// test using them proves anything about that engine.

export const MINUTE_MS = 60_000;

export type FixtureOptions = {
  homeStarters?: number;
  awayStarters?: number;
  homeBench?: number;
  awayBench?: number;
};

export type ReplayFixture = {
  json: string;
  document: Record<string, unknown>;
  roster: string[];
  homeIds: string[];
  awayIds: string[];
};

type FixturePlayer = { x: number; y: number; heading: number; on_pitch: boolean };

function ids(side: string, count: number): string[] {
  return Array.from({ length: count }, (_, index) => `${side}-${index + 1}`);
}

/**
 * Roster order is every home player (starters then bench), then every away player. Frames:
 * kick-off (t=0), a goal at 10', the away captain sent off at 30' and a home substitution at 60'.
 */
export function buildReplayFixture(options: FixtureOptions = {}): ReplayFixture {
  const { homeStarters = 11, awayStarters = 11, homeBench = 3, awayBench = 3 } = options;
  const homeIds = ids("home", homeStarters + homeBench);
  const awayIds = ids("away", awayStarters + awayBench);
  const roster = [...homeIds, ...awayIds];
  const starters = new Set([...homeIds.slice(0, homeStarters), ...awayIds.slice(0, awayStarters)]);

  const frame = (
    minute: number,
    overrides: Record<string, boolean>,
    score: { home: number; away: number },
  ) => ({
    t_ms: minute * MINUTE_MS,
    period: minute < 45 ? "first_half" : "second_half",
    ball: { x: 52.5, y: 34, z: 0 },
    score,
    players: roster.map(
      (playerId, index): FixturePlayer => ({
        x: 5 + (index % 11) * 9,
        y: 6 + Math.floor(index / 11) * 14,
        heading: 0,
        on_pitch: overrides[playerId] ?? starters.has(playerId),
      }),
    ),
  });

  const document = {
    schema_version: 1,
    home_team: "Home FC",
    away_team: "Away FC",
    roster,
    frames: [
      frame(0, {}, { home: 0, away: 0 }),
      frame(10, {}, { home: 1, away: 0 }),
      frame(30, { "away-1": false }, { home: 1, away: 0 }),
      frame(
        60,
        { "home-2": false, [homeIds[homeStarters]]: true, "away-1": false },
        { home: 1, away: 0 },
      ),
    ],
    events: [
      { t_ms: 10 * MINUTE_MS, type: "goal", side: "home", scorer: "home-9", assist: null },
      { t_ms: 30 * MINUTE_MS, type: "card", side: "away", player: "away-1", colour: "red" },
      {
        t_ms: 60 * MINUTE_MS,
        type: "substitution",
        side: "home",
        player_off: "home-2",
        player_on: homeIds[homeStarters],
      },
      { t_ms: 60 * MINUTE_MS, type: "shootout_kick", side: "home", player: "home-9" },
    ],
  };
  return { json: JSON.stringify(document), document, roster, homeIds, awayIds };
}

/** A completed result with its events but no recorded movement (frame recording off). */
export function buildEventsOnlyReplay(): string {
  const { document } = buildReplayFixture();
  return JSON.stringify({ ...document, frames: [] });
}
