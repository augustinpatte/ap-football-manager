-- V66: compact match history (Run F5.0).
--
-- v015 stored every player's line and every team's line with the match's identity copied into it:
-- six 36-character ids, the date, the competition and the score, on each of the ~27 rows of a
-- match, plus two indexes that repeat the keys (about 450 bytes a row, 12 KB a match). The lines
-- keep only what is theirs; the match's facts are written once per match and ids are written once
-- per id. Nothing is dropped: every record `StatsState` holds reads back unchanged.
--
-- Forward-only. The script runs after the verified pre-migration backup `GameDatabase::open`
-- takes of any save below the current version, which is the way back.
CREATE TABLE stat_ids (
    ref INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE
);

-- One row per distinct (fixture id, season, matchday, date, competition, clubs, score) a record
-- carries: normally one per match; a record that disagrees with its match's other records gets a
-- row of its own rather than being forced into theirs.
CREATE TABLE match_stat_fixtures (
    fixture_no INTEGER PRIMARY KEY,
    fixture_ref INTEGER NOT NULL,
    season INTEGER NOT NULL,
    matchday INTEGER NOT NULL,
    date TEXT NOT NULL,
    competition TEXT NOT NULL,
    home_ref INTEGER NOT NULL,
    away_ref INTEGER NOT NULL,
    home_goals INTEGER NOT NULL,
    away_goals INTEGER NOT NULL
);
CREATE INDEX idx_match_stat_fixtures_ref ON match_stat_fixtures(fixture_ref);

-- Rows are read back in rowid order, which is the order the history was captured in.
CREATE TABLE player_match_lines (
    fixture_no INTEGER NOT NULL,
    player_ref INTEGER NOT NULL,
    team_ref INTEGER NOT NULL,
    opponent_ref INTEGER NOT NULL,
    minutes_played INTEGER NOT NULL,
    goals INTEGER NOT NULL,
    assists INTEGER NOT NULL,
    shots INTEGER NOT NULL,
    shots_on_target INTEGER NOT NULL,
    passes_completed INTEGER NOT NULL,
    passes_attempted INTEGER NOT NULL,
    tackles_won INTEGER NOT NULL,
    interceptions INTEGER NOT NULL,
    fouls_committed INTEGER NOT NULL,
    yellow_cards INTEGER NOT NULL,
    red_cards INTEGER NOT NULL,
    rating REAL NOT NULL
);

CREATE TABLE team_match_lines (
    fixture_no INTEGER NOT NULL,
    team_ref INTEGER NOT NULL,
    opponent_ref INTEGER NOT NULL,
    goals_for INTEGER NOT NULL,
    goals_against INTEGER NOT NULL,
    possession_pct INTEGER NOT NULL,
    shots INTEGER NOT NULL,
    shots_on_target INTEGER NOT NULL,
    passes_completed INTEGER NOT NULL,
    passes_attempted INTEGER NOT NULL,
    tackles_won INTEGER NOT NULL,
    interceptions INTEGER NOT NULL,
    fouls_committed INTEGER NOT NULL,
    yellow_cards INTEGER NOT NULL,
    red_cards INTEGER NOT NULL
);

-- What the lines of seasons no longer kept add up to (`domain::stats::SEASONS_OF_LINES_KEPT`): the
-- sums the statistics overviews read, per player and per team, and the epoch they were written at.
CREATE TABLE player_stat_totals (
    player_ref INTEGER PRIMARY KEY,
    minutes_played INTEGER NOT NULL,
    shots INTEGER NOT NULL,
    shots_on_target INTEGER NOT NULL,
    passes_completed INTEGER NOT NULL,
    passes_attempted INTEGER NOT NULL,
    tackles_won INTEGER NOT NULL,
    interceptions INTEGER NOT NULL,
    fouls_committed INTEGER NOT NULL
);

CREATE TABLE team_stat_totals (
    team_ref INTEGER PRIMARY KEY,
    matches_played INTEGER NOT NULL,
    goals_for INTEGER NOT NULL,
    goals_against INTEGER NOT NULL,
    possession_total INTEGER NOT NULL,
    shots INTEGER NOT NULL,
    shots_on_target INTEGER NOT NULL,
    passes_completed INTEGER NOT NULL,
    passes_attempted INTEGER NOT NULL,
    tackles_won INTEGER NOT NULL,
    interceptions INTEGER NOT NULL,
    fouls_committed INTEGER NOT NULL
);

CREATE TABLE stat_totals_epoch (
    slot INTEGER PRIMARY KEY CHECK (slot = 1),
    epoch INTEGER NOT NULL
);
INSERT INTO stat_totals_epoch (slot, epoch) VALUES (1, 0);

-- Carry the existing history over.
INSERT OR IGNORE INTO stat_ids (id)
SELECT fixture_id FROM player_match_stats
UNION SELECT player_id FROM player_match_stats
UNION SELECT team_id FROM player_match_stats
UNION SELECT opponent_team_id FROM player_match_stats
UNION SELECT home_team_id FROM player_match_stats
UNION SELECT away_team_id FROM player_match_stats
UNION SELECT fixture_id FROM team_match_stats
UNION SELECT team_id FROM team_match_stats
UNION SELECT opponent_team_id FROM team_match_stats
UNION SELECT home_team_id FROM team_match_stats
UNION SELECT away_team_id FROM team_match_stats;

INSERT INTO match_stat_fixtures
    (fixture_ref, season, matchday, date, competition, home_ref, away_ref, home_goals, away_goals)
SELECT f.ref, m.season, m.matchday, m.date, m.competition, h.ref, a.ref, m.home_goals, m.away_goals
FROM (
    SELECT fixture_id, season, matchday, date, competition, home_team_id, away_team_id,
           home_goals, away_goals, MIN(src_rowid) AS first_seen
    FROM (
        SELECT rowid AS src_rowid, fixture_id, season, matchday, date, competition,
               home_team_id, away_team_id, home_goals, away_goals
        FROM player_match_stats
        UNION ALL
        SELECT 1000000000 + rowid, fixture_id, season, matchday, date, competition,
               home_team_id, away_team_id,
               CASE WHEN team_id = home_team_id THEN goals_for ELSE goals_against END,
               CASE WHEN team_id = home_team_id THEN goals_against ELSE goals_for END
        FROM team_match_stats
    )
    GROUP BY fixture_id, season, matchday, date, competition, home_team_id, away_team_id,
             home_goals, away_goals
) AS m
JOIN stat_ids f ON f.id = m.fixture_id
JOIN stat_ids h ON h.id = m.home_team_id
JOIN stat_ids a ON a.id = m.away_team_id
ORDER BY m.first_seen;

INSERT INTO player_match_lines
SELECT mf.fixture_no, pl.ref, tm.ref, op.ref, p.minutes_played, p.goals, p.assists, p.shots,
       p.shots_on_target, p.passes_completed, p.passes_attempted, p.tackles_won, p.interceptions,
       p.fouls_committed, p.yellow_cards, p.red_cards, p.rating
FROM player_match_stats p
JOIN stat_ids fx ON fx.id = p.fixture_id
JOIN stat_ids pl ON pl.id = p.player_id
JOIN stat_ids tm ON tm.id = p.team_id
JOIN stat_ids op ON op.id = p.opponent_team_id
JOIN stat_ids ho ON ho.id = p.home_team_id
JOIN stat_ids aw ON aw.id = p.away_team_id
JOIN match_stat_fixtures mf ON mf.fixture_ref = fx.ref AND mf.season = p.season
    AND mf.matchday = p.matchday AND mf.date = p.date AND mf.competition = p.competition
    AND mf.home_ref = ho.ref AND mf.away_ref = aw.ref AND mf.home_goals = p.home_goals
    AND mf.away_goals = p.away_goals
ORDER BY p.rowid;

INSERT INTO team_match_lines
SELECT mf.fixture_no, tm.ref, op.ref, t.goals_for, t.goals_against, t.possession_pct, t.shots,
       t.shots_on_target, t.passes_completed, t.passes_attempted, t.tackles_won, t.interceptions,
       t.fouls_committed, t.yellow_cards, t.red_cards
FROM team_match_stats t
JOIN stat_ids fx ON fx.id = t.fixture_id
JOIN stat_ids tm ON tm.id = t.team_id
JOIN stat_ids op ON op.id = t.opponent_team_id
JOIN stat_ids ho ON ho.id = t.home_team_id
JOIN stat_ids aw ON aw.id = t.away_team_id
JOIN match_stat_fixtures mf ON mf.fixture_ref = fx.ref AND mf.season = t.season
    AND mf.matchday = t.matchday AND mf.date = t.date AND mf.competition = t.competition
    AND mf.home_ref = ho.ref AND mf.away_ref = aw.ref
    AND mf.home_goals = CASE WHEN t.team_id = t.home_team_id THEN t.goals_for ELSE t.goals_against END
    AND mf.away_goals = CASE WHEN t.team_id = t.home_team_id THEN t.goals_against ELSE t.goals_for END
ORDER BY t.rowid;

DROP TABLE player_match_stats;
DROP TABLE team_match_stats;
