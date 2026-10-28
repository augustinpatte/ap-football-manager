//! Who plays a national cup, read again each time the cup is renewed.
//!
//! A cup with `feeder_ids` is fed by the divisions it names: every club registered with one of
//! them, a reserve side excepted where the cup bars them. The list comes out in seeding order
//! (division first, then reputation), which is the order the bracket gives its byes in: the
//! strongest clubs wait, the weakest play the opening round.

use std::collections::{HashMap, HashSet};

use domain::league::League;
use domain::team::Team;

/// The entrants of `cup` in seeding order, or `None` when the cup is not fed by divisions.
pub fn entrants_from_feeders(
    cup: &League,
    competitions: &[League],
    teams: &[Team],
) -> Option<Vec<String>> {
    if cup.rules.cup.feeder_ids.is_empty() {
        return None;
    }
    let reputation: HashMap<&str, u32> = teams
        .iter()
        .map(|team| (team.id.as_str(), team.reputation))
        .collect();
    let barred: HashSet<&str> = if cup.rules.cup.exclude_reserve_sides {
        cup.rules
            .reserve_sides
            .iter()
            .map(|side| side.team_id.as_str())
            .collect()
    } else {
        HashSet::new()
    };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut entrants: Vec<(usize, u32, &str)> = Vec::new();
    for (tier, feeder_id) in cup.rules.cup.feeder_ids.iter().enumerate() {
        let Some(feeder) = competitions.iter().find(|c| &c.id == feeder_id) else {
            continue;
        };
        for club in &feeder.participant_ids {
            if barred.contains(club.as_str()) || !seen.insert(club.as_str()) {
                continue;
            }
            let standing = reputation.get(club.as_str()).copied().unwrap_or(0);
            entrants.push((tier, standing, club.as_str()));
        }
    }
    entrants.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(b.2)));
    Some(entrants.into_iter().map(|(_, _, club)| club.to_string()).collect())
}

/// For every cup fed by divisions, the entrants of its next edition, keyed by the cup's id.
pub fn refreshed_entrants(
    competitions: &[League],
    teams: &[Team],
) -> HashMap<String, Vec<String>> {
    competitions
        .iter()
        .filter_map(|cup| {
            entrants_from_feeders(cup, competitions, teams).map(|list| (cup.id.clone(), list))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::competition_rules::ReserveSide;

    fn team(id: &str, reputation: u32) -> Team {
        let mut team = Team::new(
            id.to_string(),
            id.to_string(),
            id.to_uppercase(),
            "Spain".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            10_000,
        );
        team.reputation = reputation;
        team
    }

    fn division(id: &str, clubs: &[&str]) -> League {
        let ids: Vec<String> = clubs.iter().map(|c| c.to_string()).collect();
        League::new(id.to_string(), id.to_string(), 2026, &ids)
    }

    #[test]
    fn entrants_are_seeded_by_division_then_reputation_and_reserves_are_barred() {
        let top = division("top", &["rm", "ath"]);
        let second = division("second", &["castilla", "zar", "cad"]);
        let mut cup = League::new("copa".into(), "Copa".into(), 2026, &[]);
        cup.rules.cup.feeder_ids = vec!["top".into(), "second".into()];
        cup.rules.cup.exclude_reserve_sides = true;
        cup.rules.reserve_sides = vec![ReserveSide {
            team_id: "castilla".into(),
            parent_id: "rm".into(),
        }];
        let teams = vec![
            team("rm", 100),
            team("ath", 80),
            team("castilla", 60),
            team("zar", 50),
            team("cad", 55),
        ];
        let list = entrants_from_feeders(&cup, &[top, second, cup.clone()], &teams).unwrap();
        assert_eq!(list, ["rm", "ath", "cad", "zar"]);
    }

    #[test]
    fn a_club_that_moved_division_is_still_entered_once() {
        let mut top = division("top", &["a", "b"]);
        let mut cup = League::new("cup".into(), "Cup".into(), 2026, &[]);
        cup.rules.cup.feeder_ids = vec!["top".into(), "second".into()];
        // c was promoted; d arrived from above; nobody is listed twice.
        top.participant_ids = vec!["a".into(), "c".into()];
        let teams = vec![team("a", 5), team("b", 5), team("c", 9), team("d", 5)];
        let second_now = division("second", &["b", "d"]);
        let list = entrants_from_feeders(&cup, &[top, second_now], &teams).unwrap();
        assert_eq!(list, ["c", "a", "b", "d"]);
    }

    #[test]
    fn a_cup_with_no_feeders_keeps_its_field() {
        let cup = League::new("cup".into(), "Cup".into(), 2026, &["a".into()]);
        assert!(entrants_from_feeders(&cup, &[], &[]).is_none());
    }
}
