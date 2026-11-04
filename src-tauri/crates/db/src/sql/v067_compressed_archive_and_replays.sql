-- V67: compressed edition archive and replays (Run F5.0).
--
-- A finished edition's record is its whole fixture list as JSON, about 650 KB for a league season of
-- 380 matches, and it is written once and read only when a career is loaded. Deflated it is 14% of
-- that. A stored M12 replay is the position of every player and the ball at every tick: 96 MB of
-- JSON for one match, 6% of that deflated (both measured on generated careers; the figures are in
-- docs/APFM_RUN_F5_0_UNIVERSAL_SIMULATION.md).
--
-- New rows are written to the `_z` column with an empty JSON column; a row written before this
-- migration keeps its JSON and is read as it always was. Nothing is rewritten.
ALTER TABLE competition_edition_archive ADD COLUMN record_z BLOB;
ALTER TABLE match_replays ADD COLUMN replay_z BLOB;
