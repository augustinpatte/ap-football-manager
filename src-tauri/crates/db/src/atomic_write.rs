//! Replacing a file so that it is either the old contents or the new, never half of either.
//!
//! `fs::write` truncates first, so a process that dies partway through — a crash, a full disk, a
//! machine switched off — leaves a file that exists and parses as nothing. The data is written and
//! synced beside the target, then renamed over it. The root crate's `atomic_file::write_json` is a
//! JSON wrapper over this, so there is one rule for it.

use std::io::Write as _;
use std::path::Path;

/// Where a failed [`write_atomically_classified`] left the target.
#[derive(Debug)]
pub enum AtomicWriteError {
    /// The rename never happened, so the target is still the old file (or still absent). Safe to
    /// treat the write as not having taken place.
    NotReplaced(std::io::Error),
    /// The rename happened and only the directory sync after it failed. Anything reading the file
    /// now sees the new contents, but a power cut could still bring the old ones back, so the
    /// caller can neither assume the write is durable nor that it did not happen.
    ReplacedNotConfirmedDurable(std::io::Error),
}

impl AtomicWriteError {
    pub fn into_io_error(self) -> std::io::Error {
        match self {
            Self::NotReplaced(error) | Self::ReplacedNotConfirmedDurable(error) => error,
        }
    }
}

/// Writes `bytes` to `path`, creating its folder if needed. On any error the previous file, if
/// there was one, is untouched — except after a directory-sync failure, which this plain form
/// reports as an error like any other; use [`write_atomically_classified`] to tell them apart.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomically_classified(path, bytes).map_err(AtomicWriteError::into_io_error)
}

/// [`write_atomically`], reporting whether a failure happened before or after the rename.
pub fn write_atomically_classified(path: &Path, bytes: &[u8]) -> Result<(), AtomicWriteError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AtomicWriteError::NotReplaced)?;
    }
    let mut scratch_name = path.file_name().unwrap_or_default().to_owned();
    scratch_name.push(".part");
    let scratch = path.with_file_name(scratch_name);
    // The data must be on disk before the rename can be trusted: a power cut after a rename of
    // unsynced data can leave the new name pointing at an empty file.
    let mut file = std::fs::File::create(&scratch).map_err(AtomicWriteError::NotReplaced)?;
    file.write_all(bytes)
        .map_err(AtomicWriteError::NotReplaced)?;
    file.sync_all().map_err(AtomicWriteError::NotReplaced)?;
    drop(file);
    std::fs::rename(&scratch, path)
        .inspect_err(|_| {
            // Best effort: a stray `.part` is harmless, but there is no reason to leave one.
            let _ = std::fs::remove_file(&scratch);
        })
        .map_err(AtomicWriteError::NotReplaced)?;
    sync_directory(path.parent()).map_err(AtomicWriteError::ReplacedNotConfirmedDurable)
}

#[cfg(test)]
thread_local! {
    /// Lets a test fail the directory sync, the one step after the rename, which a real
    /// filesystem will not fail on demand. Per thread, so parallel tests cannot see each other's.
    static FAIL_DIRECTORY_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes the directory sync after the rename fail on this thread until it is switched off.
#[cfg(test)]
pub(crate) fn fail_directory_sync_on_this_thread(fail: bool) {
    FAIL_DIRECTORY_SYNC.with(|flag| flag.set(fail));
}

/// Makes the rename itself durable. Windows cannot open a directory to sync it, and its rename is
/// already journalled.
fn sync_directory(directory: Option<&Path>) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_DIRECTORY_SYNC.with(std::cell::Cell::get) {
        return Err(std::io::Error::other("injected directory sync failure"));
    }
    #[cfg(unix)]
    if let Some(directory) = directory {
        return std::fs::File::open(directory)?.sync_all();
    }
    let _ = directory;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a scratch name occupied by a directory, when a file is replaced, then the failure is
    /// classified as not replaced and the old contents are intact.
    #[test]
    fn a_failure_before_the_rename_is_classified_as_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("index.json");
        std::fs::write(&target, b"old").unwrap();
        std::fs::create_dir(dir.path().join("index.json.part")).unwrap();

        let result = write_atomically_classified(&target, b"new");

        assert!(matches!(result, Err(AtomicWriteError::NotReplaced(_))));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
    }

    /// Given a directory sync that fails after the rename, when a file is replaced, then the
    /// failure is classified as replaced-but-unconfirmed and the new contents are in place.
    #[test]
    fn a_failure_after_the_rename_is_classified_as_replaced_not_confirmed_durable() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("index.json");
        std::fs::write(&target, b"old").unwrap();
        fail_directory_sync_on_this_thread(true);

        let result = write_atomically_classified(&target, b"new");
        fail_directory_sync_on_this_thread(false);

        assert!(matches!(
            result,
            Err(AtomicWriteError::ReplacedNotConfirmedDurable(_))
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    /// Given a directory sync that fails after the rename, when the plain `write_atomically` is
    /// used, then it still reports an error, as its other callers expect.
    #[test]
    fn the_plain_write_still_reports_a_post_rename_failure_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("index.json");
        fail_directory_sync_on_this_thread(true);

        let result = write_atomically(&target, b"new");
        fail_directory_sync_on_this_thread(false);

        assert!(result.is_err());
    }

    /// Given a target that is a directory, so the rename itself fails, when a file is replaced,
    /// then the failure is classified as not replaced and no scratch file is left behind.
    #[test]
    fn a_failed_rename_is_classified_as_not_replaced_and_leaves_no_scratch_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("index.json");
        std::fs::create_dir(&target).unwrap();

        let result = write_atomically_classified(&target, b"new");

        assert!(matches!(result, Err(AtomicWriteError::NotReplaced(_))));
        assert!(!dir.path().join("index.json.part").exists());
    }
}
