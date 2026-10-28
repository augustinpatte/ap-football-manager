//! The 12 domestic leagues and the 6 national cups as OFM competition definitions.
//!
//! Each league lists its member clubs explicitly (from the save's own 2026/27 membership), and
//! OFM's native promotion/relegation ladder chains the tiers of a country by `priority`
//! (lower = higher division). The sporting rules (tiebreakers, promotion places, play-offs,
//! barrages, reserve sides) are authored in [`crate::rules`], each with its provenance. The cups
//! are fed by the leagues: a club is entered because it is registered with one of their
//! divisions. European competitions are not part of this world (no verified European rules --
//! see docs/FM24_IMPORT_IMPLEMENTATION.md).

use crate::Result;
use crate::ids;
use crate::journal::Journal;
use crate::refs::Refs;
use crate::rules;
use crate::source::CompetitionRow;
use domain::competition_rules::ReserveSide;
use ofm_core::generator::CompetitionDefinition;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// What the rules need to know about the clubs.
pub struct RuleContext<'a> {
    /// `false`: leagues only, with none of the authored rules.
    pub authored: bool,
    /// `(reserve side, parent)` club ids.
    pub reserve_pairs: &'a [(i64, i64)],
    /// Club id -> reputation, to seed the first edition of each cup.
    pub reputation: &'a BTreeMap<i64, i64>,
}

/// Builds the league and cup definitions. `members` maps competition id -> club ids (sorted).
pub fn build(
    competitions: &[CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    refs: &Refs,
    journal: &mut Journal,
    context: &RuleContext<'_>,
) -> Result<Vec<CompetitionDefinition>> {
    let mut definitions = Vec::new();
    for competition in competitions {
        let clubs = members
            .get(&competition.id)
            .ok_or_else(|| format!("competition {} has no members", competition.id))?;
        let nation = competition
            .nation
            .as_deref()
            .ok_or_else(|| format!("competition {} has no nation", competition.id))?;
        let country = refs
            .nation_code(nation)
            .ok_or_else(|| format!("competition {}: unknown nation {nation}", competition.id))?;
        // The ladder is ordered by `priority`; FM level 1 is the top division, which is 0 in OFM.
        let level = competition
            .level
            .ok_or_else(|| format!("competition {} has no pyramid level", competition.id))?;
        let id = ids::competition(competition.id);
        let mut value = json!({
            "id": id,
            "name": display_name(&competition.name),
            "type": "League",
            "scope": "Domestic",
            "countryId": country,
            "priority": level - 1,
            "format": { "kind": "LeagueTable", "legs": 2 },
            "participants": { "explicit": clubs.iter().map(|c| ids::club(*c)).collect::<Vec<_>>() },
        });
        match competition.season_start.as_deref().and_then(month_day) {
            Some((month, day)) => {
                value["seasonStartMonth"] = json!(month);
                value["seasonStartDay"] = json!(day);
            }
            None => journal.info(
                "competition.season_start_default",
                &id,
                format!("{}: no confirmed season start in the save's rules; OFM's default (1 August) is kept", competition.name),
            ),
        }
        let mut definition: CompetitionDefinition = serde_json::from_value(value)
            .map_err(|e| format!("competition {} definition: {e}", competition.id))?;
        if context.authored {
            let country_clubs = clubs_of_country(competitions, members, refs, &country);
            definition.rules = Some(rules::league_rules(
                competition.target_key.as_deref(),
                competition.tiebreak.as_deref(),
                reserve_sides(context, &country_clubs),
            ));
        }
        definitions.push(definition);
    }
    if context.authored {
        definitions.extend(build_cups(competitions, members, refs, journal, context)?);
    }
    Ok(definitions)
}

/// The clubs of every league of `country` in the world.
fn clubs_of_country(
    competitions: &[CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    refs: &Refs,
    country: &str,
) -> BTreeSet<i64> {
    competitions
        .iter()
        .filter(|row| {
            row.nation
                .as_deref()
                .and_then(|nation| refs.nation_code(nation))
                .is_some_and(|code| code == country)
        })
        .flat_map(|row| members.get(&row.id).into_iter().flatten().copied())
        .collect()
}

/// The reserve-side links whose side belongs to `clubs`.
fn reserve_sides(context: &RuleContext<'_>, clubs: &BTreeSet<i64>) -> Vec<ReserveSide> {
    context
        .reserve_pairs
        .iter()
        .filter(|(side, _)| clubs.contains(side))
        .map(|(side, parent)| ReserveSide {
            team_id: ids::club(*side),
            parent_id: ids::club(*parent),
        })
        .collect()
}

fn build_cups(
    competitions: &[CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    refs: &Refs,
    journal: &mut Journal,
    context: &RuleContext<'_>,
) -> Result<Vec<CompetitionDefinition>> {
    let mut definitions = Vec::new();
    for (index, cup) in rules::CUPS.iter().enumerate() {
        let feeders: Vec<&CompetitionRow> = cup
            .feeders
            .iter()
            .filter_map(|key| {
                competitions
                    .iter()
                    .find(|row| row.target_key.as_deref() == Some(key))
            })
            .collect();
        if feeders.len() != cup.feeders.len() {
            journal.info(
                "cup.skipped",
                cup.key,
                format!(
                    "{}: a feeding division is not in the world; the cup is not created",
                    cup.name
                ),
            );
            continue;
        }
        let nation = feeders[0]
            .nation
            .as_deref()
            .ok_or_else(|| format!("cup {}: no nation", cup.key))?;
        let country = refs
            .nation_code(nation)
            .ok_or_else(|| format!("cup {}: unknown nation {nation}", cup.key))?;
        let clubs = clubs_of_country(competitions, members, refs, &country);
        let reserves = reserve_sides(context, &clubs);
        let barred: BTreeSet<String> = if cup.exclude_reserve_sides {
            reserves.iter().map(|side| side.team_id.clone()).collect()
        } else {
            BTreeSet::new()
        };
        // First edition's seeding: division first, then reputation, then id.
        let mut entrants: Vec<(usize, i64, i64)> = Vec::new();
        for (tier, feeder) in feeders.iter().enumerate() {
            for club in members.get(&feeder.id).into_iter().flatten() {
                if barred.contains(&ids::club(*club)) {
                    continue;
                }
                let reputation = context.reputation.get(club).copied().unwrap_or(0);
                entrants.push((tier, reputation, *club));
            }
        }
        entrants.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let value = json!({
            "id": ids::cup(cup.key),
            "name": cup.name,
            "type": "Cup",
            "scope": "Domestic",
            "countryId": country,
            "priority": 10 + index as u32,
            "format": { "kind": "Knockout" },
            "participants": { "explicit": entrants.iter().map(|(_, _, club)| ids::club(*club)).collect::<Vec<_>>() },
            "seasonStartMonth": cup.start.0,
            "seasonStartDay": cup.start.1,
        });
        let mut definition: CompetitionDefinition = serde_json::from_value(value)
            .map_err(|e| format!("cup {} definition: {e}", cup.key))?;
        definition.rules = Some(rules::cup_rules(
            cup,
            feeders.iter().map(|row| ids::competition(row.id)).collect(),
            reserves,
        ));
        definitions.push(definition);
    }
    Ok(definitions)
}

/// FM labels the French third tier "National (Ligue 3 en 2026/27)": the parenthesis is a note for
/// the importer's authors, not a display name.
fn display_name(name: &str) -> String {
    match name.find(" (") {
        Some(index) => format!("{} (Ligue 3)", &name[..index]),
        None => name.to_string(),
    }
}

fn month_day(iso: &str) -> Option<(u8, u8)> {
    let date = chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d").ok()?;
    use chrono::Datelike;
    Some((date.month() as u8, date.day() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_and_day_come_from_the_confirmed_rule() {
        assert_eq!(month_day("2026-08-21"), Some((8, 21)));
        assert_eq!(month_day("nope"), None);
    }

    #[test]
    fn the_french_third_tier_gets_a_display_name() {
        assert_eq!(
            display_name("National (Ligue 3 en 2026/27)"),
            "National (Ligue 3)"
        );
        assert_eq!(display_name("Ligue 1"), "Ligue 1");
    }

    #[test]
    fn a_league_is_a_domestic_double_round_robin_with_explicit_members() {
        let refs = Refs::embedded().unwrap();
        let row = CompetitionRow {
            id: 12,
            fm_uid: Some(16),
            name: "Ligue 1".into(),
            nation: Some("France".into()),
            playable: true,
            simulated: false,
            level: Some(1),
            target_key: Some("fr_ligue1".into()),
            season_start: Some("2026-08-21".into()),
            tiebreak: Some("pts>gd>gf".into()),
        };
        let members = BTreeMap::from([(12, vec![464, 498])]);
        let mut journal = Journal::default();
        let context = RuleContext {
            authored: true,
            reserve_pairs: &[],
            reputation: &BTreeMap::new(),
        };
        let defs = build(&[row], &members, &refs, &mut journal, &context).unwrap();
        assert_eq!(defs.len(), 1);
        let json = serde_json::to_value(&defs[0]).unwrap();
        assert_eq!(json["id"], "fm-k-12");
        assert_eq!(json["countryId"], "FR");
        assert_eq!(json["priority"], 0);
        assert_eq!(json["participants"]["explicit"][1], "fm-c-498");
        assert_eq!(json["seasonStartMonth"], 8);
    }
}
