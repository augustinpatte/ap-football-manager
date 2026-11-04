//! Versioned storage for M12 replay documents.

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use rusqlite::{Connection, OptionalExtension, params};
use std::io::{Read, Write};

pub const REPLAY_SCHEMA_VERSION: i64 = 1;

/// Replays a career keeps: the most recently stored ones. A replay is every player's and the ball's
/// position at every tick of a match, 96 MB as JSON and some 6 MB deflated; stored without a bound,
/// twenty matches watched cost over a hundred megabytes and a season's a good fraction of a gigabyte.
/// The match itself is kept in full by the result, the statistics and the match history; only the
/// 2D film of it is bounded, and nothing but watching it again needs it.
pub const REPLAYS_KEPT: i64 = 5;

fn deflate(json: &str) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(json.as_bytes())
        .and_then(|()| encoder.finish())
        .map_err(|error| error.to_string())
}

fn inflate(bytes: &[u8]) -> Result<String, String> {
    let mut json = String::new();
    ZlibDecoder::new(bytes)
        .read_to_string(&mut json)
        .map_err(|error| error.to_string())?;
    Ok(json)
}

/// Stores the replay deflated, then drops all but the [`REPLAYS_KEPT`] most recently stored. Storing
/// a fixture's replay again makes it the most recent (the row is replaced, and a replaced row is a
/// new one).
pub fn save(
    connection: &Connection,
    competition_id: &str,
    fixture_id: &str,
    replay_json: &str,
) -> Result<(), String> {
    let replay_z = deflate(replay_json)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO match_replays
                (competition_id, fixture_id, schema_version, replay_json, replay_z)
             VALUES (?1, ?2, ?3, '', ?4)",
            params![competition_id, fixture_id, REPLAY_SCHEMA_VERSION, replay_z],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM match_replays
             WHERE rowid NOT IN (SELECT rowid FROM match_replays ORDER BY rowid DESC LIMIT ?1)",
            [REPLAYS_KEPT],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn load(
    connection: &Connection,
    competition_id: &str,
    fixture_id: &str,
) -> Result<Option<String>, String> {
    let stored: Option<(String, Option<Vec<u8>>)> = connection
        .query_row(
            "SELECT replay_json, replay_z FROM match_replays
             WHERE competition_id = ?1 AND fixture_id = ?2 AND schema_version = ?3",
            params![competition_id, fixture_id, REPLAY_SCHEMA_VERSION],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match stored {
        Some((_, Some(bytes))) => inflate(&bytes).map(Some),
        Some((json, None)) => Ok(Some(json)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{REPLAY_SCHEMA_VERSION, REPLAYS_KEPT, load, save};
    use crate::game_database::GameDatabase;

    /// Given two fixtures in different competitions, when their replays are saved, then each stable key returns its own document.
    #[test]
    fn replays_are_scoped_by_competition_and_fixture() {
        let database = GameDatabase::open_in_memory().expect("database");
        save(
            database.conn(),
            "league-a",
            "fixture-1",
            "{\"home_team\":\"A\"}",
        )
        .expect("save");
        save(
            database.conn(),
            "cup-b",
            "fixture-1",
            "{\"home_team\":\"B\"}",
        )
        .expect("save");
        assert_eq!(
            load(database.conn(), "league-a", "fixture-1").expect("load"),
            Some("{\"home_team\":\"A\"}".into())
        );
        assert_eq!(
            load(database.conn(), "cup-b", "fixture-1").expect("load"),
            Some("{\"home_team\":\"B\"}".into())
        );
    }

    /// Given more replays stored than a career keeps,
    /// When one more is saved,
    /// Then the most recent ones are there and the oldest is gone.
    #[test]
    fn only_the_most_recent_replays_are_kept() {
        let database = GameDatabase::open_in_memory().expect("database");
        for n in 0..(REPLAYS_KEPT + 3) {
            save(
                database.conn(),
                "league",
                &format!("fixture-{n}"),
                &format!("{{\"n\":{n}}}"),
            )
            .expect("save");
        }
        let kept = |n: i64| load(database.conn(), "league", &format!("fixture-{n}")).expect("load");
        assert_eq!(kept(0), None);
        assert_eq!(kept(2), None);
        assert_eq!(kept(3), Some("{\"n\":3}".to_string()));
        assert_eq!(
            kept(REPLAYS_KEPT + 2),
            Some(format!("{{\"n\":{}}}", REPLAYS_KEPT + 2))
        );
    }

    /// Given a full set of replays,
    /// When the oldest is stored again,
    /// Then it is the most recent, and the next one saved removes the one that is now oldest.
    #[test]
    fn storing_a_replay_again_makes_it_the_most_recent() {
        let database = GameDatabase::open_in_memory().expect("database");
        for n in 0..REPLAYS_KEPT {
            save(database.conn(), "league", &format!("fixture-{n}"), "{}").expect("save");
        }
        save(database.conn(), "league", "fixture-0", "{\"again\":true}").expect("save");
        save(database.conn(), "league", "fixture-new", "{}").expect("save");
        let load = |f: &str| load(database.conn(), "league", f).expect("load");
        assert_eq!(load("fixture-1"), None);
        assert_eq!(load("fixture-0"), Some("{\"again\":true}".to_string()));
    }

    /// Given a replay as large as a real one,
    /// When it is stored,
    /// Then the row holds it deflated (a small fraction of the JSON), and it loads back exactly.
    #[test]
    fn a_replay_is_stored_deflated_and_loads_back_exactly() {
        let database = GameDatabase::open_in_memory().expect("database");
        let json = format!(
            "[{}]",
            (0..20_000)
                .map(|tick| format!(
                    "{{\"tick\":{tick},\"x\":{},\"y\":{}}}",
                    tick % 105,
                    tick % 68
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        save(database.conn(), "league", "fixture", &json).expect("save");
        let (stored_json, stored_z): (String, Vec<u8>) = database
            .conn()
            .query_row(
                "SELECT replay_json, replay_z FROM match_replays",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("row");
        assert!(stored_json.is_empty());
        assert!(stored_z.len() * 5 < json.len());
        assert_eq!(
            load(database.conn(), "league", "fixture").expect("load"),
            Some(json)
        );
    }

    /// Given a replay stored before replays were deflated,
    /// When it is loaded,
    /// Then it reads as it always did.
    #[test]
    fn a_legacy_json_replay_still_loads() {
        let database = GameDatabase::open_in_memory().expect("database");
        database
            .conn()
            .execute(
                "INSERT INTO match_replays (competition_id, fixture_id, schema_version, replay_json)
                 VALUES ('league', 'old', ?1, '{\"old\":1}')",
                [REPLAY_SCHEMA_VERSION],
            )
            .expect("insert");
        assert_eq!(
            load(database.conn(), "league", "old").expect("load"),
            Some("{\"old\":1}".to_string())
        );
    }
}
