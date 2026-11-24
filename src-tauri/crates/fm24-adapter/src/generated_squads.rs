//! The passes that shape the squads of the built world: which of OFM's generated fillers are kept,
//! and how far their level and pay is brought down to the division they play in.
//!
//! Split from `pipeline` (which orders the whole import) so that file stays inside its size cap; the
//! behaviour is unchanged.

use crate::attributes;
use crate::ids;
use crate::journal::Journal;
use crate::source::ClubRow;
use domain::player::{Position, SquadRole};
use ofm_core::generator::WorldData;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn is_imported(id: &str) -> bool {
    id.starts_with("fm-p-") || id.starts_with("fm-s-")
}

/// OFM pads every club to 22 players with generated ones. Only what OFM's own squad floor needs is
/// kept (15 seniors, at least 2 G / 4 D / 4 M / 2 F counting the imported players), best rated
/// first; the rest of the generated players are removed. Imported players are never touched,
/// except that, with `promote_academy`, a club short of the floor first promotes players of its own
/// academy side (imported, never on loan), best rated first, so that a real player is preferred to a
/// generated one. Returns the number of generated players removed and the promoted players.
pub(crate) fn trim_generated_players(
    world: &mut WorldData,
    promote_academy: bool,
    journal: &mut Journal,
) -> (usize, Vec<String>) {
    use ofm_core::squad_floor::{MIN_PLAYERS_PER_GROUP, MIN_SENIOR_PLAYERS};
    let before = world.players.len();
    let team_ids: Vec<String> = world.teams.iter().map(|t| t.id.clone()).collect();
    let mut keep_generated: BTreeSet<String> = BTreeSet::new();
    let mut promoted: Vec<String> = Vec::new();
    for team_id in &team_ids {
        let members: Vec<&domain::player::Player> = world
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team_id.as_str()))
            .collect();
        let group_of = |p: &domain::player::Player| p.position.to_group_position();
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for p in members
            .iter()
            .filter(|p| is_imported(&p.id) && p.squad_role == SquadRole::Senior)
        {
            *counts.entry(group_index(&group_of(p))).or_default() += 1;
        }
        if promote_academy {
            let mut academy: Vec<&domain::player::Player> = members
                .iter()
                .copied()
                .filter(|p| {
                    is_imported(&p.id)
                        && p.squad_role == SquadRole::Youth
                        && p.active_loan.is_none()
                })
                .collect();
            academy.sort_by(|a, b| b.ovr.cmp(&a.ovr).then(a.id.cmp(&b.id)));
            let mut here: Vec<String> = Vec::new();
            for (index, (group, floor)) in MIN_PLAYERS_PER_GROUP.iter().enumerate() {
                let mut need = floor.saturating_sub(counts.get(&index).copied().unwrap_or(0));
                for p in academy.iter().filter(|p| &group_of(p) == group) {
                    if need == 0 {
                        break;
                    }
                    if here.contains(&p.id) {
                        continue;
                    }
                    here.push(p.id.clone());
                    *counts.entry(index).or_default() += 1;
                    need -= 1;
                }
            }
            let mut extra = MIN_SENIOR_PLAYERS.saturating_sub(counts.values().sum());
            for p in academy.iter() {
                if extra == 0 {
                    break;
                }
                if !here.contains(&p.id) {
                    here.push(p.id.clone());
                    *counts.entry(group_index(&group_of(p))).or_default() += 1;
                    extra -= 1;
                }
            }
            if !here.is_empty() {
                journal.warn(
                    "squad.academy_promoted",
                    team_id,
                    format!(
                        "{} academy player(s) of the save's reserve/youth sides promoted to the senior squad to reach OFM's floor: {}",
                        here.len(),
                        here.join(" ")
                    ),
                );
                promoted.extend(here);
            }
        }
        let mut chosen: Vec<&domain::player::Player> = Vec::new();
        // Generated players only. OFM makes some of them academy players; one needed to reach the
        // floor is promoted to the senior squad (seniors are preferred, then best rated, then id).
        let mut pool: Vec<&domain::player::Player> = members
            .iter()
            .copied()
            .filter(|p| !is_imported(&p.id))
            .collect();
        pool.sort_by(|a, b| {
            (b.squad_role == SquadRole::Senior)
                .cmp(&(a.squad_role == SquadRole::Senior))
                .then(b.ovr.cmp(&a.ovr))
                .then(a.id.cmp(&b.id))
        });
        for (index, (group, floor)) in MIN_PLAYERS_PER_GROUP.iter().enumerate() {
            let have = counts.get(&index).copied().unwrap_or(0);
            let mut need = floor.saturating_sub(have);
            for p in pool.iter().filter(|p| &group_of(p) == group) {
                if need == 0 {
                    break;
                }
                chosen.push(p);
                *counts.entry(index).or_default() += 1;
                need -= 1;
            }
        }
        let have_total: usize = counts.values().sum();
        let mut extra = MIN_SENIOR_PLAYERS.saturating_sub(have_total);
        for p in pool.iter() {
            if extra == 0 {
                break;
            }
            if !chosen.iter().any(|c| c.id == p.id) {
                chosen.push(p);
                extra -= 1;
            }
        }
        keep_generated.extend(chosen.into_iter().map(|p| p.id.clone()));
    }
    world
        .players
        .retain(|p| is_imported(&p.id) || keep_generated.contains(&p.id));
    for p in world.players.iter_mut().filter(|p| !is_imported(&p.id)) {
        p.squad_role = SquadRole::Senior;
    }
    let promoted_set: BTreeSet<&str> = promoted.iter().map(String::as_str).collect();
    for p in world
        .players
        .iter_mut()
        .filter(|p| promoted_set.contains(p.id.as_str()))
    {
        p.squad_role = SquadRole::Senior;
    }
    promoted.sort();
    (before - world.players.len(), promoted)
}

/// A filler OFM generated is priced by OFM's own formula, a few million for a middling player, on a
/// world whose ordinary players are worth a few hundred thousand. Once he is calibrated to his
/// division he is valued on the world's own scale, like the players around him.
pub(crate) fn reprice_generated_players(
    world: &mut WorldData,
    scale: &ofm_core::value_scale::ValueScale,
    on: chrono::NaiveDate,
) {
    if !scale.is_calibrated() {
        return;
    }
    for player in world.players.iter_mut().filter(|p| !is_imported(&p.id)) {
        player.market_value = scale.value(ofm_core::value_scale::Standing {
            ovr: player.ovr,
            potential: player.potential.max(player.ovr),
            age: domain::age::age_on_or_unknown(on, &player.date_of_birth),
        });
    }
}

/// The level and pay of OFM's generated fillers, brought down to the division they play in.
///
/// OFM generates fillers on its own scale: about 66 rated and 11 000 a week, in a club whose real
/// players are about 56 rated and paid 700 to 2 500. A National side with ten of them is the best
/// team of its division by ten points (it was promoted twice in a row, to the top flight), and a
/// club whose staff and fillers cost twenty times its wage budget is sacked in nine weeks.
/// A filler above the median rating of the imported senior players of its division is scaled
/// down to that median (all attributes by one factor, found against OFM's own rating), and its
/// wage is capped at the division's median wage; its market value follows the imported players of
/// the division at that rating. A filler already at or under the median is left as OFM made it.
/// Imported players are never touched.
pub(crate) fn calibrate_generated_players(
    world: &mut WorldData,
    clubs: &[ClubRow],
    journal: &mut Journal,
) {
    let division_of: BTreeMap<String, i64> = clubs
        .iter()
        .map(|c| (ids::club(c.id), c.competition_id))
        .collect();
    let mut references: BTreeMap<i64, Vec<(u8, u32, u64)>> = BTreeMap::new();
    for p in world
        .players
        .iter()
        .filter(|p| is_imported(&p.id) && p.squad_role == SquadRole::Senior)
    {
        if let Some(division) = p.team_id.as_ref().and_then(|t| division_of.get(t)) {
            references
                .entry(*division)
                .or_default()
                .push((p.ovr, p.wage(), p.market_value));
        }
    }
    let median = |mut values: Vec<u64>| -> u64 {
        values.sort_unstable();
        values.get(values.len() / 2).copied().unwrap_or(0)
    };
    let mut changed: BTreeMap<String, usize> = BTreeMap::new();
    for p in world.players.iter_mut().filter(|p| !is_imported(&p.id)) {
        let Some(team) = p.team_id.clone() else {
            continue;
        };
        let Some(reference) = division_of.get(&team).and_then(|d| references.get(d)) else {
            continue;
        };
        if reference.len() < 5 {
            continue;
        }
        let target_ovr = median(reference.iter().map(|r| u64::from(r.0)).collect()) as u8;
        let target_wage = median(reference.iter().map(|r| u64::from(r.1)).collect()) as u32;
        let mut touched = false;
        if p.ovr > target_ovr {
            let position = p.position.clone();
            let original = p.attributes.clone();
            let (mut low, mut high) = (0.2_f64, 1.0_f64);
            for _ in 0..24 {
                let mid = (low + high) / 2.0;
                if f64::from(attributes::ovr(
                    &scale_attributes(&original, mid),
                    &position,
                )) <= f64::from(target_ovr)
                {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            p.attributes = scale_attributes(&original, low);
            let headroom = p.potential.saturating_sub(p.ovr);
            p.ovr = attributes::ovr(&p.attributes, &position);
            p.potential = p.ovr.saturating_add(headroom).min(99);
            let near: Vec<u64> = reference
                .iter()
                .filter(|r| r.0.abs_diff(p.ovr) <= 3)
                .map(|r| r.2)
                .collect();
            let all: Vec<u64> = reference.iter().map(|r| r.2).collect();
            p.market_value = median(if near.is_empty() { all } else { near });
            touched = true;
        }
        if p.wage() > target_wage {
            p.stage_wage(target_wage);
            touched = true;
        }
        if touched {
            *changed.entry(team).or_default() += 1;
        }
    }
    for (team, n) in changed {
        journal.warn(
            "squad.generated_calibrated",
            &team,
            format!("{n} generated player(s) brought down to the level and pay of the division's imported players"),
        );
    }
}

/// Every attribute multiplied by `factor`, kept in 1..=99.
pub(crate) fn scale_attributes(
    a: &domain::player::PlayerAttributes,
    factor: f64,
) -> domain::player::PlayerAttributes {
    let f = |v: u8| ((f64::from(v) * factor).round() as i64).clamp(1, 99) as u8;
    domain::player::PlayerAttributes {
        pace: f(a.pace),
        stamina: f(a.stamina),
        strength: f(a.strength),
        agility: f(a.agility),
        passing: f(a.passing),
        shooting: f(a.shooting),
        tackling: f(a.tackling),
        dribbling: f(a.dribbling),
        defending: f(a.defending),
        positioning: f(a.positioning),
        vision: f(a.vision),
        decisions: f(a.decisions),
        composure: f(a.composure),
        aggression: f(a.aggression),
        teamwork: f(a.teamwork),
        leadership: f(a.leadership),
        handling: f(a.handling),
        reflexes: f(a.reflexes),
        aerial: f(a.aerial),
    }
}

fn group_index(group: &Position) -> usize {
    ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
        .iter()
        .position(|(g, _)| g == group)
        .unwrap_or(0)
}
