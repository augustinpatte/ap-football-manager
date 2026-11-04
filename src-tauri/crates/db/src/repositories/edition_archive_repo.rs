use domain::edition_archive::CompletedEdition;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::io::{Read, Write};

const GAME_PERSISTENCE_LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const GAME_PERSISTENCE_WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// A record's JSON, deflated. Level 6 is the default balance: the archive is written once per
/// edition, and level 9 saved under 2% more on measured careers for twice the time.
fn deflate(json: &str) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(json.as_bytes())
        .and_then(|()| encoder.finish())
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())
}

fn inflate(bytes: &[u8]) -> Result<String, String> {
    let mut json = String::new();
    ZlibDecoder::new(bytes)
        .read_to_string(&mut json)
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
    Ok(json)
}

/// Insert-only: a recorded edition is frozen, so an existing row is never rewritten or deleted.
/// Serializes every record before the first insert and relies on the caller's transaction
/// (`write_game`) for all-or-nothing writes. The record is stored deflated (v067); an edition
/// recorded before that keeps its JSON column and is still read.
pub(crate) fn persist_new_editions(
    conn: &Connection,
    editions: &[CompletedEdition],
) -> Result<(), String> {
    // Only what the file does not hold: a save happens every day of play, and serializing and
    // deflating the whole archive each time would cost more with every season played.
    let mut held_stmt = conn
        .prepare("SELECT competition_id, season FROM competition_edition_archive")
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    let held: HashSet<(String, u32)> = held_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    let new_editions: Vec<&CompletedEdition> = editions
        .iter()
        .filter(|edition| !held.contains(&(edition.competition_id.clone(), edition.season)))
        .collect();
    let records = new_editions
        .iter()
        .map(|edition| {
            let json = serde_json::to_string(edition)
                .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
            deflate(&json)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (edition, record_z) in new_editions.into_iter().zip(records) {
        conn.execute(
            "INSERT OR IGNORE INTO competition_edition_archive
                (competition_id, season, record_json, record_z)
             VALUES (?1, ?2, '', ?3)",
            params![edition.competition_id, edition.season, record_z],
        )
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    }
    Ok(())
}

pub fn load_editions(conn: &Connection) -> Result<Vec<CompletedEdition>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT record_json, record_z FROM competition_edition_archive
             ORDER BY competition_id, season",
        )
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
    })
    .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?
    .map(|row| {
        let (json, compressed) = row.map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
        let json = match compressed {
            Some(bytes) => inflate(&bytes)?,
            None => json,
        };
        serde_json::from_str(&json).map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;
    use crate::repositories::competition_repo;
    use domain::league::{League, StandingEntry};

    fn edition(competition_id: &str, season: u32, champion: &str) -> CompletedEdition {
        CompletedEdition {
            competition_id: competition_id.into(),
            season,
            completed_on: "2033-06-30".into(),
            champion_id: champion.into(),
            participant_ids: vec!["a".into(), "b".into()],
            standings: vec![StandingEntry {
                points: 7,
                goals_for: 5,
                ..StandingEntry::new(champion.into())
            }],
            groups: Vec::new(),
            knockout_rounds: Vec::new(),
            fixtures: Vec::new(),
            movements: None,
        }
    }

    fn json(editions: &[CompletedEdition]) -> serde_json::Value {
        serde_json::to_value(editions).unwrap()
    }

    /// Given non-default editions, when saved and reloaded, then every field survives.
    #[test]
    fn editions_survive_sqlite_reload() {
        let db = GameDatabase::open_in_memory().unwrap();
        let saved = vec![
            edition("ar-d1-apertura", 2033, "a"),
            edition("ar-cup", 2033, "b"),
        ];
        persist_new_editions(db.conn(), &saved).unwrap();
        let mut loaded = load_editions(db.conn()).unwrap();
        loaded.sort_by(|x, y| x.competition_id.cmp(&y.competition_id));
        let mut expected = saved;
        expected.sort_by(|x, y| x.competition_id.cmp(&y.competition_id));
        assert_eq!(json(&loaded), json(&expected));
    }

    /// Given a recorded edition, when competitions are replaced, then the archive remains.
    #[test]
    fn replacing_competitions_does_not_delete_the_archive() {
        let db = GameDatabase::open_in_memory().unwrap();
        persist_new_editions(db.conn(), &[edition("ar-d1-apertura", 2033, "a")]).unwrap();
        competition_repo::replace_competitions(
            db.conn(),
            &[League::new(
                "ar-d1-apertura".into(),
                "Opening".into(),
                2034,
                &["a".into(), "b".into()],
            )],
        )
        .unwrap();
        assert_eq!(load_editions(db.conn()).unwrap().len(), 1);
    }

    /// Given a recorded edition, when a different record claims its key, then the first record wins.
    #[test]
    fn a_recorded_edition_is_never_rewritten() {
        let db = GameDatabase::open_in_memory().unwrap();
        persist_new_editions(db.conn(), &[edition("ar-d1-apertura", 2033, "a")]).unwrap();
        persist_new_editions(db.conn(), &[edition("ar-d1-apertura", 2033, "b")]).unwrap();
        let loaded = load_editions(db.conn()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].champion_id, "a");
    }

    /// Given a corrupt record, when loaded, then load fails through the translated error.
    #[test]
    fn corrupt_archive_record_is_not_silently_dropped() {
        let db = GameDatabase::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO competition_edition_archive (competition_id, season, record_json)
                 VALUES ('x', 1, '{not json')",
                [],
            )
            .unwrap();
        assert_eq!(
            load_editions(db.conn()).unwrap_err(),
            "be.error.gamePersistence.loadFailed"
        );
    }

    /// Given an edition recorded before the archive was compressed (its JSON in `record_json`),
    /// When the archive is loaded,
    /// Then it reads as it always did, next to a new edition stored deflated.
    #[test]
    fn a_legacy_json_edition_loads_beside_a_compressed_one() {
        let db = GameDatabase::open_in_memory().unwrap();
        let legacy = edition("legacy", 2031, "a");
        db.conn()
            .execute(
                "INSERT INTO competition_edition_archive (competition_id, season, record_json)
                 VALUES ('legacy', 2031, ?1)",
                [serde_json::to_string(&legacy).unwrap()],
            )
            .unwrap();
        persist_new_editions(db.conn(), &[edition("new", 2032, "b")]).unwrap();

        let loaded = load_editions(db.conn()).unwrap();

        assert_eq!(
            loaded
                .iter()
                .map(|e| e.champion_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    /// Given a new edition with a season's fixtures,
    /// When it is stored,
    /// Then the row holds the deflated record, far smaller than the JSON, and no JSON beside it.
    #[test]
    fn a_new_edition_is_stored_deflated() {
        let db = GameDatabase::open_in_memory().unwrap();
        let mut big = edition("league", 2033, "a");
        big.fixtures = (0..380)
            .map(|n| domain::league::Fixture {
                id: format!("fixture-{n:04}-0000-0000-0000-000000000000"),
                competition_id: "league".into(),
                matchday: n / 10 + 1,
                date: "2033-09-01".into(),
                home_team_id: "team-a".into(),
                away_team_id: "team-b".into(),
                ..Default::default()
            })
            .collect();
        let json_len = serde_json::to_string(&big).unwrap().len();
        persist_new_editions(db.conn(), &[big]).unwrap();

        let (json, z): (String, Vec<u8>) = db
            .conn()
            .query_row(
                "SELECT record_json, record_z FROM competition_edition_archive",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(json.is_empty());
        assert!(z.len() * 5 < json_len, "{} deflated vs {json_len}", z.len());
    }

    /// Given a deflated record that has been damaged,
    /// When the archive is loaded,
    /// Then the load fails through the translated error rather than dropping the edition.
    #[test]
    fn a_damaged_deflated_record_is_not_silently_dropped() {
        let db = GameDatabase::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO competition_edition_archive (competition_id, season, record_json, record_z)
                 VALUES ('x', 1, '', X'789cdeadbeef')",
                [],
            )
            .unwrap();
        assert_eq!(
            load_editions(db.conn()).unwrap_err(),
            "be.error.gamePersistence.loadFailed"
        );
    }
}
