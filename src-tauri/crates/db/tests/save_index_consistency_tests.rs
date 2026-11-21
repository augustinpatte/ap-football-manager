//! A failed write of `save_index.json` must leave the index in memory, the index on disk and the
//! career databases telling one story.
//!
//! The failure is injected the same way every time: a directory occupies `save_index.json.part`,
//! the scratch name `write_atomically` creates beside the index. `File::create` then fails before
//! anything is renamed, so the durable index is provably the old one. The obstruction is removed
//! again to prove the manager recovers. Everything lives in a tempdir; no real save is touched.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use db::save_index::{SaveEntry, load_index};
use db::save_index_manager::SaveIndexManager;
use db::save_manager::SaveManager;
use domain::manager::Manager;
use domain::stats::StatsState;
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;

const WRITE_FAILED: &str = "be.error.saveIndex.writeFailed";

fn career() -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 16, 0, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr-user".to_string(),
        "Pat".to_string(),
        "Doyle".to_string(),
        "1985-03-01".to_string(),
        "Ireland".to_string(),
    );
    manager.hire("club-x".to_string());
    let club = Team::new(
        "club-x".to_string(),
        "Club X".to_string(),
        "CLX".to_string(),
        "Ireland".to_string(),
        "Dublin".to_string(),
        "Ground".to_string(),
        10_000,
    );
    Game::new(clock, manager, vec![club], vec![], vec![], vec![])
}

fn scratch_path(saves: &Path) -> PathBuf {
    saves.join("save_index.json.part")
}

/// Makes the next index write fail before its rename.
fn obstruct_index_write(saves: &Path) {
    fs::create_dir(scratch_path(saves)).unwrap();
}

fn clear_obstruction(saves: &Path) {
    fs::remove_dir(scratch_path(saves)).unwrap();
}

/// A manager whose index file already exists, so the first create is not also a rebuild.
fn ready_manager(saves: &Path) -> SaveManager {
    let mut manager = SaveManager::init(saves).unwrap();
    assert!(manager.load_saves().unwrap().is_empty());
    manager
}

fn durable_ids(saves: &Path) -> Vec<String> {
    let index = load_index(&saves.join("save_index.json"))
        .unwrap()
        .expect("the index file exists");
    index.saves.into_iter().map(|entry| entry.id).collect()
}

fn listed_ids(manager: &SaveManager) -> Vec<String> {
    manager
        .list_saves()
        .iter()
        .map(|entry| entry.id.clone())
        .collect()
}

fn db_files(saves: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(saves)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".db"))
        .collect();
    names.sort();
    names
}

/// Memory, disk index and database files must agree, and every listed career must open.
fn assert_one_story(manager: &mut SaveManager, saves: &Path) {
    let mut memory = listed_ids(manager);
    let mut disk = durable_ids(saves);
    memory.sort();
    disk.sort();
    assert_eq!(
        memory, disk,
        "the in-memory index and the durable index differ"
    );
    let expected_files: Vec<String> = {
        let mut names: Vec<String> = memory.iter().map(|id| format!("{id}.db")).collect();
        names.sort();
        names
    };
    assert_eq!(
        db_files(saves),
        expected_files,
        "a database file and the index disagree"
    );
    for id in &memory {
        manager
            .load_game(id)
            .unwrap_or_else(|e| panic!("{id} must still open: {e}"));
    }
}

/// Given an index write that fails before its rename; when a save is created; then the player
/// is told it failed, no career is listed in memory or on disk, and no database is left behind.
#[test]
fn a_failed_create_save_lists_nothing_in_memory_or_on_disk() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    obstruct_index_write(saves.path());

    let created = manager.create_save(&career(), "Ghost");

    assert_eq!(created, Err(WRITE_FAILED.to_string()));
    assert!(
        manager.list_saves().is_empty(),
        "a ghost is listed in memory"
    );
    assert!(durable_ids(saves.path()).is_empty());
    assert!(db_files(saves.path()).is_empty());
}

/// Given an index write that fails before its rename, when a save is created through
/// `create_save_with_stats` (the new-game flow), then nothing is listed or left behind.
#[test]
fn a_failed_create_save_with_stats_lists_nothing_in_memory_or_on_disk() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    obstruct_index_write(saves.path());

    let created = manager.create_save_with_stats(&career(), &StatsState::default(), "Ghost");

    assert_eq!(created, Err(WRITE_FAILED.to_string()));
    assert!(
        manager.list_saves().is_empty(),
        "a ghost is listed in memory"
    );
    assert!(durable_ids(saves.path()).is_empty());
    assert!(db_files(saves.path()).is_empty());
}

/// Given a failed create, when the obstruction is cleared and the player tries again, then
/// exactly one career exists, in memory, on disk and as a file.
#[test]
fn a_create_save_retried_after_the_obstruction_clears_yields_exactly_one_career() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    obstruct_index_write(saves.path());
    assert!(manager.create_save(&career(), "First try").is_err());
    clear_obstruction(saves.path());

    let id = manager.create_save(&career(), "Second try").unwrap();

    assert_eq!(listed_ids(&manager), vec![id]);
    assert_one_story(&mut manager, saves.path());
}

/// Given a failed create (a ghost that would be harmless while only in memory, but that the next
/// successful index write would persist), when a different save then succeeds, then the durable
/// index lists only the save that succeeded.
#[test]
fn a_ghost_from_a_failed_create_never_becomes_persistent() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    obstruct_index_write(saves.path());
    assert!(
        manager
            .create_save_with_stats(&career(), &StatsState::default(), "Ghost")
            .is_err()
    );
    clear_obstruction(saves.path());

    let real = manager.create_save(&career(), "Real").unwrap();

    assert_eq!(durable_ids(saves.path()), vec![real.clone()]);
    let restarted = SaveManager::init(saves.path()).unwrap();
    assert_eq!(listed_ids(&restarted), vec![real]);
}

/// Given a saved career, when the index update after a save fails, then the index in memory
/// still describes what the durable index describes, so a restart and this session agree.
#[test]
fn a_failed_save_game_leaves_memory_equal_to_the_durable_index() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    let id = manager.create_save(&career(), "Career").unwrap();
    let before = manager.list_saves()[0].clone();
    obstruct_index_write(saves.path());
    let mut played = career();
    played.manager.first_name = "Renamed".to_string();

    let saved = manager.save_game(&played, &id);

    assert_eq!(saved, Err(WRITE_FAILED.to_string()));
    let in_memory = manager.list_saves()[0].clone();
    let on_disk = load_index(&saves.path().join("save_index.json"))
        .unwrap()
        .unwrap()
        .saves
        .remove(0);
    assert_eq!(in_memory.checksum, on_disk.checksum);
    assert_eq!(in_memory.manager_name, on_disk.manager_name);
    assert_eq!(in_memory.last_played_at, on_disk.last_played_at);
    assert_eq!(on_disk.checksum, before.checksum);
}

fn entry(id: &str) -> SaveEntry {
    SaveEntry {
        id: id.to_string(),
        name: "Career".to_string(),
        manager_name: "Pat Doyle".to_string(),
        team_name: "Club X".to_string(),
        db_filename: format!("{id}.db"),
        checksum: "old".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        last_played_at: "2026-01-02T00:00:00Z".to_string(),
    }
}

/// Given an indexed career, when `update_save` cannot write, then the entry in memory is the
/// old one, not the half-applied new one.
#[test]
fn a_failed_update_save_keeps_the_old_entry_in_memory() {
    let saves = tempfile::tempdir().unwrap();
    let mut index = SaveIndexManager::init(saves.path()).unwrap();
    index.ensure_loaded().unwrap();
    index.record_new_save(entry("a")).unwrap();
    obstruct_index_write(saves.path());
    let mut changed = entry("a");
    changed.checksum = "new".to_string();
    changed.name = "Renamed".to_string();

    let updated = index.update_save(changed);

    assert_eq!(updated, Err(WRITE_FAILED.to_string()));
    let kept = index.find("a").unwrap();
    assert_eq!(
        (kept.checksum.as_str(), kept.name.as_str()),
        ("old", "Career")
    );
}

/// Given an indexed career, when `remove_save` cannot write, then the career is still listed in
/// memory, matching the durable index that still lists it.
#[test]
fn a_failed_remove_save_keeps_the_entry_in_memory() {
    let saves = tempfile::tempdir().unwrap();
    let mut index = SaveIndexManager::init(saves.path()).unwrap();
    index.ensure_loaded().unwrap();
    index.record_new_save(entry("a")).unwrap();
    obstruct_index_write(saves.path());

    let removed = index.remove_save("a");

    assert_eq!(removed, Err(WRITE_FAILED.to_string()));
    assert!(index.find("a").is_some());
    assert_eq!(durable_ids(saves.path()), vec!["a".to_string()]);
}

/// Given a career whose index entry cannot be removed, when it is deleted, then the failure is
/// reported and the career database survives: the old durable index still points at it, so it
/// must never be the only thing deleted.
#[test]
fn a_failed_delete_save_does_not_remove_the_only_career_database() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    let id = manager.create_save(&career(), "Only career").unwrap();
    obstruct_index_write(saves.path());

    let deleted = manager.delete_save(&id);

    assert_eq!(deleted, Err(WRITE_FAILED.to_string()));
    clear_obstruction(saves.path());
    assert_one_story(&mut manager, saves.path());
    assert_eq!(listed_ids(&manager), vec![id.clone()]);
    let restarted = SaveManager::init(saves.path()).unwrap();
    assert_eq!(listed_ids(&restarted), vec![id]);
}

/// Given a delete that fails, when the player retries after the obstruction clears, then it
/// succeeds and removes both the entry and the file.
#[test]
fn a_delete_retried_after_the_obstruction_clears_removes_the_career() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    let id = manager.create_save(&career(), "Career").unwrap();
    obstruct_index_write(saves.path());
    assert!(manager.delete_save(&id).is_err());
    clear_obstruction(saves.path());

    assert_eq!(manager.delete_save(&id), Ok(true));

    assert!(manager.list_saves().is_empty());
    assert!(durable_ids(saves.path()).is_empty());
    assert!(db_files(saves.path()).is_empty());
}

/// Given two careers, one whose database file cannot be removed (here a directory stands where
/// the file should be, inside the tempdir), when it is deleted, then the failure is reported and
/// the career is listed again with every field it had, in memory and on disk, so a retry works.
#[test]
fn a_delete_whose_file_cannot_be_removed_relists_the_career_unchanged() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    let stuck = manager.create_save(&career(), "Stuck").unwrap();
    let other = manager.create_save(&career(), "Other").unwrap();
    let entry_of = |manager: &SaveManager, id: &str| {
        format!(
            "{:?}",
            manager.list_saves().iter().find(|entry| entry.id == id)
        )
    };
    let before = (entry_of(&manager, &stuck), entry_of(&manager, &other));
    let db_path = saves.path().join(format!("{stuck}.db"));
    fs::remove_file(&db_path).unwrap();
    fs::create_dir(&db_path).unwrap();

    let deleted = manager.delete_save(&stuck);

    assert_eq!(deleted, Err("be.error.saveDeleteFailed".to_string()));
    assert_eq!(
        before,
        (entry_of(&manager, &stuck), entry_of(&manager, &other))
    );
    let restarted = SaveManager::init(saves.path()).unwrap();
    assert_eq!(entry_of(&restarted, &stuck), before.0);
    assert_eq!(restarted.list_saves().len(), 2);
}

/// Given a mix of failures, when the manager restarts, then every listed career has its file and
/// every career file is listed: the index reconciles with the surviving databases.
#[test]
fn after_a_mix_of_failures_a_restart_reconciles_index_and_databases() {
    let saves = tempfile::tempdir().unwrap();
    let mut manager = ready_manager(saves.path());
    let kept = manager.create_save(&career(), "Kept").unwrap();
    let doomed = manager.create_save(&career(), "Doomed").unwrap();
    obstruct_index_write(saves.path());
    assert!(manager.create_save(&career(), "Ghost 1").is_err());
    assert!(
        manager
            .create_save_with_stats(&career(), &StatsState::default(), "Ghost 2")
            .is_err()
    );
    assert!(manager.delete_save(&doomed).is_err());
    clear_obstruction(saves.path());
    assert_eq!(manager.delete_save(&doomed), Ok(true));

    let mut restarted = SaveManager::init(saves.path()).unwrap();

    assert_eq!(listed_ids(&restarted), vec![kept]);
    assert_one_story(&mut restarted, saves.path());
}
