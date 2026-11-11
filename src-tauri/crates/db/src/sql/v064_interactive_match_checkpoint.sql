-- V64: the one interactive (M12) match a career may have in progress. A singleton row, written in
-- the same transaction as the career, holding the compact checkpoint (input, configuration and the
-- journal of accepted commands) plus its checksum. No frames and no per-tick state are stored.
CREATE TABLE IF NOT EXISTS interactive_match_checkpoint (
    slot INTEGER PRIMARY KEY CHECK (slot = 1),
    competition_id TEXT NOT NULL,
    fixture_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    payload TEXT NOT NULL
);
