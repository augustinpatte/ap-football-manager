-- V68: retired players and managers are stored deflated (Run F5.1).
--
-- A retiree's record is the whole person as JSON, written once and read only when a career is
-- loaded. After thirty seasons of an 80-club world the two archives held about 45 MB of a 168 MB
-- file (F5.0 report, section 12), and deflated they are a tenth of that.
--
-- New rows are written to `record_z` with an empty JSON column; a row written before this
-- migration keeps its JSON and is read as it always was. Nothing is rewritten.
ALTER TABLE retired_players ADD COLUMN record_z BLOB;
ALTER TABLE retired_managers ADD COLUMN record_z BLOB;
