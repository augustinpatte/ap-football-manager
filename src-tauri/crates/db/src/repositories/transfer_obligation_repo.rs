use domain::transfer_terms::TransferObligation;
use rusqlite::{Connection, params};

const LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// Replaces the stored obligations with `obligations`. Runs in the caller's transaction, so a
/// failed save leaves the previous set intact.
pub fn replace_transfer_obligations(
    conn: &Connection,
    obligations: &[TransferObligation],
) -> Result<(), String> {
    conn.execute("DELETE FROM transfer_obligations", [])
        .map_err(|_| WRITE_ERROR.to_string())?;
    for obligation in obligations {
        let json = serde_json::to_string(obligation).map_err(|_| WRITE_ERROR.to_string())?;
        conn.execute(
            "INSERT INTO transfer_obligations (id, obligation_json) VALUES (?1, ?2)",
            params![obligation.id(), json],
        )
        .map_err(|_| WRITE_ERROR.to_string())?;
    }
    Ok(())
}

pub fn load_transfer_obligations(conn: &Connection) -> Result<Vec<TransferObligation>, String> {
    let mut statement = conn
        .prepare("SELECT obligation_json FROM transfer_obligations ORDER BY rowid")
        .map_err(|_| LOAD_ERROR.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| LOAD_ERROR.to_string())?;
    let mut obligations = Vec::new();
    for json in rows {
        let json = json.map_err(|_| LOAD_ERROR.to_string())?;
        obligations.push(serde_json::from_str(&json).map_err(|_| LOAD_ERROR.to_string())?);
    }
    Ok(obligations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;

    fn instalment(id: &str, paid_on: Option<&str>) -> TransferObligation {
        TransferObligation::Instalment {
            id: id.to_string(),
            player_id: "p1".to_string(),
            payer_team_id: "buyer".to_string(),
            payee_team_id: "seller".to_string(),
            amount: 750_000,
            due: "2027-08-01".to_string(),
            number: 2,
            of: 3,
            paid_on: paid_on.map(str::to_string),
        }
    }

    /// Given an empty database, When obligations are loaded, Then there are none (an older save).
    #[test]
    fn a_save_without_obligations_loads_none() {
        let db = GameDatabase::open_in_memory().unwrap();
        assert!(load_transfer_obligations(db.conn()).unwrap().is_empty());
    }

    /// Given a paid instalment, an unpaid one and a sell-on clause,
    /// When they are saved, replaced by a shorter set, and loaded,
    /// Then every field of what remains survives and the removed rows are gone.
    #[test]
    fn obligations_round_trip_and_replacement_drops_the_rest() {
        let db = GameDatabase::open_in_memory().unwrap();
        let all = vec![
            instalment("a", Some("2027-08-01")),
            instalment("b", None),
            TransferObligation::SellOn {
                id: "c".to_string(),
                player_id: "p1".to_string(),
                holder_team_id: "buyer".to_string(),
                beneficiary_team_id: "seller".to_string(),
                pct: 15,
                settled_on: None,
            },
        ];
        replace_transfer_obligations(db.conn(), &all).unwrap();
        assert_eq!(load_transfer_obligations(db.conn()).unwrap(), all);

        let kept = vec![all[1].clone()];
        replace_transfer_obligations(db.conn(), &kept).unwrap();
        assert_eq!(load_transfer_obligations(db.conn()).unwrap(), kept);
    }
}
