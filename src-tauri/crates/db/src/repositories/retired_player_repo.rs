use crate::compression::{deflate, inflate};
use domain::retired_player::RetiredPlayer;
use rusqlite::{Connection, params};

const GAME_PERSISTENCE_LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const GAME_PERSISTENCE_WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// Removes the active rows of players who have retired, and nothing else.
///
/// Called before the roster is written. A retiree's row still holds his club and shirt number in
/// the file, and at the rollover the shirt he leaves is free in memory: a recruit who joins the
/// club the same day is given it, the recruit's row is written first, and the club's unique shirt
/// index refuses it. The save then failed on every attempt from the rollover on. Removing the
/// rows first is what [`archive_retired_players`] does at the end anyway.
pub fn remove_active_rows(conn: &Connection, retired: &[RetiredPlayer]) -> Result<(), String> {
    for player in retired {
        conn.execute("DELETE FROM players WHERE id = ?1", params![player.id])
            .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    }
    Ok(())
}

/// Archives retirees and removes their active rows, in the caller's transaction so the two
/// halves commit together or not at all. Insert-only by id: a record already archived is
/// never rewritten, whatever the payload now says.
pub fn archive_retired_players(conn: &Connection, retired: &[RetiredPlayer]) -> Result<(), String> {
    // The record is stored deflated (v068); a retiree archived before that keeps his JSON column
    // and is still read.
    let records = retired
        .iter()
        .map(|player| {
            let json = serde_json::to_string(player)
                .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
            deflate(&json).map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (player, record_z) in retired.iter().zip(records) {
        conn.execute(
            "INSERT OR IGNORE INTO retired_players (player_id, record_json, record_z)
             VALUES (?1, '', ?2)",
            params![player.id, record_z],
        )
        .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
        conn.execute("DELETE FROM players WHERE id = ?1", params![player.id])
            .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    }
    Ok(())
}

/// Reads the whole archive back, ordered by id so a load is deterministic. A record that no
/// longer parses fails the load rather than silently dropping a person.
pub fn load_retired_players(conn: &Connection) -> Result<Vec<RetiredPlayer>, String> {
    let mut stmt = conn
        .prepare("SELECT record_json, record_z FROM retired_players ORDER BY player_id")
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
    })
    .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?
    .map(|row| {
        let (json, compressed) = row.map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
        let json = match compressed {
            Some(bytes) => inflate(&bytes).map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?,
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
    use domain::player::{Player, PlayerAttributes, Position};

    fn retiree(id: &str) -> RetiredPlayer {
        let level = 70;
        let attributes = PlayerAttributes {
            pace: level,
            stamina: level,
            strength: level,
            agility: level,
            passing: level,
            shooting: level,
            tackling: level,
            dribbling: level,
            defending: level,
            positioning: level,
            vision: level,
            decisions: level,
            composure: level,
            aggression: level,
            teamwork: level,
            leadership: level,
            handling: level,
            reflexes: level,
            aerial: level,
        };
        let mut player = Player::new(
            id.to_string(),
            "Old".to_string(),
            "Old Pro".to_string(),
            "1990-01-01".to_string(),
            "England".to_string(),
            Position::Midfielder,
            attributes,
        );
        // A career of fifteen seasons, as a retiree has: the record is mostly this history, which
        // is what deflate shrinks.
        player.career = (2016..2031)
            .map(|season| domain::player::CareerEntry {
                season,
                team_id: "club-0001".to_string(),
                team_name: "Harbour United".to_string(),
                appearances: 30,
                goals: 4,
                assists: 6,
                synthetic: false,
            })
            .collect();
        RetiredPlayer::from_player(&player, "2030-06-30", 2030)
    }

    /// Given a retiree archived by this build,
    /// When the archive is read back,
    /// Then he is the same person, and his row holds the deflated record and no JSON beside it.
    #[test]
    fn a_new_retiree_is_stored_deflated_and_reads_back_whole() {
        let db = GameDatabase::open_in_memory().unwrap();
        let original = retiree("p-new");
        archive_retired_players(db.conn(), std::slice::from_ref(&original)).unwrap();

        let (json, z): (String, Option<Vec<u8>>) = db
            .conn()
            .query_row(
                "SELECT record_json, record_z FROM retired_players WHERE player_id = 'p-new'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(json.is_empty(), "no JSON beside the deflated record");
        let json_len = serde_json::to_string(&original).unwrap().len();
        assert!(
            z.as_ref().is_some_and(|z| z.len() * 3 < json_len),
            "{:?} deflated vs {json_len} of JSON",
            z.map(|z| z.len())
        );
        assert_eq!(load_retired_players(db.conn()).unwrap(), vec![original]);
    }

    /// Given a retiree archived before v068, whose row holds JSON and no deflated record,
    /// When the archive is read next to a new one,
    /// Then both load: nothing was rewritten and nothing is lost.
    #[test]
    fn a_retiree_archived_as_json_still_loads_beside_a_deflated_one() {
        let db = GameDatabase::open_in_memory().unwrap();
        let legacy = retiree("p-legacy");
        db.conn()
            .execute(
                "INSERT INTO retired_players (player_id, record_json) VALUES (?1, ?2)",
                params!["p-legacy", serde_json::to_string(&legacy).unwrap()],
            )
            .unwrap();
        let fresh = retiree("p-new");
        archive_retired_players(db.conn(), std::slice::from_ref(&fresh)).unwrap();

        let loaded = load_retired_players(db.conn()).unwrap();

        assert_eq!(
            loaded,
            vec![legacy, fresh],
            "ordered by id: legacy, then new"
        );
    }

    /// Given a deflated record that has been damaged,
    /// When the archive is read,
    /// Then the load fails with its translated error rather than dropping a person.
    #[test]
    fn a_damaged_deflated_retiree_fails_the_load() {
        let db = GameDatabase::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO retired_players (player_id, record_json, record_z) VALUES ('p', '', ?1)",
                params![vec![1u8, 2, 3, 4]],
            )
            .unwrap();

        assert_eq!(
            load_retired_players(db.conn()).unwrap_err(),
            "be.error.gamePersistence.loadFailed"
        );
    }
}
