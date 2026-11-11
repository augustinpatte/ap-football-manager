use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const GAME_PERSISTENCE_LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const GAME_PERSISTENCE_WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// Game metadata stored as a singleton row in `game_meta`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMeta {
    pub save_id: String,
    pub save_name: String,
    pub manager_id: String,
    pub start_date: String,
    pub game_date: String,
    pub created_at: String,
    pub last_played_at: String,
    #[serde(default = "default_vacant_team_days_json")]
    pub vacant_team_days_json: String,
    #[serde(default = "default_world_history_json")]
    pub world_history_json: String,
    #[serde(default)]
    pub available_staff_market_last_activity_date: Option<String>,
    #[serde(default = "default_save_format_version")]
    pub save_format_version: u32,
    #[serde(default = "default_world_format_version")]
    pub world_format_version: u32,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub source_world_id: String,
    #[serde(default)]
    pub source_world_kind: String,
    #[serde(default = "default_active_ids_json")]
    pub active_region_ids_json: String,
    #[serde(default = "default_active_ids_json")]
    pub active_competition_ids_json: String,
    #[serde(default = "default_extra_translations_json")]
    pub extra_translations_json: String,
    #[serde(default = "default_package_lockfile_json")]
    pub package_lockfile_json: String,
    #[serde(default = "default_emitted_events_json")]
    pub emitted_events_json: String,
    /// `Game::seed`, stored as the `i64` with the same bits: SQLite integers are
    /// signed and a seed is a full `u64`. 0 is a save from before games had one.
    #[serde(default)]
    pub seed: i64,
    /// `Game::legacy_world_cup_draw`.
    #[serde(default)]
    pub legacy_world_cup_draw: bool,
    /// `Game::legacy_tactics_imported`.
    #[serde(default)]
    pub legacy_tactics_imported: bool,
    /// `Game::days_since_last_job_offer`. `None` is "no clock running", which is also what a save
    /// from before the column holds.
    #[serde(default)]
    pub days_since_last_job_offer: Option<u32>,
}

fn default_vacant_team_days_json() -> String {
    "{}".to_string()
}

fn default_world_history_json() -> String {
    "{}".to_string()
}

fn default_emitted_events_json() -> String {
    "[]".to_string()
}

/// Current game-data save format this build writes. Bumped when the in-memory
/// `Game` shape changes in a way that needs an on-load migration. The loader
/// rejects saves newer than this and migrates + restamps older ones.
/// v3 = `competitions` is the source of truth (legacy `game.league` demoted to a
/// back-compat mirror, populated from it on load for pre-v3 saves).
/// v4 = opening AI loan listings are seeded once for existing careers.
/// v5 = the inbox sent-ledger (`Game::emitted_events`); pre-v5 saves have theirs
/// seeded from the inbox on load, which must not happen to a v5 save whose
/// ledger is merely empty.
/// v7 = the game has a seed (`Game::seed`); a pre-v7 save is given one derived
/// from its save id on load, which must not happen to a v7 save.
/// v8 = World Cups are drawn from the game's seed; a pre-v8 save keeps drawing them from the
/// cup year alone (`Game::legacy_world_cup_draw`), which a v8 save must not be marked as.
/// v9 = retired players live in the `retired_players` archive, not in `game.players`; a pre-v9
/// save has its flagged retirees moved there once on load, and an older build must not open a
/// v9 save and find them gone.
pub const CURRENT_SAVE_FORMAT_VERSION: u32 = 9;

/// Baseline for a save that predates the version field entirely (reads as the
/// pre-gate format, so it gets migrated and restamped to current on load).
fn default_save_format_version() -> u32 {
    2
}

fn default_world_format_version() -> u32 {
    2
}

fn default_active_ids_json() -> String {
    "[]".to_string()
}

fn default_extra_translations_json() -> String {
    "{}".to_string()
}

fn default_package_lockfile_json() -> String {
    "[]".to_string()
}

/// Insert or replace the singleton game_meta row.
pub fn upsert_meta(conn: &Connection, meta: &GameMeta) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO game_meta (id, save_id, save_name, manager_id, start_date, game_date, created_at, last_played_at, vacant_team_days_json, world_history_json, available_staff_market_last_activity_date, save_format_version, world_format_version, app_version, source_world_id, source_world_kind, active_region_ids_json, active_competition_ids_json, extra_translations_json, package_lockfile_json, emitted_events_json, seed, legacy_world_cup_draw, legacy_tactics_imported, days_since_last_job_offer)
         VALUES ('singleton', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            meta.save_id,
            meta.save_name,
            meta.manager_id,
            meta.start_date,
            meta.game_date,
            meta.created_at,
            meta.last_played_at,
            meta.vacant_team_days_json,
            meta.world_history_json,
            meta.available_staff_market_last_activity_date,
            meta.save_format_version,
            meta.world_format_version,
            meta.app_version,
            meta.source_world_id,
            meta.source_world_kind,
            meta.active_region_ids_json,
            meta.active_competition_ids_json,
            meta.extra_translations_json,
            meta.package_lockfile_json,
            meta.emitted_events_json,
            meta.seed,
            meta.legacy_world_cup_draw,
            meta.legacy_tactics_imported,
            meta.days_since_last_job_offer,
        ],
    )
    .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    Ok(())
}

/// Load the singleton game_meta row. Returns None if no meta exists.
pub fn load_meta(conn: &Connection) -> Result<Option<GameMeta>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT save_id, save_name, manager_id, start_date, game_date, created_at, last_played_at, vacant_team_days_json, world_history_json, available_staff_market_last_activity_date, save_format_version, world_format_version, app_version, source_world_id, source_world_kind, active_region_ids_json, active_competition_ids_json, extra_translations_json, package_lockfile_json, emitted_events_json, seed, legacy_world_cup_draw, legacy_tactics_imported, days_since_last_job_offer
             FROM game_meta WHERE id = 'singleton'",
        )
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;

    let mut rows = stmt
        .query_map([], |row| {
            Ok(GameMeta {
                save_id: row.get(0)?,
                save_name: row.get(1)?,
                manager_id: row.get(2)?,
                start_date: row.get(3)?,
                game_date: row.get(4)?,
                created_at: row.get(5)?,
                last_played_at: row.get(6)?,
                vacant_team_days_json: row.get(7)?,
                world_history_json: row.get(8)?,
                available_staff_market_last_activity_date: row.get(9)?,
                save_format_version: row.get(10).unwrap_or(default_save_format_version()),
                world_format_version: row.get(11).unwrap_or(default_world_format_version()),
                app_version: row.get(12).unwrap_or_default(),
                source_world_id: row.get(13).unwrap_or_default(),
                source_world_kind: row.get(14).unwrap_or_default(),
                active_region_ids_json: row.get(15).unwrap_or_else(|_| default_active_ids_json()),
                active_competition_ids_json: row
                    .get(16)
                    .unwrap_or_else(|_| default_active_ids_json()),
                extra_translations_json: row
                    .get(17)
                    .unwrap_or_else(|_| default_extra_translations_json()),
                package_lockfile_json: row
                    .get(18)
                    .unwrap_or_else(|_| default_package_lockfile_json()),
                emitted_events_json: row
                    .get(19)
                    .unwrap_or_else(|_| default_emitted_events_json()),
                // Always present once migrated, so an error here is a damaged save, not an
                // old one: propagated, rather than read as "no seed" and silently reseeded.
                seed: row.get(20)?,
                legacy_world_cup_draw: row.get(21)?,
                legacy_tactics_imported: row.get(22)?,
                // A clock is a few days, so a stored value outside `u32` is damage, and refusing
                // to open a career over a timer would cost more than restarting it.
                days_since_last_job_offer: row
                    .get::<_, Option<i64>>(23)?
                    .and_then(|days| u32::try_from(days).ok()),
            })
        })
        .map_err(|_| GAME_PERSISTENCE_LOAD_ERROR.to_string())?;

    match rows.next() {
        Some(Ok(meta)) => Ok(Some(meta)),
        Some(Err(_)) => Err(GAME_PERSISTENCE_LOAD_ERROR.to_string()),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;

    fn test_db() -> GameDatabase {
        GameDatabase::open_in_memory().unwrap()
    }

    #[test]
    fn test_upsert_and_load_meta() {
        let db = test_db();
        let meta = GameMeta {
            save_id: "save-001".to_string(),
            save_name: "Test Career".to_string(),
            manager_id: "mgr_user".to_string(),
            start_date: "2026-07-01T00:00:00Z".to_string(),
            game_date: "2026-07-15T00:00:00Z".to_string(),
            created_at: "2026-03-05T18:00:00Z".to_string(),
            last_played_at: "2026-03-05T19:00:00Z".to_string(),
            vacant_team_days_json: "{}".to_string(),
            world_history_json: "{}".to_string(),
            available_staff_market_last_activity_date: Some("2026-07-01".to_string()),
            save_format_version: 2,
            world_format_version: 2,
            app_version: String::new(),
            source_world_id: String::new(),
            source_world_kind: String::new(),
            active_region_ids_json: "[]".to_string(),
            active_competition_ids_json: "[]".to_string(),
            extra_translations_json: "{}".to_string(),
            package_lockfile_json: "[]".to_string(),
            emitted_events_json: "[]".to_string(),
            seed: 0,
            legacy_world_cup_draw: false,
            legacy_tactics_imported: false,
            days_since_last_job_offer: None,
        };

        upsert_meta(db.conn(), &meta).unwrap();
        let loaded = load_meta(db.conn()).unwrap().unwrap();

        assert_eq!(loaded.save_id, "save-001");
        assert_eq!(loaded.save_name, "Test Career");
        assert_eq!(loaded.manager_id, "mgr_user");
        assert_eq!(loaded.game_date, "2026-07-15T00:00:00Z");
        assert_eq!(loaded.world_history_json, "{}");
        assert_eq!(
            loaded.available_staff_market_last_activity_date.as_deref(),
            Some("2026-07-01")
        );
    }

    /// A meta row with every field at a value a default would not produce, for the tests below.
    fn meta_with_clock(clock: Option<u32>) -> GameMeta {
        GameMeta {
            save_id: "save-clock".to_string(),
            save_name: "Clock".to_string(),
            manager_id: "mgr_user".to_string(),
            start_date: "2026-07-01T00:00:00Z".to_string(),
            game_date: "2026-07-15T00:00:00Z".to_string(),
            created_at: "2026-03-05T18:00:00Z".to_string(),
            last_played_at: "2026-03-05T19:00:00Z".to_string(),
            vacant_team_days_json: "{}".to_string(),
            world_history_json: "{}".to_string(),
            available_staff_market_last_activity_date: None,
            save_format_version: CURRENT_SAVE_FORMAT_VERSION,
            world_format_version: 2,
            app_version: String::new(),
            source_world_id: String::new(),
            source_world_kind: String::new(),
            active_region_ids_json: "[]".to_string(),
            active_competition_ids_json: "[]".to_string(),
            extra_translations_json: "{}".to_string(),
            package_lockfile_json: "[]".to_string(),
            emitted_events_json: "[]".to_string(),
            seed: 1,
            legacy_world_cup_draw: false,
            legacy_tactics_imported: false,
            days_since_last_job_offer: clock,
        }
    }

    /// Given a meta row with the job-offer clock at seven days
    /// When it is written and read through a second connection to the same file
    /// Then the clock is seven, and replacing the row with "no clock" reads back as none.
    #[test]
    fn the_job_offer_clock_is_stored_in_the_row_and_none_is_stored_as_null() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock.db");
        let writer = GameDatabase::open(&path).unwrap();
        upsert_meta(writer.conn(), &meta_with_clock(Some(7))).unwrap();

        let reader = GameDatabase::open(&path).unwrap();
        assert_eq!(
            load_meta(reader.conn())
                .unwrap()
                .unwrap()
                .days_since_last_job_offer,
            Some(7)
        );

        upsert_meta(writer.conn(), &meta_with_clock(None)).unwrap();
        let stored: Option<i64> = reader
            .conn()
            .query_row(
                "SELECT days_since_last_job_offer FROM game_meta",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, None, "no clock must be NULL, not a number");
        assert_eq!(
            load_meta(reader.conn())
                .unwrap()
                .unwrap()
                .days_since_last_job_offer,
            None
        );
    }

    /// Given a stored clock outside `u32` (a damaged value)
    /// When the row is loaded
    /// Then the career still opens, with the clock restarted rather than the load refused.
    #[test]
    fn a_damaged_job_offer_clock_restarts_instead_of_refusing_the_load() {
        let db = test_db();
        upsert_meta(db.conn(), &meta_with_clock(Some(3))).unwrap();
        for damaged in [-1_i64, i64::from(u32::MAX) + 1] {
            db.conn()
                .execute(
                    "UPDATE game_meta SET days_since_last_job_offer = ?1",
                    [damaged],
                )
                .unwrap();

            let loaded = load_meta(db.conn()).unwrap().unwrap();

            assert_eq!(loaded.days_since_last_job_offer, None, "stored {damaged}");
        }
    }

    /// Given meta JSON written before the field existed
    /// When it is deserialised
    /// Then the clock is absent, as every old save read it.
    #[test]
    fn meta_json_without_a_job_offer_clock_deserialises_to_none() {
        let mut value = serde_json::to_value(meta_with_clock(Some(5))).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("days_since_last_job_offer");

        let meta: GameMeta = serde_json::from_value(value).unwrap();

        assert_eq!(meta.days_since_last_job_offer, None);
    }

    #[test]
    fn test_load_meta_empty() {
        let db = test_db();
        let loaded = load_meta(db.conn()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_upsert_meta_overwrites() {
        let db = test_db();
        let meta1 = GameMeta {
            save_id: "save-001".to_string(),
            save_name: "Career v1".to_string(),
            manager_id: "mgr_user".to_string(),
            start_date: "2026-07-01T00:00:00Z".to_string(),
            game_date: "2026-07-15T00:00:00Z".to_string(),
            created_at: "2026-03-05T18:00:00Z".to_string(),
            last_played_at: "2026-03-05T19:00:00Z".to_string(),
            vacant_team_days_json: "{}".to_string(),
            world_history_json: "{}".to_string(),
            available_staff_market_last_activity_date: None,
            save_format_version: 2,
            world_format_version: 2,
            app_version: String::new(),
            source_world_id: String::new(),
            source_world_kind: String::new(),
            active_region_ids_json: "[]".to_string(),
            active_competition_ids_json: "[]".to_string(),
            extra_translations_json: "{}".to_string(),
            package_lockfile_json: "[]".to_string(),
            emitted_events_json: "[]".to_string(),
            seed: 0,
            legacy_world_cup_draw: false,
            legacy_tactics_imported: false,
            days_since_last_job_offer: None,
        };
        upsert_meta(db.conn(), &meta1).unwrap();

        let meta2 = GameMeta {
            save_id: "save-001".to_string(),
            save_name: "Career v2".to_string(),
            manager_id: "mgr_user".to_string(),
            start_date: "2026-07-01T00:00:00Z".to_string(),
            game_date: "2026-08-01T00:00:00Z".to_string(),
            created_at: "2026-03-05T18:00:00Z".to_string(),
            last_played_at: "2026-03-06T10:00:00Z".to_string(),
            vacant_team_days_json: "{}".to_string(),
            world_history_json: r#"{"rivalries":[{"team_a_id":"team-1","team_b_id":"team-2","intensity":80}],"season_awards":[]}"#.to_string(),
            available_staff_market_last_activity_date: Some("2026-08-01".to_string()),
            save_format_version: 2,
            world_format_version: 2,
            app_version: String::new(),
            source_world_id: String::new(),
            source_world_kind: String::new(),
            active_region_ids_json: "[]".to_string(),
            active_competition_ids_json: "[]".to_string(),
            extra_translations_json: "{}".to_string(),
            package_lockfile_json: "[]".to_string(),
            emitted_events_json: "[]".to_string(),
            seed: 0,
            legacy_world_cup_draw: false,
            legacy_tactics_imported: false,
            days_since_last_job_offer: None,
        };
        upsert_meta(db.conn(), &meta2).unwrap();

        let loaded = load_meta(db.conn()).unwrap().unwrap();
        assert_eq!(loaded.save_name, "Career v2");
        assert_eq!(loaded.game_date, "2026-08-01T00:00:00Z");
        assert!(loaded.world_history_json.contains("rivalries"));
        assert_eq!(
            loaded.available_staff_market_last_activity_date.as_deref(),
            Some("2026-08-01")
        );
    }

    #[test]
    fn test_upsert_meta_returns_backend_key_when_schema_is_missing() {
        let conn = Connection::open_in_memory().unwrap();
        let meta = GameMeta {
            save_id: "save-001".to_string(),
            save_name: "Test Career".to_string(),
            manager_id: "mgr_user".to_string(),
            start_date: "2026-07-01T00:00:00Z".to_string(),
            game_date: "2026-07-15T00:00:00Z".to_string(),
            created_at: "2026-03-05T18:00:00Z".to_string(),
            last_played_at: "2026-03-05T19:00:00Z".to_string(),
            vacant_team_days_json: "{}".to_string(),
            world_history_json: "{}".to_string(),
            available_staff_market_last_activity_date: None,
            save_format_version: 2,
            world_format_version: 2,
            app_version: String::new(),
            source_world_id: String::new(),
            source_world_kind: String::new(),
            active_region_ids_json: "[]".to_string(),
            active_competition_ids_json: "[]".to_string(),
            extra_translations_json: "{}".to_string(),
            package_lockfile_json: "[]".to_string(),
            emitted_events_json: "[]".to_string(),
            seed: 0,
            legacy_world_cup_draw: false,
            legacy_tactics_imported: false,
            days_since_last_job_offer: None,
        };

        let result = upsert_meta(&conn, &meta);

        assert_eq!(result.unwrap_err(), GAME_PERSISTENCE_WRITE_ERROR);
    }

    #[test]
    fn test_load_meta_returns_backend_key_when_schema_is_missing() {
        let conn = Connection::open_in_memory().unwrap();

        let result = load_meta(&conn);

        assert_eq!(result.unwrap_err(), GAME_PERSISTENCE_LOAD_ERROR);
    }
}
