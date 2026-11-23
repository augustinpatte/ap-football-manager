use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use ofm_core::game::Game;

use crate::game_database::GameDatabase;
use crate::game_persistence::GamePersistenceReader;
use crate::repositories::meta_repo;

/// A single entry in the save index, representing one save session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveEntry {
    pub id: String,
    pub name: String,
    pub manager_name: String,
    #[serde(default)]
    pub team_name: String,
    pub db_filename: String,
    pub checksum: String,
    pub created_at: String,
    pub last_played_at: String,
}

/// The save index file that tracks all save sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveIndex {
    pub version: u32,
    pub saves: Vec<SaveEntry>,
}

fn save_index_checksum_error() -> String {
    "be.error.saveIndex.checksumReadFailed".to_string()
}

fn save_index_load_error() -> String {
    "be.error.saveIndex.loadFailed".to_string()
}

fn save_index_write_error() -> String {
    "be.error.saveIndex.writeFailed".to_string()
}

impl SaveIndex {
    /// Create a new empty save index.
    pub fn new() -> Self {
        Self {
            version: 1,
            saves: Vec::new(),
        }
    }

    /// Add a save entry to the index.
    pub fn add(&mut self, entry: SaveEntry) {
        self.saves.push(entry);
    }

    /// Update an existing entry by id. Returns false if not found.
    pub fn update(&mut self, entry: &SaveEntry) -> bool {
        if let Some(existing) = self.saves.iter_mut().find(|e| e.id == entry.id) {
            existing.name = entry.name.clone();
            existing.manager_name = entry.manager_name.clone();
            existing.team_name = entry.team_name.clone();
            existing.checksum = entry.checksum.clone();
            existing.last_played_at = entry.last_played_at.clone();
            true
        } else {
            false
        }
    }

    /// Remove a save entry by id. Returns true if removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.saves.len();
        self.saves.retain(|e| e.id != id);
        self.saves.len() < before
    }

    /// Find a save entry by id.
    pub fn find(&self, id: &str) -> Option<&SaveEntry> {
        self.saves.iter().find(|e| e.id == id)
    }
}

impl Default for SaveIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA-256 checksum of a file. Returns hex string.
pub fn compute_checksum(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|_| save_index_checksum_error())?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Ok(result.iter().map(|byte| format!("{:02x}", byte)).collect())
}

/// Load save index from a JSON file. Returns None if the file doesn't exist.
pub fn load_index(index_path: &Path) -> Result<Option<SaveIndex>, String> {
    if !index_path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(index_path).map_err(|_| save_index_load_error())?;
    let index: SaveIndex = serde_json::from_str(&data).map_err(|_| save_index_load_error())?;
    Ok(Some(index))
}

/// Load the save index for startup, treating a damaged one as missing rather than fatal.
///
/// A file that exists but is not a valid index (an interrupted write by an older version, a disk
/// error) is moved aside whole as `save_index.json.corrupt-<time>` and reported as `None`, so the
/// caller rebuilds from the career databases. Nothing is deleted: the damaged copy stays for
/// diagnosis and the careers' own files are never touched. A file that cannot be read at all
/// (permissions, I/O) is still an error: that is not evidence it is damaged.
pub fn load_index_or_quarantine(index_path: &Path) -> Result<Option<SaveIndex>, String> {
    if !index_path.exists() {
        return Ok(None);
    }
    let data = fs::read(index_path).map_err(|_| save_index_load_error())?;
    // Parsed from bytes: a write cut off inside a multi-byte character is not valid UTF-8.
    match serde_json::from_slice::<SaveIndex>(&data) {
        Ok(index) => Ok(Some(index)),
        Err(_) => {
            set_damaged_index_aside(index_path)?;
            Ok(None)
        }
    }
}

fn set_damaged_index_aside(index_path: &Path) -> Result<(), String> {
    let file_name = index_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("save_index.json");
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S%.3f");
    for attempt in 0..100 {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let aside = index_path.with_file_name(format!("{file_name}.corrupt-{stamp}{suffix}"));
        // Never replace an earlier damaged copy: rename would on most platforms.
        if aside.exists() {
            continue;
        }
        fs::rename(index_path, &aside).map_err(|_| save_index_load_error())?;
        log::warn!("[saves] the save index was damaged; kept as {aside:?} and rebuilding");
        return Ok(());
    }
    Err(save_index_load_error())
}

/// Write save index to a JSON file.
pub fn write_index(index_path: &Path, index: &SaveIndex) -> Result<(), String> {
    write_index_classified(index_path, index).map_err(|_| save_index_write_error())
}

/// Why [`write_index_classified`] failed, in terms of what a reader of the file now sees.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexWriteFailure {
    /// The durable index is still the old one.
    NotWritten,
    /// The new index replaced the old one, but the sync that makes it survive a power cut failed.
    /// Readers see the new index now; whether it persists is unknown.
    WrittenNotConfirmedDurable,
}

/// [`write_index`], telling a write that did not happen from one that happened but is not
/// confirmed durable. A caller keeping its own copy of the index needs the difference: rolling
/// back after the second would disagree with the file it just wrote.
pub fn write_index_classified(
    index_path: &Path,
    index: &SaveIndex,
) -> Result<(), IndexWriteFailure> {
    let data = serde_json::to_string_pretty(index).map_err(|_| IndexWriteFailure::NotWritten)?;
    // Replaced whole or not at all: a write cut short must never leave the live index truncated.
    crate::atomic_write::write_atomically_classified(index_path, data.as_bytes()).map_err(|error| {
        match error {
            crate::atomic_write::AtomicWriteError::NotReplaced(_) => IndexWriteFailure::NotWritten,
            crate::atomic_write::AtomicWriteError::ReplacedNotConfirmedDurable(_) => {
                IndexWriteFailure::WrittenNotConfirmedDurable
            }
        }
    })
}

/// Result of validating a single database file during rebuild.
#[derive(Debug)]
pub enum DbValidation {
    /// Database is valid and has this metadata.
    Valid(SaveEntry),
    /// Database file is corrupted or has invalid schema.
    Invalid { filename: String, reason: String },
}

/// Rebuild the save index by scanning a directory for `.db` files,
/// validating each one, and constructing a new index.
pub fn rebuild_index(saves_dir: &Path) -> Result<(SaveIndex, Vec<DbValidation>), String> {
    let mut index = SaveIndex::new();
    let mut validations = Vec::new();

    if !saves_dir.exists() {
        return Ok((index, validations));
    }

    let entries = fs::read_dir(saves_dir).map_err(|_| save_index_load_error())?;

    for entry in entries {
        let entry = entry.map_err(|_| save_index_load_error())?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.ends_with(".db") => name.to_string(),
            _ => continue,
        };

        match validate_db_file(&path) {
            Ok(save_entry) => {
                index.add(save_entry.clone());
                validations.push(DbValidation::Valid(save_entry));
            }
            Err(reason) => {
                validations.push(DbValidation::Invalid {
                    filename: filename.clone(),
                    reason,
                });
            }
        }
    }

    Ok((index, validations))
}

pub fn save_entry_metadata_from_game(game: &Game) -> (String, String) {
    let manager_name = format!("{} {}", game.manager.first_name, game.manager.last_name);
    let team_name = game
        .manager
        .team_id
        .as_ref()
        .and_then(|team_id| game.teams.iter().find(|team| team.id == *team_id))
        .map(|team| team.name.clone())
        .unwrap_or_default();
    (manager_name, team_name)
}

/// Validate a single `.db` file: check schema and extract metadata.
fn validate_db_file(path: &Path) -> Result<SaveEntry, String> {
    let db = GameDatabase::open(path)?;

    if !db.validate_schema()? {
        return Err(save_index_load_error());
    }

    let meta = meta_repo::load_meta(db.conn())?
        .ok_or_else(|| "be.error.gamePersistence.gameMetaMissing".to_string())?;

    let checksum = compute_checksum(path)?;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.db")
        .to_string();

    let (manager_name, team_name) = GamePersistenceReader::read_game(&db)
        .map(|game| save_entry_metadata_from_game(&game))
        .unwrap_or_default();

    Ok(SaveEntry {
        id: meta.save_id.clone(),
        name: meta.save_name,
        manager_name,
        team_name,
        db_filename: filename,
        checksum,
        created_at: meta.created_at,
        last_played_at: meta.last_played_at,
    })
}

/// Load or rebuild the save index.
/// If the index file exists, load it. Otherwise, rebuild from the saves directory.
pub fn load_or_rebuild_index(
    index_path: &Path,
    saves_dir: &Path,
) -> Result<(SaveIndex, Vec<DbValidation>), String> {
    if let Some(index) = load_index_or_quarantine(index_path)? {
        return Ok((index, Vec::new()));
    }

    let (index, validations) = rebuild_index(saves_dir)?;
    write_index(index_path, &index)?;
    Ok((index, validations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::meta_repo::{GameMeta, upsert_meta};

    #[test]
    fn test_save_index_new() {
        let index = SaveIndex::new();
        assert_eq!(index.version, 1);
        assert!(index.saves.is_empty());
    }

    #[test]
    fn test_save_index_add_and_find() {
        let mut index = SaveIndex::new();
        let entry = SaveEntry {
            id: "save-001".to_string(),
            name: "Test Career".to_string(),
            manager_name: "John Smith".to_string(),
            team_name: String::new(),
            db_filename: "save-001.db".to_string(),
            checksum: "abc123".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-02".to_string(),
        };
        index.add(entry);

        assert_eq!(index.saves.len(), 1);
        let found = index.find("save-001").unwrap();
        assert_eq!(found.name, "Test Career");
    }

    #[test]
    fn test_save_index_update() {
        let mut index = SaveIndex::new();
        index.add(SaveEntry {
            id: "save-001".to_string(),
            name: "Old Name".to_string(),
            manager_name: "John".to_string(),
            team_name: String::new(),
            db_filename: "save-001.db".to_string(),
            checksum: "old".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-01".to_string(),
        });

        let updated = index.update(&SaveEntry {
            id: "save-001".to_string(),
            name: "New Name".to_string(),
            manager_name: "John".to_string(),
            team_name: String::new(),
            db_filename: "save-001.db".to_string(),
            checksum: "new".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-05".to_string(),
        });

        assert!(updated);
        assert_eq!(index.find("save-001").unwrap().name, "New Name");
        assert_eq!(index.find("save-001").unwrap().checksum, "new");
    }

    #[test]
    fn test_save_index_update_not_found() {
        let mut index = SaveIndex::new();
        let result = index.update(&SaveEntry {
            id: "nonexistent".to_string(),
            name: "x".to_string(),
            manager_name: "x".to_string(),
            team_name: String::new(),
            db_filename: "x.db".to_string(),
            checksum: "x".to_string(),
            created_at: "x".to_string(),
            last_played_at: "x".to_string(),
        });
        assert!(!result);
    }

    #[test]
    fn test_save_index_remove() {
        let mut index = SaveIndex::new();
        index.add(SaveEntry {
            id: "save-001".to_string(),
            name: "Career".to_string(),
            manager_name: "John".to_string(),
            team_name: String::new(),
            db_filename: "save-001.db".to_string(),
            checksum: "abc".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-01".to_string(),
        });
        assert!(index.remove("save-001"));
        assert!(index.saves.is_empty());
        assert!(!index.remove("save-001")); // already removed
    }

    #[test]
    fn test_load_index_defaults_missing_team_name() {
        let legacy_json = r#"{
            "version": 1,
            "saves": [{
                "id": "save-001",
                "name": "Career",
                "manager_name": "John Smith",
                "db_filename": "save-001.db",
                "checksum": "abc123",
                "created_at": "2026-01-01",
                "last_played_at": "2026-01-02"
            }]
        }"#;

        let index: SaveIndex = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(index.saves[0].team_name, "");
    }

    #[test]
    fn test_save_index_serialization_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("save_index.json");

        let mut index = SaveIndex::new();
        index.add(SaveEntry {
            id: "save-001".to_string(),
            name: "Career".to_string(),
            manager_name: "John".to_string(),
            team_name: String::new(),
            db_filename: "save-001.db".to_string(),
            checksum: "abc123".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-02".to_string(),
        });

        write_index(&index_path, &index).unwrap();
        let loaded = load_index(&index_path).unwrap().unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.saves.len(), 1);
        assert_eq!(loaded.saves[0].id, "save-001");
        assert_eq!(loaded.saves[0].checksum, "abc123");
    }

    #[test]
    fn test_load_index_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("nonexistent.json");
        let result = load_index(&index_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let checksum = compute_checksum(&file_path).unwrap();
        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // SHA-256 hex is 64 chars

        // Same content should produce same checksum
        let checksum2 = compute_checksum(&file_path).unwrap();
        assert_eq!(checksum, checksum2);
    }

    #[test]
    fn test_compute_checksum_missing_file_uses_backend_key() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("missing.txt");

        let result = compute_checksum(&file_path);

        assert_eq!(result.unwrap_err(), "be.error.saveIndex.checksumReadFailed");
    }

    #[test]
    fn test_load_index_invalid_json_uses_backend_key() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("save_index.json");
        fs::write(&index_path, "not-json").unwrap();

        let result = load_index(&index_path);

        assert_eq!(result.unwrap_err(), "be.error.saveIndex.loadFailed");
    }

    #[test]
    fn test_write_index_parent_file_uses_backend_key() {
        let dir = tempfile::tempdir().unwrap();
        let blocking_parent = dir.path().join("not-a-directory");
        fs::write(&blocking_parent, "blocking file").unwrap();

        let index_path = blocking_parent.join("save_index.json");
        let index = SaveIndex::new();

        let result = write_index(&index_path, &index);

        assert_eq!(result.unwrap_err(), "be.error.saveIndex.writeFailed");
    }

    #[test]
    fn test_rebuild_index_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert!(index.saves.is_empty());
        assert!(validations.is_empty());
    }

    #[test]
    fn test_rebuild_index_nonexistent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("nonexistent");

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert!(index.saves.is_empty());
        assert!(validations.is_empty());
    }

    #[test]
    fn test_rebuild_index_returns_load_failed_when_path_is_not_directory() {
        let dir = tempfile::tempdir().unwrap();
        let saves_path = dir.path().join("not-a-directory");
        fs::write(&saves_path, "occupied").unwrap();

        let result = rebuild_index(&saves_path);

        assert_eq!(result.unwrap_err(), "be.error.saveIndex.loadFailed");
    }

    #[test]
    fn test_rebuild_index_with_valid_db() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        // Create a valid game database with metadata
        let db_path = saves_dir.join("test-save.db");
        let db = GameDatabase::open(&db_path).unwrap();
        upsert_meta(
            db.conn(),
            &GameMeta {
                save_id: "test-save".to_string(),
                save_name: "Test Career".to_string(),
                manager_id: "mgr-001".to_string(),
                start_date: "2026-07-01".to_string(),
                game_date: "2026-08-01".to_string(),
                created_at: "2026-01-01".to_string(),
                last_played_at: "2026-01-02".to_string(),
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
                value_scale_json: String::new(),
            },
        )
        .unwrap();
        drop(db); // Close the database

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert_eq!(index.saves.len(), 1);
        assert_eq!(index.saves[0].id, "test-save");
        assert_eq!(index.saves[0].name, "Test Career");
        assert!(!index.saves[0].checksum.is_empty());

        assert_eq!(validations.len(), 1);
        assert!(matches!(&validations[0], DbValidation::Valid(_)));
    }

    #[test]
    fn test_rebuild_index_with_invalid_db() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        // Create a DB file with valid schema but no metadata
        let db_path = saves_dir.join("empty.db");
        let _db = GameDatabase::open(&db_path).unwrap();
        drop(_db);

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert!(index.saves.is_empty());
        assert_eq!(validations.len(), 1);
        assert!(matches!(
            &validations[0],
            DbValidation::Invalid { reason, .. }
                if reason == "be.error.gamePersistence.gameMetaMissing"
        ));
    }

    #[test]
    fn test_rebuild_index_with_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        // Create a corrupt .db file
        let db_path = saves_dir.join("corrupt.db");
        fs::write(&db_path, "not a sqlite database").unwrap();

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert!(index.saves.is_empty());
        assert_eq!(validations.len(), 1);
        assert!(matches!(&validations[0], DbValidation::Invalid { .. }));
    }

    #[test]
    fn test_rebuild_index_ignores_non_db_files() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        fs::write(saves_dir.join("readme.txt"), "not a db").unwrap();
        fs::write(saves_dir.join("data.json"), "{}").unwrap();

        let (index, validations) = rebuild_index(&saves_dir).unwrap();
        assert!(index.saves.is_empty());
        assert!(validations.is_empty());
    }

    #[test]
    fn test_load_or_rebuild_uses_existing_index() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("save_index.json");
        let saves_dir = dir.path().join("saves");

        let mut index = SaveIndex::new();
        index.add(SaveEntry {
            id: "existing".to_string(),
            name: "Existing".to_string(),
            manager_name: "John".to_string(),
            team_name: String::new(),
            db_filename: "existing.db".to_string(),
            checksum: "abc".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-01".to_string(),
        });
        write_index(&index_path, &index).unwrap();

        let (loaded, validations) = load_or_rebuild_index(&index_path, &saves_dir).unwrap();
        assert_eq!(loaded.saves.len(), 1);
        assert_eq!(loaded.saves[0].id, "existing");
        assert!(validations.is_empty()); // No rebuild needed
    }

    #[test]
    fn test_load_or_rebuild_rebuilds_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("save_index.json");
        let saves_dir = dir.path().join("saves");
        fs::create_dir(&saves_dir).unwrap();

        // Create a valid DB
        let db_path = saves_dir.join("my-save.db");
        let db = GameDatabase::open(&db_path).unwrap();
        upsert_meta(
            db.conn(),
            &GameMeta {
                save_id: "my-save".to_string(),
                save_name: "My Career".to_string(),
                manager_id: "mgr-001".to_string(),
                start_date: "2026-07-01".to_string(),
                game_date: "2026-08-01".to_string(),
                created_at: "2026-01-01".to_string(),
                last_played_at: "2026-01-02".to_string(),
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
                value_scale_json: String::new(),
            },
        )
        .unwrap();
        drop(db);

        let (loaded, validations) = load_or_rebuild_index(&index_path, &saves_dir).unwrap();
        assert_eq!(loaded.saves.len(), 1);
        assert_eq!(loaded.saves[0].id, "my-save");
        assert_eq!(validations.len(), 1);

        // Index file should now exist
        assert!(index_path.exists());
    }

    fn entry(id: &str) -> SaveEntry {
        SaveEntry {
            id: id.to_string(),
            name: format!("Career {id}"),
            manager_name: "John Smith".to_string(),
            team_name: "London FC".to_string(),
            db_filename: format!("{id}.db"),
            checksum: "checksum".to_string(),
            created_at: "2026-01-01".to_string(),
            last_played_at: "2026-01-02".to_string(),
        }
    }

    fn index_of(ids: &[&str]) -> SaveIndex {
        let mut index = SaveIndex::new();
        for id in ids {
            index.add(entry(id));
        }
        index
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Given an index on disk, when a new one is written over it,
    /// Then the file holds exactly the new index and no scratch file is left beside it.
    #[test]
    fn writing_the_index_replaces_it_and_leaves_no_scratch_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save_index.json");
        write_index(&path, &index_of(&["a"])).unwrap();

        write_index(&path, &index_of(&["a", "b"])).unwrap();

        assert_eq!(load_index(&path).unwrap().unwrap().saves.len(), 2);
        assert_eq!(names_in(dir.path()), ["save_index.json"]);
    }

    /// Given a valid index and a replacement that cannot be completed,
    /// When the write fails,
    /// Then the previous index is still there, byte for byte, and still loads.
    #[test]
    fn a_failed_write_keeps_the_previous_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save_index.json");
        write_index(&path, &index_of(&["a"])).unwrap();
        let before = fs::read(&path).unwrap();
        // A directory where the scratch file goes makes the write fail before the swap.
        fs::create_dir(dir.path().join("save_index.json.part")).unwrap();

        assert!(write_index(&path, &index_of(&["a", "b"])).is_err());

        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(load_index(&path).unwrap().unwrap().saves.len(), 1);
    }

    /// Given a process that died after starting the scratch file but before the swap,
    /// When the index is loaded and then written again,
    /// Then the previous index loaded intact, and the next write succeeds and clears the debris.
    #[test]
    fn an_interrupted_write_leaves_the_previous_index_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save_index.json");
        write_index(&path, &index_of(&["a"])).unwrap();
        fs::write(
            dir.path().join("save_index.json.part"),
            b"{\"version\": 1, \"sa",
        )
        .unwrap();

        assert_eq!(load_index(&path).unwrap().unwrap().saves.len(), 1);
        write_index(&path, &index_of(&["a", "b"])).unwrap();

        assert_eq!(load_index(&path).unwrap().unwrap().saves.len(), 2);
        assert_eq!(names_in(dir.path()), ["save_index.json"]);
    }

    /// Given a malformed index file, when it is loaded for recovery,
    /// Then it is moved aside intact, the live name is free, and loading reports "no index".
    #[test]
    fn a_malformed_index_is_moved_aside_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save_index.json");
        fs::write(&path, b"{\"version\": 1, \"saves\": [").unwrap();

        assert!(load_index_or_quarantine(&path).unwrap().is_none());

        assert!(!path.exists());
        let kept: Vec<_> = names_in(dir.path());
        assert_eq!(kept.len(), 1);
        assert!(kept[0].starts_with("save_index.json.corrupt-"));
        assert_eq!(
            fs::read(dir.path().join(&kept[0])).unwrap(),
            b"{\"version\": 1, \"saves\": ["
        );
    }

    /// Given two damaged indexes in a row, when each is set aside,
    /// Then the second does not overwrite the first.
    #[test]
    fn setting_aside_twice_keeps_both_damaged_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save_index.json");
        for damaged in [&b"{one"[..], &b"{two"[..]] {
            fs::write(&path, damaged).unwrap();
            assert!(load_index_or_quarantine(&path).unwrap().is_none());
        }

        let mut contents: Vec<_> = names_in(dir.path())
            .iter()
            .map(|name| fs::read(dir.path().join(name)).unwrap())
            .collect();
        contents.sort();
        assert_eq!(contents, [b"{one".to_vec(), b"{two".to_vec()]);
    }

    /// Given a truncated index and the saves folder it described,
    /// When the index is loaded or rebuilt,
    /// Then the rebuild proceeds from the career files instead of failing.
    #[test]
    fn load_or_rebuild_recovers_from_a_truncated_index() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        fs::create_dir_all(&saves_dir).unwrap();
        let path = saves_dir.join("save_index.json");
        fs::write(&path, b"{\"version\"").unwrap();

        let (index, _) = load_or_rebuild_index(&path, &saves_dir).expect("recovers");

        assert!(index.saves.is_empty());
        assert!(
            load_index(&path).unwrap().is_some(),
            "a valid index is written"
        );
    }
}
