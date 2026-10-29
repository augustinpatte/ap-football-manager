use domain::delegation::DelegationState;
use rusqlite::{Connection, OptionalExtension, params};

const GAME_PERSISTENCE_LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const GAME_PERSISTENCE_WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// Stores what the manager has delegated to the staff. Runs in the caller's transaction.
pub fn replace_delegation(conn: &Connection, state: &DelegationState) -> Result<(), String> {
    let json =
        serde_json::to_string(state).map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO staff_delegation (id, state_json) VALUES (1, ?1)",
        params![json],
    )
    .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    Ok(())
}

/// The stored delegation; a save from before delegation existed has none and gets the default
/// (nothing delegated).
pub fn load_delegation(conn: &Connection) -> Result<DelegationState, String> {
    let json: Option<String> = conn
        .query_row(
            "SELECT state_json FROM staff_delegation WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
    match json {
        Some(json) => {
            serde_json::from_str(&json).map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())
        }
        None => Ok(DelegationState::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;
    use domain::delegation::{Duty, ProposalAction};

    #[test]
    fn delegation_round_trips_and_a_missing_row_means_nothing_delegated() {
        let db = GameDatabase::open_in_memory().unwrap();
        assert_eq!(
            load_delegation(db.conn()).unwrap(),
            DelegationState::default()
        );

        let mut state = DelegationState::default();
        state.set(Duty::Scouting, true, false);
        state.mark_run(Duty::Scouting, "2026-08-01");
        state.propose(
            Duty::StaffRecruitment,
            "2026-08-02",
            ProposalAction::HireStaff {
                staff_id: "s1".into(),
                wage: 900,
                years: 2,
            },
            "t",
            vec![("name".into(), "X".into())],
            "r",
        );
        replace_delegation(db.conn(), &state).unwrap();
        replace_delegation(db.conn(), &state).unwrap();
        assert_eq!(load_delegation(db.conn()).unwrap(), state);
    }
}
