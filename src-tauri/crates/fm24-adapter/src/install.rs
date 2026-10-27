//! Installing the FM24 world from inside the game, so nobody has to run a command to play.
//!
//! The adapter reads the player's own `world.db` read-only, runs its integrity checks and writes the
//! world into the game's `databases/` directory, where the native world listing finds it. Nothing
//! of the source ever lands in the repository or the build: the file is produced on the player's
//! machine from the player's data.

use crate::{Config, Result, checks, pipeline, report};
use std::path::{Path, PathBuf};

/// What an installation produced, in the terms the world screen shows.
#[derive(Debug, Clone)]
pub struct Installed {
    pub world_path: PathBuf,
    pub world_id: String,
    pub sha256: String,
    pub bytes: u64,
    pub checks_total: usize,
}

/// Builds the world from `world_db` and installs it as `<databases_dir>/<world id>.json`.
///
/// `work_dir` receives the sidecars (id mapping, FM originals, report). The world is written beside
/// its destination and renamed into place, so a failed run never leaves a half-written world that
/// the listing would pick up; any blocking check that fails refuses the install and keeps the
/// world already there.
pub fn install_world(world_db: &Path, work_dir: &Path, databases_dir: &Path) -> Result<Installed> {
    if !world_db.is_file() {
        return Err("be.error.fm24.worldDbMissing".to_string());
    }
    let cfg = Config::default_config();
    // The package directory is rebuilt by the pipeline; start clean so a previous run cannot leak in.
    let _ = std::fs::remove_dir_all(work_dir);
    let run = pipeline::run(world_db, &work_dir.join("package"), &cfg)
        .map_err(|_| "be.error.fm24.importFailed".to_string())?;
    let results = checks::run_checks(&run);
    if results.iter().any(|c| c.blocking && !c.ok) {
        return Err("be.error.fm24.checksFailed".to_string());
    }
    let written = report::write_all(&run, &results, work_dir)
        .map_err(|_| "be.error.fm24.writeFailed".to_string())?;
    std::fs::create_dir_all(databases_dir).map_err(|_| "be.error.fm24.writeFailed".to_string())?;
    let destination = databases_dir.join(format!("{}.json", run.cfg.world.id));
    let staging = destination.with_extension("json.part");
    std::fs::copy(&written.world_path, &staging)
        .and_then(|_| std::fs::rename(&staging, &destination))
        .map_err(|_| "be.error.fm24.writeFailed".to_string())?;
    Ok(Installed {
        world_path: destination,
        world_id: run.cfg.world.id.clone(),
        sha256: written.world_sha256,
        bytes: written.world_bytes,
        checks_total: results.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given no world.db at the path, when installing, then the refusal is a translation key and
    /// the databases directory is not created.
    #[test]
    fn a_missing_source_is_refused_with_a_translation_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let databases = dir.path().join("databases");

        let result = install_world(
            &dir.path().join("nope.db"),
            &dir.path().join("work"),
            &databases,
        );

        assert_eq!(result.unwrap_err(), "be.error.fm24.worldDbMissing");
        assert!(!databases.exists());
    }

    /// Given the player's real world.db (`APFM_WORLD_DB`), when installing twice, then the world
    /// lands in the databases folder, is the same bytes both times, and the source is untouched.
    /// Skipped (passes) when the variable is not set: the data never lives in the repository.
    #[test]
    fn installing_the_real_world_is_reproducible_and_read_only() {
        let Some(source) = std::env::var_os("APFM_WORLD_DB").map(PathBuf::from) else {
            return;
        };
        let before = std::fs::metadata(&source)
            .expect("source metadata")
            .modified()
            .expect("mtime");
        let dir = tempfile::tempdir().expect("temp dir");
        let databases = dir.path().join("databases");

        let first =
            install_world(&source, &dir.path().join("work"), &databases).expect("first install");
        let first_bytes = std::fs::read(&first.world_path).expect("world file");
        let second =
            install_world(&source, &dir.path().join("work"), &databases).expect("second install");

        assert_eq!(first.sha256, second.sha256);
        assert_eq!(
            first_bytes,
            std::fs::read(&second.world_path).expect("world file again")
        );
        assert!(!databases.join("apfm-fm24-2026.json.part").exists());
        assert_eq!(
            before,
            std::fs::metadata(&source)
                .expect("source metadata")
                .modified()
                .expect("mtime")
        );
    }
}
