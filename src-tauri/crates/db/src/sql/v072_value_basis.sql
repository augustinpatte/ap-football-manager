-- What a player's market value was last set against (domain::player::ValueBasis, JSON), so the
-- season's end can carry the value by what he has become since. A save from before this column has
-- NULL: the first load anchors the basis to the value, rating, ceiling and age already stored, and
-- no value is repriced.
ALTER TABLE players ADD COLUMN value_basis TEXT;

-- The scale the career's market values sit on (ofm_core::value_scale::ValueScale, JSON), kept in the
-- save so a value reads the same whatever world is installed later. NULL reads as the native
-- formula: a career begun before scales were stored never silently adopts a newer world's scale.
ALTER TABLE game_meta ADD COLUMN value_scale TEXT;
