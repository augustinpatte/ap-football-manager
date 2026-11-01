//! An opt-in, fully isolated data root for GUI testing.
//!
//! Everything the game persists is derived from the account's home directory: Tauri's app data,
//! cache and log folders (`~/Library/...` on macOS, `$XDG_*` on Linux), the webview's own storage
//! and the importer's default `~/apfootballmanager/data/world.db` lookup. Setting
//! [`ISOLATED_ROOT_VARIABLE`] to an absolute folder points all of it at that folder, so a test
//! session cannot read or write the player's real saves, databases or preferences.
//!
//! Unset, none of this runs and the game behaves exactly as shipped.
//!
//! The mechanism fails closed: a root that is, contains or sits inside a protected folder is
//! refused before anything is created, and once Tauri has resolved its app data folder the game
//! checks that it really landed inside the root, so a redirect the platform ignored stops the
//! launch before the first write instead of after it.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Environment variable naming the isolated root. Must be an absolute path.
pub const ISOLATED_ROOT_VARIABLE: &str = "APFM_TEST_DATA_ROOT";

/// The saves-folder override of `save_location`; an inherited value would otherwise escape the root.
const SAVES_DIR_VARIABLE: &str = "APFM_SAVES_DIR";

/// Bundle identifier of the shipped app; its per-user folders are what a test must never touch.
const APP_IDENTIFIER: &str = "com.apfootballmanager.app";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsolationError {
    /// The variable is set but empty or relative: it would depend on the launch directory.
    NotAbsolute,
    /// The root is, contains or sits inside a folder that holds real player data.
    TouchesProtectedPath(PathBuf),
    /// Tauri resolved its app data folder outside the root: the redirect did not take.
    AppDataOutsideRoot(PathBuf),
    /// The root or one of its folders could not be created.
    CannotCreate(String),
}

impl std::fmt::Display for IsolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAbsolute => write!(f, "{ISOLATED_ROOT_VARIABLE} must be an absolute path"),
            Self::TouchesProtectedPath(path) => write!(
                f,
                "{ISOLATED_ROOT_VARIABLE} overlaps protected player data at {}",
                path.display()
            ),
            Self::AppDataOutsideRoot(path) => write!(
                f,
                "the app data folder {} is outside the isolated root; refusing to start",
                path.display()
            ),
            Self::CannotCreate(reason) => write!(f, "cannot create the isolated root: {reason}"),
        }
    }
}

/// What activating the isolation will do, computed without touching the environment or the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationPlan {
    pub root: PathBuf,
    /// Variables to set; every persistent location the game or its webview derives hangs off these.
    pub variables: Vec<(&'static str, PathBuf)>,
    /// Folders to create before the first launch step.
    pub folders: Vec<PathBuf>,
}

/// The folders of the real installation that an isolated session must not overlap.
pub fn protected_paths(home: &Path, saves_override: Option<&OsString>) -> Vec<PathBuf> {
    let mut protected = vec![
        home.join("Library")
            .join("Application Support")
            .join(APP_IDENTIFIER),
        home.join("Library").join("Caches").join(APP_IDENTIFIER),
        home.join("Library").join("Logs").join(APP_IDENTIFIER),
        home.join("Library").join("WebKit").join(APP_IDENTIFIER),
        home.join(".local").join("share").join(APP_IDENTIFIER),
        home.join(".cache").join(APP_IDENTIFIER),
        home.join(".config").join(APP_IDENTIFIER),
        home.join("apfootballmanager"),
    ];
    if let Some(saves) = saves_override
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        protected.push(saves);
    }
    protected
}

/// Resolves `.`/`..` lexically and follows symlinks of the part that exists, so `/tmp/x/../y` and a
/// symlinked temp folder compare by where they really are.
fn normalise(path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                lexical.pop();
            }
            Component::CurDir => {}
            other => lexical.push(other.as_os_str()),
        }
    }
    let mut existing = lexical.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_owned());
                existing = parent;
            }
            _ => break,
        }
    }
    let mut resolved = existing
        .canonicalize()
        .unwrap_or_else(|_| existing.to_path_buf());
    resolved.extend(missing.iter().rev());
    resolved
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Validates `root` and lists what to set and create. Pure apart from reading the disk to resolve
/// symlinks.
pub fn plan(root: &Path, protected: &[PathBuf]) -> Result<IsolationPlan, IsolationError> {
    if !root.is_absolute() {
        return Err(IsolationError::NotAbsolute);
    }
    let root = normalise(root);
    if let Some(hit) = protected
        .iter()
        .find(|protected| overlaps(&root, &normalise(protected)))
    {
        return Err(IsolationError::TouchesProtectedPath(hit.clone()));
    }
    let home = root.join("home");
    let variables = vec![
        ("HOME", home.clone()),
        ("XDG_DATA_HOME", home.join(".local").join("share")),
        ("XDG_CACHE_HOME", home.join(".cache")),
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("XDG_STATE_HOME", home.join(".local").join("state")),
        ("TMPDIR", root.join("tmp")),
        (SAVES_DIR_VARIABLE, root.join("saves")),
    ];
    let folders = variables.iter().map(|(_, path)| path.clone()).collect();
    Ok(IsolationPlan {
        root,
        variables,
        folders,
    })
}

/// After Tauri resolved the app data folder: it must be inside the root, or the launch stops.
pub fn verify_app_data_inside(root: &Path, app_data_dir: &Path) -> Result<(), IsolationError> {
    if normalise(app_data_dir).starts_with(normalise(root)) {
        Ok(())
    } else {
        Err(IsolationError::AppDataOutsideRoot(
            app_data_dir.to_path_buf(),
        ))
    }
}

/// Reads the variable and, when set, redirects the process's persistent locations into the root.
/// Call once, first thing in `run`, before any thread or plugin reads those variables.
///
/// Returns the root when isolation is active, `None` when the game runs as shipped.
pub fn activate() -> Result<Option<PathBuf>, IsolationError> {
    let Some(raw) = std::env::var_os(ISOLATED_ROOT_VARIABLE) else {
        return Ok(None);
    };
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let protected = protected_paths(&home, std::env::var_os(SAVES_DIR_VARIABLE).as_ref());
    let plan = plan(Path::new(&raw), &protected)?;
    for folder in &plan.folders {
        std::fs::create_dir_all(folder).map_err(|e| IsolationError::CannotCreate(e.to_string()))?;
    }
    for (name, value) in &plan.variables {
        std::env::set_var(name, value);
    }
    Ok(Some(plan.root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_home_protection() -> Vec<PathBuf> {
        protected_paths(Path::new("/Users/player"), None)
    }

    /// Given a relative or empty root
    /// When planning
    /// Then it is refused: it would depend on the launch directory.
    #[test]
    fn a_relative_root_is_refused() {
        assert_eq!(
            plan(Path::new("scratch"), &real_home_protection()),
            Err(IsolationError::NotAbsolute)
        );
        assert_eq!(
            plan(Path::new(""), &real_home_protection()),
            Err(IsolationError::NotAbsolute)
        );
    }

    /// Given the real app data folder, a folder inside it, and a parent of it
    /// When planning
    /// Then every one is refused as overlapping player data.
    #[test]
    fn a_root_overlapping_the_real_app_data_is_refused() {
        let real = "/Users/player/Library/Application Support/com.apfootballmanager.app";
        for root in [
            real.to_string(),
            format!("{real}/test-session"),
            "/Users/player/Library/Application Support".to_string(),
            "/Users/player/Library".to_string(),
            "/Users/player".to_string(),
            "/Users".to_string(),
            "/".to_string(),
        ] {
            assert!(
                matches!(
                    plan(Path::new(&root), &real_home_protection()),
                    Err(IsolationError::TouchesProtectedPath(_))
                ),
                "{root} should be refused"
            );
        }
    }

    /// Given the importer's default world.db folder and a path that reaches it through `..`
    /// When planning
    /// Then both are refused.
    #[test]
    fn the_default_world_db_folder_is_protected_even_through_dot_dot() {
        for root in [
            "/Users/player/apfootballmanager/data",
            "/Users/player/other/../apfootballmanager",
        ] {
            assert!(matches!(
                plan(Path::new(root), &real_home_protection()),
                Err(IsolationError::TouchesProtectedPath(_))
            ));
        }
    }

    /// Given a saves folder the player redirected with APFM_SAVES_DIR
    /// When planning a root inside it
    /// Then it is refused: that folder holds real saves.
    #[test]
    fn a_custom_saves_folder_is_protected_too() {
        let saves = OsString::from("/Users/player/Dropbox/apfm-saves");
        let protected = protected_paths(Path::new("/Users/player"), Some(&saves));

        assert!(plan(Path::new("/Users/player/Dropbox/apfm-saves/t"), &protected).is_err());
        assert!(plan(Path::new("/Users/player/Dropbox"), &protected).is_err());
    }

    /// Given a symlink that points at the real data
    /// When a root is given through the link
    /// Then it is refused by where it really is.
    #[cfg(unix)]
    #[test]
    fn a_symlink_into_protected_data_is_refused() {
        let sandbox = tempfile::tempdir().expect("sandbox");
        let real = sandbox.path().join("apfootballmanager");
        std::fs::create_dir_all(&real).expect("real dir");
        let link = sandbox.path().join("innocent");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let protected = protected_paths(sandbox.path(), None);

        assert!(matches!(
            plan(&link, &protected),
            Err(IsolationError::TouchesProtectedPath(_))
        ));
    }

    /// Given a fresh temp folder
    /// When planning
    /// Then every persistent location lies inside the root and the saves override is pinned to it.
    #[test]
    fn a_temp_root_redirects_every_persistent_location_inside_itself() {
        let temp = tempfile::tempdir().expect("temp");
        let planned = plan(temp.path(), &real_home_protection()).expect("plan");

        let names: Vec<_> = planned.variables.iter().map(|(name, _)| *name).collect();
        for expected in [
            "HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "TMPDIR",
            "APFM_SAVES_DIR",
        ] {
            assert!(names.contains(&expected), "{expected} not redirected");
        }
        for (name, path) in &planned.variables {
            assert!(path.starts_with(&planned.root), "{name} escapes the root");
        }
        assert_eq!(planned.folders.len(), planned.variables.len());
    }

    /// Given the app data folder Tauri resolved
    /// When it is outside the root (the platform ignored HOME)
    /// Then the launch is refused; inside the root it passes.
    #[test]
    fn an_app_data_folder_outside_the_root_stops_the_launch() {
        let root = Path::new("/tmp/apfm-gui-test");

        assert!(
            verify_app_data_inside(root, Path::new("/tmp/apfm-gui-test/home/app-data")).is_ok()
        );
        assert_eq!(
            verify_app_data_inside(
                root,
                Path::new("/Users/player/Library/Application Support/com.apfootballmanager.app")
            ),
            Err(IsolationError::AppDataOutsideRoot(PathBuf::from(
                "/Users/player/Library/Application Support/com.apfootballmanager.app"
            )))
        );
        assert!(verify_app_data_inside(root, Path::new("/tmp/apfm-gui-test-evil/x")).is_err());
    }
}
