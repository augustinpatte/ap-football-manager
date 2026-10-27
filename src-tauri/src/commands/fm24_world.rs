//! The FM24 world, installed from inside the game.
//!
//! The player's own `world.db` is turned into a playable world on their machine by the
//! `fm24-adapter` crate; this file only finds the source, picks the directories and runs the work
//! off the UI thread. Neither `world.db` nor the world built from it is ever part of the build.

use std::path::PathBuf;

use log::info;
use serde::Serialize;
use tauri::Manager;

/// Environment variable naming a `world.db`, for a source that lives outside the usual places.
const WORLD_DB_ENV: &str = "APFM_WORLD_DB";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fm24InstallStatus {
    pub installed: bool,
    pub outdated: bool,
    pub revision: Option<u32>,
    pub current_revision: u32,
}

/// Is the FM24 world installed, and is it the current generation (else the screen offers to rebuild)?
#[tauri::command]
pub fn get_fm24_install_status(app_handle: tauri::AppHandle) -> Result<Fm24InstallStatus, String> {
    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|_| "be.error.fm24.writeFailed".to_string())?;
    let status = fm24_adapter::install::install_status(&app_data.join("databases"));
    Ok(Fm24InstallStatus {
        installed: status.installed,
        outdated: status.outdated,
        revision: status.revision,
        current_revision: status.current_revision,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fm24WorldInstalled {
    pub world_id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub checks_total: usize,
}

/// Where a `world.db` is looked for, best first: the explicit variable, the game's own data
/// directory, then the importer's default output in the project checkout.
fn source_candidates(app_data: Option<PathBuf>, home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(explicit) = std::env::var_os(WORLD_DB_ENV) {
        candidates.push(PathBuf::from(explicit));
    }
    if let Some(dir) = app_data {
        candidates.push(dir.join("fm24").join("world.db"));
    }
    if let Some(dir) = home {
        candidates.push(dir.join("apfootballmanager").join("data").join("world.db"));
    }
    candidates
}

/// The first `world.db` found where the game looks, or `None` so the screen can ask for the file.
#[tauri::command]
pub fn find_fm24_world_source(app_handle: tauri::AppHandle) -> Option<String> {
    let resolver = app_handle.path();
    source_candidates(resolver.app_data_dir().ok(), resolver.home_dir().ok())
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
}

/// Builds the FM24 world from `world_db` (or the first source found) and installs it among the
/// world databases. The build takes a couple of seconds and holds a few hundred MB, so it runs on
/// a blocking thread rather than the async executor.
#[tauri::command]
pub async fn install_fm24_world(
    app_handle: tauri::AppHandle,
    world_db: Option<String>,
) -> Result<Fm24WorldInstalled, String> {
    info!(
        "[cmd] install_fm24_world: explicit_source={}",
        world_db.is_some()
    );
    let source = match world_db {
        Some(path) => PathBuf::from(path),
        None => find_fm24_world_source(app_handle.clone())
            .map(PathBuf::from)
            .ok_or_else(|| "be.error.fm24.worldDbMissing".to_string())?,
    };
    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|_| "be.error.fm24.writeFailed".to_string())?;
    let installed = tauri::async_runtime::spawn_blocking(move || {
        fm24_adapter::install::install_world(
            &source,
            &app_data.join("fm24-import"),
            &app_data.join("databases"),
        )
    })
    .await
    .map_err(|_| "be.error.fm24.importFailed".to_string())??;
    Ok(Fm24WorldInstalled {
        world_id: installed.world_id,
        path: installed.world_path.to_string_lossy().into_owned(),
        sha256: installed.sha256,
        bytes: installed.bytes,
        checks_total: installed.checks_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given the app data dir and the home dir, when listing candidates, then the game's own
    /// directory comes before the project checkout.
    #[test]
    fn the_games_own_directory_is_searched_before_the_checkout() {
        let candidates =
            source_candidates(Some(PathBuf::from("/data")), Some(PathBuf::from("/home/u")));

        let own = candidates.iter().position(|p| p.starts_with("/data"));
        let checkout = candidates.iter().position(|p| p.starts_with("/home/u"));
        assert!(own < checkout);
    }
}
