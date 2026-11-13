//! The external player registry: a local, indexed SQLite projection of every player in the source
//! save.
//!
//! The registry is not a second game. It holds one row per source person, built once from the
//! player's own `world.db`, read-only at query time, and it never participates in the daily
//! simulation. The active world keeps its 240 clubs and 13 157 players; the registry is the market
//! around it.
//!
//! Identity is the source internal `person.id`, projected as `fm-p-<person_id>`. FM Unique IDs are
//! kept as metadata only because the source has duplicates. A record is never silently dropped:
//! every source player is stored with a status, and the ones the game cannot use are explicitly
//! classified (`missing_identity`, `missing_nation`, `no_position`, `outside_age_range`,
//! `active_world`).
//!
//! Availability and retirement are projected at query time from the source birth date and contract
//! end — a deterministic, O(1)-per-row rule, not a daily simulation. A player is no longer available
//! at 38; a contract that has expired makes him a free agent. Source attributes and position
//! mappings are frozen at the snapshot, which is the bounded limitation documented in
//! `docs/APFM_D7B1_GLOBAL_REGISTRY.md`.

use crate::config::Config;
use crate::ids;
use crate::journal::Journal;
use crate::pipeline::convert_player;
use crate::refs::Refs;
use crate::source::Source;
use chrono::NaiveDate;
use domain::player::{Footedness, PlayerAttributes, Position};
use ofm_core::external_registry::{ExternalClub, ExternalPlayer, MAX_EXTERNAL_AGE, asking_price};
use rusqlite::{Connection, OpenFlags, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const REGISTRY_SCHEMA_VERSION: u32 = 1;
pub const REGISTRY_FILE_NAME: &str = "registry.db";
pub const DEFAULT_PAGE_SIZE: u32 = 25;
pub const MAX_PAGE_SIZE: u32 = 100;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE registry_player (
    person_id INTEGER PRIMARY KEY,
    ofm_id TEXT NOT NULL UNIQUE,
    fm_uid INTEGER,
    fm_uid_status TEXT NOT NULL,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    common_name TEXT,
    display_name TEXT NOT NULL,
    search_name TEXT NOT NULL,
    birth_date TEXT,
    nation TEXT,
    nation_code TEXT,
    natural_position TEXT,
    position_group TEXT,
    alternate_positions TEXT NOT NULL,
    attributes_json TEXT,
    ovr INTEGER NOT NULL,
    potential INTEGER NOT NULL,
    footedness TEXT NOT NULL,
    weak_foot INTEGER NOT NULL,
    club_id INTEGER,
    club_name TEXT NOT NULL,
    club_search TEXT NOT NULL,
    club_nation TEXT,
    club_reputation INTEGER NOT NULL,
    club_is_active_world INTEGER NOT NULL,
    ownership TEXT NOT NULL,
    contract_end TEXT,
    wage_eur_week INTEGER NOT NULL,
    market_value_eur INTEGER NOT NULL,
    value_source TEXT NOT NULL,
    wage_source TEXT NOT NULL,
    status TEXT NOT NULL,
    exclusion_reason TEXT,
    source_row_hash TEXT NOT NULL
);
CREATE TABLE registry_journal (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    person_id INTEGER NOT NULL,
    ofm_id TEXT NOT NULL,
    code TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE INDEX idx_registry_search ON registry_player(search_name);
CREATE INDEX idx_registry_club_search ON registry_player(club_search);
CREATE INDEX idx_registry_nation ON registry_player(nation_code);
CREATE INDEX idx_registry_position ON registry_player(position_group);
CREATE INDEX idx_registry_birth ON registry_player(birth_date);
CREATE INDEX idx_registry_status ON registry_player(status);
CREATE INDEX idx_registry_ovr ON registry_player(ovr);
CREATE INDEX idx_registry_value ON registry_player(market_value_eur);
CREATE INDEX idx_registry_contract_end ON registry_player(contract_end);
"#;

/// Why a source record is or is not usable by the active game. Stored, never inferred at display
/// time, so a coverage screen can count every source player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryStatus {
    Eligible,
    MissingIdentity,
    MissingNation,
    NoPosition,
    OutsideAgeRange,
    ActiveWorld,
}

impl RegistryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::MissingIdentity => "missing_identity",
            Self::MissingNation => "missing_nation",
            Self::NoPosition => "no_position",
            Self::OutsideAgeRange => "outside_age_range",
            Self::ActiveWorld => "active_world",
        }
    }

    pub fn is_eligible(self) -> bool {
        self == Self::Eligible
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryBuildReport {
    pub players_seen: usize,
    pub stored: usize,
    pub eligible: usize,
    pub free_agents: usize,
    pub active_world: usize,
    pub missing_identity: usize,
    pub missing_nation: usize,
    pub no_position: usize,
    pub outside_age_range: usize,
    pub journal_entries: usize,
    pub bytes: u64,
    pub source_sha256: String,
    pub elapsed_ms: u128,
}

/// Build the sidecar registry from `world_db` at `destination`.
///
/// The source is opened read-only by [`Source`]; a previous registry is replaced only once the new
/// one is complete, so a failed build leaves the old file usable.
pub fn build_registry(world_db: &Path, destination: &Path) -> crate::Result<RegistryBuildReport> {
    let started = Instant::now();
    if !world_db.is_file() {
        return Err("be.error.fm24.worldDbMissing".to_string());
    }
    let source = Source::open(world_db)?;
    let facts = source.schema_facts()?;
    let source_sha256 = Source::sha256(world_db)?;
    let perimeter: HashSet<i64> = source
        .perimeter(facts.start_season)?
        .into_iter()
        .map(|(_, club)| club)
        .collect();
    let cfg = Config::default_config();
    let refs = Refs::embedded()?;
    let built_on = NaiveDate::parse_from_str(&cfg.start.date, "%Y-%m-%d")
        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    let rows = source.every_player()?;
    let players_seen = rows.len();

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    }
    let staging = staging_path(destination);
    let _ = std::fs::remove_file(&staging);
    let mut conn =
        Connection::open(&staging).map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    conn.execute_batch(SCHEMA)
        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;

    let mut report = RegistryBuildReport {
        players_seen,
        stored: 0,
        eligible: 0,
        free_agents: 0,
        active_world: 0,
        missing_identity: 0,
        missing_nation: 0,
        no_position: 0,
        outside_age_range: 0,
        journal_entries: 0,
        bytes: 0,
        source_sha256: source_sha256.clone(),
        elapsed_ms: 0,
    };
    let mut journal = Journal::default();
    {
        let transaction = conn
            .transaction()
            .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO registry_player (
                        person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, common_name,
                        display_name, search_name, birth_date, nation, nation_code, natural_position,
                        position_group, alternate_positions, attributes_json, ovr, potential,
                        footedness, weak_foot, club_id, club_name, club_search, club_nation,
                        club_reputation, club_is_active_world, ownership, contract_end, wage_eur_week,
                        market_value_eur, value_source, wage_source, status, exclusion_reason,
                        source_row_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                             ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29,
                             ?30, ?31, ?32, ?33, ?34, ?35)",
                )
                .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
            let mut journal_insert = transaction
                .prepare(
                    "INSERT INTO registry_journal (person_id, ofm_id, code, detail)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;

            for row in &rows {
                let ofm_id = ids::player(row.person_id);
                let notes_before = journal.notes().len();
                let converted = convert_player(row, &cfg, &mut journal);
                let display_name = display_name(row);
                let nation_code = row
                    .nation
                    .as_deref()
                    .and_then(|nation| refs.nation_code(nation));
                let age = row
                    .birth_date
                    .as_deref()
                    .and_then(|birth| NaiveDate::parse_from_str(birth, "%Y-%m-%d").ok())
                    .map(|birth| domain::age::age_between(birth, built_on));
                let club_is_active_world = row.club_id > 0 && perimeter.contains(&row.club_id);

                let (status, exclusion_reason) = if display_name.is_empty() || age.is_none() {
                    (
                        RegistryStatus::MissingIdentity,
                        Some("no usable name or birth date in the source"),
                    )
                } else if nation_code.is_none() {
                    (
                        RegistryStatus::MissingNation,
                        Some("no nation the game knows in the source"),
                    )
                } else if converted.is_none() {
                    (
                        RegistryStatus::NoPosition,
                        Some("no usable FM position note in the source"),
                    )
                } else if club_is_active_world {
                    (
                        RegistryStatus::ActiveWorld,
                        Some("the club is in the active world's simulation perimeter"),
                    )
                } else if age.is_none_or(|age| !(14..=40).contains(&age)) {
                    (
                        RegistryStatus::OutsideAgeRange,
                        Some("outside the registry's 14-40 build-time age range"),
                    )
                } else {
                    (RegistryStatus::Eligible, None)
                };

                let (
                    position,
                    position_group,
                    alternates_json,
                    attributes_json,
                    ovr,
                    potential,
                    footedness,
                    weak_foot,
                ) = match &converted {
                    Some(converted) => {
                        let deduped_alternates: Vec<Position> = converted.alternates.clone();
                        (
                            Some(converted.position.clone()),
                            Some(group_name(&converted.position)),
                            serde_json::to_string(&deduped_alternates)
                                .unwrap_or_else(|_| "[]".to_string()),
                            serde_json::to_string(&converted.attributes).ok(),
                            converted.ovr,
                            converted.potential,
                            footedness_name(&converted.foot).to_string(),
                            converted.weak_foot.unwrap_or(0),
                        )
                    }
                    None => (
                        None,
                        None,
                        "[]".to_string(),
                        None,
                        0,
                        0,
                        "Right".to_string(),
                        0,
                    ),
                };

                let (market_value, value_source) = match row.value_gbp {
                    Some(value) if value > 0 => (
                        cfg.to_eur(value).max(0) as u64,
                        row.value_source
                            .clone()
                            .unwrap_or_else(|| "save".to_string()),
                    ),
                    _ => (
                        estimate_market_value(ovr, potential, age.unwrap_or(30)),
                        "estimated".to_string(),
                    ),
                };
                let wage_eur = cfg.to_eur(row.wage_gbp_week).max(0) as u64;
                let club_name = if row.club_id > 0 {
                    row.club_name.clone()
                } else {
                    String::new()
                };
                let ownership = if row.club_id > 0 {
                    "contracted"
                } else {
                    "free_agent"
                };
                let source_row_hash = row_hash(row, &ofm_id);

                insert
                    .execute(params![
                        row.person_id,
                        ofm_id,
                        row.fm_uid,
                        row.fm_uid_status,
                        row.first_name,
                        row.last_name,
                        row.common_name,
                        display_name,
                        fold_text(&display_name),
                        row.birth_date,
                        row.nation,
                        nation_code,
                        position
                            .map(|position| serde_json::to_string(&position).unwrap_or_default()),
                        position_group,
                        alternates_json,
                        attributes_json,
                        ovr,
                        potential,
                        footedness,
                        weak_foot,
                        if row.club_id > 0 {
                            Some(row.club_id)
                        } else {
                            None
                        },
                        club_name,
                        fold_text(&club_name),
                        row.club_nation,
                        row.club_reputation,
                        if club_is_active_world { 1 } else { 0 },
                        ownership,
                        row.end_date,
                        wage_eur as i64,
                        market_value as i64,
                        value_source,
                        row.wage_source,
                        status.as_str(),
                        exclusion_reason,
                        source_row_hash,
                    ])
                    .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;

                // Conversion notes (a missing position, a fallback) and the registry's own verdict
                // both land in the journal, so no decision is invisible.
                for note in &journal.notes()[notes_before..] {
                    journal_insert
                        .execute(params![row.person_id, ofm_id, note.code, note.message])
                        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
                    report.journal_entries += 1;
                }
                journal_insert
                    .execute(params![
                        row.person_id,
                        ofm_id,
                        status.as_str(),
                        exclusion_reason.unwrap_or("usable external player")
                    ])
                    .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
                report.journal_entries += 1;

                report.stored += 1;
                match status {
                    RegistryStatus::Eligible => report.eligible += 1,
                    RegistryStatus::MissingIdentity => report.missing_identity += 1,
                    RegistryStatus::MissingNation => report.missing_nation += 1,
                    RegistryStatus::NoPosition => report.no_position += 1,
                    RegistryStatus::OutsideAgeRange => report.outside_age_range += 1,
                    RegistryStatus::ActiveWorld => report.active_world += 1,
                }
                if status.is_eligible() && row.club_id == 0 {
                    report.free_agents += 1;
                }
            }
        }
        transaction
            .commit()
            .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    }

    let meta = [
        ("schema_version", REGISTRY_SCHEMA_VERSION.to_string()),
        ("source_sha256", source_sha256),
        ("source_game_version", facts.game_version.clone()),
        ("source_importer_version", facts.importer_version.clone()),
        ("start_season", facts.start_season.to_string()),
        ("built_on", cfg.start.date.clone()),
        ("players_seen", report.players_seen.to_string()),
        ("eligible", report.eligible.to_string()),
        ("free_agents", report.free_agents.to_string()),
        ("active_world", report.active_world.to_string()),
        ("missing_identity", report.missing_identity.to_string()),
        ("missing_nation", report.missing_nation.to_string()),
        ("no_position", report.no_position.to_string()),
        ("outside_age_range", report.outside_age_range.to_string()),
    ];
    {
        let transaction = conn
            .transaction()
            .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
        {
            let mut insert = transaction
                .prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")
                .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
            for (key, value) in meta {
                insert
                    .execute(params![key, value])
                    .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
            }
        }
        transaction
            .commit()
            .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    }
    conn.execute_batch(&format!("PRAGMA user_version = {REGISTRY_SCHEMA_VERSION};"))
        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    drop(conn);

    std::fs::rename(&staging, destination)
        .map_err(|_| "be.error.fm24.registryBuildFailed".to_string())?;
    report.bytes = std::fs::metadata(destination).map(|m| m.len()).unwrap_or(0);
    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn staging_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from(REGISTRY_FILE_NAME));
    name.push(".part");
    destination.with_file_name(name)
}

fn display_name(row: &crate::source::PlayerRow) -> String {
    let common = row.common_name.as_deref().unwrap_or("").trim();
    if !common.is_empty() {
        return common.to_string();
    }
    format!("{} {}", row.first_name.trim(), row.last_name.trim())
        .trim()
        .to_string()
}

fn row_hash(row: &crate::source::PlayerRow, ofm_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            ofm_id,
            display_name(row),
            row.birth_date.as_deref().unwrap_or(""),
            row.nation.as_deref().unwrap_or(""),
            row.club_id,
            row.club_name,
            row.start_date.as_deref().unwrap_or(""),
            row.end_date.as_deref().unwrap_or(""),
            row.wage_gbp_week
        )
        .as_bytes(),
    );
    hex::encode(hasher.finalize())
}

pub(crate) fn group_name(position: &Position) -> &'static str {
    match position.to_group_position() {
        Position::Goalkeeper => "goalkeeper",
        Position::Defender => "defender",
        Position::Midfielder => "midfielder",
        _ => "forward",
    }
}

pub(crate) fn footedness_name(foot: &Footedness) -> &'static str {
    match foot {
        Footedness::Left => "Left",
        Footedness::Right => "Right",
        Footedness::Both => "Both",
    }
}

/// A deterministic value for the source rows that carry none. It is a valuation, never an identity:
/// the player's name and dates are untouched. The curve is deliberately conservative (a 50 OVR
/// player is worth about €2m, an 80 OVR about €8m before the youth/ceiling factors).
fn estimate_market_value(ovr: u8, potential: u8, age: i32) -> u64 {
    if ovr == 0 {
        return 0;
    }
    let ability = f64::from(ovr);
    let ceiling = f64::from(potential.max(ovr));
    let youth = if age <= 21 {
        1.6
    } else if age <= 25 {
        1.3
    } else if age <= 29 {
        1.0
    } else if age <= 32 {
        0.6
    } else {
        0.25
    };
    let growth = 1.0 + (ceiling - ability).max(0.0) / 100.0;
    let value = (ability / 50.0).powi(3) * 2_000_000.0 * youth * growth;
    value.max(0.0) as u64
}

/// Accent-folded lowercase text for substring search. Stored, so a search never needs a custom
/// SQLite function and the same needle answers the same way on every platform.
pub fn fold_text(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => {
                folded.push('a')
            }
            'ç' | 'Ç' | 'ć' | 'č' => folded.push('c'),
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => folded.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => folded.push('i'),
            'ñ' | 'Ñ' => folded.push('n'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' | 'Ø' => {
                folded.push('o')
            }
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => folded.push('u'),
            'ß' => folded.push_str("ss"),
            other => folded.extend(other.to_lowercase()),
        }
    }
    folded
}

// ---------------------------------------------------------------------------------------------
// queries
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegistryAvailability {
    #[default]
    All,
    FreeAgent,
    Contracted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegistrySort {
    #[default]
    Name,
    Age,
    Ovr,
    Value,
    Club,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegistryPositionFilter {
    #[default]
    Any,
    Goalkeeper,
    Defender,
    Midfielder,
    Forward,
}

/// One request against the registry. `today` is supplied by the active game, never by the screen, so
/// age and contract projections follow the career's clock.
#[derive(Debug, Clone)]
pub struct RegistryQuery {
    pub text: String,
    pub nation: Option<String>,
    pub position: RegistryPositionFilter,
    pub min_age: Option<u32>,
    pub max_age: Option<u32>,
    pub club: String,
    pub availability: RegistryAvailability,
    pub sort: RegistrySort,
    pub descending: bool,
    pub page: u32,
    pub page_size: u32,
    pub include_incomplete: bool,
    pub today: NaiveDate,
}

impl Default for RegistryQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            nation: None,
            position: RegistryPositionFilter::Any,
            min_age: None,
            max_age: None,
            club: String::new(),
            availability: RegistryAvailability::All,
            sort: RegistrySort::Name,
            descending: false,
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
            include_incomplete: false,
            today: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap_or(NaiveDate::MAX),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySummary {
    pub id: String,
    pub name: String,
    pub age: Option<u32>,
    pub nationality: Option<String>,
    pub position: Option<String>,
    pub position_group: Option<String>,
    pub ovr: u8,
    pub potential: u8,
    pub club: Option<String>,
    pub ownership: String,
    pub contract_end: Option<String>,
    pub market_value: u64,
    pub weekly_wage: u64,
    pub asking_price: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegistryCounts {
    pub all: u32,
    pub free_agent: u32,
    pub contracted: u32,
    pub incomplete: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPage {
    pub items: Vec<RegistrySummary>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
    pub counts: RegistryCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPlayerDetail {
    pub id: String,
    pub name: String,
    pub first_name: String,
    pub last_name: String,
    pub common_name: Option<String>,
    pub age: Option<u32>,
    pub birth_date: Option<String>,
    pub nation: Option<String>,
    pub nationality: Option<String>,
    pub position: Option<String>,
    pub position_group: Option<String>,
    pub natural_position: Option<Position>,
    pub alternate_positions: Vec<Position>,
    pub footedness: Footedness,
    pub weak_foot: u8,
    pub attributes: Option<PlayerAttributes>,
    pub ovr: u8,
    pub potential: u8,
    pub club_id: Option<i64>,
    pub club_name: Option<String>,
    pub club_nation: Option<String>,
    pub club_reputation: i64,
    pub ownership: String,
    pub contract_end: Option<String>,
    pub market_value: u64,
    pub weekly_wage: u64,
    pub value_source: String,
    pub wage_source: String,
    pub asking_price: u64,
    pub status: String,
    pub exclusion_reason: Option<String>,
    pub fm_uid: Option<i64>,
    pub fm_uid_status: String,
    pub source_row_hash: String,
}

impl RegistryPlayerDetail {
    /// The promotion payload. `None` when the stored record cannot become a `Player` without
    /// inventing something: no attributes, no natural position or no mapped nation.
    pub fn into_external_player(&self) -> Option<ExternalPlayer> {
        let attributes = self.attributes.clone()?;
        let position = self.natural_position.clone()?;
        let nationality = self.nationality.clone()?;
        let birth_date = self.birth_date.clone()?;
        let club = self.club_id.map(|id| ExternalClub {
            id: ids::club(id),
            name: self.club_name.clone().unwrap_or_default(),
            contract_end: self.contract_end.clone(),
            weekly_wage: u32::try_from(self.weekly_wage).unwrap_or(u32::MAX),
            reputation: self.club_reputation,
        });
        let full_name = self.name.clone();
        let match_name = self
            .common_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| {
                if self.last_name.trim().is_empty() {
                    full_name.clone()
                } else {
                    self.last_name.clone()
                }
            });
        Some(ExternalPlayer {
            id: self.id.clone(),
            match_name,
            full_name,
            date_of_birth: birth_date,
            nationality,
            position: position.clone(),
            natural_position: position,
            alternate_positions: self.alternate_positions.clone(),
            footedness: self.footedness,
            weak_foot: self.weak_foot,
            attributes,
            ovr: self.ovr,
            potential: self.potential,
            market_value: self.market_value,
            club,
        })
    }
}

pub fn open_registry(path: &Path) -> crate::Result<Connection> {
    if !path.is_file() {
        return Err("be.error.fm24.registryMissing".to_string());
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "be.error.fm24.registryMissing".to_string())?;
    conn.execute_batch("PRAGMA query_only = ON;")
        .map_err(|_| "be.error.fm24.registryMissing".to_string())?;
    Ok(conn)
}

fn push_text_clause(sql: &mut String, params: &mut Vec<Value>, clause: &str, value: &str) {
    sql.push_str(clause);
    params.push(Value::Text(value.to_string()));
}

/// Apply the filters shared by the count and the page query.
fn base_where(query: &RegistryQuery, excluded: &[String]) -> (String, Vec<Value>) {
    let mut sql = String::from(" WHERE 1 = 1");
    let mut params: Vec<Value> = Vec::new();
    if !query.include_incomplete {
        sql.push_str(" AND status = 'eligible'");
    }
    let today = query.today.format("%Y-%m-%d").to_string();
    sql.push_str(" AND (birth_date IS NULL OR birth_date > date(?, ?))");
    params.push(Value::Text(today.clone()));
    params.push(Value::Text(format!("-{} years", MAX_EXTERNAL_AGE + 1)));

    let excluded_json = serde_json::to_string(excluded).unwrap_or_else(|_| "[]".to_string());
    sql.push_str(" AND ofm_id NOT IN (SELECT value FROM json_each(?))");
    params.push(Value::Text(excluded_json));

    let needle = fold_text(query.text.trim());
    if !needle.is_empty() {
        let pattern = format!("%{needle}%");
        sql.push_str(" AND (search_name LIKE ? OR club_search LIKE ?)");
        params.push(Value::Text(pattern.clone()));
        params.push(Value::Text(pattern));
    }
    if let Some(nation) = query.nation.as_deref().filter(|nation| !nation.is_empty()) {
        push_text_clause(&mut sql, &mut params, " AND nation_code = ?", nation);
    }
    if query.position != RegistryPositionFilter::Any {
        let group = match query.position {
            RegistryPositionFilter::Any => "",
            RegistryPositionFilter::Goalkeeper => "goalkeeper",
            RegistryPositionFilter::Defender => "defender",
            RegistryPositionFilter::Midfielder => "midfielder",
            RegistryPositionFilter::Forward => "forward",
        };
        push_text_clause(&mut sql, &mut params, " AND position_group = ?", group);
    }
    if let Some(min_age) = query.min_age {
        push_text_clause(
            &mut sql,
            &mut params,
            " AND birth_date <= date(?, ?)",
            &today,
        );
        params.push(Value::Text(format!("-{min_age} years")));
    }
    if let Some(max_age) = query.max_age {
        push_text_clause(
            &mut sql,
            &mut params,
            " AND birth_date > date(?, ?)",
            &today,
        );
        params.push(Value::Text(format!("-{} years", max_age.saturating_add(1))));
    }
    let club = fold_text(query.club.trim());
    if !club.is_empty() {
        push_text_clause(
            &mut sql,
            &mut params,
            " AND club_search LIKE ?",
            &format!("%{club}%"),
        );
    }
    (sql, params)
}

fn availability_clause(
    availability: RegistryAvailability,
    today: &str,
) -> Option<(String, Vec<Value>)> {
    match availability {
        RegistryAvailability::All => None,
        RegistryAvailability::FreeAgent => Some((
            " AND (COALESCE(club_id, 0) = 0 OR (contract_end IS NOT NULL AND contract_end <= ?))"
                .to_string(),
            vec![Value::Text(today.to_string())],
        )),
        RegistryAvailability::Contracted => Some((
            " AND (COALESCE(club_id, 0) <> 0 AND (contract_end IS NULL OR contract_end > ?))"
                .to_string(),
            vec![Value::Text(today.to_string())],
        )),
    }
}

/// The page order. `descending` always means "the larger value first" from the screen's point of
/// view: Z before A for a name, 90 before 50 for a rating or a fee, the oldest first for an age.
/// Age is the one column stored the other way round (an earlier birth date is older), so it alone
/// reverses the direction.
fn order_clause(query: &RegistryQuery) -> String {
    let direction = if query.descending { "DESC" } else { "ASC" };
    let age_direction = if query.descending { "ASC" } else { "DESC" };
    match query.sort {
        RegistrySort::Name => format!(" ORDER BY search_name {direction}, ofm_id ASC"),
        RegistrySort::Age => format!(" ORDER BY birth_date {age_direction}, ofm_id ASC"),
        RegistrySort::Ovr => format!(" ORDER BY ovr {direction}, search_name ASC"),
        RegistrySort::Value => {
            format!(" ORDER BY market_value_eur {direction}, search_name ASC")
        }
        RegistrySort::Club => format!(" ORDER BY club_name {direction}, search_name ASC"),
    }
}

fn count_rows(
    conn: &Connection,
    sql: &str,
    params: &[Value],
    today: &str,
) -> crate::Result<(u32, RegistryCounts)> {
    let counts_sql = format!(
        "SELECT COUNT(*),
                SUM(CASE WHEN COALESCE(club_id, 0) = 0 OR (contract_end IS NOT NULL AND contract_end <= ?) THEN 1 ELSE 0 END),
                SUM(CASE WHEN COALESCE(club_id, 0) <> 0 AND (contract_end IS NULL OR contract_end > ?) THEN 1 ELSE 0 END),
                SUM(CASE WHEN status <> 'eligible' THEN 1 ELSE 0 END)
         FROM registry_player {sql}"
    );
    let mut count_params = Vec::with_capacity(params.len() + 2);
    count_params.push(Value::Text(today.to_string()));
    count_params.push(Value::Text(today.to_string()));
    count_params.extend(params.iter().cloned());
    let mut stmt = conn
        .prepare(&counts_sql)
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    stmt.query_row(params_from_iter(count_params.iter()), |row| {
        Ok((
            row.get::<_, i64>(0)?.max(0) as u32,
            RegistryCounts {
                all: row.get::<_, i64>(0)?.max(0) as u32,
                free_agent: row.get::<_, Option<i64>>(1)?.unwrap_or(0).max(0) as u32,
                contracted: row.get::<_, Option<i64>>(2)?.unwrap_or(0).max(0) as u32,
                incomplete: row.get::<_, Option<i64>>(3)?.unwrap_or(0).max(0) as u32,
            },
        ))
    })
    .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())
}

pub fn query_registry(
    conn: &Connection,
    query: &RegistryQuery,
    excluded: &[String],
) -> crate::Result<RegistryPage> {
    let page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
    let page = query.page.max(1);
    let (base, base_params) = base_where(query, excluded);
    let today = query.today.format("%Y-%m-%d").to_string();
    let (total, counts) = count_rows(conn, &base, &base_params, &today)
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let (availability, availability_params) = match availability_clause(query.availability, &today)
    {
        Some((sql, params)) => (sql, params),
        None => (String::new(), Vec::new()),
    };
    let filtered_total = if availability.is_empty() {
        total
    } else {
        let sql = format!("SELECT COUNT(*) FROM registry_player {base}{availability}");
        let mut params = base_params.clone();
        params.extend(availability_params.iter().cloned());
        conn.query_row(&sql, params_from_iter(params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map(|value| value.max(0) as u32)
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?
    };

    let offset = u64::from((page - 1).saturating_mul(page_size));
    let sql = format!(
        "SELECT ofm_id, display_name, birth_date, nation_code, natural_position, position_group,
                ovr, potential, club_name, COALESCE(club_id, 0), contract_end, market_value_eur,
                wage_eur_week, status
         FROM registry_player {base}{availability}{}
         LIMIT ? OFFSET ?",
        order_clause(query)
    );
    let mut params = base_params;
    params.extend(availability_params);
    params.push(Value::Integer(i64::from(page_size)));
    params.push(Value::Integer(i64::try_from(offset).unwrap_or(i64::MAX)));
    let mut statement = conn
        .prepare(&sql)
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let rows = statement
        .query_map(params_from_iter(params.iter()), |row| {
            let birth_date: Option<String> = row.get(2)?;
            let club_id: i64 = row.get(9)?;
            let contract_end: Option<String> = row.get(10)?;
            let market_value: i64 = row.get(11)?;
            let club_name: String = row.get(8)?;
            let status: String = row.get(13)?;
            let market_value = market_value.max(0) as u64;
            Ok(RegistrySummary {
                id: row.get(0)?,
                name: row.get(1)?,
                age: birth_date
                    .as_deref()
                    .and_then(|birth| NaiveDate::parse_from_str(birth, "%Y-%m-%d").ok())
                    .map(|birth| domain::age::age_between(birth, query.today).max(0) as u32),
                nationality: row.get(3)?,
                position: row
                    .get::<_, Option<String>>(4)?
                    .and_then(|value| serde_json::from_str::<Position>(&value).ok())
                    .map(|position| format!("{position:?}")),
                position_group: row.get(5)?,
                ovr: row.get::<_, i64>(6)?.clamp(0, 255) as u8,
                potential: row.get::<_, i64>(7)?.clamp(0, 255) as u8,
                club: if club_name.is_empty() {
                    None
                } else {
                    Some(club_name)
                },
                ownership: if club_id == 0
                    || contract_end
                        .as_deref()
                        .is_some_and(|end| end <= today.as_str())
                {
                    "free_agent".to_string()
                } else {
                    "contracted".to_string()
                },
                contract_end: contract_end.clone(),
                market_value,
                weekly_wage: row.get::<_, i64>(12)?.max(0) as u64,
                asking_price: asking_price(market_value, contract_end.as_deref(), query.today),
                status,
            })
        })
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let items = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    Ok(RegistryPage {
        items,
        total: filtered_total,
        page,
        page_size,
        counts,
    })
}

pub fn get_registry_player(
    conn: &Connection,
    ofm_id: &str,
    today: NaiveDate,
) -> crate::Result<Option<RegistryPlayerDetail>> {
    let mut statement = conn
        .prepare(
            "SELECT person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, common_name,
                    display_name, birth_date, nation, nation_code, natural_position,
                    alternate_positions, attributes_json, ovr, potential, footedness, weak_foot,
                    club_id, club_name, club_nation, club_reputation, ownership, contract_end,
                    wage_eur_week, market_value_eur, value_source, wage_source, status,
                    exclusion_reason, source_row_hash
             FROM registry_player WHERE ofm_id = ?1",
        )
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let mut rows = statement
        .query_map(params![ofm_id], |row| {
            let birth_date: Option<String> = row.get(8)?;
            let natural_position: Option<String> = row.get(11)?;
            let club_id: Option<i64> = row.get(18)?;
            let club_name: String = row.get(19)?;
            let contract_end: Option<String> = row.get(23)?;
            let market_value: i64 = row.get(25)?;
            let market_value = market_value.max(0) as u64;
            Ok(RegistryPlayerDetail {
                id: row.get(1)?,
                name: row.get(7)?,
                first_name: row.get(4)?,
                last_name: row.get(5)?,
                common_name: row.get(6)?,
                age: birth_date
                    .as_deref()
                    .and_then(|birth| NaiveDate::parse_from_str(birth, "%Y-%m-%d").ok())
                    .map(|birth| domain::age::age_between(birth, today).max(0) as u32),
                birth_date,
                nation: row.get(9)?,
                nationality: row.get(10)?,
                position: natural_position
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Position>(value).ok())
                    .map(|position| format!("{position:?}")),
                position_group: natural_position
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Position>(value).ok())
                    .map(|position| group_name(&position).to_string()),
                natural_position: natural_position
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Position>(value).ok()),
                alternate_positions: serde_json::from_str::<Vec<Position>>(
                    &row.get::<_, String>(12)?,
                )
                .unwrap_or_default(),
                attributes: row
                    .get::<_, Option<String>>(13)?
                    .and_then(|value| serde_json::from_str(&value).ok()),
                ovr: row.get::<_, i64>(14)?.clamp(0, 255) as u8,
                potential: row.get::<_, i64>(15)?.clamp(0, 255) as u8,
                footedness: serde_json::from_value(serde_json::Value::String(
                    row.get::<_, String>(16)?,
                ))
                .unwrap_or_default(),
                weak_foot: row.get::<_, i64>(17)?.clamp(0, 255) as u8,
                club_id,
                club_name: if club_name.is_empty() {
                    None
                } else {
                    Some(club_name)
                },
                club_nation: row.get(20)?,
                club_reputation: row.get(21)?,
                ownership: row.get(22)?,
                contract_end: contract_end.clone(),
                weekly_wage: row.get::<_, i64>(24)?.max(0) as u64,
                market_value,
                value_source: row.get(26)?,
                wage_source: row.get(27)?,
                asking_price: asking_price(market_value, contract_end.as_deref(), today),
                status: row.get(28)?,
                exclusion_reason: row.get(29)?,
                fm_uid: row.get(2)?,
                fm_uid_status: row.get(3)?,
                source_row_hash: row.get(30)?,
            })
        })
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let detail = rows
        .next()
        .transpose()
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    Ok(detail)
}

/// Read the registry's meta table. Used by the status surface and by the performance report.
pub fn registry_meta(conn: &Connection) -> crate::Result<Vec<(String, String)>> {
    let mut statement = conn
        .prepare("SELECT key, value FROM meta ORDER BY key")
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| "be.error.fm24.registryQueryFailed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::ATTRIBUTE_COLUMNS;
    use rusqlite::Connection;
    use std::path::Path;

    fn attribute_columns_sql() -> String {
        ATTRIBUTE_COLUMNS
            .iter()
            .map(|column| format!("{column} INTEGER"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn attribute_values_sql() -> String {
        ATTRIBUTE_COLUMNS
            .iter()
            .map(|_| "70")
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// A tiny but structurally faithful source: every table the registry reads, with a perimeter of
    /// one club and records that exercise each classification.
    fn synthetic_source(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE import_run(id INTEGER PRIMARY KEY, start_season INTEGER, game_version TEXT,
                source_sha256 TEXT, importer_version TEXT, imported_at TEXT);
             INSERT INTO import_run VALUES (1, 2026, 'FM24.3', 'source-hash', '1', '2026-01-01');
             CREATE TABLE nation(id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO nation VALUES (1, 'England'), (2, 'France'), (3, 'Brazil'), (4, 'Nowhere');
             CREATE TABLE club(id INTEGER PRIMARY KEY, fm_uid TEXT, name TEXT, short_name TEXT,
                nation_id INTEGER, reputation INTEGER);
             INSERT INTO club VALUES (1, '1', 'Alpha FC', 'ALP', 1, 60),
                                     (900, '900', 'External United', 'EXT', 2, 45),
                                     (901, '901', 'Unattached Town', 'UTN', 3, 20);
             CREATE TABLE team(id INTEGER PRIMARY KEY, club_id INTEGER, slot INTEGER, is_youth INTEGER);
             INSERT INTO team VALUES (10, 1, 0, 0), (9000, 900, 0, 0);
             CREATE TABLE person(id INTEGER PRIMARY KEY, fm_uid INTEGER, fm_uid_status TEXT,
                first_name TEXT, last_name TEXT, common_name TEXT, birth_date TEXT, nation_id INTEGER,
                height_cm INTEGER);
             INSERT INTO person VALUES
                (1, '10', 'unique', 'Active', 'Player', NULL, '2000-01-01', 1, 180),
                (2, '11', 'unique', 'Free', 'Agent', 'Freebie', '2001-02-03', 2, 175),
                (3, '12', 'unique', 'External', 'Star', NULL, '1999-03-04', 3, 182),
                (4, '13', 'duplicate', '', '', NULL, NULL, NULL, NULL),
                (5, '14', 'unique', 'No', 'Nation', NULL, '2002-04-05', 4, 178),
                (6, '15', 'unique', 'No', 'Position', NULL, '2003-05-06', 1, 176),
                (7, '16', 'unique', 'Old', 'Timer', NULL, '1970-06-07', 1, 170);
             CREATE TABLE player(person_id INTEGER PRIMARY KEY, ca INTEGER, pa INTEGER,
                rep_current INTEGER, rep_home INTEGER, rep_world INTEGER, foot_left INTEGER,
                foot_right INTEGER, value_gbp INTEGER, value_source TEXT, registered_team_id INTEGER);
             INSERT INTO player VALUES
                (1, 120, 150, 50, 40, 30, 20, 80, 1000000, 'save', 10),
                (2, 110, 140, 40, 30, 20, 10, 90, 2000000, 'save', NULL),
                (3, 130, 160, 60, 50, 40, 15, 85, 5000000, 'save', 9000),
                (4, 80, 90, 10, 10, 5, 20, 80, 1000, 'save', NULL),
                (5, 100, 120, 30, 20, 10, 20, 80, 500000, 'save', NULL),
                (6, 100, 120, 30, 20, 10, 20, 80, 500000, 'save', NULL),
                (7, 100, 120, 30, 20, 10, 20, 80, 500000, 'save', NULL);
             CREATE TABLE player_attributes(person_id INTEGER PRIMARY KEY, {});
             INSERT INTO player_attributes VALUES
                (1, {}), (2, {}), (3, {}), (4, {}), (5, {}), (6, {}), (7, {});
             CREATE TABLE player_position(person_id INTEGER, position TEXT, rating INTEGER);
             INSERT INTO player_position VALUES
                (1, 'STC', 20), (2, 'AMC', 20), (3, 'DC', 20), (4, 'MC', 20),
                (5, 'MC', 20), (7, 'MC', 20);
             CREATE TABLE contract(id INTEGER PRIMARY KEY, person_id INTEGER, club_id INTEGER,
                team_id INTEGER, is_current INTEGER, wage_gbp_week INTEGER, wage_original INTEGER,
                wage_source TEXT, start_date TEXT, end_date TEXT, contract_type_code INTEGER,
                squad_status_code INTEGER, role_code TEXT);
             INSERT INTO contract VALUES
                (1, 1, 1, 10, 1, 5000, 5000, 'save', '2024-07-01', '2028-06-30', 1, 1, NULL),
                (2, 3, 900, 9000, 1, 12000, 12000, 'save', '2024-07-01', '2028-06-30', 1, 1, NULL),
                (3, 7, 901, NULL, 1, 1000, 1000, 'save', '2024-07-01', '2028-06-30', 1, 1, NULL);
             CREATE TABLE competition(id INTEGER PRIMARY KEY, kind TEXT, playable INTEGER, simulated INTEGER);
             INSERT INTO competition VALUES (1, 'league', 1, 0);
             CREATE TABLE competition_season(id INTEGER PRIMARY KEY, competition_id INTEGER, season INTEGER);
             INSERT INTO competition_season VALUES (1, 1, 2026);
             CREATE TABLE competition_season_team(competition_season_id INTEGER, team_id INTEGER);
             INSERT INTO competition_season_team VALUES (1, 10);",
            attribute_columns_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
            attribute_values_sql(),
        ))
        .unwrap();
    }

    fn build_fixture(dir: &Path) -> (PathBuf, RegistryBuildReport) {
        let source = dir.join("world.db");
        synthetic_source(&source);
        let destination = dir.join("registry.db");
        let report = build_registry(&source, &destination).expect("registry builds");
        (destination, report)
    }

    #[test]
    fn every_source_player_is_stored_and_classified() {
        let dir = tempfile::tempdir().unwrap();
        let (path, report) = build_fixture(dir.path());
        assert_eq!(report.players_seen, 7);
        assert_eq!(report.stored, 7);
        assert_eq!(report.missing_identity, 1);
        assert_eq!(report.missing_nation, 1);
        assert_eq!(report.no_position, 1);
        assert_eq!(report.outside_age_range, 1);
        assert_eq!(report.active_world, 1);
        assert_eq!(report.eligible, 2, "the free agent and the external star");
        assert_eq!(report.free_agents, 1);

        let conn = open_registry(&path).unwrap();
        let mut statement = conn
            .prepare("SELECT COUNT(*), COUNT(DISTINCT ofm_id) FROM registry_player")
            .unwrap();
        let (rows, distinct): (i64, i64) = statement
            .query_row([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        assert_eq!((rows, distinct), (7, 7));
        let journal: i64 = conn
            .query_row("SELECT COUNT(*) FROM registry_journal", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(journal as usize, report.journal_entries);
    }

    #[test]
    fn ofm_ids_come_from_the_source_person_id_not_the_fm_uid() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = build_fixture(dir.path());
        let conn = open_registry(&path).unwrap();
        let id: String = conn
            .query_row(
                "SELECT ofm_id FROM registry_player WHERE person_id = 3",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(id, "fm-p-3");
        let fm_uid: Option<i64> = conn
            .query_row(
                "SELECT fm_uid FROM registry_player WHERE ofm_id = 'fm-p-3'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fm_uid, Some(12));
    }

    /// Given the fixture, when the registry is searched, then name, nation, position, age and club
    /// each narrow the eligible set without loading incomplete rows.
    #[test]
    fn search_filters_and_paginates_eligible_players() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = build_fixture(dir.path());
        let conn = open_registry(&path).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();

        let mut query = RegistryQuery {
            today,
            ..RegistryQuery::default()
        };
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);

        query.text = "freebie".to_string();
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-2");

        query.text.clear();
        query.nation = Some("BR".to_string());
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-3");
        assert_eq!(page.items[0].club.as_deref(), Some("External United"));
        assert!(page.items[0].asking_price > 0);

        query.nation = None;
        query.position = RegistryPositionFilter::Midfielder;
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-2");

        query.position = RegistryPositionFilter::Any;
        query.min_age = Some(26);
        query.max_age = Some(27);
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-3");

        query.min_age = None;
        query.max_age = None;
        query.club = "external".to_string();
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);

        query.club.clear();
        query.availability = RegistryAvailability::FreeAgent;
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-2");

        query.availability = RegistryAvailability::Contracted;
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-3");

        query.availability = RegistryAvailability::All;
        query.page_size = 1;
        let first = query_registry(&conn, &query, &[]).unwrap();
        query.page = 2;
        let second = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(first.items.len(), 1);
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].id, second.items[0].id);
    }

    /// Given an id that the active game already owns, when the registry is searched, then the row
    /// is excluded; this is the display half of the duplicate guard.
    #[test]
    fn excluded_ids_never_appear_and_counts_follow_them() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = build_fixture(dir.path());
        let conn = open_registry(&path).unwrap();
        let query = RegistryQuery::default();
        let page = query_registry(&conn, &query, &["fm-p-3".to_string()]).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "fm-p-2");
        assert_eq!(page.counts.all, 1);
    }

    #[test]
    fn incomplete_records_are_searchable_only_when_explicitly_requested() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = build_fixture(dir.path());
        let conn = open_registry(&path).unwrap();
        let mut query = RegistryQuery {
            include_incomplete: true,
            ..RegistryQuery::default()
        };
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 6, "all rows except the projected retiree");
        assert_eq!(page.counts.incomplete, 4);

        query.text = "zzz".to_string();
        let page = query_registry(&conn, &query, &[]).unwrap();
        assert_eq!(page.total, 0);
    }

    #[test]
    fn a_detail_round_trips_into_the_promotion_payload() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = build_fixture(dir.path());
        let conn = open_registry(&path).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let detail = get_registry_player(&conn, "fm-p-3", today)
            .unwrap()
            .expect("player");
        assert_eq!(detail.id, "fm-p-3");
        assert_eq!(detail.nationality.as_deref(), Some("BR"));
        assert_eq!(detail.natural_position, Some(Position::CenterBack));
        assert_eq!(detail.contract_end.as_deref(), Some("2028-06-30"));
        let external = detail.into_external_player().expect("promotable");
        assert_eq!(external.id, "fm-p-3");
        assert_eq!(external.club.as_ref().unwrap().id, "fm-c-900");
        assert_eq!(external.club.as_ref().unwrap().name, "External United");
        assert_eq!(external.ovr, detail.ovr);
    }

    #[test]
    fn reading_a_missing_registry_fails_with_a_key_and_does_not_create_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.db");
        assert_eq!(
            open_registry(&path).err().as_deref(),
            Some("be.error.fm24.registryMissing")
        );
        assert!(!path.exists());
    }

    #[test]
    fn a_rebuild_is_content_identical() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("world.db");
        synthetic_source(&source);
        let first = dir.path().join("first.db");
        let second = dir.path().join("second.db");
        build_registry(&source, &first).unwrap();
        build_registry(&source, &second).unwrap();
        let a = open_registry(&first).unwrap();
        let b = open_registry(&second).unwrap();
        let rows_a: Vec<(String, String, String, Option<String>, String)> = a
            .prepare("SELECT ofm_id, display_name, status, contract_end, source_row_hash FROM registry_player ORDER BY person_id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        let rows_b: Vec<(String, String, String, Option<String>, String)> = b
            .prepare("SELECT ofm_id, display_name, status, contract_end, source_row_hash FROM registry_player ORDER BY person_id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(rows_a, rows_b);
    }

    /// Not run by default: it writes synthetic registries at the shipped-world, half-source and
    /// full-source scales and times insert, page and count separately. The exclusion set is 12% of
    /// the rows, the share of the real source the active world already owns. Run with
    /// `cargo test -p fm24-adapter --lib large_registry -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark: 13 157, 40 000 and 81 762-row registries"]
    fn large_registry_query_benchmark() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let dir = tempfile::tempdir().unwrap();
        for rows in [13_157_i64, 40_000, 81_762] {
            let path = dir.path().join(format!("large-{rows}.db"));
            let mut conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            let start = Instant::now();
            {
                let transaction = conn.transaction().unwrap();
                {
                    let mut insert = transaction
                        .prepare(
                            "INSERT INTO registry_player (
                                person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, common_name,
                                display_name, search_name, birth_date, nation, nation_code, natural_position,
                                position_group, alternate_positions, attributes_json, ovr, potential,
                                footedness, weak_foot, club_id, club_name, club_search, club_nation,
                                club_reputation, club_is_active_world, ownership, contract_end, wage_eur_week,
                                market_value_eur, value_source, wage_source, status, exclusion_reason,
                                source_row_hash)
                             VALUES (?1, ?2, NULL, 'unique', ?3, ?4, NULL, ?5, ?6, ?7, 'France', 'FR',
                                     '\"Striker\"', 'forward', '[]', NULL, ?8, ?9, 'Right', 3, ?10, ?11, ?12,
                                     'France', 40, 0, ?13, ?14, 5000, 1000000, 'estimated', 'save', 'eligible',
                                     NULL, 'hash')",
                        )
                        .unwrap();
                    for person in 1..=rows {
                        let first = format!("First{person}");
                        let last = format!("Last{person}");
                        let name = format!("{first} {last}");
                        let club = if person % 3 == 0 {
                            Some(format!("Club {}", person % 500))
                        } else {
                            None
                        };
                        insert
                            .execute(params![
                                person,
                                format!("fm-p-{person}"),
                                first,
                                last,
                                name,
                                fold_text(&name),
                                format!("{}-01-01", 1988 + person % 38),
                                person % 99,
                                person % 99,
                                club.as_ref().map(|_| (person % 500) + 1),
                                club.clone().unwrap_or_default(),
                                fold_text(club.as_deref().unwrap_or("")),
                                if club.is_some() {
                                    "contracted"
                                } else {
                                    "free_agent"
                                },
                                if club.is_some() {
                                    Some("2028-06-30".to_string())
                                } else {
                                    None
                                },
                            ])
                            .unwrap();
                    }
                }
                transaction.commit().unwrap();
            }
            let build_ms = start.elapsed().as_millis();
            let size = std::fs::metadata(&path).unwrap().len();

            let conn = open_registry(&path).unwrap();
            let excluded: Vec<String> = (1..=rows * 12 / 100)
                .map(|person| format!("fm-p-{person}"))
                .collect();
            let target = rows - 10;
            let mut query = RegistryQuery {
                text: format!("First{target} Last{target}"),
                today,
                ..RegistryQuery::default()
            };
            let started = Instant::now();
            let page = query_registry(&conn, &query, &excluded).unwrap();
            let name_ms = started.elapsed().as_secs_f64() * 1000.0;

            query.text.clear();
            query.sort = RegistrySort::Value;
            let started = Instant::now();
            let sorted = query_registry(&conn, &query, &excluded).unwrap();
            let sort_ms = started.elapsed().as_secs_f64() * 1000.0;

            query.availability = RegistryAvailability::FreeAgent;
            let started = Instant::now();
            let free = query_registry(&conn, &query, &excluded).unwrap();
            let free_ms = started.elapsed().as_secs_f64() * 1000.0;

            println!(
                "registry rows={rows} build_ms={build_ms} bytes={size} excluded={} name_total={} \
                 name_ms={name_ms:.3} sorted_total={} sort_ms={sort_ms:.3} free_total={} \
                 free_ms={free_ms:.3}",
                excluded.len(),
                page.total,
                sorted.total,
                free.total
            );
            assert_eq!(page.total, 1);
            assert_eq!(sorted.items.len(), 25);
            assert!(free.total > 0);
        }
    }

    /// A small registry with one player per direction of every sortable column, plus the filters
    /// the search screen sends. Rows are inserted directly so the test is about the query, not
    /// about the source importer.
    fn sortable_registry(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let mut insert = conn
            .prepare(
                "INSERT INTO registry_player (
                    person_id, ofm_id, fm_uid, fm_uid_status, first_name, last_name, common_name,
                    display_name, search_name, birth_date, nation, nation_code, natural_position,
                    position_group, alternate_positions, attributes_json, ovr, potential, footedness,
                    weak_foot, club_id, club_name, club_search, club_nation, club_reputation,
                    club_is_active_world, ownership, contract_end, wage_eur_week, market_value_eur,
                    value_source, wage_source, status, exclusion_reason, source_row_hash)
                 VALUES (?1, ?2, NULL, 'unique', ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         '[]', NULL, ?12, ?13, 'Right', 3, ?14, ?15, ?16, ?17, 600, 0, ?18, ?19,
                         5000, ?20, 'save', 'save', 'eligible', NULL, ?21)",
            )
            .unwrap();
        for (
            id,
            first,
            last,
            birth,
            nation,
            code,
            position,
            group,
            ovr,
            value,
            club_id,
            club,
            ownership,
            contract_end,
        ) in [
            (
                1,
                "Alpha",
                "Keeper",
                "2000-01-01",
                "France",
                "FR",
                "\"Goalkeeper\"",
                "goalkeeper",
                60,
                1_000_000,
                Some(900),
                "Zulu FC",
                "contracted",
                Some("2028-06-30"),
            ),
            (
                2,
                "Bravo",
                "Defender",
                "1996-01-01",
                "England",
                "GB",
                "\"CenterBack\"",
                "defender",
                90,
                9_000_000,
                Some(901),
                "Alpha FC",
                "contracted",
                Some("2028-06-30"),
            ),
            (
                3,
                "Charlie",
                "Mid",
                "2001-01-01",
                "England",
                "GB",
                "\"CentralMidfielder\"",
                "midfielder",
                70,
                5_000_000,
                None,
                "",
                "free_agent",
                None,
            ),
            (
                4,
                "Delta",
                "Striker",
                "2010-01-01",
                "Brazil",
                "BR",
                "\"Striker\"",
                "forward",
                50,
                500_000,
                Some(902),
                "Mike FC",
                "contracted",
                Some("2027-06-30"),
            ),
        ] {
            let display = format!("{first} {last}");
            insert
                .execute(params![
                    id,
                    format!("fm-p-{id}"),
                    first,
                    last,
                    display,
                    fold_text(&display),
                    birth,
                    nation,
                    code,
                    position,
                    group,
                    ovr,
                    ovr + 10,
                    club_id,
                    club,
                    fold_text(club),
                    if club.is_empty() { None } else { Some(code) },
                    ownership,
                    contract_end,
                    value,
                    format!("hash-{id}"),
                ])
                .unwrap();
        }
        drop(insert);
        conn
    }

    fn ids_for(conn: &Connection, query: &RegistryQuery, excluded: &[String]) -> Vec<String> {
        query_registry(conn, query, excluded)
            .unwrap()
            .items
            .into_iter()
            .map(|item| item.id)
            .collect()
    }

    fn sort_query(sort: RegistrySort, descending: bool) -> RegistryQuery {
        RegistryQuery {
            sort,
            descending,
            page_size: 10,
            today: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            ..RegistryQuery::default()
        }
    }

    /// Given players whose name, age, rating, value and club all differ
    /// When each sortable column is requested in both directions
    /// Then the page is ordered the way the screen's arrow says, and an age descending starts with
    ///      the oldest while rating and value descending start with the largest
    #[test]
    fn every_sort_direction_orders_the_page_the_screen_asks_for() {
        let dir = tempfile::tempdir().unwrap();
        let conn = sortable_registry(&dir.path().join("registry.db"));
        let excluded: Vec<String> = vec![];

        let asc = |sort| ids_for(&conn, &sort_query(sort, false), &excluded);
        let desc = |sort| ids_for(&conn, &sort_query(sort, true), &excluded);

        assert_eq!(
            asc(RegistrySort::Name),
            ["fm-p-1", "fm-p-2", "fm-p-3", "fm-p-4"]
        );
        assert_eq!(
            desc(RegistrySort::Name),
            ["fm-p-4", "fm-p-3", "fm-p-2", "fm-p-1"]
        );

        // Birth date ascending is oldest first, so an ascending age is the reverse.
        assert_eq!(
            asc(RegistrySort::Age),
            ["fm-p-4", "fm-p-3", "fm-p-1", "fm-p-2"]
        );
        assert_eq!(
            desc(RegistrySort::Age),
            ["fm-p-2", "fm-p-1", "fm-p-3", "fm-p-4"]
        );

        // The D7B1 defect: these two used the age reversal, so "best first" returned the worst.
        assert_eq!(
            asc(RegistrySort::Ovr),
            ["fm-p-4", "fm-p-1", "fm-p-3", "fm-p-2"]
        );
        assert_eq!(
            desc(RegistrySort::Ovr),
            ["fm-p-2", "fm-p-3", "fm-p-1", "fm-p-4"]
        );
        assert_eq!(
            asc(RegistrySort::Value),
            ["fm-p-4", "fm-p-1", "fm-p-3", "fm-p-2"]
        );
        assert_eq!(
            desc(RegistrySort::Value),
            ["fm-p-2", "fm-p-3", "fm-p-1", "fm-p-4"]
        );

        // A free agent has no club; SQLite sorts the empty name first ascending.
        assert_eq!(
            asc(RegistrySort::Club),
            ["fm-p-3", "fm-p-2", "fm-p-4", "fm-p-1"]
        );
        assert_eq!(
            desc(RegistrySort::Club),
            ["fm-p-1", "fm-p-4", "fm-p-2", "fm-p-3"]
        );
    }

    /// Given the same page of players
    /// When the search screen's filters are applied one at a time
    /// Then only the players matching that filter are returned, and the career's exclusions still
    ///      remove a player from every filtered page
    #[test]
    fn every_search_filter_narrows_the_same_page() {
        let dir = tempfile::tempdir().unwrap();
        let conn = sortable_registry(&dir.path().join("registry.db"));
        let base = sort_query(RegistrySort::Name, false);

        let filtered = |patch: &dyn Fn(&mut RegistryQuery)| {
            let mut query = base.clone();
            patch(&mut query);
            ids_for(&conn, &query, &[])
        };

        assert_eq!(
            filtered(&|query| query.text = "bravo".to_string()),
            ["fm-p-2"]
        );
        assert_eq!(
            filtered(&|query| query.club = "alpha".to_string()),
            ["fm-p-2"]
        );
        assert_eq!(
            filtered(&|query| query.nation = Some("GB".to_string())),
            ["fm-p-2", "fm-p-3"]
        );
        assert_eq!(
            filtered(&|query| query.position = RegistryPositionFilter::Defender),
            ["fm-p-2"]
        );
        assert_eq!(
            filtered(&|query| {
                query.min_age = Some(25);
                query.max_age = Some(30);
            }),
            ["fm-p-1", "fm-p-2", "fm-p-3"]
        );
        assert_eq!(
            filtered(&|query| query.availability = RegistryAvailability::FreeAgent),
            ["fm-p-3"]
        );
        assert_eq!(
            filtered(&|query| query.availability = RegistryAvailability::Contracted),
            ["fm-p-1", "fm-p-2", "fm-p-4"]
        );

        let excluded = vec!["fm-p-2".to_string()];
        assert_eq!(
            ids_for(&conn, &sort_query(RegistrySort::Ovr, true), &excluded),
            ["fm-p-3", "fm-p-1", "fm-p-4"]
        );
    }
}
