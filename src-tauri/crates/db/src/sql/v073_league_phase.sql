-- The state of a league-phase competition (domain::league_phase::LeaguePhaseState, JSON): pots,
-- draw seed, the derived table, the final ranking, the knockout ties and the edition's stage.
-- NULL for every other competition format and for every save written before the format existed;
-- those load exactly as they did.
ALTER TABLE competitions ADD COLUMN league_phase_json TEXT;
