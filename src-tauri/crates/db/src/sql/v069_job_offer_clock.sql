-- The days since the manager was last sent a job offer (`Game::days_since_last_job_offer`), which
-- decides on what day the next one arrives. NULL is "no clock running yet": what a save from before
-- this column held in memory after loading, so an old save reads exactly as it always did.
ALTER TABLE game_meta ADD COLUMN days_since_last_job_offer INTEGER;
