-- What a profile may state as fact about a player: whether his earlier career is known, and the
-- club outside the world that owns his contract, if any. JSON of domain::player::PlayerProvenance.
-- A save from before this column has NULL, which loads as the default (earlier career as the
-- generator wrote it, no outside owner): old saves are not reclassified.
ALTER TABLE players ADD COLUMN provenance TEXT;
