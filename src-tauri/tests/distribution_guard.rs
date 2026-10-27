//! The player's own FM24 data must never ship with the app.
//!
//! The FM24 world is built on the player's machine from their own `world.db` and lives in the app
//! data folder. These tests fail if the bundle configuration starts embedding a database or an FM24
//! world, or if the repository stops ignoring them.

use std::path::Path;

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Given the Tauri bundle config, when its resources are listed, then none is a world database or FM24 data.
#[test]
fn the_bundle_embeds_no_world_database_or_fm24_data() {
    let config: serde_json::Value =
        serde_json::from_str(&read("tauri.conf.json")).expect("tauri.conf.json is JSON");
    let resources = config["bundle"]["resources"].to_string().to_lowercase();

    for forbidden in ["world.db", "fm24", "databases", "apfm-"] {
        assert!(
            !resources.contains(forbidden),
            "bundle resources must not embed `{forbidden}`: {resources}"
        );
    }
}

/// Given the repository ignore file, when read, then it keeps the world database and the built FM24 world out of git.
#[test]
fn the_repository_ignores_the_world_database_and_the_built_world() {
    let ignore = read("../.gitignore");

    for pattern in ["world.db", "apfm-fm24-*.json"] {
        assert!(
            ignore.lines().any(|line| line.trim() == pattern),
            "`{pattern}` must be listed in .gitignore"
        );
    }
}

/// Given the committed `databases/` folder, when listed, then it holds no FM24 world.
#[test]
fn no_fm24_world_is_committed_under_the_databases_folder() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("databases");
    let names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    assert!(
        names.iter().all(|n| !n.contains("fm24")),
        "an FM24 world is committed: {names:?}"
    );
}
