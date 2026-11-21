use std::path::{Path, PathBuf};

use crate::save_index::{
    DbValidation, IndexWriteFailure, SaveEntry, SaveIndex, load_index_or_quarantine, rebuild_index,
    write_index, write_index_classified,
};

const SAVE_INDEX_WRITE_ERROR: &str = "be.error.saveIndex.writeFailed";

pub struct SaveIndexManager {
    saves_dir: PathBuf,
    index_path: PathBuf,
    index: SaveIndex,
    index_needs_rebuild: bool,
}

impl SaveIndexManager {
    pub fn init(saves_dir: &Path) -> Result<Self, String> {
        let index_path = saves_dir.join("save_index.json");
        let (index, index_needs_rebuild) = match load_index_or_quarantine(&index_path)? {
            Some(index) => (index, false),
            None => (SaveIndex::new(), true),
        };

        Ok(Self {
            saves_dir: saves_dir.to_path_buf(),
            index_path,
            index,
            index_needs_rebuild,
        })
    }

    pub fn ensure_loaded(&mut self) -> Result<(), String> {
        if !self.index_needs_rebuild {
            return Ok(());
        }

        let (index, validations) = rebuild_index(&self.saves_dir)?;
        for validation in &validations {
            // Left on disk untouched and unlisted: say so rather than skip it silently.
            if let DbValidation::Invalid { filename, reason } = validation {
                log::warn!(
                    "[saves] {filename} is not a usable career and was not indexed: {reason}"
                );
            }
        }

        write_index(&self.index_path, &index)?;
        self.index = index;
        self.index_needs_rebuild = false;
        Ok(())
    }

    pub fn list_saves(&self) -> &[SaveEntry] {
        &self.index.saves
    }

    pub fn find(&self, save_id: &str) -> Option<&SaveEntry> {
        self.index.find(save_id)
    }

    /// Adds a career. The index in memory only changes once the write to disk has.
    pub fn record_new_save(&mut self, entry: SaveEntry) -> Result<(), String> {
        let mut staged = self.index.clone();
        staged.add(entry);
        self.commit(staged).map(drop)
    }

    pub fn update_save(&mut self, entry: SaveEntry) -> Result<(), String> {
        let mut staged = self.index.clone();
        if !staged.update(&entry) {
            return Err(SAVE_INDEX_WRITE_ERROR.to_string());
        }

        self.commit(staged).map(drop)
    }

    /// Removes a career from the index. The returned [`IndexDurability`] matters to a caller that
    /// is about to delete the career's database: only once the index without it is confirmed
    /// durable is the file safe to remove.
    pub fn remove_save(&mut self, save_id: &str) -> Result<(bool, IndexDurability), String> {
        let mut staged = self.index.clone();
        let removed = staged.remove(save_id);
        let durability = self.commit(staged)?;
        Ok((removed, durability))
    }

    /// Writes `staged` and, only if that happened, makes it the index in memory.
    ///
    /// Mutating memory first left a ghost whenever the write failed: the entry stayed listed, and
    /// the next successful write persisted it. A failure before the rename leaves the durable
    /// index untouched, so memory stays as it was. A failure after the rename leaves the new index
    /// in place, so memory adopts it rather than disagree with the file; rolling back there would
    /// be wrong half the time, and the caller is told it is unconfirmed.
    fn commit(&mut self, staged: SaveIndex) -> Result<IndexDurability, String> {
        match write_index_classified(&self.index_path, &staged) {
            Ok(()) => {
                self.index = staged;
                Ok(IndexDurability::Confirmed)
            }
            Err(IndexWriteFailure::NotWritten) => Err(SAVE_INDEX_WRITE_ERROR.to_string()),
            Err(IndexWriteFailure::WrittenNotConfirmedDurable) => {
                log::warn!(
                    "[saves] the save index was replaced but could not be confirmed durable"
                );
                self.index = staged;
                Ok(IndexDurability::Unconfirmed)
            }
        }
    }
}

/// Whether an index change is known to have reached the disk for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexDurability {
    Confirmed,
    /// The new index is what a reader sees now, but a power cut could bring the old one back.
    Unconfirmed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> SaveEntry {
        SaveEntry {
            id: "save-1".to_string(),
            name: "Career".to_string(),
            manager_name: "John Smith".to_string(),
            team_name: "London FC".to_string(),
            db_filename: "save-1.db".to_string(),
            checksum: "checksum".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_played_at: "2026-01-02T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn update_save_returns_backend_key_when_entry_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let mut manager = SaveIndexManager::init(&saves_dir).unwrap();

        let result = manager.update_save(sample_entry());

        assert_eq!(result.unwrap_err(), SAVE_INDEX_WRITE_ERROR);
    }

    /// Given an index write whose rename succeeded but whose directory sync failed, when a career
    /// is recorded, then memory adopts the new index (it is what the file now holds) and the
    /// caller is not told the career failed to save.
    #[test]
    fn an_ambiguous_post_rename_write_is_adopted_not_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = SaveIndexManager::init(dir.path()).unwrap();
        manager.ensure_loaded().unwrap();
        crate::atomic_write::fail_directory_sync_on_this_thread(true);

        let recorded = manager.record_new_save(sample_entry());
        crate::atomic_write::fail_directory_sync_on_this_thread(false);

        assert_eq!(recorded, Ok(()));
        assert!(manager.find("save-1").is_some());
        let on_disk = crate::save_index::load_index(&dir.path().join("save_index.json"))
            .unwrap()
            .unwrap();
        assert_eq!(on_disk.saves.len(), 1);
    }

    /// Given the same ambiguous write on a removal, then the caller is told the removal is
    /// unconfirmed, which is what keeps it from deleting the database.
    #[test]
    fn an_ambiguous_removal_reports_unconfirmed_durability() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = SaveIndexManager::init(dir.path()).unwrap();
        manager.ensure_loaded().unwrap();
        manager.record_new_save(sample_entry()).unwrap();
        crate::atomic_write::fail_directory_sync_on_this_thread(true);

        let removed = manager.remove_save("save-1");
        crate::atomic_write::fail_directory_sync_on_this_thread(false);

        assert_eq!(removed, Ok((true, IndexDurability::Unconfirmed)));
        assert!(manager.find("save-1").is_none());
    }

    /// Given a clean index write, when a career is removed, then the removal reports confirmed
    /// durability, which is what lets the caller delete the database.
    #[test]
    fn a_clean_removal_reports_confirmed_durability() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = SaveIndexManager::init(dir.path()).unwrap();
        manager.ensure_loaded().unwrap();
        manager.record_new_save(sample_entry()).unwrap();

        assert_eq!(
            manager.remove_save("save-1"),
            Ok((true, IndexDurability::Confirmed))
        );
    }
}
