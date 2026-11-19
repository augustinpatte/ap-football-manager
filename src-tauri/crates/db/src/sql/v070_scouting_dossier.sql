-- The club's scouting dossier: the reports it holds on players (estimate ranges, never true
-- values) and the manager's shortlist. One row: ofm_core::scouting_dossier::ScoutingDossier.
-- A save from before this table has no row and loads an empty dossier.
CREATE TABLE scouting_dossier (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    state_json  TEXT NOT NULL
);
