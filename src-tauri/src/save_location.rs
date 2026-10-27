//! Where the saves live, and who last opened them.
//!
//! This only *prepares* syncing the saves between two Macs: the folder can be pointed somewhere a
//! sync tool already mirrors (`APFM_SAVES_DIR`), and `sync_info.json` beside the saves records
//! which machine opened them last, so a later sync feature can tell "the other Mac played this
//! career since I did" without guessing. Nothing here moves, merges or uploads a save.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// Environment variable that moves the saves folder, for a folder another tool keeps in sync.
pub const SAVES_DIR_VARIABLE: &str = "APFM_SAVES_DIR";
const SYNC_INFO_FILE: &str = "sync_info.json";
const DEVICE_ID_FILE: &str = "device_id";
const FORMAT_VERSION: u32 = 1;

/// The saves folder: the variable when it names an absolute path, else `<app data>/saves`.
/// A relative or empty value is ignored — it would depend on the launch directory.
pub fn resolve_saves_dir(app_data_dir: &Path, override_dir: Option<OsString>) -> PathBuf {
    override_dir
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| app_data_dir.join("saves"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncInfo {
    pub format_version: u32,
    pub app: String,
    pub app_version: String,
    pub platform: String,
    /// The machine that opened the saves last (a random id kept in this machine's app data).
    pub device_id: String,
    pub last_opened_at: String,
    /// The machine before that one, when it differs: the signal that another Mac used these saves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_device_id: Option<String>,
}

/// This machine's id, created on first use and never synced (it lives in the app data folder).
fn device_id(app_data_dir: &Path) -> Result<String, String> {
    let path = app_data_dir.join(DEVICE_ID_FILE);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }
    let mut rng = rand::rng();
    let fresh = format!("{:016x}{:016x}", rng.random::<u64>(), rng.random::<u64>());
    std::fs::write(&path, &fresh).map_err(|e| e.to_string())?;
    Ok(fresh)
}

/// Records this launch in `sync_info.json`; never touches a save.
pub fn record_launch(
    app_data_dir: &Path,
    saves_dir: &Path,
    app_version: &str,
    now: DateTime<Utc>,
) -> Result<SyncInfo, String> {
    std::fs::create_dir_all(saves_dir).map_err(|e| e.to_string())?;
    let device = device_id(app_data_dir)?;
    let path = saves_dir.join(SYNC_INFO_FILE);
    let before: Option<SyncInfo> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let previous_device_id = match &before {
        Some(info) if info.device_id != device => Some(info.device_id.clone()),
        Some(info) => info.previous_device_id.clone(),
        None => None,
    };
    let info = SyncInfo {
        format_version: FORMAT_VERSION,
        app: "AP Football Manager".to_string(),
        app_version: app_version.to_string(),
        platform: std::env::consts::OS.to_string(),
        device_id: device,
        last_opened_at: now.to_rfc3339(),
        previous_device_id,
    };
    let text = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given no override, an empty one or a relative one, when resolving, then the default folder is used.
    #[test]
    fn only_an_absolute_override_moves_the_saves_folder() {
        let data = Path::new("/data");
        assert_eq!(resolve_saves_dir(data, None), PathBuf::from("/data/saves"));
        assert_eq!(
            resolve_saves_dir(data, Some("".into())),
            PathBuf::from("/data/saves")
        );
        assert_eq!(
            resolve_saves_dir(data, Some("rel/dir".into())),
            PathBuf::from("/data/saves")
        );
        assert_eq!(
            resolve_saves_dir(data, Some("/sync/saves".into())),
            PathBuf::from("/sync/saves")
        );
    }

    /// Given two machines opening the same saves folder, when the second records its launch, then
    /// the first is remembered as the previous device and the saves are untouched.
    #[test]
    fn a_second_machine_is_recorded_as_a_change_of_device() {
        let shared = tempfile::tempdir().expect("saves");
        let mac_a = tempfile::tempdir().expect("app data a");
        let mac_b = tempfile::tempdir().expect("app data b");
        std::fs::write(shared.path().join("keep.db"), b"save").expect("a save");

        let first = record_launch(mac_a.path(), shared.path(), "1", Utc::now()).expect("a");
        let again = record_launch(mac_a.path(), shared.path(), "1", Utc::now()).expect("a again");
        let second = record_launch(mac_b.path(), shared.path(), "1", Utc::now()).expect("b");

        assert_eq!(again.device_id, first.device_id);
        assert_eq!(again.previous_device_id, None);
        assert_ne!(second.device_id, first.device_id);
        assert_eq!(second.previous_device_id, Some(first.device_id));
        assert_eq!(
            std::fs::read(shared.path().join("keep.db")).expect("read"),
            b"save"
        );
    }
}
