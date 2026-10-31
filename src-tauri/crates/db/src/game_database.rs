use log::{debug, error, info};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::migrations::{
    MIGRATION_COUNT, all_migrations, expected_layout, first_layout_difference, layout_of,
    migration_owned_tables, needs_run2_dev_repair, repair_run2_dev_save,
};
use crate::save_load_error::SaveLoadError;

const GAME_DATABASE_OPEN_FAILED: &str = "be.error.gameDatabase.openFailed";
const GAME_DATABASE_MIGRATION_FAILED: &str = "be.error.gameDatabase.migrationFailed";
const GAME_DATABASE_SCHEMA_VERSION_READ_FAILED: &str =
    "be.error.gameDatabase.schemaVersionReadFailed";
const GAME_DATABASE_CURRENT_VERSION_READ_FAILED: &str =
    "be.error.gameDatabase.currentVersionReadFailed";

fn verify_layout_matches_version(
    conn: &Connection,
    save_version: usize,
    path: &Path,
) -> Result<(), SaveLoadError> {
    let expected = expected_layout(save_version).map_err(|e| {
        error!("[game_db] cannot build the reference layout v{save_version}: {e}");
        SaveLoadError::Unknown
    })?;
    let found = layout_of(conn).map_err(|e| {
        error!("[game_db] cannot read the layout of {path:?}: {e}");
        SaveLoadError::Corrupted
    })?;
    let owned = migration_owned_tables().map_err(|e| {
        error!("[game_db] cannot list the tables the migrations own: {e}");
        SaveLoadError::Unknown
    })?;
    match first_layout_difference(&expected, &found, &owned) {
        None => Ok(()),
        Some(first_difference) => {
            error!("[game_db] save {path:?} claims schema v{save_version} but {first_difference}");
            Err(SaveLoadError::IncompatibleLayout { first_difference })
        }
    }
}

/// Upgrades a save written by a Run 2 development build (see `migrations::needs_run2_dev_repair`).
/// A copy of the file is taken first; if it cannot be taken the save is left untouched and the
/// caller sees the original, unrepaired outcome.
fn reconcile_run2_dev_save(conn: &mut Connection, path: Option<&Path>) {
    match needs_run2_dev_repair(conn) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            error!("[game_db] cannot inspect {path:?} for a Run 2 development save: {e}");
            return;
        }
    }
    if let Some(path) = path {
        let backup = path.with_extension("db.pre-v063.bak");
        if !backup.exists()
            && let Err(e) = std::fs::copy(path, &backup)
        {
            error!("[game_db] no backup of {path:?} ({e}); not repairing the Run 2 save");
            return;
        }
    }
    match repair_run2_dev_save(conn) {
        Ok(()) => info!("[game_db] repaired the Run 2 development save {path:?} to v063"),
        Err(e) => error!("[game_db] repairing the Run 2 development save {path:?} failed: {e}"),
    }
}

/// Represents an open per-save game database with migrations applied.
pub struct GameDatabase {
    conn: Connection,
    path: Option<PathBuf>,
}

impl GameDatabase {
    /// Open (or create) a game database at the given path and apply all migrations.
    pub fn open(path: &Path) -> Result<Self, String> {
        debug!("[game_db] opening database at {:?}", path);
        let mut conn = Connection::open(path).map_err(|e| {
            error!("[game_db] failed to open database at {:?}: {}", path, e);
            GAME_DATABASE_OPEN_FAILED.to_string()
        })?;

        reconcile_run2_dev_save(&mut conn, Some(path));
        let migrations = all_migrations();
        migrations.to_latest(&mut conn).map_err(|e| {
            error!("[game_db] migration failed for {:?}: {}", path, e);
            GAME_DATABASE_MIGRATION_FAILED.to_string()
        })?;

        info!("[game_db] database ready at {:?}", path);
        Ok(Self {
            conn,
            path: Some(path.to_path_buf()),
        })
    }

    /// Open an existing save with structured error classification, so the UI
    /// can distinguish a corrupted file from one created by a newer build. Used
    /// by the save-loading path; other callers use [`GameDatabase::open`].
    pub fn open_save(path: &Path) -> Result<Self, SaveLoadError> {
        debug!("[game_db] opening save at {:?}", path);
        let mut conn = Connection::open(path).map_err(|e| {
            error!("[game_db] failed to open save at {:?}: {}", path, e);
            SaveLoadError::Corrupted
        })?;

        // A save from a newer build carries a higher schema version than we can
        // apply; detect it before migrating so the failure is explainable rather
        // than a generic migration error.
        let save_version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap_or(0);
        if save_version > MIGRATION_COUNT as i64 {
            return Err(SaveLoadError::IncompatibleVersion {
                save_version,
                supported: MIGRATION_COUNT as i64,
            });
        }

        reconcile_run2_dev_save(&mut conn, Some(path));
        let save_version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap_or(save_version);

        // `user_version` alone decides which migrations run, so a file whose tables
        // contradict it would be "migrated" into something half-built. Refuse it
        // before touching a byte.
        verify_layout_matches_version(&conn, save_version as usize, path)?;

        let migrations = all_migrations();
        migrations.to_latest(&mut conn).map_err(|e| {
            error!("[game_db] migration failed for save {:?}: {}", path, e);
            SaveLoadError::Corrupted
        })?;

        info!("[game_db] save ready at {:?}", path);
        Ok(Self {
            conn,
            path: Some(path.to_path_buf()),
        })
    }

    /// Create an in-memory game database (useful for tests and pre-save state).
    pub fn open_in_memory() -> Result<Self, String> {
        debug!("[game_db] opening in-memory database");
        let mut conn = Connection::open_in_memory().map_err(|e| {
            error!("[game_db] failed to open in-memory database: {}", e);
            GAME_DATABASE_OPEN_FAILED.to_string()
        })?;

        let migrations = all_migrations();
        migrations.to_latest(&mut conn).map_err(|e| {
            error!("[game_db] migration failed for in-memory db: {}", e);
            GAME_DATABASE_MIGRATION_FAILED.to_string()
        })?;

        Ok(Self { conn, path: None })
    }

    /// Get a reference to the underlying connection (for repositories).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get the file path, if this is a file-backed database.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the current schema version (number of applied migrations).
    pub fn schema_version(&self) -> Result<i64, String> {
        self.conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| GAME_DATABASE_SCHEMA_VERSION_READ_FAILED.to_string())
    }

    /// Validate that the database has the expected schema version.
    /// Returns Ok(true) if valid, Ok(false) if version mismatch.
    pub fn validate_schema(&self) -> Result<bool, String> {
        let migrations = all_migrations();
        let current: usize = migrations
            .current_version(&self.conn)
            .map_err(|_| GAME_DATABASE_CURRENT_VERSION_READ_FAILED.to_string())?
            .into();
        // We expect the version to equal the number of migrations (1 for V1)
        let expected = MIGRATION_COUNT;
        Ok(current == expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = GameDatabase::open_in_memory().unwrap();
        assert!(db.path().is_none());
        assert_eq!(
            db.schema_version().unwrap(),
            crate::migrations::MIGRATION_COUNT as i64
        );
    }

    #[test]
    fn test_open_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_game.db");

        let db = GameDatabase::open(&db_path).unwrap();
        assert_eq!(db.path().unwrap(), db_path);
        assert_eq!(
            db.schema_version().unwrap(),
            crate::migrations::MIGRATION_COUNT as i64
        );
        assert!(db.validate_schema().unwrap());
    }

    #[test]
    fn test_reopen_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_reopen.db");

        // Create and close
        {
            let db = GameDatabase::open(&db_path).unwrap();
            assert_eq!(
                db.schema_version().unwrap(),
                crate::migrations::MIGRATION_COUNT as i64
            );
        }

        // Reopen — migrations should be idempotent
        let db = GameDatabase::open(&db_path).unwrap();
        assert_eq!(
            db.schema_version().unwrap(),
            crate::migrations::MIGRATION_COUNT as i64
        );
        assert!(db.validate_schema().unwrap());
    }

    #[test]
    fn test_validate_schema_on_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("empty.db");

        // Create a raw DB without migrations
        {
            let _conn = Connection::open(&db_path).unwrap();
        }

        // Opening via GameDatabase applies migrations, so it should be valid
        let db = GameDatabase::open(&db_path).unwrap();
        assert!(db.validate_schema().unwrap());
    }

    #[test]
    fn test_conn_is_usable() {
        let db = GameDatabase::open_in_memory().unwrap();
        // Verify we can query a table created by the migration
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM teams", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_open_directory_path_returns_backend_key() {
        let dir = tempfile::tempdir().unwrap();

        let result = GameDatabase::open(dir.path());

        match result {
            Err(error) => assert_eq!(error, GAME_DATABASE_OPEN_FAILED),
            Ok(_) => panic!("expected directory open to fail"),
        }
    }

    #[test]
    fn open_save_rejects_a_future_version() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("future.db");
        {
            // A save written by a newer build: schema version above ours.
            let conn = Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", (MIGRATION_COUNT + 5) as i64)
                .unwrap();
        }

        match GameDatabase::open_save(&db_path) {
            Err(SaveLoadError::IncompatibleVersion {
                save_version,
                supported,
            }) => {
                assert_eq!(save_version, (MIGRATION_COUNT + 5) as i64);
                assert_eq!(supported, MIGRATION_COUNT as i64);
            }
            Err(other) => panic!("expected IncompatibleVersion, got {other:?}"),
            Ok(_) => panic!("expected IncompatibleVersion, got an open database"),
        }
    }

    #[test]
    fn open_save_reports_corruption_for_a_non_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("corrupt.db");
        std::fs::write(&db_path, b"this is not a sqlite database").unwrap();

        match GameDatabase::open_save(&db_path) {
            Err(SaveLoadError::Corrupted) => {}
            Err(other) => panic!("expected Corrupted, got {other:?}"),
            Ok(_) => panic!("expected Corrupted, got an open database"),
        }
    }

    #[test]
    fn open_save_loads_a_current_save() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("current.db");
        // Create a fully-migrated save, then reopen it through open_save.
        GameDatabase::open(&db_path).unwrap();

        let db = GameDatabase::open_save(&db_path).expect("a current save should open");
        assert!(db.validate_schema().unwrap());
    }

    fn save_claiming_version(dir: &Path, claimed: usize, extra_sql: &str) -> PathBuf {
        let path = dir.join(format!("claims-{claimed}.db"));
        let mut conn = Connection::open(&path).unwrap();
        all_migrations().to_version(&mut conn, claimed).unwrap();
        conn.execute_batch(extra_sql).unwrap();
        path
    }

    /// Given a save whose tables are exactly what its version says
    /// When it is opened
    /// Then it loads and is migrated to the latest.
    #[test]
    fn open_save_migrates_an_honest_older_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(dir.path(), MIGRATION_COUNT - 3, "");

        let db = GameDatabase::open_save(&path).expect("an honest older save loads");

        assert_eq!(db.schema_version().unwrap(), MIGRATION_COUNT as i64);
    }

    /// Given a save at version N that lacks a column migration N built
    /// When it is opened
    /// Then it is refused as an incompatible layout naming the column, and the file is byte-identical.
    #[test]
    fn open_save_refuses_a_layout_that_contradicts_its_version_and_leaves_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(
            dir.path(),
            MIGRATION_COUNT,
            "CREATE TABLE stray (id INTEGER); DROP TABLE game_meta;",
        );
        let before = std::fs::read(&path).unwrap();

        match GameDatabase::open_save(&path) {
            Err(SaveLoadError::IncompatibleLayout { first_difference }) => {
                assert!(first_difference.contains("game_meta"), "{first_difference}");
            }
            Err(other) => panic!("expected IncompatibleLayout, got {other:?}"),
            Ok(_) => panic!("expected IncompatibleLayout, got an open database"),
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    /// Given an honest save that also carries a table of someone else's
    /// When it is opened
    /// Then it loads: only the migrations' own tables are held to the version.
    #[test]
    fn open_save_tolerates_a_table_no_migration_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(dir.path(), MIGRATION_COUNT, "CREATE TABLE probe (x);");

        GameDatabase::open_save(&path).expect("an extra table is not a contradiction");
    }

    /// Given an honest save plus a foreign table whose name contains a double quote
    /// When it is opened
    /// Then it loads: inspecting the layout must not choke on an awkward table name.
    #[test]
    fn open_save_tolerates_a_foreign_table_with_a_quote_in_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let path =
            save_claiming_version(dir.path(), MIGRATION_COUNT, r#"CREATE TABLE "a""b" (x);"#);

        GameDatabase::open_save(&path).expect("an awkward foreign table is not a contradiction");
    }

    fn assert_refused_naming(path: &Path, expected: &str) {
        match GameDatabase::open_save(path) {
            Err(SaveLoadError::IncompatibleLayout { first_difference }) => {
                assert!(first_difference.contains(expected), "{first_difference}");
            }
            Err(other) => panic!("expected IncompatibleLayout, got {other:?}"),
            Ok(_) => panic!("expected IncompatibleLayout, got an open database"),
        }
    }

    /// Given a save at v43 that already holds a `cash_journal` of its own making (v44 creates it)
    /// When it is opened
    /// Then it is refused as an incompatible layout, not migrated into a generic failure.
    #[test]
    fn open_save_refuses_a_table_a_later_migration_would_create() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(dir.path(), 43, "CREATE TABLE cash_journal (id INTEGER);");

        assert_refused_naming(&path, "cash_journal");
    }

    /// Given an honest save with a generated (hidden) column added to one of its tables
    /// When it is opened
    /// Then the stray column is seen and the save refused.
    #[test]
    fn open_save_refuses_a_hidden_generated_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(
            dir.path(),
            MIGRATION_COUNT,
            "ALTER TABLE game_meta ADD COLUMN sneaky INTEGER GENERATED ALWAYS AS (1) VIRTUAL;",
        );

        assert_refused_naming(&path, "sneaky");
    }

    const RUN2_DEV_COLUMN_SQL: &str =
        "ALTER TABLE teams ADD COLUMN out_of_possession_formation TEXT NOT NULL DEFAULT '';";
    const TEAM_ROW_SQL: &str = "INSERT INTO teams (id, name, short_name, country, city, \
        stadium_name, stadium_capacity, wage_budget, transfer_budget, formation) \
        VALUES ('t1', 'Team', 'TM', 'FR', 'Paris', 'Stade', 40000, 1, 1, '4-3-3')";

    fn user_version(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    /// A save as the Run 1 development build wrote it: 62 migrations, the old numbering where
    /// `v062` is the transfer obligations table.
    fn run1_dev_save(dir: &Path) -> PathBuf {
        let path = dir.join("run1.db");
        let mut conn = Connection::open(&path).unwrap();
        all_migrations().to_version(&mut conn, 62).unwrap();
        conn.execute_batch(TEAM_ROW_SQL).unwrap();
        conn.execute(
            "INSERT INTO transfer_obligations (id, obligation_json) VALUES ('o1', '{\"kind\":\"x\"}')",
            [],
        )
        .unwrap();
        path
    }

    /// A save as the Run 2 development build wrote it: 61 migrations, then the out-of-possession
    /// column under the old `v062` label, and no transfer obligations table.
    fn run2_dev_save(dir: &Path) -> PathBuf {
        let path = dir.join("run2.db");
        let mut conn = Connection::open(&path).unwrap();
        all_migrations().to_version(&mut conn, 61).unwrap();
        conn.execute_batch(RUN2_DEV_COLUMN_SQL).unwrap();
        conn.pragma_update(None, "user_version", 62).unwrap();
        conn.execute_batch(TEAM_ROW_SQL).unwrap();
        conn.execute(
            "UPDATE teams SET out_of_possession_formation = '5-4-1' WHERE id = 't1'",
            [],
        )
        .unwrap();
        path
    }

    fn team_defensive_shape(db: &GameDatabase) -> String {
        db.conn()
            .query_row(
                "SELECT out_of_possession_formation FROM teams WHERE id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    /// Given a save a Run 1 build wrote at schema 62 with a pending obligation
    /// When it is opened by the integrated build
    /// Then v063 adds the defensive-shape column and the obligation and team survive untouched.
    #[test]
    fn a_run1_development_save_upgrades_to_the_integrated_schema_without_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = run1_dev_save(dir.path());

        let db = GameDatabase::open_save(&path).expect("a Run 1 dev save loads");

        assert_eq!(db.schema_version().unwrap(), MIGRATION_COUNT as i64);
        assert_eq!(team_defensive_shape(&db), "", "legacy: defend with the attacking shape");
        let obligations: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM transfer_obligations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(obligations, 1);
        let formation: String = db
            .conn()
            .query_row("SELECT formation FROM teams WHERE id = 't1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(formation, "4-3-3");
    }

    /// Given a save a Run 2 build wrote at schema 62 (defensive shape set, no obligations table)
    /// When it is opened by the integrated build
    /// Then it gains the obligations table, keeps its defensive shape, a backup copy is taken,
    /// and a second open is a no-op.
    #[test]
    fn a_run2_development_save_is_repaired_to_the_integrated_schema_without_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = run2_dev_save(dir.path());

        let db = GameDatabase::open_save(&path).expect("a Run 2 dev save loads");

        assert_eq!(db.schema_version().unwrap(), MIGRATION_COUNT as i64);
        assert_eq!(team_defensive_shape(&db), "5-4-1");
        let obligations: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM transfer_obligations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(obligations, 0);
        drop(db);
        let backup = dir.path().join("run2.db.pre-v063.bak");
        assert!(backup.exists(), "a pre-repair copy is kept");
        assert_eq!(user_version(&backup), 62, "the backup is the untouched original");

        let again = GameDatabase::open_save(&path).expect("reopening works");
        assert_eq!(team_defensive_shape(&again), "5-4-1");
    }

    /// Given the same Run 2 development save
    /// When it is opened through the non-save entry point (`GameDatabase::open`)
    /// Then it is repaired there too instead of failing on a duplicate column.
    #[test]
    fn a_run2_development_save_also_opens_through_the_plain_entry_point() {
        let dir = tempfile::tempdir().unwrap();
        let path = run2_dev_save(dir.path());

        let db = GameDatabase::open(&path).expect("plain open repairs the Run 2 dev save");

        assert_eq!(db.schema_version().unwrap(), MIGRATION_COUNT as i64);
        assert_eq!(team_defensive_shape(&db), "5-4-1");
    }

    /// Given a v62 save holding BOTH the column and the table (neither build wrote this)
    /// When it is opened
    /// Then it is not "repaired" blindly: it is refused as a layout contradiction, file untouched.
    #[test]
    fn a_v62_save_that_matches_neither_dev_build_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(dir.path(), 62, RUN2_DEV_COLUMN_SQL);
        let before = std::fs::read(&path).unwrap();

        assert_refused_naming(&path, "out_of_possession_formation");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    /// Given a released v61 save with data
    /// When it is opened
    /// Then both new migrations apply in order and the data survives.
    #[test]
    fn a_v61_save_upgrades_through_both_new_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_claiming_version(dir.path(), 61, TEAM_ROW_SQL);

        let db = GameDatabase::open_save(&path).expect("a v61 save loads");

        assert_eq!(db.schema_version().unwrap(), 63);
        assert_eq!(team_defensive_shape(&db), "");
        let formation: String = db
            .conn()
            .query_row("SELECT formation FROM teams WHERE id = 't1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(formation, "4-3-3");
    }
}
