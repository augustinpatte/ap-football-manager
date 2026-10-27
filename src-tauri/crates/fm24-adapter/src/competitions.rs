//! The 12 domestic leagues as OFM competition definitions.
//!
//! Each league lists its member clubs explicitly (from the save's own 2026/27 membership), and
//! OFM's native promotion/relegation ladder chains the tiers of a country by `priority`
//! (lower = higher division). The FM-specific play-off formats are **not** reproduced: OFM's
//! native rules are kept and the differences are documented. Cups and European competitions are
//! not part of this world (no verified European rules -- see docs/FM24_IMPORT_IMPLEMENTATION.md).

use crate::Result;
use crate::ids;
use crate::journal::Journal;
use crate::refs::Refs;
use crate::source::CompetitionRow;
use ofm_core::generator::CompetitionDefinition;
use serde_json::json;
use std::collections::BTreeMap;

/// Builds the league definitions. `members` maps competition id -> club ids (sorted).
pub fn build(
    competitions: &[CompetitionRow],
    members: &BTreeMap<i64, Vec<i64>>,
    refs: &Refs,
    journal: &mut Journal,
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
        let definition: CompetitionDefinition = serde_json::from_value(value)
            .map_err(|e| format!("competition {} definition: {e}", competition.id))?;
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
        assert_eq!(display_name("National (Ligue 3 en 2026/27)"), "National (Ligue 3)");
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
        };
        let members = BTreeMap::from([(12, vec![464, 498])]);
        let mut journal = Journal::default();
        let defs = build(&[row], &members, &refs, &mut journal).unwrap();
        assert_eq!(defs.len(), 1);
        let json = serde_json::to_value(&defs[0]).unwrap();
        assert_eq!(json["id"], "fm-k-12");
        assert_eq!(json["countryId"], "FR");
        assert_eq!(json["priority"], 0);
        assert_eq!(json["participants"]["explicit"][1], "fm-c-498");
        assert_eq!(json["seasonStartMonth"], 8);
    }
}
