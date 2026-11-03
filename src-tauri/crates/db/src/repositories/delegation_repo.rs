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
            let mut state: DelegationState =
                serde_json::from_str(&json).map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;
            // A save from before a duty existed has no setting for it.
            state.complete_settings();
            Ok(state)
        }
        None => Ok(DelegationState::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;
    use domain::delegation::{Duty, ProposalAction};

    /// Given a "Full" delegation saved before the tactical duty existed
    /// When the save is loaded
    /// Then it is still "Full" and the new duty is delegated like the others.
    #[test]
    fn a_full_delegation_saved_before_the_tactics_duty_loads_as_full() {
        let db = GameDatabase::open_in_memory().unwrap();
        let mut state = DelegationState::default();
        state.apply_mode(domain::delegation::DelegationMode::Full);
        state.settings.retain(|s| s.duty != Duty::Tactics);
        replace_delegation(db.conn(), &state).unwrap();

        let loaded = load_delegation(db.conn()).unwrap();

        assert_eq!(loaded.mode(), domain::delegation::DelegationMode::Full);
        assert!(loaded.is_delegated(Duty::Tactics));
    }

    /// Given an override in force and a fingerprint of what the assistant last wrote
    /// When the game is saved and loaded
    /// Then both are kept: the assistant still stands back after a reload.
    #[test]
    fn an_override_survives_a_reload() {
        let db = GameDatabase::open_in_memory().unwrap();
        let mut state = DelegationState::default();
        state.set(Duty::Lineup, true, false);
        state.record_applied(Duty::Lineup, "a,b,c");
        state.stand_back(Duty::Lineup, "2026-08-12");
        replace_delegation(db.conn(), &state).unwrap();

        let loaded = load_delegation(db.conn()).unwrap();

        assert_eq!(loaded.applied(Duty::Lineup), Some("a,b,c"));
        assert!(loaded.is_standing_back(Duty::Lineup, "2026-08-11"));
    }

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
        // The assistant coach's additions, at values that are not their defaults.
        state.apply_mode(domain::delegation::DelegationMode::Full);
        state.limits = domain::delegation::DelegationLimits {
            max_staff_wage: 1_111,
            max_player_wage: 2_222,
            max_commitment: 3_333_333,
            max_transfer_fee: 444_444,
        };
        state.last_digest = Some("2026-08-14".into());
        state.last_proposal_notice = Some("2026-08-10".into());
        state.propose(
            Duty::PlayerRenewals,
            "2026-08-03",
            ProposalAction::RenewPlayer {
                player_id: "p9".into(),
                wage: 7_000,
                years: 3,
            },
            "delegation.proposal.renewPlayer",
            vec![],
            "delegation.reason.aboveLimit",
        );
        state.push_log(domain::delegation::DelegationLogEntry {
            date: "2026-08-04".into(),
            duty: Duty::Lineup,
            title_key: "delegation.log.lineupChosen".into(),
            params: vec![("changes".into(), "2".into())],
            outcome: domain::delegation::DecisionOutcome::Declined,
        });
        replace_delegation(db.conn(), &state).unwrap();
        replace_delegation(db.conn(), &state).unwrap();
        assert_eq!(load_delegation(db.conn()).unwrap(), state);
    }
}
