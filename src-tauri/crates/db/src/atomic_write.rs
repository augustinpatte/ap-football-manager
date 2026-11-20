//! Replacing a file so that it is either the old contents or the new, never half of either.
//!
//! `fs::write` truncates first, so a process that dies partway through — a crash, a full disk, a
//! machine switched off — leaves a file that exists and parses as nothing. The data is written and
//! synced beside the target, then renamed over it. The root crate's `atomic_file::write_json` is a
//! JSON wrapper over this, so there is one rule for it.

use std::io::Write as _;
use std::path::Path;

/// Writes `bytes` to `path`, creating its folder if needed. On any error the previous file, if
/// there was one, is untouched.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut scratch_name = path.file_name().unwrap_or_default().to_owned();
    scratch_name.push(".part");
    let scratch = path.with_file_name(scratch_name);
    // The data must be on disk before the rename can be trusted: a power cut after a rename of
    // unsynced data can leave the new name pointing at an empty file.
    let mut file = std::fs::File::create(&scratch)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&scratch, path).inspect_err(|_| {
        // Best effort: a stray `.part` is harmless, but there is no reason to leave one.
        let _ = std::fs::remove_file(&scratch);
    })?;
    sync_directory(path.parent())
}

/// Makes the rename itself durable. Windows cannot open a directory to sync it, and its rename is
/// already journalled.
fn sync_directory(directory: Option<&Path>) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(directory) = directory {
        return std::fs::File::open(directory)?.sync_all();
    }
    let _ = directory;
    Ok(())
}
