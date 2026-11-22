//! REALISM3 regression tests on the player's own `world.db` (`APFM_WORLD_DB`).
//!
//! Ignored by default: the data never lives in the repository. Run them with
//! `APFM_WORLD_DB=<path> cargo test -p fm24-adapter --test realism3_value_scale -- --include-ignored`.

use chrono::NaiveDate;
use fm24_adapter::career::ensure_career_cache;
use fm24_adapter::registry::build_registry;
use fm24_adapter::{Config, pipeline};
use ofm_core::value_scale::{Standing, ValueScale};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn source() -> Option<PathBuf> {
    std::env::var_os("APFM_WORLD_DB").map(PathBuf::from)
}

fn rows(path: &std::path::Path) -> BTreeMap<i64, (u8, u8, u64, u64)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).expect("opens");
    let mut statement = conn
        .prepare(
            "SELECT person_id, ovr, potential, market_value_eur, wage_eur_week
             FROM registry_player WHERE status = 'eligible' AND market_value_eur > 0",
        )
        .expect("prepares");
    statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (
                    r.get::<_, i64>(1)? as u8,
                    r.get::<_, i64>(2)? as u8,
                    r.get::<_, i64>(3)? as u64,
                    r.get::<_, i64>(4)? as u64,
                ),
            ))
        })
        .expect("queries")
        .collect::<Result<_, _>>()
        .expect("reads")
}

/// Given the real save,
/// When the world and the external registry are built from it,
/// Then they carry the very same value scale: the active world, the market around it and its newgens
/// are all priced on one curve.
#[test]
#[ignore = "needs APFM_WORLD_DB: the real world.db, never in the repository"]
fn the_world_and_its_registry_share_one_value_scale() {
    let Some(source) = source() else { return };
    let dir = tempfile::tempdir().expect("temp dir");
    let run = pipeline::run(
        &source,
        &dir.path().join("package"),
        &Config::default_config(),
    )
    .expect("the world builds");
    let registry = dir.path().join("registry.db");
    build_registry(&source, &registry).expect("the registry builds");
    let conn =
        Connection::open_with_flags(&registry, OpenFlags::SQLITE_OPEN_READ_ONLY).expect("opens");
    let stored: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'value_scale'",
            [],
            |r| r.get(0),
        )
        .expect("the registry records its scale");

    let from_registry: ValueScale = serde_json::from_str(&stored).expect("a scale");

    assert!(run.value_scale.is_calibrated());
    assert_eq!(from_registry, run.value_scale);
}

/// Given the real save's external market,
/// When it is projected one season and five seasons on through the career cache,
/// Then no player is re-priced onto another scale: the elite keep their value, ordinary players are
/// not inflated, and wages stay where they were. (With the native formula 97% of players more than
/// doubled in the first season, the elite kept a quarter of their value and the median wage rose
/// seventeenfold.)
#[test]
#[ignore = "needs APFM_WORLD_DB: the real world.db, never in the repository"]
fn external_players_keep_their_value_and_pay_across_seasons() {
    let Some(source) = source() else { return };
    let dir = tempfile::tempdir().expect("temp dir");
    let registry = dir.path().join("registry.db");
    build_registry(&source, &registry).expect("the registry builds");
    let cache = dir.path().join("cache");
    let at = |date: &str| NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date");
    let start = rows(
        &ensure_career_cache(&registry, &cache, 7, at("2026-06-01"))
            .expect("start")
            .path,
    );
    let first = rows(
        &ensure_career_cache(&registry, &cache, 7, at("2027-07-01"))
            .expect("s1")
            .path,
    );
    let fifth = rows(
        &ensure_career_cache(&registry, &cache, 7, at("2031-07-01"))
            .expect("s5")
            .path,
    );

    let ratios = |later: &BTreeMap<i64, (u8, u8, u64, u64)>| -> Vec<(f64, f64, u64)> {
        start
            .iter()
            .filter_map(|(id, then)| {
                let now = later.get(id)?;
                Some((
                    now.2 as f64 / then.2 as f64,
                    now.3 as f64 / then.3.max(1) as f64,
                    then.2,
                ))
            })
            .collect()
    };
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let one = ratios(&first);
    let elite: Vec<f64> = one
        .iter()
        .filter(|r| r.2 >= 20_000_000)
        .map(|r| r.0)
        .collect();
    let doubled = one.iter().filter(|r| r.0 > 2.0).count() as f64 / one.len() as f64;

    assert!(one.len() > 50_000, "{}", one.len());
    assert!(
        median(elite) > 0.8,
        "the elite keep their value across one season"
    );
    assert!(
        doubled < 0.05,
        "{doubled}: almost nobody doubles in one season"
    );
    assert!((0.5..2.0).contains(&median(one.iter().map(|r| r.0).collect())));
    assert!(
        (0.5..2.0).contains(&median(one.iter().map(|r| r.1).collect())),
        "wages stay put"
    );
    let five = ratios(&fifth);
    assert!(median(five.iter().map(|r| r.0).collect()) < 3.0);
    let after: Vec<u64> = first.values().map(|v| v.2).collect();
    assert!(
        after.iter().copied().max() > Some(60_000_000),
        "the top of the market keeps its scale"
    );
}

/// Given the real world,
/// When it is built,
/// Then every generated filler is valued on the world's own scale, like the players around him, not
/// by the generator's formula (which prices a middling player in millions).
#[test]
#[ignore = "needs APFM_WORLD_DB: the real world.db, never in the repository"]
fn generated_fillers_are_valued_on_the_worlds_scale() {
    let Some(source) = source() else { return };
    let dir = tempfile::tempdir().expect("temp dir");
    let run = pipeline::run(
        &source,
        &dir.path().join("package"),
        &Config::default_config(),
    )
    .expect("the world builds");
    let on = NaiveDate::parse_from_str(&run.cfg.start.date, "%Y-%m-%d").expect("date");

    let fillers: Vec<_> = run
        .world
        .players
        .iter()
        .filter(|p| !p.id.starts_with("fm-p-"))
        .collect();

    assert!(!fillers.is_empty());
    for filler in fillers {
        let standing = Standing {
            ovr: filler.ovr,
            potential: filler.potential.max(filler.ovr),
            age: domain::age::age_on_or_unknown(on, &filler.date_of_birth),
        };
        assert_eq!(
            filler.market_value,
            run.value_scale.value(standing),
            "{}",
            filler.id
        );
    }
}
