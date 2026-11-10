-- V62: what completed transfers still owe: later instalments of a fee, sell-on clauses.
-- One row per obligation, JSON of domain::transfer_terms::TransferObligation. A save from before
-- this table reads as no obligations.
CREATE TABLE IF NOT EXISTS transfer_obligations (
    id              TEXT PRIMARY KEY,
    obligation_json TEXT NOT NULL
);
