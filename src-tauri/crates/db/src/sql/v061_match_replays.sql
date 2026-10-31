-- V61: Immutable M12 replay payloads, scoped to a competition fixture identity.
CREATE TABLE IF NOT EXISTS match_replays (
    competition_id TEXT NOT NULL,
    fixture_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    replay_json TEXT NOT NULL,
    PRIMARY KEY (competition_id, fixture_id)
);
