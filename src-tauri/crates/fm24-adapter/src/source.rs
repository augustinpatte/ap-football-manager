//! Read-only access to `world.db`.
//!
//! The connection is opened with `SQLITE_OPEN_READ_ONLY` and `PRAGMA query_only = ON`; the file's
//! SHA-256 is taken before and after a run (`Source::sha256`) so the report can prove it was not
//! touched. The perimeter is selected from the save's own competition membership, by internal ids.

use crate::Result;
use rusqlite::{Connection, OpenFlags, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The 52 player attribute columns of `player_attributes`, FM scale 1-100 (FM 1-20 x 5).
pub const ATTRIBUTE_COLUMNS: [&str; 52] = [
    "crossing",
    "dribbling",
    "finishing",
    "heading",
    "long_shots",
    "marking",
    "off_the_ball",
    "passing",
    "penalty_taking",
    "tackling",
    "vision",
    "handling",
    "aerial_reach",
    "command_of_area",
    "communication",
    "kicking",
    "throwing",
    "anticipation",
    "decisions",
    "one_on_ones",
    "positioning",
    "reflexes",
    "first_touch",
    "technique",
    "flair",
    "corners",
    "teamwork",
    "work_rate",
    "long_throws",
    "eccentricity",
    "rushing_out",
    "punching",
    "acceleration",
    "free_kick_taking",
    "strength",
    "stamina",
    "pace",
    "jumping_reach",
    "leadership",
    "dirtiness",
    "balance",
    "bravery",
    "consistency",
    "aggression",
    "agility",
    "important_matches",
    "injury_proneness",
    "versatility",
    "natural_fitness",
    "determination",
    "composure",
    "concentration",
];

/// A player's 52 FM attributes, by column name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FmAttrs(pub BTreeMap<&'static str, i64>);

impl FmAttrs {
    /// Attribute value, 1-100. A name that is not a column is a programming error.
    pub fn get(&self, name: &str) -> f64 {
        *self
            .0
            .get(name)
            .unwrap_or_else(|| panic!("unknown FM attribute {name}")) as f64
    }

    pub fn from_pairs(pairs: &[(&'static str, i64)]) -> Self {
        let mut map: BTreeMap<&'static str, i64> =
            ATTRIBUTE_COLUMNS.iter().map(|name| (*name, 50)).collect();
        for (name, value) in pairs {
            assert!(ATTRIBUTE_COLUMNS.contains(name), "unknown attribute {name}");
            map.insert(name, *value);
        }
        Self(map)
    }
}

/// The scale a save's attribute columns are stored on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeScale {
    /// FM's own 1-20.
    OneToTwenty,
    /// FM's 1-20 times five, which is what `ATTRIBUTE_COLUMNS` documents.
    OneToHundred,
}

/// Which scale `rows` are on, read from the data rather than assumed: no attribute of any player
/// above 20 can only be FM's own 1-20 (a database of professionals always holds a 90 somewhere on
/// the 1-100 scale: pace, acceleration, finishing). Unknown (`None`) for no rows.
pub fn detect_attribute_scale(rows: &[PlayerRow]) -> Option<AttributeScale> {
    let highest = rows
        .iter()
        .flat_map(|row| row.attrs.0.values())
        .copied()
        .max()?;
    Some(if highest <= 20 {
        AttributeScale::OneToTwenty
    } else {
        AttributeScale::OneToHundred
    })
}

/// Puts `rows` on the 1-100 scale, once. A save already on 1-100 is not touched, so the factor
/// can never be applied twice. Returns the scale the rows were found on.
pub fn normalise_attribute_scale(rows: &mut [PlayerRow]) -> Option<AttributeScale> {
    let scale = detect_attribute_scale(rows)?;
    if scale == AttributeScale::OneToTwenty {
        for row in rows.iter_mut() {
            for value in row.attrs.0.values_mut() {
                *value *= 5;
            }
        }
    }
    Some(scale)
}

/// A bare player row for tests of conversion: a club player with the given ability, position
/// notes and attributes (every other attribute 50).
#[cfg(test)]
pub(crate) fn sample_row(
    ca: i64,
    pa: i64,
    position_code: &str,
    attributes: &[(&'static str, i64)],
) -> PlayerRow {
    PlayerRow {
        person_id: 1,
        fm_uid: None,
        fm_uid_status: String::new(),
        first_name: "A".into(),
        last_name: "Player".into(),
        common_name: None,
        birth_date: Some("1998-01-01".into()),
        nation: Some("England".into()),
        height_cm: None,
        club_id: 1,
        club_name: String::new(),
        team_slot: Some(0),
        team_is_youth: false,
        registered_club_id: None,
        registered_club_name: None,
        registered_slot: None,
        registered_is_youth: false,
        wage_gbp_week: 0,
        wage_original: 0,
        wage_source: String::new(),
        start_date: None,
        end_date: None,
        contract_type_code: None,
        squad_status_code: None,
        ca,
        pa,
        rep_current: None,
        rep_home: None,
        rep_world: None,
        foot_left: 50,
        foot_right: 80,
        value_gbp: None,
        value_source: None,
        attrs: FmAttrs::from_pairs(attributes),
        positions: BTreeMap::from([(position_code.to_string(), 20)]),
    }
}

#[derive(Debug, Clone)]
pub struct CompetitionRow {
    pub id: i64,
    pub fm_uid: Option<i64>,
    pub name: String,
    pub nation: Option<String>,
    pub playable: bool,
    pub simulated: bool,
    /// Pyramid level within its country (1 = top), from `competition_target`.
    pub level: Option<i64>,
    pub target_key: Option<String>,
    /// Confirmed `season_start` rule (ISO date), if any.
    pub season_start: Option<String>,
    /// The `tiebreak` rule of the referential (`pts>gd>gf>...`), confirmed or not.
    pub tiebreak: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Finance {
    pub balance_gbp: i64,
    pub transfer_budget_gbp: i64,
    pub wage_budget_gbp_week: i64,
    pub wage_bill_gbp_week: i64,
    pub tv_gbp: i64,
    pub other_revenue_gbp: i64,
    pub other_costs_gbp: i64,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct Colours {
    pub primary: String,
    pub secondary: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ClubRow {
    pub id: i64,
    pub fm_uid: Option<i64>,
    pub name: String,
    pub short_name: String,
    pub nation: Option<String>,
    pub reputation: i64,
    pub competition_id: i64,
    pub colours: Option<Colours>,
    pub finance: Option<Finance>,
}

#[derive(Debug, Clone)]
pub struct PlayerRow {
    pub person_id: i64,
    pub fm_uid: Option<i64>,
    pub fm_uid_status: String,
    pub first_name: String,
    pub last_name: String,
    pub common_name: Option<String>,
    pub birth_date: Option<String>,
    pub nation: Option<String>,
    pub height_cm: Option<i64>,
    /// The club that holds his current contract (his owner, whoever he plays for).
    pub club_id: i64,
    pub club_name: String,
    /// The owner club's nation, from the source. `None` when the club has none.
    pub club_nation: Option<String>,
    /// The owner club's reputation as the source records it (0 when absent).
    pub club_reputation: i64,
    pub team_slot: Option<i64>,
    pub team_is_youth: bool,
    /// Where the save has him registered (`player.registered_team_id`): the club he plays for and
    /// the team of that club (slot 0 = first team, above 0 = reserve or youth sides). When this
    /// club is not `club_id`, he is on loan (or in an affiliated side) with `club_id` as owner.
    pub registered_club_id: Option<i64>,
    pub registered_club_name: Option<String>,
    pub registered_slot: Option<i64>,
    pub registered_is_youth: bool,
    pub wage_gbp_week: i64,
    pub wage_original: i64,
    pub wage_source: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub contract_type_code: Option<i64>,
    pub squad_status_code: Option<i64>,
    pub ca: i64,
    pub pa: i64,
    pub rep_current: Option<i64>,
    pub rep_home: Option<i64>,
    pub rep_world: Option<i64>,
    pub foot_left: i64,
    pub foot_right: i64,
    pub value_gbp: Option<i64>,
    pub value_source: Option<String>,
    pub attrs: FmAttrs,
    /// FM position code -> note 1-20 (15 codes, `SW` included).
    pub positions: BTreeMap<String, i64>,
}

#[derive(Debug, Clone)]
pub struct StaffRow {
    pub person_id: i64,
    pub fm_uid: Option<i64>,
    pub fm_uid_status: String,
    pub first_name: String,
    pub last_name: String,
    pub common_name: Option<String>,
    pub birth_date: Option<String>,
    pub nation: Option<String>,
    pub club_id: i64,
    pub team_slot: Option<i64>,
    pub role_code: String,
    pub wage_gbp_week: i64,
    pub end_date: Option<String>,
    pub ca: Option<i64>,
    pub pa: Option<i64>,
    pub rep_current: Option<i64>,
    /// `staff_attribute.attr_key` -> raw value 0-100.
    pub attrs: BTreeMap<String, i64>,
}

pub struct Source {
    path: PathBuf,
    conn: Connection,
}

impl Source {
    /// Opens `path` read-only. Fails if the file does not exist (never creates one).
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(format!("world.db not found: {}", path.display()));
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("open {}: {e}", path.display()))?;
        conn.execute_batch("PRAGMA query_only = ON;")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path: path.to_path_buf(),
            conn,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// SHA-256 of the file, streamed (the file is ~240 MB).
    pub fn sha256(path: &Path) -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn schema_facts(&self) -> Result<SchemaFacts> {
        self.conn
            .query_row(
                "SELECT start_season, game_version, source_sha256, importer_version, imported_at FROM import_run ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    Ok(SchemaFacts {
                        start_season: r.get(0)?,
                        game_version: r.get(1)?,
                        source_sha256: r.get(2)?,
                        importer_version: r.get(3)?,
                        imported_at: r.get(4)?,
                    })
                },
            )
            .map_err(|e| format!("import_run: {e}"))
    }

    /// (competition, club) pairs of the perimeter: clubs of the playable or simulated leagues in
    /// `season`, as recorded by the save. Sorted by competition then club id.
    pub fn perimeter(&self, season: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT c.id, t.club_id
                 FROM competition c
                 JOIN competition_season cs ON cs.competition_id = c.id AND cs.season = ?1
                 JOIN competition_season_team cst ON cst.competition_season_id = cs.id
                 JOIN team t ON t.id = cst.team_id
                 WHERE c.kind = 'league' AND (c.playable = 1 OR c.simulated = 1)
                 ORDER BY c.id, t.club_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![season], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn competitions(&self, ids: &[i64]) -> Result<Vec<CompetitionRow>> {
        let list = id_list(ids);
        let sql = format!(
            "SELECT c.id, c.fm_uid, c.name, n.name, c.playable, c.simulated, ct.level, ct.competition_key,
                    (SELECT value FROM competition_rule r WHERE r.competition_key = ct.competition_key
                       AND r.rule_key = 'season_start' AND r.status = 'confirmée'),
                    (SELECT value FROM competition_rule r WHERE r.competition_key = ct.competition_key
                       AND r.rule_key = 'tiebreak' AND r.value IS NOT NULL)
             FROM competition c
             LEFT JOIN nation n ON n.id = c.nation_id
             LEFT JOIN competition_target ct ON ct.fm_uid = CAST(c.fm_uid AS TEXT)
             WHERE c.id IN ({list}) ORDER BY c.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CompetitionRow {
                    id: r.get(0)?,
                    fm_uid: r.get(1)?,
                    name: r.get(2)?,
                    nation: r.get(3)?,
                    playable: r.get::<_, i64>(4)? == 1,
                    simulated: r.get::<_, i64>(5)? == 1,
                    level: r.get(6)?,
                    target_key: r.get(7)?,
                    season_start: r.get(8)?,
                    tiebreak: r.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn clubs(&self, perimeter: &[(i64, i64)]) -> Result<Vec<ClubRow>> {
        let club_ids: Vec<i64> = perimeter.iter().map(|(_, club)| *club).collect();
        let competition_of: BTreeMap<i64, i64> = perimeter
            .iter()
            .map(|(comp, club)| (*club, *comp))
            .collect();
        let list = id_list(&club_ids);
        let sql = format!(
            "SELECT c.id, c.fm_uid, c.name, COALESCE(c.short_name, ''), n.name, COALESCE(c.reputation, 0),
                    k.primary_rgb, k.secondary_rgb, k.source,
                    f.balance_gbp, f.transfer_budget_gbp, f.wage_budget_gbp_week, f.wage_bill_gbp_week,
                    f.tv_gbp, f.other_revenue_gbp, f.other_costs_gbp, f.source
             FROM club c
             LEFT JOIN nation n ON n.id = c.nation_id
             LEFT JOIN club_colours k ON k.club_id = c.id
             LEFT JOIN club_finance_start f ON f.club_id = c.id
             WHERE c.id IN ({list}) ORDER BY c.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let primary: Option<String> = r.get(6)?;
                let secondary: Option<String> = r.get(7)?;
                let colour_source: Option<String> = r.get(8)?;
                let balance: Option<i64> = r.get(9)?;
                let finance = match balance {
                    Some(balance_gbp) => Some(Finance {
                        balance_gbp,
                        transfer_budget_gbp: r.get(10)?,
                        wage_budget_gbp_week: r.get(11)?,
                        wage_bill_gbp_week: r.get(12)?,
                        tv_gbp: r.get(13)?,
                        other_revenue_gbp: r.get(14)?,
                        other_costs_gbp: r.get(15)?,
                        source: r.get(16)?,
                    }),
                    None => None,
                };
                Ok(ClubRow {
                    id,
                    fm_uid: r.get(1)?,
                    name: r.get(2)?,
                    short_name: r.get(3)?,
                    nation: r.get(4)?,
                    reputation: r.get(5)?,
                    competition_id: competition_of[&id],
                    colours: match (primary, secondary, colour_source) {
                        (Some(primary), Some(secondary), Some(source)) => Some(Colours {
                            primary,
                            secondary,
                            source,
                        }),
                        _ => None,
                    },
                    finance,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Current players (contract `is_current = 1`) of the given clubs, with their 52 attributes and
    /// 15 position notes. Sorted by `person.id`.
    pub fn players(&self, club_ids: &[i64]) -> Result<Vec<PlayerRow>> {
        let list = id_list(club_ids);
        let attr_cols = ATTRIBUTE_COLUMNS
            .iter()
            .map(|c| format!("a.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT p.id, p.fm_uid, p.fm_uid_status, COALESCE(p.first_name, ''), COALESCE(p.last_name, ''),
                    p.common_name, p.birth_date, n.name, p.height_cm,
                    k.club_id, t.slot, COALESCE(t.is_youth, 0),
                    k.wage_gbp_week, k.wage_original, k.wage_source, k.start_date, k.end_date,
                    k.contract_type_code, k.squad_status_code,
                    pl.ca, pl.pa, pl.rep_current, pl.rep_home, pl.rep_world, pl.foot_left, pl.foot_right,
                    pl.value_gbp, pl.value_source,
                    rt.club_id, rc.name, rt.slot, COALESCE(rt.is_youth, 0), oc.name,
                    cn.name, COALESCE(oc.reputation, 0),
                    {attr_cols}
             FROM contract k
             JOIN person p ON p.id = k.person_id
             JOIN player pl ON pl.person_id = p.id
             JOIN player_attributes a ON a.person_id = p.id
             LEFT JOIN nation n ON n.id = p.nation_id
             LEFT JOIN team t ON t.id = k.team_id
             LEFT JOIN team rt ON rt.id = pl.registered_team_id
             LEFT JOIN club rc ON rc.id = rt.club_id
             LEFT JOIN club oc ON oc.id = k.club_id
             LEFT JOIN nation cn ON cn.id = oc.nation_id
             WHERE k.is_current = 1 AND (k.club_id IN ({list}) OR rt.club_id IN ({list}))
             ORDER BY p.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let first_attr = 35;
        let rows = stmt
            .query_map([], |r| {
                let mut attrs = BTreeMap::new();
                for (offset, name) in ATTRIBUTE_COLUMNS.iter().enumerate() {
                    let value: Option<i64> = r.get(first_attr + offset)?;
                    attrs.insert(*name, value.unwrap_or(0));
                }
                Ok(PlayerRow {
                    person_id: r.get(0)?,
                    fm_uid: r.get(1)?,
                    fm_uid_status: r.get(2)?,
                    first_name: r.get(3)?,
                    last_name: r.get(4)?,
                    common_name: r.get(5)?,
                    birth_date: r.get(6)?,
                    nation: r.get(7)?,
                    height_cm: r.get(8)?,
                    club_id: r.get(9)?,
                    team_slot: r.get(10)?,
                    team_is_youth: r.get::<_, i64>(11)? == 1,
                    registered_club_id: r.get(28)?,
                    registered_club_name: r.get(29)?,
                    registered_slot: r.get(30)?,
                    registered_is_youth: r.get::<_, i64>(31)? == 1,
                    club_name: r.get::<_, Option<String>>(32)?.unwrap_or_default(),
                    club_nation: r.get(33)?,
                    club_reputation: r.get(34)?,
                    wage_gbp_week: r.get(12)?,
                    wage_original: r.get(13)?,
                    wage_source: r.get(14)?,
                    start_date: r.get(15)?,
                    end_date: r.get(16)?,
                    contract_type_code: r.get(17)?,
                    squad_status_code: r.get(18)?,
                    ca: r.get(19)?,
                    pa: r.get(20)?,
                    rep_current: r.get(21)?,
                    rep_home: r.get(22)?,
                    rep_world: r.get(23)?,
                    foot_left: r.get(24)?,
                    foot_right: r.get(25)?,
                    value_gbp: r.get(26)?,
                    value_source: r.get(27)?,
                    attrs: FmAttrs(attrs),
                    positions: BTreeMap::new(),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut players = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let pos_sql = format!(
            "SELECT pp.person_id, pp.position, pp.rating FROM player_position pp
             WHERE pp.person_id IN (
                 SELECT k.person_id FROM contract k
                 JOIN player pl ON pl.person_id = k.person_id
                 LEFT JOIN team rt ON rt.id = pl.registered_team_id
                 WHERE k.is_current = 1 AND (k.club_id IN ({list}) OR rt.club_id IN ({list})))
             ORDER BY pp.person_id, pp.position"
        );
        let mut stmt = self.conn.prepare(&pos_sql).map_err(|e| e.to_string())?;
        let mut by_person: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (person, position, rating) = row.map_err(|e| e.to_string())?;
            by_person
                .entry(person)
                .or_default()
                .insert(position, rating);
        }
        for player in &mut players {
            player.positions = by_person.remove(&player.person_id).unwrap_or_default();
        }
        if normalise_attribute_scale(&mut players) == Some(AttributeScale::OneToTwenty) {
            eprintln!(
                "world.db attributes are on FM's own 1-20 scale: scaled x5 once to the 1-100 scale the adapter reads"
            );
        }
        Ok(players)
    }

    /// Every player of the save with no current contract: the free agents and, mixed in with
    /// them, players of clubs the save does not load and people long retired. `club_id` is 0 and
    /// the contract fields are empty; selecting among them is `free_agents::select`'s job.
    pub fn unattached_players(&self) -> Result<Vec<PlayerRow>> {
        let attr_cols = ATTRIBUTE_COLUMNS
            .iter()
            .map(|c| format!("a.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT p.id, p.fm_uid, p.fm_uid_status, COALESCE(p.first_name, ''), COALESCE(p.last_name, ''),
                    p.common_name, p.birth_date, n.name, p.height_cm,
                    pl.ca, pl.pa, pl.rep_current, pl.rep_home, pl.rep_world, pl.foot_left, pl.foot_right,
                    pl.value_gbp, pl.value_source, {attr_cols}
             FROM player pl
             JOIN person p ON p.id = pl.person_id
             JOIN player_attributes a ON a.person_id = p.id
             LEFT JOIN nation n ON n.id = p.nation_id
             WHERE NOT EXISTS (SELECT 1 FROM contract k WHERE k.person_id = p.id AND k.is_current = 1)
             ORDER BY p.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let first_attr = 18;
        let rows = stmt
            .query_map([], |r| {
                let mut attrs = BTreeMap::new();
                for (offset, name) in ATTRIBUTE_COLUMNS.iter().enumerate() {
                    let value: Option<i64> = r.get(first_attr + offset)?;
                    attrs.insert(*name, value.unwrap_or(0));
                }
                Ok(PlayerRow {
                    person_id: r.get(0)?,
                    fm_uid: r.get(1)?,
                    fm_uid_status: r.get(2)?,
                    first_name: r.get(3)?,
                    last_name: r.get(4)?,
                    common_name: r.get(5)?,
                    birth_date: r.get(6)?,
                    nation: r.get(7)?,
                    height_cm: r.get(8)?,
                    club_id: 0,
                    club_name: String::new(),
                    club_nation: None,
                    club_reputation: 0,
                    team_slot: None,
                    team_is_youth: false,
                    registered_club_id: None,
                    registered_club_name: None,
                    registered_slot: None,
                    registered_is_youth: false,
                    wage_gbp_week: 0,
                    wage_original: 0,
                    wage_source: String::new(),
                    start_date: None,
                    end_date: None,
                    contract_type_code: None,
                    squad_status_code: None,
                    ca: r.get(9)?,
                    pa: r.get(10)?,
                    rep_current: r.get(11)?,
                    rep_home: r.get(12)?,
                    rep_world: r.get(13)?,
                    foot_left: r.get(14)?,
                    foot_right: r.get(15)?,
                    value_gbp: r.get(16)?,
                    value_source: r.get(17)?,
                    attrs: FmAttrs(attrs),
                    positions: BTreeMap::new(),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut players = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT pp.person_id, pp.position, pp.rating FROM player_position pp
                 WHERE NOT EXISTS (SELECT 1 FROM contract k WHERE k.person_id = pp.person_id AND k.is_current = 1)
                 ORDER BY pp.person_id, pp.position",
            )
            .map_err(|e| e.to_string())?;
        let mut by_person: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (person, position, rating) = row.map_err(|e| e.to_string())?;
            by_person
                .entry(person)
                .or_default()
                .insert(position, rating);
        }
        for player in &mut players {
            player.positions = by_person.remove(&player.person_id).unwrap_or_default();
        }
        if normalise_attribute_scale(&mut players) == Some(AttributeScale::OneToTwenty) {
            eprintln!(
                "world.db attributes are on FM's own 1-20 scale: scaled x5 once to the 1-100 scale the adapter reads"
            );
        }
        Ok(players)
    }

    /// Every player of the save, with his current contract if he has one.
    ///
    /// Unlike [`Self::players`] this is not limited to a perimeter: it reads all 81 762 players so
    /// the external registry can classify every source record, including the ones the active world
    /// does not import. The current contract is chosen deterministically (lowest club id, then
    /// contract id) because a person can legitimately carry more than one current row.
    pub fn every_player(&self) -> Result<Vec<PlayerRow>> {
        let attr_cols = ATTRIBUTE_COLUMNS
            .iter()
            .map(|c| format!("COALESCE(a.{c}, 0)"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT p.id, p.fm_uid, p.fm_uid_status, COALESCE(p.first_name, ''), COALESCE(p.last_name, ''),
                    p.common_name, p.birth_date, n.name, p.height_cm,
                    COALESCE(k.club_id, 0), t.slot, COALESCE(t.is_youth, 0),
                    COALESCE(k.wage_gbp_week, 0), COALESCE(k.wage_original, 0), COALESCE(k.wage_source, ''),
                    k.start_date, k.end_date, k.contract_type_code, k.squad_status_code,
                    COALESCE(pl.ca, 0), COALESCE(pl.pa, 0), pl.rep_current, pl.rep_home, pl.rep_world,
                    COALESCE(pl.foot_left, 0), COALESCE(pl.foot_right, 0),
                    pl.value_gbp, pl.value_source,
                    rt.club_id, rc.name, rt.slot, COALESCE(rt.is_youth, 0), oc.name, cn.name,
                    COALESCE(oc.reputation, 0),
                    {attr_cols}
             FROM player pl
             JOIN person p ON p.id = pl.person_id
             LEFT JOIN player_attributes a ON a.person_id = p.id
             LEFT JOIN contract k ON k.id = (
                 SELECT c.id FROM contract c
                 WHERE c.person_id = p.id AND c.is_current = 1
                 ORDER BY c.club_id, c.id LIMIT 1)
             LEFT JOIN team t ON t.id = k.team_id
             LEFT JOIN team rt ON rt.id = pl.registered_team_id
             LEFT JOIN club rc ON rc.id = rt.club_id
             LEFT JOIN club oc ON oc.id = k.club_id
             LEFT JOIN nation cn ON cn.id = oc.nation_id
             LEFT JOIN nation n ON n.id = p.nation_id
             ORDER BY p.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let first_attr = 35;
        let rows = stmt
            .query_map([], |r| {
                let mut attrs = BTreeMap::new();
                for (offset, name) in ATTRIBUTE_COLUMNS.iter().enumerate() {
                    let value: Option<i64> = r.get(first_attr + offset)?;
                    attrs.insert(*name, value.unwrap_or(0));
                }
                Ok(PlayerRow {
                    person_id: r.get(0)?,
                    fm_uid: r.get(1)?,
                    fm_uid_status: r.get(2)?,
                    first_name: r.get(3)?,
                    last_name: r.get(4)?,
                    common_name: r.get(5)?,
                    birth_date: r.get(6)?,
                    nation: r.get(7)?,
                    height_cm: r.get(8)?,
                    club_id: r.get(9)?,
                    team_slot: r.get(10)?,
                    team_is_youth: r.get::<_, i64>(11)? == 1,
                    registered_club_id: r.get(28)?,
                    registered_club_name: r.get(29)?,
                    registered_slot: r.get(30)?,
                    registered_is_youth: r.get::<_, i64>(31)? == 1,
                    club_name: r.get::<_, Option<String>>(32)?.unwrap_or_default(),
                    club_nation: r.get(33)?,
                    club_reputation: r.get(34)?,
                    wage_gbp_week: r.get(12)?,
                    wage_original: r.get(13)?,
                    wage_source: r.get(14)?,
                    start_date: r.get(15)?,
                    end_date: r.get(16)?,
                    contract_type_code: r.get(17)?,
                    squad_status_code: r.get(18)?,
                    ca: r.get(19)?,
                    pa: r.get(20)?,
                    rep_current: r.get(21)?,
                    rep_home: r.get(22)?,
                    rep_world: r.get(23)?,
                    foot_left: r.get(24)?,
                    foot_right: r.get(25)?,
                    value_gbp: r.get(26)?,
                    value_source: r.get(27)?,
                    attrs: FmAttrs(attrs),
                    positions: BTreeMap::new(),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut players = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT person_id, position, rating FROM player_position
                 ORDER BY person_id, position",
            )
            .map_err(|e| e.to_string())?;
        let mut by_person: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (person, position, rating) = row.map_err(|e| e.to_string())?;
            by_person
                .entry(person)
                .or_default()
                .insert(position, rating);
        }
        for player in &mut players {
            player.positions = by_person.remove(&player.person_id).unwrap_or_default();
        }
        Ok(players)
    }

    /// Current staff contracts of the given clubs (people who are in `staff`), with attributes.
    pub fn staff(&self, club_ids: &[i64]) -> Result<Vec<StaffRow>> {
        let list = id_list(club_ids);
        let sql = format!(
            "SELECT p.id, p.fm_uid, p.fm_uid_status, COALESCE(p.first_name, ''), COALESCE(p.last_name, ''),
                    p.common_name, p.birth_date, n.name, k.club_id, t.slot, k.role_code,
                    k.wage_gbp_week, k.end_date, s.ca, s.pa, s.rep_current
             FROM contract k
             JOIN person p ON p.id = k.person_id
             JOIN staff s ON s.person_id = p.id
             LEFT JOIN nation n ON n.id = p.nation_id
             LEFT JOIN team t ON t.id = k.team_id
             WHERE k.is_current = 1 AND k.club_id IN ({list}) AND k.role_code IS NOT NULL
             ORDER BY p.id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(StaffRow {
                    person_id: r.get(0)?,
                    fm_uid: r.get(1)?,
                    fm_uid_status: r.get(2)?,
                    first_name: r.get(3)?,
                    last_name: r.get(4)?,
                    common_name: r.get(5)?,
                    birth_date: r.get(6)?,
                    nation: r.get(7)?,
                    club_id: r.get(8)?,
                    team_slot: r.get(9)?,
                    role_code: r.get(10)?,
                    wage_gbp_week: r.get(11)?,
                    end_date: r.get(12)?,
                    ca: r.get(13)?,
                    pa: r.get(14)?,
                    rep_current: r.get(15)?,
                    attrs: BTreeMap::new(),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut staff = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let attr_sql = format!(
            "SELECT sa.person_id, sa.attr_key, sa.value_raw FROM staff_attribute sa
             WHERE sa.person_id IN (SELECT person_id FROM contract WHERE is_current = 1 AND club_id IN ({list}))
             ORDER BY sa.person_id, sa.attr_key"
        );
        let mut stmt = self.conn.prepare(&attr_sql).map_err(|e| e.to_string())?;
        let mut by_person: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (person, key, value) = row.map_err(|e| e.to_string())?;
            by_person.entry(person).or_default().insert(key, value);
        }
        for member in &mut staff {
            member.attrs = by_person.remove(&member.person_id).unwrap_or_default();
        }
        Ok(staff)
    }

    /// `person_id`s whose current contract is at a perimeter club, grouped as (players, staff, other).
    pub fn current_contract_counts(&self, club_ids: &[i64]) -> Result<(i64, i64)> {
        let list = id_list(club_ids);
        let sql = format!(
            "SELECT
               (SELECT COUNT(*) FROM contract k WHERE k.is_current = 1 AND k.club_id IN ({list})
                  AND k.person_id IN (SELECT person_id FROM player)),
               (SELECT COUNT(*) FROM contract k WHERE k.is_current = 1 AND k.club_id IN ({list})
                  AND k.person_id IN (SELECT person_id FROM staff))"
        );
        self.conn
            .query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())
    }

    /// Players in the whole database (for the "81 762 distinct players" check).
    pub fn total_players(&self) -> Result<(i64, i64)> {
        self.conn
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT person_id) FROM player",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct SchemaFacts {
    pub start_season: i64,
    pub game_version: String,
    pub source_sha256: String,
    pub importer_version: String,
    pub imported_at: String,
}

/// `1,2,3` for an `IN (...)` clause. Integers only, so no quoting issue.
fn id_list(ids: &[i64]) -> String {
    if ids.is_empty() {
        return "NULL".to_string();
    }
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_list_is_comma_separated_integers() {
        assert_eq!(id_list(&[3, 1, 2]), "3,1,2");
        assert_eq!(id_list(&[]), "NULL");
    }

    #[test]
    fn there_are_52_distinct_attribute_columns() {
        let mut names: Vec<_> = ATTRIBUTE_COLUMNS.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 52);
    }

    #[test]
    fn opening_a_missing_file_fails_instead_of_creating_it() {
        let path = std::env::temp_dir().join("apfm-does-not-exist-12345.db");
        assert!(Source::open(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn a_read_only_connection_refuses_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (1);")
                .unwrap();
        }
        let source = Source::open(&path).unwrap();
        assert!(source.conn.execute("INSERT INTO t VALUES (2)", []).is_err());
    }
}
