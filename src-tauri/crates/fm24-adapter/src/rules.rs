//! The sporting rules of the 12 leagues and the 6 national cups, as authored data.
//!
//! Where a rule comes from, and how far it can be trusted, is part of the data: every boundary
//! between two divisions carries a [`RuleStatus`] and a note. The importer's referential
//! (`competition_rule` in world.db, read from public pages) supplies the table tiebreakers and the
//! confirmed counts; where it is silent or contradicts the known structure of a competition, the
//! value here is marked `Assumed` and says why. Nothing here is read from the FM24 save: the save
//! carries neither promotion rules nor cups.
//!
//! The values apply from the 2026/27 season onward. A later season that changes a rule adds an
//! entry with its own `from_season` to the table; nothing else has to change.

use crate::source::PlayerRow;
use domain::competition_rules::{
    Barrage, BoundaryRule, CupRules, DrawMode, LadderRules, MonthDay, PromotionPlayoff,
    ReserveSide, RuleStatus, SeasonBoundary, Tiebreaker,
};
use ofm_core::generator::AuthoredRules;
use std::collections::{BTreeMap, BTreeSet};

/// Players of one owner registered with another club of the world for that club to be read as the
/// owner's reserve side. Real B teams hold 16 to 18 of their parent's players in the FM24 save;
/// the next highest pair in the whole perimeter holds fewer than four.
pub const RESERVE_MIN_REGISTERED: usize = 7;

/// A club that is the reserve side of another club of the world, found from the save's player
/// registrations: `(side, parent)` club ids. A club's name is never enough: a side needs
/// `RESERVE_MIN_REGISTERED` of its parent's players registered with it, or three together with
/// the names agreeing. Each side has one parent, the club with the most players registered there.
pub fn reserve_pairs(rows: &[PlayerRow], perimeter: &BTreeSet<i64>) -> Vec<(i64, i64)> {
    let mut counts: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    let mut names: BTreeMap<(i64, i64), (&str, &str)> = BTreeMap::new();
    for row in rows {
        let Some(side) = row.registered_club_id else {
            continue;
        };
        if side == row.club_id || !perimeter.contains(&side) || !perimeter.contains(&row.club_id) {
            continue;
        }
        *counts.entry((side, row.club_id)).or_default() += 1;
        names.insert(
            (side, row.club_id),
            (
                row.registered_club_name.as_deref().unwrap_or(""),
                row.club_name.as_str(),
            ),
        );
    }
    let mut best: BTreeMap<i64, (usize, i64)> = BTreeMap::new();
    for (&(side, parent), &count) in &counts {
        let (side_name, parent_name) = names[&(side, parent)];
        let enough = count >= RESERVE_MIN_REGISTERED
            || (count >= 3 && crate::placement::name_suggests_affiliate(parent_name, side_name));
        if enough && best.get(&side).is_none_or(|(most, _)| count > *most) {
            best.insert(side, (count, parent));
        }
    }
    best.into_iter()
        .map(|(side, (_, parent))| (side, parent))
        .collect()
}

/// First season (the year it starts) the authored rules apply to.
pub const FROM_SEASON: u32 = 2026;

/// A promotion play-off: ranks `from..=to` of the lower division, legs of the rounds / the final.
type Playoff = (u32, u32, u8, u8);
/// A barrage: rank of the upper club, ranks `from..=to` of the lower division, legs of the lower
/// bracket / of the tie against the upper club.
type BarrageSpec = (u32, u32, u32, u8, u8);

struct Boundary {
    upper: &'static str,
    promoted_direct: u32,
    playoff: Option<Playoff>,
    relegated_direct: u32,
    barrage: Option<BarrageSpec>,
    status: RuleStatus,
    note: &'static str,
}

/// The boundary below each division. A division missing here is the lowest of its country in the
/// world: nothing is relegated from it, and nothing is promoted into it from outside.
const BOUNDARIES: &[Boundary] = &[
    Boundary {
        upper: "en_pl",
        promoted_direct: 2,
        playoff: Some((3, 6, 2, 1)),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Unverified,
        note: "Relegation (3) confirmed by the referential. The Championship's two direct places and its 3rd-6th play-off are the EFL's rule; the page read did not state them (promotion_direct and promotion_playoff are missing), so they are assumed. Semi-finals over two legs, one-leg final.",
    },
    Boundary {
        upper: "en_champ",
        promoted_direct: 2,
        playoff: Some((3, 6, 2, 1)),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Unverified,
        note: "Relegation (3) confirmed by the referential. League One's two direct places and its 3rd-6th play-off are the EFL's rule, not stated by the page read: assumed.",
    },
    Boundary {
        upper: "fr_ligue1",
        promoted_direct: 2,
        playoff: None,
        relegated_direct: 2,
        barrage: Some((16, 3, 5, 1, 2)),
        status: RuleStatus::Unverified,
        note: "Two direct relegations and a 16th-place barrage confirmed by the referential. Ligue 2's 3rd-5th bracket (the page labels the 3rd and 4th) feeds the barrage; its format is not cited, so single-leg bracket and two-leg barrage are assumed.",
    },
    Boundary {
        upper: "fr_ligue2",
        promoted_direct: 2,
        playoff: None,
        relegated_direct: 2,
        barrage: Some((16, 3, 3, 1, 2)),
        status: RuleStatus::Assumed,
        note: "Ligue 2's side is in the referential (17th direct, 16th barrage; the 18th is not labelled but is below the 17th). Ligue 3 (National) has no rules in the referential: its 1st and 2nd are assumed promoted and its 3rd plays the 16th, mirroring Ligue 1 / Ligue 2.",
    },
    Boundary {
        upper: "es_laliga",
        promoted_direct: 2,
        playoff: Some((3, 6, 2, 2)),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Unverified,
        note: "The referential's LaLiga relegation is missing and its reading (19th-20th) contradicts the known three; three is assumed. LaLiga 2's 3rd-6th play-off (4 clubs) is confirmed; two-leg semi-finals and final assumed. Reserve sides take no part in promotion or in the play-off.",
    },
    Boundary {
        upper: "it_seriea",
        promoted_direct: 2,
        playoff: Some((3, 8, 1, 2)),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Unverified,
        note: "Serie A relegation (3) is unverified in the referential. Serie B's 3rd-8th play-off is the known format, simplified to a seeded bracket (one-leg rounds, two-leg final); the real preliminary rounds and the gap rule between 3rd and 4th are not reproduced.",
    },
    Boundary {
        upper: "de_bundesliga",
        promoted_direct: 2,
        playoff: None,
        relegated_direct: 2,
        barrage: Some((16, 3, 3, 1, 2)),
        status: RuleStatus::Assumed,
        note: "The referential gives one direct relegation and a 16th-place barrage. With 18 clubs the 17th and 18th go down directly: two are assumed, which the referential's reading (only the 18th labelled) does not state. The barrage is the 16th against the 2. Bundesliga's 3rd over two legs.",
    },
];

fn boundary_rule(spec: &Boundary) -> BoundaryRule {
    BoundaryRule {
        promoted_direct: spec.promoted_direct,
        promotion_playoff: spec
            .playoff
            .map(
                |(from_rank, to_rank, round_legs, final_legs)| PromotionPlayoff {
                    from_rank,
                    to_rank,
                    round_legs,
                    final_legs,
                },
            ),
        relegated_direct: spec.relegated_direct,
        barrage: spec.barrage.map(
            |(upper_rank, lower_from_rank, lower_to_rank, bracket_legs, tie_legs)| Barrage {
                upper_rank,
                lower_from_rank,
                lower_to_rank,
                bracket_legs,
                tie_legs,
            },
        ),
        status: spec.status,
        note: spec.note.to_string(),
    }
}

/// The rule below the division with this referential key, if it has a division below in the world.
pub fn boundary_below(target_key: &str) -> Option<BoundaryRule> {
    BOUNDARIES
        .iter()
        .find(|spec| spec.upper == target_key)
        .map(boundary_rule)
}

/// Every authored boundary, for the rules document and the consistency check.
pub fn all_boundaries() -> Vec<(&'static str, BoundaryRule)> {
    BOUNDARIES
        .iter()
        .map(|spec| (spec.upper, boundary_rule(spec)))
        .collect()
}

/// Parses the referential's tiebreak string (`pts>gd>gf>h2h>wins>away_gf>discipline`).
///
/// `fairplay` and `discipline` are dropped: the game keeps no per-club card record to read them
/// from. A draw of lots closes every list, so a tie nothing breaks is settled the same way on
/// every reload rather than by the order the clubs happen to be listed in.
pub fn parse_tiebreakers(text: Option<&str>) -> (Vec<Tiebreaker>, Vec<String>) {
    let mut list = Vec::new();
    let mut dropped = Vec::new();
    let source = text
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("pts>gd>gf");
    for token in source.split('>').map(str::trim) {
        match token {
            "pts" => list.push(Tiebreaker::Points),
            "gd" => list.push(Tiebreaker::GoalDifference),
            "gf" => list.push(Tiebreaker::GoalsFor),
            "wins" => list.push(Tiebreaker::Wins),
            "away_wins" => list.push(Tiebreaker::AwayWins),
            "away_gf" => list.push(Tiebreaker::AwayGoalsFor),
            "h2h" => {
                list.push(Tiebreaker::HeadToHeadPoints);
                list.push(Tiebreaker::HeadToHeadGoalDifference);
            }
            "h2h_pts" => list.push(Tiebreaker::HeadToHeadPoints),
            "h2h_gd" => list.push(Tiebreaker::HeadToHeadGoalDifference),
            "h2h_gf" => list.push(Tiebreaker::HeadToHeadGoalsFor),
            "h2h_away_gf" => list.push(Tiebreaker::HeadToHeadAwayGoals),
            "lots" => list.push(Tiebreaker::Lots),
            other => dropped.push(other.to_string()),
        }
    }
    if !list.contains(&Tiebreaker::Lots) {
        list.push(Tiebreaker::Lots);
    }
    (list, dropped)
}

/// A national cup.
pub struct Cup {
    pub key: &'static str,
    pub name: &'static str,
    /// Referential keys of the divisions whose clubs play it, top division first.
    pub feeders: &'static [&'static str],
    pub exclude_reserve_sides: bool,
    /// Month and day of the first round.
    pub start: (u8, u8),
    /// Rounds counted back from the final: final, semi-finals, quarter-finals, round of 16, ...
    pub round_dates: &'static [(u8, u8)],
    pub note: &'static str,
}

pub const CUPS: &[Cup] = &[
    Cup {
        key: "fr_coupe",
        name: "Coupe de France",
        feeders: &["fr_ligue1", "fr_ligue2", "fr_ligue3"],
        exclude_reserve_sides: false,
        start: (11, 18),
        round_dates: &[(5, 23), (4, 7), (2, 3), (1, 6), (12, 9)],
        note: "Reduced format: the 54 clubs of Ligue 1, Ligue 2 and Ligue 3 only (the real cup has about 7,000 entrants and the amateur rounds are not in the world). One-leg ties, seeded byes in the first round, then a draw every round.",
    },
    Cup {
        key: "en_facup",
        name: "FA Cup",
        feeders: &["en_pl", "en_champ", "en_l1"],
        exclude_reserve_sides: false,
        start: (11, 7),
        round_dates: &[(5, 15), (4, 24), (3, 13), (2, 20), (1, 30), (1, 9)],
        note: "Reduced format: the 68 clubs of the Premier League, Championship and League One only (League Two and non-league clubs are not in the world). No replays: a level tie goes to extra time and penalties. Seeded byes in the first round, then a draw every round.",
    },
    Cup {
        key: "en_leaguecup",
        name: "EFL Cup",
        feeders: &["en_pl", "en_champ", "en_l1"],
        exclude_reserve_sides: false,
        start: (8, 11),
        round_dates: &[(2, 28), (1, 26), (12, 15), (10, 27), (9, 22), (8, 25)],
        note: "Reduced format: the same 68 clubs as the FA Cup (League Two is not in the world). The semi-finals are one leg, not two. No European-qualification exemptions.",
    },
    Cup {
        key: "es_copa",
        name: "Copa del Rey",
        feeders: &["es_laliga", "es_laliga2"],
        exclude_reserve_sides: true,
        start: (11, 25),
        round_dates: &[(4, 24), (3, 10), (2, 3), (1, 13), (12, 16)],
        note: "Reduced format: LaLiga and LaLiga 2 clubs only, reserve sides excluded (as in the real cup): 39 clubs. One-leg ties throughout (the real semi-finals are two legs). The lower-division-hosts rule of the early rounds is not reproduced.",
    },
    Cup {
        key: "it_coppa",
        name: "Coppa Italia",
        feeders: &["it_seriea", "it_serieb"],
        exclude_reserve_sides: false,
        start: (8, 9),
        round_dates: &[(5, 19), (4, 21), (2, 3), (12, 16), (10, 28)],
        note: "Reduced format: the 40 clubs of Serie A and Serie B only (Serie C and amateur clubs are not in the world). The real cup seeds the top eight of Serie A into the round of 16; here the byes of the first round go to the best-seeded clubs instead. One-leg ties, final included.",
    },
    Cup {
        key: "de_pokal",
        name: "DFB-Pokal",
        feeders: &["de_bundesliga", "de_bundesliga2"],
        exclude_reserve_sides: true,
        start: (8, 14),
        round_dates: &[(5, 22), (4, 21), (2, 24), (12, 15), (10, 27)],
        note: "Reduced format: the 36 clubs of the Bundesliga and 2. Bundesliga only (3. Liga and amateur clubs are not in the world), reserve sides excluded. The real first round has 64 clubs. The final falls after the Bundesliga's last day: the season waits for it.",
    },
];

/// The rules a cup carries, from its table row.
pub fn cup_rules(
    cup: &Cup,
    feeder_ids: Vec<String>,
    reserve_sides: Vec<ReserveSide>,
) -> AuthoredRules {
    AuthoredRules {
        tiebreakers: Vec::new(),
        ladder: LadderRules::default(),
        reserve_sides,
        cup: CupRules {
            draw: DrawMode::Random,
            feeder_ids,
            exclude_reserve_sides: cup.exclude_reserve_sides,
            round_dates: cup
                .round_dates
                .iter()
                .map(|&(month, day)| MonthDay { month, day })
                .collect(),
            draw_salt: 0,
            note: cup.note.to_string(),
        },
        // All the ties of a round are played on its date.
        knockout_round_gap_days: Some(7),
    }
}

/// A league's authored rules.
pub fn league_rules(
    target_key: Option<&str>,
    tiebreak_text: Option<&str>,
    reserve_sides: Vec<ReserveSide>,
) -> AuthoredRules {
    let (tiebreakers, _) = parse_tiebreakers(tiebreak_text);
    let boundaries = target_key
        .and_then(boundary_below)
        .map(|rule| {
            vec![SeasonBoundary {
                from_season: FROM_SEASON,
                rule,
            }]
        })
        .unwrap_or_default();
    AuthoredRules {
        tiebreakers,
        ladder: LadderRules { boundaries },
        reserve_sides,
        cup: CupRules::default(),
        knockout_round_gap_days: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_authored_boundary_holds_sizes() {
        for (upper, rule) in all_boundaries() {
            assert!(
                rule.is_consistent(),
                "{upper}: the two sides of the boundary differ"
            );
        }
    }

    #[test]
    fn a_referential_string_becomes_an_ordered_list_closed_by_lots() {
        let (list, dropped) = parse_tiebreakers(Some("pts>h2h_pts>h2h_gd>gd>gf>fairplay"));
        assert_eq!(
            list,
            vec![
                Tiebreaker::Points,
                Tiebreaker::HeadToHeadPoints,
                Tiebreaker::HeadToHeadGoalDifference,
                Tiebreaker::GoalDifference,
                Tiebreaker::GoalsFor,
                Tiebreaker::Lots,
            ]
        );
        assert_eq!(dropped, ["fairplay"]);
    }

    #[test]
    fn a_generic_h2h_is_points_then_goal_difference() {
        let (list, _) = parse_tiebreakers(Some("pts>gd>gf>h2h>wins>away_gf>discipline"));
        assert_eq!(list[3], Tiebreaker::HeadToHeadPoints);
        assert_eq!(list[4], Tiebreaker::HeadToHeadGoalDifference);
    }

    #[test]
    fn a_division_with_no_rule_in_the_table_is_the_bottom_of_its_country() {
        assert!(boundary_below("en_l1").is_none());
        assert!(boundary_below("es_laliga2").is_none());
        assert!(boundary_below("it_serieb").is_none());
        assert!(boundary_below("fr_ligue3").is_none());
        assert!(boundary_below("de_bundesliga2").is_none());
    }

    #[test]
    fn every_cup_has_a_date_for_each_late_round_it_will_play() {
        for cup in CUPS {
            assert!(cup.round_dates.len() >= 5, "{}", cup.key);
        }
    }
}
