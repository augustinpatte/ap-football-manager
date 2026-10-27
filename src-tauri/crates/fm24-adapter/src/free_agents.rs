//! The save's unattached players, as a bounded pool of free agents.
//!
//! `world.db` holds 20 448 players with no current contract. They are not all free agents: they
//! mix real ones with players of clubs in leagues the save does not load and people long retired
//! (some are 60 to 85). Putting them all in the world would triple the number of players every
//! daily scan walks and the state sent to the screen, so a **pool** enters instead:
//!
//! 1. *eligible*: a birth date, an age inside the configured range, a level a professional club
//!    would roster (or a prospect), a nationality OFM knows, a position the converter can place;
//! 2. *chosen*: the eligible ranked by a seeded hash of their id, the first `pool_size` kept — a
//!    uniform draw, so the pool has the positions, ages and levels of the eligible, not of a
//!    sort order;
//! 3. *priced*: each asks the wage and carries the value of contracted players of his ability and
//!    age — read from the save's own figures, not from OFM's `value / 200`, which is an order of
//!    magnitude above what the save's clubs pay.
//!
//! Nothing here touches `world.db`: whoever is not chosen stays there, with the reason counted.

use crate::config::Config;
use crate::config::FreeAgentsCfg;
use crate::ids;
use crate::journal::Journal;
use crate::pipeline::{Converted, convert_player};
use crate::refs::Refs;
use crate::source::PlayerRow;
use chrono::NaiveDate;
use ofm_core::generator::PlayerDef;

/// What was decided for one free agent, kept so the checks and the report can read it back.
#[derive(Debug, Clone)]
pub struct FreeAgentPlan {
    pub person_id: i64,
    pub fm_uid: Option<i64>,
    pub fm_uid_status: String,
    pub name: String,
    pub def: PlayerDef,
    pub ovr_expected: u8,
    /// Weekly wage he asks, in euros; staged on the player after the world is built.
    pub asked_wage: Option<u32>,
    pub value_source: &'static str,
    pub wage_source: &'static str,
    pub age: i32,
    pub ca: i64,
}

/// Why each of the save's unattached players is or is not in the pool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolStats {
    pub candidates: usize,
    pub no_birth_date: usize,
    pub outside_age_range: usize,
    pub below_level: usize,
    pub no_known_nation: usize,
    pub no_position: usize,
    pub eligible: usize,
    pub chosen: usize,
}

pub struct Pool {
    pub plans: Vec<FreeAgentPlan>,
    pub stats: PoolStats,
}

/// What contracted players of the perimeter earn and are worth, by ability and age.
pub struct Reference {
    /// (current ability, age in years, weekly wage in euros, market value in euros)
    rows: Vec<(i64, f64, i64, Option<i64>)>,
}

/// Neighbours needed before an estimate is trusted.
const MIN_SAMPLES: usize = 40;
/// Half-width of the age window around the player, in years.
const AGE_WINDOW: f64 = 3.5;
/// The ability window is widened (doubled) until enough neighbours are found, up to this.
const MAX_CA_WINDOW: i64 = 40;

fn age_years(birth_date: &str, on: NaiveDate) -> Option<f64> {
    let born = NaiveDate::parse_from_str(birth_date, "%Y-%m-%d").ok()?;
    Some((on - born).num_days() as f64 / 365.25)
}

fn quantile(mut values: Vec<i64>, q: f64) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * q).round() as usize;
    Some(values[index])
}

/// splitmix64: a fixed, seedless-in-the-code mixer, so the draw depends on the id and the
/// configured seed and nothing else.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl Reference {
    /// Built from the perimeter's senior contracts (youth-team players are paid like trainees).
    pub fn from_contracted(players: &[PlayerRow], cfg: &Config, on: NaiveDate) -> Self {
        let rows = players
            .iter()
            .filter(|p| !p.team_is_youth && p.wage_gbp_week > 0)
            .filter_map(|p| {
                let age = age_years(p.birth_date.as_deref()?, on)?;
                Some((
                    p.ca,
                    age,
                    cfg.to_eur(p.wage_gbp_week),
                    p.value_gbp.map(|v| cfg.to_eur(v)).filter(|v| *v > 0),
                ))
            })
            .collect();
        Self { rows }
    }

    /// Wages and values of the contracted players closest to (`ca`, `age`): the ability window
    /// is widened until [`MIN_SAMPLES`] are found.
    fn neighbours(
        &self,
        ca: i64,
        age: f64,
        base_window: i64,
    ) -> Vec<&(i64, f64, i64, Option<i64>)> {
        let mut window = base_window.max(1);
        loop {
            let found: Vec<_> = self
                .rows
                .iter()
                .filter(|r| (r.0 - ca).abs() <= window && (r.1 - age).abs() <= AGE_WINDOW)
                .collect();
            if found.len() >= MIN_SAMPLES || window >= MAX_CA_WINDOW {
                return found;
            }
            window *= 2;
        }
    }

    pub fn wage(&self, ca: i64, age: f64, cfg: &FreeAgentsCfg) -> Option<i64> {
        let near = self.neighbours(ca, age, cfg.ca_window);
        quantile(near.iter().map(|r| r.2).collect(), cfg.wage_quantile)
    }

    pub fn value(&self, ca: i64, age: f64, cfg: &FreeAgentsCfg) -> Option<i64> {
        let near = self.neighbours(ca, age, cfg.ca_window);
        quantile(near.iter().filter_map(|r| r.3).collect(), 0.5)
    }
}

struct Eligible<'a> {
    hash: u64,
    row: &'a PlayerRow,
    converted: Converted,
    nationality: String,
    age: f64,
}

/// Choose the pool among `unattached` and price it against `contracted`.
pub fn select(
    unattached: &[PlayerRow],
    contracted: &[PlayerRow],
    cfg: &Config,
    refs: &Refs,
) -> Pool {
    let fa = &cfg.free_agents;
    let mut stats = PoolStats::default();
    let on = NaiveDate::parse_from_str(&fa.age_on, "%Y-%m-%d")
        .expect("free_agents.age_on is a date (checked by the config test)");
    // The converter reports a player it cannot place to the journal; here that is a count.
    let mut scratch = Journal::default();

    let mut eligible: Vec<Eligible> = Vec::new();
    for row in unattached {
        stats.candidates += 1;
        let Some(age) = row.birth_date.as_deref().and_then(|b| age_years(b, on)) else {
            stats.no_birth_date += 1;
            continue;
        };
        if age < fa.min_age as f64 || age >= (fa.max_age + 1) as f64 {
            stats.outside_age_range += 1;
            continue;
        }
        let established = row.ca >= fa.min_ca;
        let prospect = age < (fa.prospect_max_age + 1) as f64
            && row.pa >= fa.prospect_min_pa
            && row.ca >= fa.prospect_min_ca;
        if !established && !prospect {
            stats.below_level += 1;
            continue;
        }
        let Some(nationality) = row.nation.as_deref().and_then(|n| refs.nation_code(n)) else {
            stats.no_known_nation += 1;
            continue;
        };
        let Some(converted) = convert_player(row, cfg, &mut scratch) else {
            stats.no_position += 1;
            continue;
        };
        eligible.push(Eligible {
            hash: mix(row.person_id as u64 ^ cfg.world.seed),
            row,
            converted,
            nationality,
            age,
        });
    }
    stats.eligible = eligible.len();

    eligible.sort_by_key(|e| (e.hash, e.row.person_id));
    eligible.truncate(fa.pool_size);
    eligible.sort_by_key(|e| e.row.person_id);
    stats.chosen = eligible.len();

    let reference = Reference::from_contracted(contracted, cfg, on);
    let plans = eligible
        .into_iter()
        .map(|e| plan(e, &reference, cfg))
        .collect();
    Pool { plans, stats }
}

fn plan(e: Eligible, reference: &Reference, cfg: &Config) -> FreeAgentPlan {
    let fa = &cfg.free_agents;
    let row = e.row;
    let value = reference.value(row.ca, e.age, fa);
    let wage = reference.wage(row.ca, e.age, fa);
    let name = row
        .common_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_default();
    let (value, value_source) = match (row.value_gbp, row.value_source.as_deref()) {
        (Some(v), Some("save")) => (Some(cfg.to_eur(v).max(0) as u64), "save"),
        _ => match value {
            Some(v) => (Some(v.max(0) as u64), "estimée (adaptateur)"),
            None => (None, "ofm-générée"),
        },
    };
    let def = PlayerDef {
        id: ids::player(row.person_id),
        name,
        first_name: row.first_name.clone(),
        last_name: row.last_name.clone(),
        // Empty is "unattached" in the package format.
        club: String::new(),
        nationality: e.nationality,
        position: e.converted.position.clone(),
        date_of_birth: row.birth_date.clone(),
        age: None,
        overall: None,
        potential: Some(e.converted.potential),
        attributes: Some(e.converted.attributes.clone()),
        photo: None,
        footedness: Some(crate::positions::footedness_name(&e.converted.foot).to_string()),
        youth: false,
        contract_start: None,
        contract_end: None,
        contract_length: None,
        wage: None,
        value,
        weak_foot: e.converted.weak_foot,
        alternate_positions: e.converted.alternates.clone(),
        condition: None,
        morale: None,
        career_history: Vec::new(),
    };
    FreeAgentPlan {
        person_id: row.person_id,
        fm_uid: row.fm_uid,
        fm_uid_status: row.fm_uid_status.clone(),
        name: format!("{} {}", row.first_name, row.last_name)
            .trim()
            .to_string(),
        def,
        ovr_expected: e.converted.ovr,
        asked_wage: wage.map(|w| u32::try_from(w.max(0)).unwrap_or(u32::MAX)),
        value_source,
        wage_source: if wage.is_some() {
            "estimée (adaptateur)"
        } else {
            "ofm-générée"
        },
        age: e.age.floor() as i32,
        ca: row.ca,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FmAttrs;
    use std::collections::BTreeMap;

    fn row(person_id: i64, birth: &str, ca: i64, pa: i64, nation: &str) -> PlayerRow {
        let mut positions = BTreeMap::new();
        positions.insert("STC".to_string(), 20);
        PlayerRow {
            person_id,
            fm_uid: None,
            fm_uid_status: String::new(),
            first_name: "A".into(),
            last_name: format!("P{person_id}"),
            common_name: None,
            birth_date: Some(birth.into()),
            nation: Some(nation.into()),
            height_cm: None,
            club_id: 0,
            team_slot: None,
            team_is_youth: false,
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
            attrs: FmAttrs::from_pairs(&[]),
            positions,
        }
    }

    fn paid(person_id: i64, ca: i64, wage: i64, value: i64) -> PlayerRow {
        PlayerRow {
            wage_gbp_week: wage,
            value_gbp: Some(value),
            club_id: 1,
            ..row(person_id, "1998-01-01", ca, ca, "France")
        }
    }

    fn cfg() -> Config {
        let mut cfg = Config::default_config();
        cfg.free_agents.pool_size = 3;
        cfg
    }

    /// Given unattached players: a retiree, a child, a weak veteran, a gifted teenager, two regulars
    /// When the pool is chosen
    /// Then only the teenager and the regulars are eligible, each exclusion is counted under its
    ///      own reason, and nothing is dropped without being counted.
    #[test]
    fn each_exclusion_is_counted_under_its_reason() {
        let refs = Refs::embedded().unwrap();
        let unattached = vec![
            row(1, "1944-01-01", 120, 120, "France"), // 82
            row(2, "2015-01-01", 120, 120, "France"), // 11
            row(3, "1992-01-01", 40, 40, "France"),   // weak
            row(4, "2008-01-01", 50, 120, "France"),  // 18, gifted: a prospect
            row(5, "1996-01-01", 100, 100, "France"),
            row(6, "1999-01-01", 90, 95, "France"),
            row(7, "1999-01-01", 90, 95, "Narnia"), // no nation OFM knows
        ];
        let pool = select(&unattached, &[], &cfg(), &refs);
        let s = &pool.stats;
        assert_eq!(s.candidates, 7);
        assert_eq!(s.outside_age_range, 2);
        assert_eq!(s.below_level, 1);
        assert_eq!(s.no_known_nation, 1);
        assert_eq!(s.eligible, 3);
        assert_eq!(s.chosen, 3);
        let ids: Vec<i64> = pool.plans.iter().map(|p| p.person_id).collect();
        assert_eq!(ids, vec![4, 5, 6]);
        assert!(pool.plans.iter().all(|p| p.def.club.is_empty()));
    }

    /// Given more eligible players than the pool holds
    /// When the pool is chosen twice, with the input in two orders
    /// Then the same players are chosen: the draw depends on ids and seed, not on order.
    #[test]
    fn the_draw_does_not_depend_on_input_order() {
        let refs = Refs::embedded().unwrap();
        let mut all: Vec<PlayerRow> = (10..60)
            .map(|id| row(id, "1996-06-06", 100, 100, "France"))
            .collect();
        let first = select(&all, &[], &cfg(), &refs);
        all.reverse();
        let second = select(&all, &[], &cfg(), &refs);
        let ids = |p: &Pool| p.plans.iter().map(|x| x.person_id).collect::<Vec<_>>();
        assert_eq!(ids(&first), ids(&second));
        assert_eq!(first.stats.chosen, 3);
        assert_eq!(first.stats.eligible, 50);
    }

    /// Given contracted players of similar ability paid around 1 000 a week and worth 1.5 M
    /// When a free agent of that ability is priced
    /// Then he asks a wage of that order (not OFM's `value / 200`) and carries their value.
    #[test]
    fn a_free_agent_is_priced_from_the_contracted_players_around_him() {
        let refs = Refs::embedded().unwrap();
        let contracted: Vec<PlayerRow> = (0..80)
            .map(|i| paid(1_000 + i, 100, 900 + (i % 5) * 50, 1_500_000))
            .collect();
        let unattached = vec![row(5, "1996-01-01", 100, 100, "France")];
        let pool = select(&unattached, &contracted, &cfg(), &refs);
        let plan = &pool.plans[0];
        let wage = plan.asked_wage.expect("priced");
        // 900..1 100 pounds, 1.17 euros each, the 30th percentile
        assert!((1_000..=1_300).contains(&wage), "asked {wage}");
        assert_eq!(plan.def.value, Some(1_755_000));
        assert_eq!(plan.value_source, "estimée (adaptateur)");
    }

    #[test]
    fn the_quantile_picks_a_member_of_the_sample() {
        assert_eq!(quantile(vec![], 0.3), None);
        assert_eq!(quantile(vec![5, 1, 9, 3, 7], 0.0), Some(1));
        assert_eq!(quantile(vec![5, 1, 9, 3, 7], 0.5), Some(5));
        assert_eq!(quantile(vec![5, 1, 9, 3, 7], 1.0), Some(9));
    }
}
