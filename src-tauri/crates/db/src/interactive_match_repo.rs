//! Storage for the interactive match a career has in progress.
//!
//! One row at most (`slot = 1`): a career has one active fixture. The payload is opaque here, it is
//! produced and validated by `ofm_core::interactive_match`; this module only guarantees that what
//! comes back is what went in (a checksum) and that it was written by a schema this build reads.
//! Writing `None` deletes the row, which is how a finished match stops being resumable.

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

/// Bumped when the stored envelope (not the payload, which versions itself) changes.
pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMatch {
    pub competition_id: String,
    pub fixture_id: String,
    pub payload: String,
}

/// Why a stored match could not be read back. The career still loads without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// Written by a build with a different envelope schema.
    IncompatibleSchema(i64),
    /// The payload no longer matches its checksum.
    Corrupt,
}

fn checksum(payload: &str) -> String {
    Sha256::digest(payload.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Replaces the stored match (`Some`) or clears it (`None`).
pub fn replace(connection: &Connection, stored: Option<&StoredMatch>) -> Result<(), String> {
    connection
        .execute("DELETE FROM interactive_match_checkpoint", [])
        .map_err(|error| error.to_string())?;
    if let Some(stored) = stored {
        connection
            .execute(
                "INSERT INTO interactive_match_checkpoint \
                 (slot, competition_id, fixture_id, schema_version, checksum, payload) \
                 VALUES (1, ?1, ?2, ?3, ?4, ?5)",
                params![
                    stored.competition_id,
                    stored.fixture_id,
                    SCHEMA_VERSION,
                    checksum(&stored.payload),
                    stored.payload
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// The stored match, `Ok(None)` when there is none, `Ok(Some(Err(_)))` when there is one that cannot
/// be trusted (the caller discards it and tells the player; the career is unaffected).
pub fn load(connection: &Connection) -> Result<Option<Result<StoredMatch, Unreadable>>, String> {
    let row = connection
        .query_row(
            "SELECT competition_id, fixture_id, schema_version, checksum, payload \
             FROM interactive_match_checkpoint WHERE slot = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(
        row.map(|(competition_id, fixture_id, version, sum, payload)| {
            if version != SCHEMA_VERSION {
                Err(Unreadable::IncompatibleSchema(version))
            } else if checksum(&payload) != sum {
                Err(Unreadable::Corrupt)
            } else {
                Ok(StoredMatch {
                    competition_id,
                    fixture_id,
                    payload,
                })
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;

    fn stored(payload: &str) -> StoredMatch {
        StoredMatch {
            competition_id: "league".into(),
            fixture_id: "fixture-7".into(),
            payload: payload.into(),
        }
    }

    /// Given an empty career database,
    /// When nothing was ever stored,
    /// Then there is no match to resume (an old save behaves like this).
    #[test]
    fn a_career_with_no_stored_match_has_none() {
        let database = GameDatabase::open_in_memory().expect("database");
        assert_eq!(load(database.conn()).expect("load"), None);
    }

    /// Given a stored match,
    /// When it is read back,
    /// Then the identity and the payload are exactly what was written.
    #[test]
    fn a_stored_match_comes_back_unchanged() {
        let database = GameDatabase::open_in_memory().expect("database");
        replace(database.conn(), Some(&stored("{\"a\":1}"))).expect("write");
        assert_eq!(
            load(database.conn()).expect("load"),
            Some(Ok(stored("{\"a\":1}")))
        );
    }

    /// Given a stored match,
    /// When another is stored and then `None`,
    /// Then there is only ever one row, and `None` clears it (a finished match is not resumable).
    #[test]
    fn storing_replaces_and_none_clears() {
        let database = GameDatabase::open_in_memory().expect("database");
        replace(database.conn(), Some(&stored("first"))).expect("write");
        replace(database.conn(), Some(&stored("second"))).expect("write");
        assert_eq!(
            load(database.conn()).expect("load"),
            Some(Ok(stored("second")))
        );
        let rows: i64 = database
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM interactive_match_checkpoint",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(rows, 1);
        replace(database.conn(), None).expect("clear");
        assert_eq!(load(database.conn()).expect("load"), None);
    }

    /// Given a stored match whose payload was altered on disk,
    /// When it is read,
    /// Then it is reported as corrupt, never handed back as if it were sound.
    #[test]
    fn a_tampered_payload_is_detected() {
        let database = GameDatabase::open_in_memory().expect("database");
        replace(database.conn(), Some(&stored("{\"score\":\"1-0\"}"))).expect("write");
        database
            .conn()
            .execute(
                "UPDATE interactive_match_checkpoint SET payload = '{\"score\":\"9-0\"}'",
                [],
            )
            .expect("tamper");
        assert_eq!(
            load(database.conn()).expect("load"),
            Some(Err(Unreadable::Corrupt))
        );
    }

    /// Given a row written under another envelope schema,
    /// When it is read,
    /// Then it is reported as incompatible with that version, not misread.
    #[test]
    fn another_schema_version_is_reported_not_misread() {
        let database = GameDatabase::open_in_memory().expect("database");
        replace(database.conn(), Some(&stored("x"))).expect("write");
        database
            .conn()
            .execute(
                "UPDATE interactive_match_checkpoint SET schema_version = 99",
                [],
            )
            .expect("alter");
        assert_eq!(
            load(database.conn()).expect("load"),
            Some(Err(Unreadable::IncompatibleSchema(99)))
        );
    }
}
