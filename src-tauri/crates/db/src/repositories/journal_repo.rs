use std::collections::HashSet;

use domain::finance::{CashKind, CashPost};
use ofm_core::game::Game;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

const WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";
const LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";

fn encode_enum<T: Serialize>(value: &T) -> Result<String, String> {
    match serde_json::to_value(value).map_err(|_| WRITE_ERROR.to_string())? {
        serde_json::Value::String(text) => Ok(text),
        _ => Err(WRITE_ERROR.to_string()),
    }
}

fn decode_enum<T: DeserializeOwned>(text: String, column: usize) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(text)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
    })
}

/// Insert posts whose ids are not already stored. Collision is a no-op.
pub fn insert_cash_posts<'a, I>(conn: &Connection, posts: I) -> Result<usize, String>
where
    I: IntoIterator<Item = &'a CashPost>,
{
    let mut posts = posts.into_iter().peekable();
    if posts.peek().is_none() {
        return Ok(0);
    }
    // The caller (`write_game_to_connection`) already holds a transaction.
    let mut stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO cash_journal
             (id, club_id, amount, kind, date)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|_| WRITE_ERROR.to_string())?;
    let mut inserted = 0;
    for post in posts {
        let kind = encode_enum(&post.kind)?;
        inserted += stmt
            .execute(params![post.id, post.club_id, post.amount, kind, post.date,])
            .map_err(|_| WRITE_ERROR.to_string())?;
    }
    Ok(inserted)
}

/// Persist the dirty-id hint from `game`. Does not clear the hint (`&Game`).
pub fn insert_dirty_cash_posts(conn: &Connection, game: &Game) -> Result<usize, String> {
    if game.cash_journal_dirty_ids.is_empty() {
        return Ok(0);
    }
    let wanted: HashSet<&str> = game
        .cash_journal_dirty_ids
        .iter()
        .map(String::as_str)
        .collect();
    insert_cash_posts(
        conn,
        game.cash_journal
            .iter()
            .filter(|post| wanted.contains(post.id.as_str())),
    )
}

/// Overwrite saves only need new rows. A brand-new `.db` (`create_save`) must
/// get the whole journal: loaded games have an empty dirty list.
/// Removes the stored posts a fold replaced: everything before `cutoff` that is not an opening
/// balance and not itself a fold. The folds are inserted by the same save, in the same transaction.
fn remove_folded_posts(conn: &Connection, cutoff: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM cash_journal
         WHERE date < ?1 AND kind <> 'opening_balance' AND id NOT LIKE 'fold:%'",
        params![cutoff],
    )
    .map_err(|_| WRITE_ERROR.to_string())
}

pub fn persist_cash_journal(conn: &Connection, game: &Game) -> Result<usize, String> {
    if cash_journal_has_rows(conn)? {
        if let Some(cutoff) = &game.cash_journal_fold_pending {
            remove_folded_posts(conn, cutoff)?;
        }
        insert_dirty_cash_posts(conn, game)
    } else {
        insert_cash_posts(conn, game.cash_journal.iter())
    }
}

pub fn load_cash_journal(conn: &Connection) -> Result<Vec<CashPost>, String> {
    if !table_exists(conn, "cash_journal")? {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, club_id, amount, kind, date
             FROM cash_journal
             ORDER BY rowid",
        )
        .map_err(|_| LOAD_ERROR.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let kind_str: String = row.get(3)?;
            let kind: CashKind = decode_enum(kind_str, 3)?;
            Ok(CashPost {
                id: row.get(0)?,
                club_id: row.get(1)?,
                amount: row.get(2)?,
                kind,
                date: row.get(4)?,
            })
        })
        .map_err(|_| LOAD_ERROR.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| LOAD_ERROR.to_string())
}

fn cash_journal_has_rows(conn: &Connection) -> Result<bool, String> {
    if !table_exists(conn, "cash_journal")? {
        return Ok(false);
    }
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM cash_journal LIMIT 1", [], |row| row.get(0))
        .optional()
        .map_err(|_| LOAD_ERROR.to_string())?;
    Ok(exists.is_some())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, String> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| LOAD_ERROR.to_string())?;
    Ok(exists.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;
    use domain::finance::CashKind;

    fn sample_post(id: &str, amount: i64) -> CashPost {
        CashPost {
            id: id.to_string(),
            club_id: "club-a".to_string(),
            amount,
            kind: CashKind::Matchday,
            date: "2026-02-16".to_string(),
        }
    }

    #[test]
    fn insert_or_ignore_does_not_duplicate() {
        let db = GameDatabase::open_in_memory().unwrap();
        let post = sample_post("post-1", 12_000);
        assert_eq!(
            insert_cash_posts(db.conn(), std::iter::once(&post)).unwrap(),
            1
        );
        assert_eq!(
            insert_cash_posts(db.conn(), std::iter::once(&post)).unwrap(),
            0
        );
        let loaded = load_cash_journal(db.conn()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].amount, 12_000);
        assert_eq!(loaded[0].kind, CashKind::Matchday);
    }

    #[test]
    fn load_preserves_insert_order_for_same_date() {
        let db = GameDatabase::open_in_memory().unwrap();
        let first = sample_post("z-last-alphabetically", 1);
        let second = sample_post("a-first-alphabetically", 2);
        insert_cash_posts(db.conn(), [&first, &second]).unwrap();
        let loaded = load_cash_journal(db.conn()).unwrap();
        assert_eq!(loaded[0].id, "z-last-alphabetically");
        assert_eq!(loaded[1].id, "a-first-alphabetically");
    }

    #[test]
    fn empty_destination_writes_the_whole_journal() {
        use chrono::{TimeZone, Utc};
        use domain::manager::Manager;
        use domain::team::Team;
        use ofm_core::clock::GameClock;
        use ofm_core::game::Game;

        let db = GameDatabase::open_in_memory().unwrap();
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 2, 16, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("club-a".to_string());
        let mut team = Team::new(
            "club-a".to_string(),
            "Club A".to_string(),
            "CLA".to_string(),
            "England".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            20_000,
        );
        team.finance = 13_000;
        let mut game = Game::new(clock, manager, vec![team], vec![], vec![], vec![]);
        game.cash_journal = domain::finance::CashJournal::from_vec(vec![
            sample_post("old-1", 12_000),
            sample_post("old-2", 1_000),
        ]);

        assert_eq!(persist_cash_journal(db.conn(), &game).unwrap(), 2);
        assert_eq!(load_cash_journal(db.conn()).unwrap().len(), 2);
        assert_eq!(persist_cash_journal(db.conn(), &game).unwrap(), 0);
        assert_eq!(load_cash_journal(db.conn()).unwrap().len(), 2);
    }

    fn dated_post(id: &str, club: &str, amount: i64, kind: CashKind, date: &str) -> CashPost {
        CashPost {
            id: id.to_string(),
            club_id: club.to_string(),
            amount,
            kind,
            date: date.to_string(),
        }
    }

    /// Given a stored journal and a fold of its old months that a save is carrying,
    /// When the cutoff is applied to the file,
    /// Then the old single payments are gone, the opening balance, the recent payments and any
    /// earlier fold stay, and the club's stored balance is what it was.
    #[test]
    fn a_fold_removes_the_stored_payments_it_replaced_and_nothing_else() {
        let db = GameDatabase::open_in_memory().unwrap();
        let stored = vec![
            dated_post("o", "a", 1_000, CashKind::OpeningBalance, "2020-07-01"),
            dated_post("old-fold", "a", -300, CashKind::PlayerWages, "2020-08-01"),
            dated_post("w1", "a", -100, CashKind::PlayerWages, "2021-09-06"),
            dated_post("w2", "a", -100, CashKind::PlayerWages, "2021-09-13"),
            dated_post("recent", "a", -100, CashKind::PlayerWages, "2024-01-08"),
        ];
        // The id that marks a fold is part of the contract, so the earlier fold is named as one.
        let mut stored = stored;
        stored[1].id = "fold:a:PlayerWages:2020-08".to_string();
        insert_cash_posts(db.conn(), stored.iter()).unwrap();
        let balance_before: i64 = load_cash_journal(db.conn())
            .unwrap()
            .iter()
            .map(|p| p.amount)
            .sum();

        let removed = remove_folded_posts(db.conn(), "2023-01-01").unwrap();
        insert_cash_posts(
            db.conn(),
            std::iter::once(&dated_post(
                "fold:a:PlayerWages:2021-09",
                "a",
                -200,
                CashKind::PlayerWages,
                "2021-09-01",
            )),
        )
        .unwrap();

        assert_eq!(removed, 2, "only the two single payments before the cutoff");
        let after = load_cash_journal(db.conn()).unwrap();
        let ids: Vec<&str> = after.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "o",
                "fold:a:PlayerWages:2020-08",
                "recent",
                "fold:a:PlayerWages:2021-09"
            ]
        );
        assert_eq!(after.iter().map(|p| p.amount).sum::<i64>(), balance_before);
    }
}
