//! Versioned storage for M12 replay documents.

use rusqlite::{Connection, OptionalExtension, params};

pub const REPLAY_SCHEMA_VERSION: i64 = 1;

pub fn save(
    connection: &Connection,
    competition_id: &str,
    fixture_id: &str,
    replay_json: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT OR REPLACE INTO match_replays (competition_id, fixture_id, schema_version, replay_json) VALUES (?1, ?2, ?3, ?4)",
            params![competition_id, fixture_id, REPLAY_SCHEMA_VERSION, replay_json],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn load(
    connection: &Connection,
    competition_id: &str,
    fixture_id: &str,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT replay_json FROM match_replays WHERE competition_id = ?1 AND fixture_id = ?2 AND schema_version = ?3",
            params![competition_id, fixture_id, REPLAY_SCHEMA_VERSION],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{load, save};
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
}
