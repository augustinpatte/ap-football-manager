//! A career's materialised external market.
//!
//! D7B1's registry is a read-only snapshot: every career sees the same 2026 attributes. D7B2
//! projects that snapshot forward with `ofm_core::external_world`, and this module stores the
//! result in one local SQLite file per career so a search does not re-project ten thousand players
//! on every request.
//!
//! The cache is **derived, not authoritative**. It lives beside the shared registry, is named from
//! the career's seed, the source's start season and the source file's SHA-256, and can be deleted
//! and rebuilt at any time. The shared `registry.db` is opened read-only and is never mutated, so
//! two careers built from the same source diverge without touching each other.
//!
//! Rebuild policy: at most one season advance per season of the career clock. A cache that is
//! already at the target season is reused as is; an older one is advanced one absolute season at a
//! time; a cache from the future (an older save loaded after a newer one) is rebuilt from the
//! source. Every step is a transaction, and a failed build writes to a staging file that is renamed
//! only once complete.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use chrono::NaiveDate;
use domain::player::{PlayerAttributes, Position};
use ofm_core::external_registry::{ExternalClub, ExternalPlayer};
use ofm_core::external_world::{
    ExternalCareerKey, ExternalClubRef, MAX_NEWGEN_INTAKE_PER_SEASON, newgen_cohort,
    newgen_intake_for, project_from, projection_season,
};
use rusqlite::{Connection, OpenFlags, params};

use crate::ids;
use crate::refs::Refs;
use crate::registry::{
    self, REGISTRY_SCHEMA_VERSION, SCHEMA, fold_text, footedness_name, group_name,
};

/// Every column of `registry_player`, in schema order. Kept once so the copy, the read and the
/// insert cannot drift.
const COLUMNS: &str = "person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, \
     common_name, display_name, search_name, birth_date, nation, nation_code, natural_position, \
     position_group, alternate_positions, attributes_json, ovr, potential, footedness, weak_foot, \
     club_id, club_name, club_search, club_nation, club_reputation, club_is_active_world, \
     ownership, contract_end, wage_eur_week, market_value_eur, value_source, wage_source, status, \
     exclusion_reason, source_row_hash";

const INSERT_SQL: &str = "INSERT INTO registry_player (person_id, ofm_id, fm_uid, fm_uid_status, \
     first_name, last_name, common_name, display_name, search_name, birth_date, nation, nation_code, \
     natural_position, position_group, alternate_positions, attributes_json, ovr, potential, \
     footedness, weak_foot, club_id, club_name, club_search, club_nation, club_reputation, \
     club_is_active_world, ownership, contract_end, wage_eur_week, market_value_eur, value_source, \
     wage_source, status, exclusion_reason, source_row_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, \
     ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, \
     ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35)";

const SELECT_SQL: &str = "SELECT person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, \
     common_name, display_name, search_name, birth_date, nation, nation_code, natural_position, \
     position_group, alternate_positions, attributes_json, ovr, potential, footedness, weak_foot, \
     club_id, club_name, club_search, club_nation, club_reputation, club_is_active_world, \
     ownership, contract_end, wage_eur_week, market_value_eur, value_source, wage_source, status, \
     exclusion_reason, source_row_hash FROM registry_player";

/// What a cache build or reuse did, for the log and the validation report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerCacheReport {
    pub path: PathBuf,
    pub key: ExternalCareerKey,
    pub season: i32,
    pub rows: usize,
    /// A cache was built from the source because none was usable.
    pub rebuilt: bool,
    /// Seasons replayed by this call (zero when an up-to-date cache was reused).
    pub seasons_advanced: i32,
    pub elapsed_ms: u128,
}

/// Where a career's cache lives. The source hash belongs in the name: a rebuilt registry must not
/// be projected with an old career's rows.
pub fn career_cache_path(cache_dir: &Path, key: ExternalCareerKey, source_sha256: &str) -> PathBuf {
    let digest = source_sha256.get(..12).unwrap_or(source_sha256);
    cache_dir.join(format!(
        "career-{}-{}-{}.db",
        key.seed, key.start_season, digest
    ))
}

/// Every cache build shares one staging file name (`<cache>.staging`). Two commands arriving while
/// the cache is stale would otherwise delete and rewrite each other's half-built file, so the
/// maintenance path is serialised inside the process. It is also the honest scope of the guarantee:
/// two app processes are not coordinated, and the last complete rename wins.
static CACHE_MAINTENANCE: Mutex<()> = Mutex::new(());

/// Make sure the career's cache exists at `today`'s season, then return where it is.
pub fn ensure_career_cache(
    registry_path: &Path,
    cache_dir: &Path,
    seed: u64,
    today: NaiveDate,
) -> crate::Result<CareerCacheReport> {
    let _guard = CACHE_MAINTENANCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let started = Instant::now();
    if !registry_path.is_file() {
        return Err("be.error.fm24.registryMissing".to_string());
    }
    let source = registry::open_registry(registry_path)?;
    let source_meta = meta_of(&source)?;
    let source_sha256 = source_meta
        .get("source_sha256")
        .cloned()
        .unwrap_or_default();
    let start_season = source_meta
        .get("start_season")
        .and_then(|value| value.parse::<i32>().ok())
        .ok_or_else(|| "be.error.fm24.registryMissing".to_string())?;
    let key = ExternalCareerKey::new(seed, start_season);
    let season = projection_season(today, start_season);
    let path = career_cache_path(cache_dir, key, &source_sha256);
    std::fs::create_dir_all(cache_dir).map_err(|_| build_failed())?;

    let existing = cache_season(&path, key, &source_sha256);
    if let Some(cached_season) = existing
        && cached_season == season
    {
        let rows = count_rows(&path)?;
        return Ok(CareerCacheReport {
            path,
            key,
            season,
            rows,
            rebuilt: false,
            seasons_advanced: 0,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let clubs = source_clubs(&source)?;
    let staging = staging_path(&path);
    let _ = std::fs::remove_file(&staging);
    let rebuilt = existing.is_none_or(|cached| cached > season);
    let mut advanced_from = start_season;
    if rebuilt {
        copy_source(&staging, registry_path)?;
        write_meta(&staging, key, &source_sha256, start_season)?;
    } else if let Some(cached_season) = existing {
        // A usable cache from an earlier season: replay the missing absolute seasons on a copy so
        // an interruption cannot leave the live file half-advanced.
        advanced_from = cached_season;
        std::fs::copy(&path, &staging).map_err(|_| build_failed())?;
    }

    if season > advanced_from {
        let mut conn = open_write(&staging)?;
        advance(&mut conn, key, advanced_from, season, &clubs)?;
        drop(conn);
    }
    // A cache shrinks as source players retire; without this the file keeps the free pages of
    // every season it has ever held.
    let maintenance = open_write(&staging)?;
    maintenance
        .execute_batch("VACUUM;")
        .map_err(|_| build_failed())?;
    drop(maintenance);
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&staging, &path).map_err(|_| build_failed())?;
    let rows = count_rows(&path)?;
    Ok(CareerCacheReport {
        path,
        key,
        season,
        rows,
        rebuilt,
        seasons_advanced: (season - advanced_from).max(0),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn build_failed() -> String {
    "be.error.fm24.registryBuildFailed".to_string()
}

fn staging_path(path: &Path) -> PathBuf {
    let mut staging = path.to_path_buf();
    staging.set_extension("staging");
    staging
}

fn open_write(path: &Path) -> crate::Result<Connection> {
    Connection::open(path).map_err(|_| build_failed())
}

fn open_read(path: &Path) -> crate::Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| build_failed())
}

fn meta_of(conn: &Connection) -> crate::Result<HashMap<String, String>> {
    let mut statement = conn
        .prepare("SELECT key, value FROM meta")
        .map_err(|_| build_failed())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| build_failed())?;
    let mut meta = HashMap::new();
    for row in rows {
        let (key, value) = row.map_err(|_| build_failed())?;
        meta.insert(key, value);
    }
    Ok(meta)
}

/// The season a cache holds, or `None` when it is missing, from another source, another seed or
/// an older schema. That is the whole invalidation rule.
fn cache_season(path: &Path, key: ExternalCareerKey, source_sha256: &str) -> Option<i32> {
    if !path.is_file() {
        return None;
    }
    let conn = open_read(path).ok()?;
    let meta = meta_of(&conn).ok()?;
    let schema_ok = meta
        .get("schema_version")
        .and_then(|v| v.parse::<u32>().ok())
        == Some(REGISTRY_SCHEMA_VERSION);
    let seed_ok = meta.get("career_seed").and_then(|v| v.parse::<u64>().ok()) == Some(key.seed);
    let start_ok = meta
        .get("career_start_season")
        .and_then(|v| v.parse::<i32>().ok())
        == Some(key.start_season);
    let source_ok = meta.get("source_sha256").map(String::as_str) == Some(source_sha256);
    if !(schema_ok && seed_ok && start_ok && source_ok) {
        return None;
    }
    meta.get("career_projected_season")
        .and_then(|v| v.parse::<i32>().ok())
}

fn count_rows(path: &Path) -> crate::Result<usize> {
    let conn = open_read(path)?;
    conn.query_row("SELECT COUNT(*) FROM registry_player", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|value| value.max(0) as usize)
    .map_err(|_| build_failed())
}

/// Copy the snapshot into a new cache file, schema and all. `ATTACH` keeps this a single statement
/// instead of ten thousand binds.
fn copy_source(destination: &Path, source: &Path) -> crate::Result<()> {
    let conn = open_write(destination)?;
    conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
        .map_err(|_| build_failed())?;
    conn.execute_batch(SCHEMA).map_err(|_| build_failed())?;
    conn.execute(
        "ATTACH DATABASE ?1 AS source_registry",
        params![source.to_string_lossy().to_string()],
    )
    .map_err(|_| build_failed())?;
    conn.execute(
        &format!(
            "INSERT INTO registry_player ({COLUMNS}) SELECT {COLUMNS} FROM source_registry.registry_player"
        ),
        [],
    )
    .map_err(|_| build_failed())?;
    conn.execute_batch("DETACH DATABASE source_registry")
        .map_err(|_| build_failed())?;
    Ok(())
}

fn write_meta(
    path: &Path,
    key: ExternalCareerKey,
    source_sha256: &str,
    projected_season: i32,
) -> crate::Result<()> {
    let conn = open_write(path)?;
    let meta = [
        ("schema_version", REGISTRY_SCHEMA_VERSION.to_string()),
        ("source_sha256", source_sha256.to_string()),
        ("career_seed", key.seed.to_string()),
        ("career_start_season", key.start_season.to_string()),
        ("career_projected_season", projected_season.to_string()),
    ];
    for (name, value) in meta {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![name, value],
        )
        .map_err(|_| build_failed())?;
    }
    Ok(())
}

/// The clubs the market knows: the source clubs that employ at least one player **outside the
/// active perimeter**. Built from the shared registry, never from the career cache, so club
/// selection is the same on a rebuild.
///
/// Active-world clubs are excluded on purpose. Their squads are simulated by the game; putting a
/// projected external player "at" one of them without adding him to its roster would show a club
/// sheet that contradicts the market. Keeping the two club sets disjoint removes that class of
/// inconsistency.
fn source_clubs(conn: &Connection) -> crate::Result<Vec<ExternalClubRef>> {
    let refs = Refs::embedded()?;
    let mut statement = conn
        .prepare(
            "SELECT DISTINCT club_id, club_name, club_nation, club_reputation
             FROM registry_player
             WHERE club_id IS NOT NULL AND club_name <> '' AND club_is_active_world = 0",
        )
        .map_err(|_| build_failed())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|_| build_failed())?;
    let mut clubs = Vec::new();
    for row in rows {
        let (id, name, nation, reputation) = row.map_err(|_| build_failed())?;
        let nation_code = nation
            .as_deref()
            .and_then(|name| refs.nation_code(name))
            .unwrap_or_else(|| "GB".to_string());
        clubs.push(ExternalClubRef {
            id: ids::club(id),
            name,
            nation_code,
            reputation,
        });
    }
    clubs.sort_by(|a, b| {
        a.reputation
            .cmp(&b.reputation)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(clubs)
}

/// One stored row, owned so the table can be rewritten while it is being projected.
#[derive(Debug, Clone)]
struct CachedRow {
    person_id: i64,
    ofm_id: String,
    fm_uid: Option<i64>,
    fm_uid_status: String,
    first_name: String,
    last_name: String,
    common_name: Option<String>,
    display_name: String,
    search_name: String,
    birth_date: Option<String>,
    nation: Option<String>,
    nation_code: Option<String>,
    natural_position: Option<String>,
    position_group: Option<String>,
    alternate_positions: String,
    attributes_json: Option<String>,
    ovr: i64,
    potential: i64,
    footedness: String,
    weak_foot: i64,
    club_id: Option<i64>,
    club_name: String,
    club_search: String,
    club_nation: Option<String>,
    club_reputation: i64,
    club_is_active_world: i64,
    ownership: String,
    contract_end: Option<String>,
    wage_eur_week: i64,
    market_value_eur: i64,
    value_source: String,
    wage_source: String,
    status: String,
    exclusion_reason: Option<String>,
    source_row_hash: String,
}

impl CachedRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            person_id: row.get(0)?,
            ofm_id: row.get(1)?,
            fm_uid: row.get(2)?,
            fm_uid_status: row.get(3)?,
            first_name: row.get(4)?,
            last_name: row.get(5)?,
            common_name: row.get(6)?,
            display_name: row.get(7)?,
            search_name: row.get(8)?,
            birth_date: row.get(9)?,
            nation: row.get(10)?,
            nation_code: row.get(11)?,
            natural_position: row.get(12)?,
            position_group: row.get(13)?,
            alternate_positions: row.get(14)?,
            attributes_json: row.get(15)?,
            ovr: row.get(16)?,
            potential: row.get(17)?,
            footedness: row.get(18)?,
            weak_foot: row.get(19)?,
            club_id: row.get(20)?,
            club_name: row.get(21)?,
            club_search: row.get(22)?,
            club_nation: row.get(23)?,
            club_reputation: row.get(24)?,
            club_is_active_world: row.get(25)?,
            ownership: row.get(26)?,
            contract_end: row.get(27)?,
            wage_eur_week: row.get(28)?,
            market_value_eur: row.get(29)?,
            value_source: row.get(30)?,
            wage_source: row.get(31)?,
            status: row.get(32)?,
            exclusion_reason: row.get(33)?,
            source_row_hash: row.get(34)?,
        })
    }

    fn to_external(&self) -> Option<ExternalPlayer> {
        let attributes: PlayerAttributes =
            serde_json::from_str(self.attributes_json.as_deref()?).ok()?;
        let natural_position: Position =
            serde_json::from_str(self.natural_position.as_deref()?).ok()?;
        let nationality = self.nation_code.clone()?;
        let birth_date = self.birth_date.clone()?;
        let alternate_positions: Vec<Position> =
            serde_json::from_str(&self.alternate_positions).unwrap_or_default();
        let footedness = serde_json::from_value(serde_json::Value::String(self.footedness.clone()))
            .unwrap_or_default();
        let club = self.club_id.map(|id| ExternalClub {
            id: ids::club(id),
            name: self.club_name.clone(),
            contract_end: self.contract_end.clone(),
            weekly_wage: u32::try_from(self.wage_eur_week.max(0)).unwrap_or(u32::MAX),
            reputation: self.club_reputation,
        });
        let match_name = self
            .common_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| self.last_name.clone());
        Some(ExternalPlayer {
            id: self.ofm_id.clone(),
            match_name,
            full_name: self.display_name.clone(),
            date_of_birth: birth_date,
            nationality,
            position: natural_position.clone(),
            natural_position,
            alternate_positions,
            footedness,
            weak_foot: u8::try_from(self.weak_foot.clamp(0, 255)).unwrap_or(0),
            attributes,
            ovr: u8::try_from(self.ovr.clamp(0, 255)).unwrap_or(0),
            potential: u8::try_from(self.potential.clamp(0, 255)).unwrap_or(0),
            market_value: self.market_value_eur.max(0) as u64,
            club,
        })
    }

    fn apply_projection(&mut self, projected: &ofm_core::external_world::ProjectedExternalPlayer) {
        let player = &projected.player;
        self.attributes_json = serde_json::to_string(&player.attributes).ok();
        self.ovr = i64::from(player.ovr);
        self.potential = i64::from(player.potential);
        self.market_value_eur = player.market_value as i64;
        self.value_source = "projected".to_string();
        self.wage_source = "projected".to_string();
        self.club_is_active_world = 0;
        self.status = "eligible".to_string();
        self.exclusion_reason = None;
        match &player.club {
            Some(club) => {
                self.club_id = ids::internal_key(&club.id, "fm-c-");
                self.club_name = club.name.clone();
                self.club_search = fold_text(&club.name);
                self.ownership = "contracted".to_string();
                self.contract_end = club.contract_end.clone();
                self.wage_eur_week = i64::from(club.weekly_wage);
                self.club_reputation = club.reputation;
            }
            None => {
                self.club_id = None;
                self.club_name.clear();
                self.club_search.clear();
                self.club_nation = None;
                self.club_reputation = 0;
                self.ownership = "free_agent".to_string();
                self.contract_end = None;
                self.wage_eur_week = 0;
            }
        }
    }
}

fn read_rows(conn: &Connection) -> crate::Result<Vec<CachedRow>> {
    let mut statement = conn.prepare(SELECT_SQL).map_err(|_| build_failed())?;
    let rows = statement
        .query_map([], CachedRow::from_row)
        .map_err(|_| build_failed())?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| build_failed())
}

fn bind_row(statement: &mut rusqlite::Statement<'_>, row: &CachedRow) -> crate::Result<()> {
    statement
        .execute(params![
            row.person_id,
            row.ofm_id,
            row.fm_uid,
            row.fm_uid_status,
            row.first_name,
            row.last_name,
            row.common_name,
            row.display_name,
            row.search_name,
            row.birth_date,
            row.nation,
            row.nation_code,
            row.natural_position,
            row.position_group,
            row.alternate_positions,
            row.attributes_json,
            row.ovr,
            row.potential,
            row.footedness,
            row.weak_foot,
            row.club_id,
            row.club_name,
            row.club_search,
            row.club_nation,
            row.club_reputation,
            row.club_is_active_world,
            row.ownership,
            row.contract_end,
            row.wage_eur_week,
            row.market_value_eur,
            row.value_source,
            row.wage_source,
            row.status,
            row.exclusion_reason,
            row.source_row_hash,
        ])
        .map(|_| ())
        .map_err(|_| build_failed())
}

fn write_rows(conn: &mut Connection, rows: &[CachedRow], season: i32) -> crate::Result<()> {
    let transaction = conn.transaction().map_err(|_| build_failed())?;
    transaction
        .execute("DELETE FROM registry_player", [])
        .map_err(|_| build_failed())?;
    {
        let mut statement = transaction
            .prepare(INSERT_SQL)
            .map_err(|_| build_failed())?;
        for row in rows {
            bind_row(&mut statement, row)?;
        }
    }
    for (name, value) in [
        ("career_projected_season", season.to_string()),
        ("career_rows", rows.len().to_string()),
    ] {
        transaction
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                params![name, value],
            )
            .map_err(|_| build_failed())?;
    }
    transaction.commit().map_err(|_| build_failed())
}

/// Replay the missing seasons one at a time. Each step reads the previous step's rows, so the
/// result is the same as a full replay from the snapshot (the projection is a pure function of the
/// stored row, the seed and the absolute season).
fn advance(
    conn: &mut Connection,
    key: ExternalCareerKey,
    from_season: i32,
    to_season: i32,
    clubs: &[ExternalClubRef],
) -> crate::Result<()> {
    for season in (from_season + 1)..=to_season {
        let rows = read_rows(conn)?;
        let mut next: Vec<CachedRow> =
            Vec::with_capacity(rows.len() + MAX_NEWGEN_INTAKE_PER_SEASON);
        let mut retirements = 0usize;
        for mut row in rows {
            if row.status != "eligible" {
                next.push(row);
                continue;
            }
            let Some(external) = row.to_external() else {
                continue;
            };
            let is_newgen = row.ofm_id.starts_with("fm-n-");
            let projected = project_from(&external, key, season - 1, season, clubs, is_newgen);
            if projected.retired {
                retirements += 1;
                continue;
            }
            row.apply_projection(&projected);
            next.push(row);
        }
        // One-for-one replacement: the market is a population, so a season that lost players gets
        // the same number of new ones. The fixed D7B2 intake shrank the searchable market by ~77%
        // over thirty seasons; sizing births to deaths keeps it stable without inflating quality.
        for (index, newgen) in newgen_cohort(key, season, clubs, newgen_intake_for(retirements))
            .into_iter()
            .enumerate()
        {
            let projected = project_from(&newgen, key, season, season, clubs, true);
            if projected.retired {
                continue;
            }
            next.push(newgen_row(&projected, season, index));
        }
        write_rows(conn, &next, season)?;
    }
    Ok(())
}

fn newgen_row(
    projected: &ofm_core::external_world::ProjectedExternalPlayer,
    season: i32,
    index: usize,
) -> CachedRow {
    let player = &projected.player;
    let (first_name, last_name) = split_name(&player.full_name, &player.match_name);
    let natural_position = serde_json::to_string(&player.natural_position)
        .unwrap_or_else(|_| "\"Striker\"".to_string());
    let alternate_positions =
        serde_json::to_string(&player.alternate_positions).unwrap_or_else(|_| "[]".to_string());
    let attributes_json = serde_json::to_string(&player.attributes).ok();
    let (
        club_id,
        club_name,
        club_search,
        club_nation,
        club_reputation,
        ownership,
        contract_end,
        wage,
    ) = match &player.club {
        Some(club) => (
            ids::internal_key(&club.id, "fm-c-"),
            club.name.clone(),
            fold_text(&club.name),
            Some(player.nationality.clone()),
            club.reputation,
            "contracted".to_string(),
            club.contract_end.clone(),
            i64::from(club.weekly_wage),
        ),
        None => (
            None,
            String::new(),
            String::new(),
            None,
            0,
            "free_agent".to_string(),
            None,
            0,
        ),
    };
    CachedRow {
        person_id: -(i64::from(season) * 1_000_000 + index as i64 + 1),
        ofm_id: player.id.clone(),
        fm_uid: None,
        fm_uid_status: "generated".to_string(),
        first_name,
        last_name,
        common_name: None,
        display_name: player.full_name.clone(),
        search_name: fold_text(&player.full_name),
        birth_date: Some(player.date_of_birth.clone()),
        nation: Some(player.nationality.clone()),
        nation_code: Some(player.nationality.clone()),
        natural_position: Some(natural_position),
        position_group: Some(group_name(&player.natural_position).to_string()),
        alternate_positions,
        attributes_json,
        ovr: i64::from(player.ovr),
        potential: i64::from(player.potential),
        footedness: footedness_name(&player.footedness).to_string(),
        weak_foot: i64::from(player.weak_foot),
        club_id,
        club_name,
        club_search,
        club_nation,
        club_reputation,
        club_is_active_world: 0,
        ownership,
        contract_end,
        wage_eur_week: wage,
        market_value_eur: player.market_value as i64,
        value_source: "generated".to_string(),
        wage_source: "generated".to_string(),
        status: "eligible".to_string(),
        exclusion_reason: None,
        source_row_hash: format!("generated:{}", player.id),
    }
}

fn split_name(full_name: &str, match_name: &str) -> (String, String) {
    let mut parts = full_name.split_whitespace();
    let first = parts.next().unwrap_or(match_name).to_string();
    let rest: Vec<&str> = parts.collect();
    let last = if rest.is_empty() {
        match_name.to_string()
    } else {
        rest.join(" ")
    };
    (first, last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    fn attrs(ovr: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: ovr,
            stamina: ovr,
            strength: ovr,
            agility: ovr,
            passing: ovr,
            shooting: ovr,
            tackling: ovr,
            dribbling: ovr,
            defending: ovr,
            positioning: ovr,
            vision: ovr,
            decisions: ovr,
            composure: ovr,
            aggression: ovr,
            teamwork: ovr,
            leadership: ovr,
            handling: 20,
            reflexes: 20,
            aerial: ovr,
        }
    }

    fn row(id: i64, birth_year: i32, ovr: u8, potential: u8, club: Option<i64>) -> CachedRow {
        let club_name = club.map(|club| format!("Club {club}")).unwrap_or_default();
        CachedRow {
            person_id: id,
            ofm_id: format!("fm-p-{id}"),
            fm_uid: None,
            fm_uid_status: "unique".to_string(),
            first_name: "Test".to_string(),
            last_name: format!("Player{id}"),
            common_name: None,
            display_name: format!("Test Player{id}"),
            search_name: fold_text(&format!("Test Player{id}")),
            birth_date: Some(format!("{birth_year}-01-01")),
            nation: Some("England".to_string()),
            nation_code: Some("GB".to_string()),
            natural_position: Some("\"CentralMidfielder\"".to_string()),
            position_group: Some("midfielder".to_string()),
            alternate_positions: "[]".to_string(),
            attributes_json: serde_json::to_string(&attrs(ovr)).ok(),
            ovr: i64::from(ovr),
            potential: i64::from(potential),
            footedness: "Right".to_string(),
            weak_foot: 3,
            club_id: club,
            club_name: club_name.clone(),
            club_search: fold_text(&club_name),
            club_nation: club.map(|_| "England".to_string()),
            club_reputation: if club.is_some() { 600 } else { 0 },
            club_is_active_world: 0,
            ownership: if club.is_some() {
                "contracted"
            } else {
                "free_agent"
            }
            .to_string(),
            contract_end: club.map(|_| "2030-06-30".to_string()),
            wage_eur_week: if club.is_some() { 8_000 } else { 0 },
            market_value_eur: 1_000_000,
            value_source: "save".to_string(),
            wage_source: "save".to_string(),
            status: "eligible".to_string(),
            exclusion_reason: None,
            source_row_hash: format!("hash-{id}"),
        }
    }

    fn synthetic_registry(dir: &Path, rows: &[CachedRow]) -> PathBuf {
        let path = dir.join("registry.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        for (name, value) in [
            ("schema_version", REGISTRY_SCHEMA_VERSION.to_string()),
            ("source_sha256", "sha-test".to_string()),
            ("start_season", "2026".to_string()),
            ("built_on", "2026-06-01".to_string()),
        ] {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                params![name, value],
            )
            .unwrap();
        }
        let mut statement = conn.prepare(INSERT_SQL).unwrap();
        for row in rows {
            bind_row(&mut statement, row).unwrap();
        }
        drop(statement);
        drop(conn);
        path
    }

    fn today(year: i32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, 8, 1).unwrap()
    }

    fn source_rows() -> Vec<CachedRow> {
        (1..=40)
            .map(|n| {
                let age = 18 + (n % 20) as i32;
                row(
                    n,
                    2026 - age,
                    48 + (n % 30) as u8,
                    70 + (n % 25) as u8,
                    if n % 4 == 0 { None } else { Some((n % 5) + 1) },
                )
            })
            .collect()
    }

    /// Given the same source and career, when the cache is built twice, then the second call reuses
    /// the file and the rows are identical: a reload cannot re-roll the market.
    #[test]
    fn the_same_career_and_season_reuse_the_same_rows() {
        let source_dir = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());

        let first = ensure_career_cache(&registry, cache_dir.path(), 7, today(2030)).unwrap();
        let second = ensure_career_cache(&registry, cache_dir.path(), 7, today(2030)).unwrap();

        assert!(!second.rebuilt);
        assert_eq!(second.seasons_advanced, 0);
        assert_eq!(first.rows, second.rows);
        let one = read_rows(&open_read(&first.path).unwrap()).unwrap();
        let two = read_rows(&open_read(&second.path).unwrap()).unwrap();
        let snapshot = |rows: &[CachedRow]| {
            rows.iter()
                .map(|row| {
                    (
                        row.ofm_id.clone(),
                        row.ovr,
                        row.potential,
                        row.club_id,
                        row.contract_end.clone(),
                        row.market_value_eur,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(snapshot(&one), snapshot(&two));
    }

    /// Given a career advanced one season at a time and the same career built cold at the target
    /// season, when both markets are read, then they are byte-for-byte the same rows: the cache is
    /// a replay, not a different roll of the dice.
    #[test]
    fn an_incremental_cache_matches_a_cold_build_at_the_same_season() {
        let source_dir = TempDir::new().unwrap();
        let incremental_dir = TempDir::new().unwrap();
        let cold_dir = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());

        for year in 2027..=2036 {
            ensure_career_cache(&registry, incremental_dir.path(), 31, today(year)).unwrap();
        }
        let incremental =
            ensure_career_cache(&registry, incremental_dir.path(), 31, today(2036)).unwrap();
        let cold = ensure_career_cache(&registry, cold_dir.path(), 31, today(2036)).unwrap();

        assert_eq!(
            incremental.seasons_advanced, 0,
            "the loop left the cache current"
        );
        assert!(cold.rebuilt, "the cold career has no cache to reuse");
        let fingerprint = |report: &CareerCacheReport| {
            read_rows(&open_read(&report.path).unwrap())
                .unwrap()
                .into_iter()
                .map(|row| {
                    format!(
                        "{}|{}|{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
                        row.ofm_id,
                        row.ovr,
                        row.potential,
                        row.birth_date.unwrap_or_default(),
                        row.club_id.unwrap_or(0),
                        row.ownership,
                        row.contract_end.unwrap_or_default(),
                        row.attributes_json,
                        row.market_value_eur,
                        row.wage_eur_week,
                        row.status,
                        row.source_row_hash,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(fingerprint(&incremental), fingerprint(&cold));
    }

    /// Given two careers with the same source but different seeds, when both project to the same
    /// season, then their markets differ: the shared registry is not one career's save.
    #[test]
    fn two_seeds_build_different_markets() {
        let source_dir = TempDir::new().unwrap();
        let cache_a = TempDir::new().unwrap();
        let cache_b = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());

        let a = ensure_career_cache(&registry, cache_a.path(), 1, today(2036)).unwrap();
        let b = ensure_career_cache(&registry, cache_b.path(), 2, today(2036)).unwrap();
        let a_rows = read_rows(&open_read(&a.path).unwrap()).unwrap();
        let b_rows = read_rows(&open_read(&b.path).unwrap()).unwrap();
        let fingerprint = |rows: &[CachedRow]| {
            rows.iter()
                .map(|row| {
                    format!(
                        "{}:{}:{}:{:?}",
                        row.ofm_id,
                        row.ovr,
                        row.club_id.unwrap_or(0),
                        row.contract_end
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_ne!(fingerprint(&a_rows), fingerprint(&b_rows));
    }

    /// Given a career already projected to one season, when it is advanced further, then a player
    /// who retired is gone for good and the newgen intake keeps the market populated.
    #[test]
    fn advancing_never_resurrects_a_retired_player() {
        let source_dir = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());

        let first = ensure_career_cache(&registry, cache_dir.path(), 11, today(2030)).unwrap();
        let before: Vec<String> = read_rows(&open_read(&first.path).unwrap())
            .unwrap()
            .into_iter()
            .map(|row| row.ofm_id)
            .collect();
        let later = ensure_career_cache(&registry, cache_dir.path(), 11, today(2040)).unwrap();
        let after: Vec<String> = read_rows(&open_read(&later.path).unwrap())
            .unwrap()
            .into_iter()
            .map(|row| row.ofm_id)
            .collect();

        assert_eq!(later.seasons_advanced, 10);
        assert!(after.iter().any(|id| id.starts_with("fm-n-")));
        let retired: Vec<&String> = before.iter().filter(|id| !after.contains(id)).collect();
        assert!(!retired.is_empty(), "no source player retired");
        assert!(
            retired
                .iter()
                .all(|id| id.starts_with("fm-p-") || id.starts_with("fm-n-")),
            "an unknown identity appeared"
        );
    }

    /// Given a cache from a later season, when an older save asks for an earlier season, then the
    /// cache is rebuilt rather than showing players who had retired by then.
    #[test]
    fn a_clock_moved_backwards_rebuilds_from_the_source() {
        let source_dir = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());

        let later = ensure_career_cache(&registry, cache_dir.path(), 13, today(2040)).unwrap();
        let earlier = ensure_career_cache(&registry, cache_dir.path(), 13, today(2030)).unwrap();

        assert!(earlier.rebuilt);
        assert_eq!(earlier.season, 2030);
        assert_eq!(earlier.path, later.path, "same career, same file");
    }

    /// Given a stale cache and two commands arriving together, when both ask for the same season,
    /// then both succeed and see the same rows: they cannot corrupt each other's staging file.
    #[test]
    fn concurrent_callers_see_one_complete_cache() {
        let source_dir = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();
        let registry = synthetic_registry(source_dir.path(), &source_rows());
        let cache = cache_dir.path().to_path_buf();

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let registry = registry.clone();
                let cache = cache.clone();
                std::thread::spawn(move || {
                    ensure_career_cache(&registry, &cache, 21, today(2035)).unwrap()
                })
            })
            .collect();
        let reports: Vec<CareerCacheReport> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert_eq!(reports[0].season, reports[1].season);
        assert_eq!(reports[0].rows, reports[1].rows);
        assert!(reports[0].rows > 0);
        let rows = read_rows(&open_read(&reports[0].path).unwrap()).unwrap();
        assert_eq!(rows.len(), reports[0].rows);
    }

    /// Given a source club inside the active world, when the market's club pool is built, then that
    /// club is not a move target: an external player is never shown at a simulated club's id.
    #[test]
    fn active_world_clubs_are_not_market_targets() {
        let source_dir = TempDir::new().unwrap();
        let mut rows = source_rows();
        rows[0].club_id = Some(900);
        rows[0].club_name = "Active United".to_string();
        rows[0].club_search = fold_text("Active United");
        rows[0].club_is_active_world = 1;
        let registry = synthetic_registry(source_dir.path(), &rows);

        let clubs = source_clubs(&open_read(&registry).unwrap()).unwrap();

        assert!(!clubs.is_empty());
        assert!(clubs.iter().all(|club| club.id != ids::club(900)));
    }

    /// The D7B2 benchmark, against the player's own read-only `world.db`:
    /// `APFM_D7B2_WORLD_DB=~/apfootballmanager/data/world.db \
    ///  cargo test -p fm24-adapter --lib career::tests::benchmark_real_career_cache -- --ignored --nocapture`
    ///
    /// Builds the real registry once, then a cold career cache from scratch for seasons 1, 5, 10,
    /// 20 and 30, and times the first search on each. Never part of the suite: it needs the source
    /// file and takes minutes.
    #[test]
    #[ignore = "real-source career-cache benchmark; needs APFM_D7B2_WORLD_DB"]
    fn benchmark_real_career_cache() {
        let Ok(world_db) = std::env::var("APFM_D7B2_WORLD_DB") else {
            println!("APFM_D7B2_WORLD_DB is not set; skipping");
            return;
        };
        let dir = TempDir::new().unwrap();
        let registry = dir.path().join("registry.db");
        let seed = std::env::var("APFM_D7B3_SEED")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(777);
        let started = Instant::now();
        let report = crate::registry::build_registry(Path::new(&world_db), &registry).unwrap();
        println!(
            "seed={seed} registry: stored={} eligible={} active_world={} bytes={} build_ms={} total_ms={}",
            report.stored,
            report.eligible,
            report.active_world,
            report.bytes,
            report.elapsed_ms,
            started.elapsed().as_millis()
        );
        for year in [1_i32, 5, 10, 20, 30] {
            let cache_dir = dir.path().join(format!("cache-{year}"));
            let on = NaiveDate::from_ymd_opt(2026 + year, 8, 1).unwrap();
            let build_started = Instant::now();
            let cache = ensure_career_cache(&registry, &cache_dir, seed, on).unwrap();
            let build_ms = build_started.elapsed().as_millis();
            let bytes = std::fs::metadata(&cache.path).map(|m| m.len()).unwrap_or(0);
            let conn = open_read(&cache.path).unwrap();
            let excluded: Vec<String> = {
                let base = open_read(&registry).unwrap();
                let mut statement = base
                    .prepare("SELECT ofm_id FROM registry_player WHERE status = 'active_world'")
                    .unwrap();
                statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap()
            };
            let stats: (i64, f64, i64, i64, i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*),
                            COALESCE(AVG((julianday(?1) - julianday(birth_date)) / 365.25), 0),
                            SUM(CASE WHEN club_id IS NULL THEN 1 ELSE 0 END),
                            SUM(CASE WHEN ovr >= 85 THEN 1 ELSE 0 END),
                            SUM(CASE WHEN ovr >= 80 THEN 1 ELSE 0 END),
                            SUM(CASE WHEN ofm_id LIKE 'fm-n-%' THEN 1 ELSE 0 END)
                     FROM registry_player WHERE status = 'eligible'",
                    params![on.format("%Y-%m-%d").to_string()],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .unwrap();
            let eligible = stats.0.max(1);
            println!(
                "year={year} season={} rows={} eligible={} newgens={} avg_age={:.1} free={:.1}% \
                 ovr80+={:.2}% ovr85+={:.2}% bytes={} build_ms={build_ms}",
                cache.season,
                cache.rows,
                stats.0,
                stats.5,
                stats.1,
                stats.2 as f64 * 100.0 / eligible as f64,
                stats.4 as f64 * 100.0 / eligible as f64,
                stats.3 as f64 * 100.0 / eligible as f64,
                bytes,
            );
            let query = crate::registry::RegistryQuery {
                today: on,
                page_size: 25,
                ..Default::default()
            };
            let query_started = Instant::now();
            let page = crate::registry::query_registry(&conn, &query, &excluded).unwrap();
            let cold_query_ms = query_started.elapsed().as_millis();
            let query_started = Instant::now();
            let _ = crate::registry::query_registry(&conn, &query, &excluded).unwrap();
            let warm_query_ms = query_started.elapsed().as_millis();
            println!(
                "year={year} rebuild={} advanced={} excluded={} cold_query_ms={cold_query_ms} \
                 warm_query_ms={warm_query_ms} total_matches={}",
                cache.rebuilt,
                cache.seasons_advanced,
                excluded.len(),
                page.total
            );
        }
    }
}
