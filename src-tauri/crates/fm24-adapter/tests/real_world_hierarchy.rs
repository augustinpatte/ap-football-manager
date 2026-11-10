//! Comparative checks of the rating hierarchy against the user's own FM24 `world.db`.
//!
//! Skipped (ignored) unless run on purpose: the data is a commercial game database that is not in
//! this repository and must never be. Nothing here names a player or a rating: the pairs of FM
//! person ids to compare come from a local JSON file the user writes, and the test only asserts
//! the *order* the converted ratings must keep.
//!
//! ```bash
//! APFM_WORLD_DB=/path/to/world.db APFM_RATING_PAIRS=/path/to/pairs.json \
//!   cargo test -p fm24-adapter --test real_world_hierarchy -- --ignored --nocapture
//! ```
//!
//! `pairs.json`: `[{"stronger": 1234, "weaker": 5678}, ...]`, FM person ids as in `person.id`.

use fm24_adapter::journal::Journal;
use fm24_adapter::source::Source;
use fm24_adapter::{Config, pipeline};
use std::path::PathBuf;

#[test]
#[ignore = "needs the user's world.db: set APFM_WORLD_DB and APFM_RATING_PAIRS"]
fn the_stronger_player_of_each_pair_is_rated_higher() {
    let (Some(db), Some(pairs)) = (
        std::env::var_os("APFM_WORLD_DB").map(PathBuf::from),
        std::env::var_os("APFM_RATING_PAIRS").map(PathBuf::from),
    ) else {
        eprintln!("APFM_WORLD_DB / APFM_RATING_PAIRS not set: nothing to compare");
        return;
    };
    let pairs: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(pairs).expect("pairs file"))
            .expect("pairs json");
    let source = Source::open(&db).expect("world.db opens read-only");
    let schema = source.schema_facts().expect("schema");
    let perimeter = source.perimeter(schema.start_season).expect("perimeter");
    let club_ids: Vec<i64> = perimeter.iter().map(|(_, club)| *club).collect();
    let players = source.players(&club_ids).expect("players");
    let cfg = Config::default_config();
    let rate = |person_id: i64| -> u8 {
        let row = players
            .iter()
            .find(|row| row.person_id == person_id)
            .unwrap_or_else(|| panic!("person {person_id} is not in the perimeter"));
        pipeline::rate_for_comparison(row, &cfg, &mut Journal::default())
            .unwrap_or_else(|| panic!("person {person_id} has no usable position"))
    };
    for pair in &pairs {
        let stronger = pair["stronger"].as_i64().expect("stronger id");
        let weaker = pair["weaker"].as_i64().expect("weaker id");
        let (high, low) = (rate(stronger), rate(weaker));
        assert!(
            high > low,
            "person {stronger} is rated {high} but person {weaker} {low}"
        );
    }
}
