//! Installing the FM24 world from inside the game, so nobody has to run a command to play.
//!
//! The adapter reads the player's own `world.db` read-only, runs its integrity checks and writes the
//! world into the game's `databases/` directory, where the native world listing finds it. Nothing
//! of the source ever lands in the repository or the build: the file is produced on the player's
//! machine from the player's data.

use crate::{Config, Result, checks, pipeline, report};
use std::path::{Path, PathBuf};

/// Which generation of the world an installation builds. Raise it whenever a correction changes the
/// world itself (economy, rosters, rules): a world installed under an older revision, or by a
/// version that wrote no marker at all, is reported `outdated` and offered for regeneration.
/// Revision 2: recurring income and wage budget of the FM24 economy (mission 4).
/// Revision 3: gate income for every club and the pool of FM24 free agents (mission 6).
/// Revision 4: squads follow the team the save registers each player with: reserve and youth sides are
/// the academy, loans carry their owner, affiliated B sides are the owner's reserve (roster audit).
/// Revision 6: a player's level follows his FM Current Ability (Run F2) instead of the mean of his
/// converted attributes, which had squeezed the whole hierarchy into the 55-70 band.
pub const WORLD_REVISION: u32 = 6;

/// What an installation produced, in the terms the world screen shows.
#[derive(Debug, Clone)]
pub struct Installed {
    pub world_path: PathBuf,
    pub world_id: String,
    pub sha256: String,
    pub bytes: u64,
    pub checks_total: usize,
}

/// Is the FM24 world in `databases_dir` there, and is it the current generation?
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStatus {
    pub installed: bool,
    /// The revision the marker records; `None` for a world installed before markers existed.
    pub revision: Option<u32>,
    pub current_revision: u32,
    pub outdated: bool,
}

/// The marker lives beside `databases/`, never inside it: the world listing reads every file there.
fn marker_path(databases_dir: &Path) -> PathBuf {
    databases_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("fm24-install.json")
}

pub fn install_status(databases_dir: &Path) -> InstallStatus {
    let world = databases_dir.join(format!("{}.json", Config::default_config().world.id));
    let installed = world.is_file();
    let revision = std::fs::read_to_string(marker_path(databases_dir))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|marker| marker["revision"].as_u64())
        .and_then(|revision| u32::try_from(revision).ok());
    InstallStatus {
        installed,
        revision,
        current_revision: WORLD_REVISION,
        outdated: installed && revision.is_none_or(|found| found < WORLD_REVISION),
    }
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
    let marker = serde_json::json!({ "revision": WORLD_REVISION, "sha256": written.world_sha256 });
    std::fs::write(marker_path(databases_dir), marker.to_string())
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

    /// Given a world installed by a version that wrote no marker (before the economy fixes)
    /// When the status is read
    /// Then it is installed but outdated, and a current marker clears that
    #[test]
    fn a_world_without_a_marker_is_outdated() {
        let dir = tempfile::tempdir().expect("temp dir");
        let databases = dir.path().join("databases");
        assert!(!install_status(&databases).installed);
        assert!(!install_status(&databases).outdated);

        std::fs::create_dir_all(&databases).unwrap();
        let id = Config::default_config().world.id;
        std::fs::write(databases.join(format!("{id}.json")), "{}").unwrap();
        let old = install_status(&databases);
        assert!(old.installed && old.outdated && old.revision.is_none());

        std::fs::write(
            marker_path(&databases),
            format!("{{\"revision\":{WORLD_REVISION}}}"),
        )
        .unwrap();
        assert!(!install_status(&databases).outdated);
        std::fs::write(marker_path(&databases), "{\"revision\":1}").unwrap();
        assert!(install_status(&databases).outdated);
    }

    /// Given a source that is not a database
    /// When installing over an existing world
    /// Then the install is refused with a translation key and the world already there is kept
    #[test]
    fn a_corrupt_source_is_refused_and_keeps_the_installed_world() {
        let dir = tempfile::tempdir().expect("temp dir");
        let databases = dir.path().join("databases");
        std::fs::create_dir_all(&databases).unwrap();
        let id = Config::default_config().world.id;
        let installed = databases.join(format!("{id}.json"));
        std::fs::write(&installed, "previous world").unwrap();
        let source = dir.path().join("world.db");
        std::fs::write(&source, "this is not sqlite").unwrap();

        let result = install_world(&source, &dir.path().join("work"), &databases);

        assert_eq!(result.unwrap_err(), "be.error.fm24.importFailed");
        assert_eq!(
            std::fs::read_to_string(&installed).unwrap(),
            "previous world"
        );
    }

    /// Given a databases folder that cannot be written
    /// When installing the real world
    /// Then the refusal is `writeFailed`, not a crash. Skipped without `APFM_WORLD_DB`.
    #[test]
    #[cfg(unix)]
    fn an_unwritable_databases_folder_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let Some(source) = std::env::var_os("APFM_WORLD_DB").map(PathBuf::from) else {
            return;
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        if std::fs::write(locked.join("probe"), "x").is_ok() {
            return; // running as a user the mode does not bind
        }

        let result = install_world(&source, &dir.path().join("work"), &locked.join("databases"));

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result.unwrap_err(), "be.error.fm24.writeFailed");
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
