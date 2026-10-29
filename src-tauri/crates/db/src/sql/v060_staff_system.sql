-- The seat a staff member fills (when finer than their role) and the FM24 attributes behind
-- their four OFM attributes; and what the manager has delegated to the staff.
ALTER TABLE staff ADD COLUMN post TEXT;
ALTER TABLE staff ADD COLUMN detail_json TEXT;

-- One row: domain::delegation::DelegationState.
CREATE TABLE staff_delegation (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    state_json  TEXT NOT NULL
);
