use super::*;
use domain::competition_rules::{
    Barrage, BoundaryRule, LadderRules, PromotionPlayoff, ReserveSide, RuleStatus, SeasonBoundary,
};
use domain::league::{League, StandingEntry};

fn division(id: &str, priority: u32, season: u32, finishing_order: &[&str]) -> League {
    let team_ids: Vec<String> = finishing_order
        .iter()
        .map(|club| club.to_string())
        .collect();
    let mut league = League::new(id.to_string(), id.to_string(), season, &team_ids);
    league.priority = priority;
    league.standings = finishing_order
        .iter()
        .enumerate()
        .map(|(place, club)| {
            let mut entry = StandingEntry::new(club.to_string());
            entry.points = 100 - place as u32;
            entry
        })
        .collect();
    league
}

fn england() -> BoundaryRule {
    BoundaryRule {
        promoted_direct: 2,
        promotion_playoff: Some(PromotionPlayoff {
            from_rank: 3,
            to_rank: 6,
            round_legs: 2,
            final_legs: 1,
        }),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Confirmed,
        note: String::new(),
    }
}

fn france() -> BoundaryRule {
    BoundaryRule {
        promoted_direct: 2,
        promotion_playoff: None,
        relegated_direct: 2,
        barrage: Some(Barrage {
            upper_rank: 5,
            lower_from_rank: 3,
            lower_to_rank: 4,
            bracket_legs: 2,
            tie_legs: 2,
        }),
        status: RuleStatus::Unverified,
        note: String::new(),
    }
}

fn with_rule(mut league: League, rule: BoundaryRule) -> League {
    league.rules.ladder = LadderRules {
        boundaries: vec![SeasonBoundary {
            from_season: 2026,
            rule,
        }],
    };
    league
}

/// The club earlier in `order` (the stronger one) wins every tie, or, for `Underdogs`, the later.
struct Strength {
    order: Vec<&'static str>,
    favourites: bool,
}
fn favourites(order: &[&'static str]) -> Strength {
    Strength {
        order: order.to_vec(),
        favourites: true,
    }
}
fn underdogs(order: &[&'static str]) -> Strength {
    Strength {
        order: order.to_vec(),
        favourites: false,
    }
}
impl TieResolver for Strength {
    fn resolve(&mut self, request: &TieRequest<'_>) -> PlayoffTie {
        let place = |club: &str| {
            self.order
                .iter()
                .position(|c| *c == club)
                .expect("club in the order")
        };
        let home_is_stronger = place(request.home) < place(request.away);
        let winner = if home_is_stronger == self.favourites {
            request.home
        } else {
            request.away
        };
        PlayoffTie {
            stage: request.stage.to_string(),
            home_team_id: request.home.to_string(),
            away_team_id: request.away.to_string(),
            legs: vec![(1, 0); usize::from(request.legs)],
            penalties: None,
            winner_id: winner.to_string(),
        }
    }
}

const ENGLAND_ORDER: [&str; 16] = [
    "l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8",
];

/// Every tie is won by the club named second: the lower seed of a one-leg tie, the better seed of
/// a two-leg one. Shows who reaches the end of a bracket when the underdogs always win.
struct AwayWins;
impl TieResolver for AwayWins {
    fn resolve(&mut self, request: &TieRequest<'_>) -> PlayoffTie {
        PlayoffTie {
            stage: request.stage.to_string(),
            home_team_id: request.home.to_string(),
            away_team_id: request.away.to_string(),
            legs: vec![(0, 1); usize::from(request.legs)],
            penalties: None,
            winner_id: request.away.to_string(),
        }
    }
}

#[test]
fn england_sends_two_up_directly_and_a_third_through_the_playoff() {
    let upper = with_rule(
        division(
            "pl",
            0,
            2026,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    let lower = division(
        "ch",
        1,
        2026,
        &["l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8"],
    );
    let outcome = plan_ruled_ladder(&[upper, lower], &mut favourites(&ENGLAND_ORDER));
    let record = &outcome.movements[0];
    assert_eq!(record.promoted_direct, ["l1", "l2"]);
    assert_eq!(
        record.promoted_playoff,
        ["l3"],
        "the best seed wins when favourites win"
    );
    assert_eq!(record.relegated_direct, ["u8", "u7", "u6"]);
    // 4 entrants: two semi-finals, one final.
    assert_eq!(record.ties.len(), 3);
    assert_eq!(outcome.participants[0].len(), 8);
    assert_eq!(outcome.participants[1].len(), 8);
    assert!(outcome.participants[0].contains(&"l3".to_string()));
    assert!(outcome.participants[1].contains(&"u6".to_string()));
}

#[test]
fn a_playoff_won_by_underdogs_promotes_one_of_them() {
    let upper = with_rule(
        division(
            "pl",
            0,
            2026,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    let lower = division(
        "ch",
        1,
        2026,
        &["l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8"],
    );
    let outcome = plan_ruled_ladder(&[upper, lower], &mut underdogs(&ENGLAND_ORDER));
    let record = &outcome.movements[0];
    // Semi-finals 3 v 6 and 4 v 5: the underdogs win -> l6 and l5; the final between them: l6.
    assert_eq!(record.promoted_playoff, ["l6"]);
}

#[test]
fn a_barrage_exchanges_a_club_only_when_the_lower_side_wins_it() {
    let make = || {
        (
            with_rule(
                division("l1", 0, 2026, &["u1", "u2", "u3", "u4", "u5", "u6", "u7"]),
                france(),
            ),
            division("l2", 1, 2026, &["l1", "l2", "l3", "l4", "l5", "l6", "l7"]),
        )
    };
    let (upper, lower) = make();
    let order = [
        "u1", "u2", "u3", "u4", "u5", "u6", "u7", "l1", "l2", "l3", "l4", "l5", "l6", "l7",
    ];
    let kept = plan_ruled_ladder(&[upper, lower], &mut favourites(&order));
    assert_eq!(
        kept.movements[0].promoted_playoff,
        Vec::<String>::new(),
        "the 5th keeps its place"
    );
    assert_eq!(kept.movements[0].promoted_direct.len(), 2);
    assert_eq!(kept.movements[0].relegated_direct, ["u7", "u6"]);

    let (upper, lower) = make();
    let exchanged = plan_ruled_ladder(&[upper, lower], &mut underdogs(&order));
    let record = &exchanged.movements[0];
    assert_eq!(record.relegated_barrage, ["u5"]);
    assert_eq!(record.promoted_playoff.len(), 1);
    assert_eq!(exchanged.participants[0].len(), 7);
    assert_eq!(exchanged.participants[1].len(), 7);
}

#[test]
fn a_reserve_side_is_never_promoted_to_its_parents_division_and_the_next_club_goes_up() {
    let mut upper = with_rule(
        division(
            "laliga",
            0,
            2026,
            &["rm", "fc", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    let mut lower = division(
        "laliga2",
        1,
        2026,
        &["castilla", "l2", "l3", "l4", "l5", "l6", "l7", "l8"],
    );
    let link = ReserveSide {
        team_id: "castilla".into(),
        parent_id: "rm".into(),
    };
    upper.rules.reserve_sides = vec![link.clone()];
    lower.rules.reserve_sides = vec![link];
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    let record = &outcome.movements[0];
    assert!(!record.promoted().any(|club| club == "castilla"));
    assert_eq!(
        record.promoted_direct,
        ["l2", "l3"],
        "the next two eligible clubs go up"
    );
    assert_eq!(record.reserves_blocked, ["castilla"]);
    assert!(outcome.participants[1].contains(&"castilla".to_string()));
}

#[test]
fn a_reserve_side_is_kept_out_of_the_playoff_places_too() {
    let mut upper = with_rule(
        division(
            "laliga",
            0,
            2026,
            &["rm", "u2", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    let mut lower = division(
        "laliga2",
        1,
        2026,
        &["l1", "l2", "castilla", "l4", "l5", "l6", "l7", "l8"],
    );
    let link = ReserveSide {
        team_id: "castilla".into(),
        parent_id: "rm".into(),
    };
    upper.rules.reserve_sides = vec![link.clone()];
    lower.rules.reserve_sides = vec![link];
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    let record = &outcome.movements[0];
    assert_eq!(record.promoted_direct, ["l1", "l2"]);
    let entrants: HashSet<&str> = record
        .ties
        .iter()
        .flat_map(|tie| [tie.home_team_id.as_str(), tie.away_team_id.as_str()])
        .collect();
    assert!(!entrants.contains("castilla"));
    assert!(entrants.contains("l4") && entrants.contains("l7"));
}

#[test]
fn a_parent_relegated_into_its_reserves_division_withdraws_the_reserve_when_nothing_lies_below() {
    let mut upper = with_rule(
        division(
            "laliga",
            0,
            2026,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "rm"],
        ),
        england(),
    );
    let mut lower = division(
        "laliga2",
        1,
        2026,
        &["l1", "l2", "l3", "l4", "l5", "l6", "castilla", "l8"],
    );
    let link = ReserveSide {
        team_id: "castilla".into(),
        parent_id: "rm".into(),
    };
    upper.rules.reserve_sides = vec![link.clone()];
    lower.rules.reserve_sides = vec![link];
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    assert!(outcome.participants[1].contains(&"rm".to_string()));
    assert!(!outcome.participants[1].contains(&"castilla".to_string()));
    assert_eq!(outcome.withdrawn[1].len(), 1);
    assert_eq!(outcome.withdrawn[1][0].team_id, "castilla");
    // The division is one club short until the reserve side may come back.
    assert_eq!(outcome.participants[1].len(), 7);
    assert_eq!(outcome.movements[0].reserves_forced_down, ["castilla"]);
}

#[test]
fn a_withdrawn_reserve_side_returns_when_its_parent_is_promoted_back() {
    let mut upper = with_rule(
        division(
            "laliga",
            0,
            2027,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    // Real Madrid is in the second division, won it; Castilla is out of it.
    let mut lower = division(
        "laliga2",
        1,
        2027,
        &["rm", "l2", "l3", "l4", "l5", "l6", "l7"],
    );
    let link = ReserveSide {
        team_id: "castilla".into(),
        parent_id: "rm".into(),
    };
    upper.rules.reserve_sides = vec![link.clone()];
    lower.rules.reserve_sides = vec![link];
    lower.rules.withdrawn_sides = vec![WithdrawnSide {
        team_id: "castilla".into(),
        since_season: 2026,
    }];
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    assert!(outcome.participants[0].contains(&"rm".to_string()));
    assert!(outcome.participants[1].contains(&"castilla".to_string()));
    assert!(outcome.withdrawn[1].is_empty());
    assert_eq!(outcome.participants[1].len(), 8);
}

#[test]
fn a_reserve_whose_parent_is_still_in_its_division_stays_withdrawn() {
    let upper = with_rule(
        division(
            "laliga",
            0,
            2027,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8"],
        ),
        england(),
    );
    let mut lower = division(
        "laliga2",
        1,
        2027,
        &["l1", "l2", "l3", "l4", "l5", "l6", "rm"],
    );
    let link = ReserveSide {
        team_id: "castilla".into(),
        parent_id: "rm".into(),
    };
    lower.rules.reserve_sides = vec![link];
    lower.rules.withdrawn_sides = vec![WithdrawnSide {
        team_id: "castilla".into(),
        since_season: 2026,
    }];
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    assert_eq!(outcome.withdrawn[1].len(), 1);
    assert!(!outcome.participants[1].contains(&"castilla".to_string()));
}

#[test]
fn an_inconsistent_rule_falls_back_to_the_generic_swap() {
    let mut broken = england();
    broken.relegated_direct = 5;
    let upper = with_rule(
        division(
            "a",
            0,
            2026,
            &["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8", "u9", "u10"],
        ),
        broken,
    );
    let lower = division(
        "b",
        1,
        2026,
        &["l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "l9", "l10"],
    );
    let outcome = plan_ruled_ladder(&[upper, lower], &mut HigherSeedWins);
    assert_eq!(outcome.participants[0].len(), 10);
    assert_eq!(
        outcome.movements[0].promoted_direct.len(),
        2,
        "10 / 5 clubs swap"
    );
}

/// A pseudo-random finishing order, fixed by the season and the division.
fn shuffled(clubs: &[String], seed: u64) -> Vec<String> {
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut out = clubs.to_vec();
    for index in (1..out.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.swap(index, (state >> 33) as usize % (index + 1));
    }
    out
}

/// Ten seasons of a Spanish-like pyramid with three reserve sides: the sizes hold apart from the
/// withdrawn sides, no club is registered twice, and no reserve is ever level with or above its
/// parent. The strong clubs tend to finish high, so the parents do get relegated now and then.
#[test]
fn ten_seasons_keep_every_reserve_below_its_parent_and_every_division_whole() {
    let top: Vec<String> = [
        "rm", "fc", "at", "se", "ce", "rs", "t7", "t8", "t9", "t10", "t11", "t12",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    let second: Vec<String> = [
        "castilla", "fortuna", "rsb", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "s12",
        "s13",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    let links = vec![
        ReserveSide {
            team_id: "castilla".into(),
            parent_id: "rm".into(),
        },
        ReserveSide {
            team_id: "fortuna".into(),
            parent_id: "ce".into(),
        },
        ReserveSide {
            team_id: "rsb".into(),
            parent_id: "rs".into(),
        },
    ];
    let total = top.len() + second.len();
    let mut upper = division(
        "top",
        0,
        2026,
        &top.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut lower = division(
        "second",
        1,
        2026,
        &second.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    upper.rules.reserve_sides = links.clone();
    lower.rules.reserve_sides = links.clone();
    let rule = BoundaryRule {
        promoted_direct: 2,
        promotion_playoff: Some(PromotionPlayoff {
            from_rank: 3,
            to_rank: 6,
            round_legs: 2,
            final_legs: 1,
        }),
        relegated_direct: 3,
        barrage: None,
        status: RuleStatus::Confirmed,
        note: String::new(),
    };
    upper.rules.ladder = LadderRules {
        boundaries: vec![SeasonBoundary {
            from_season: 2026,
            rule,
        }],
    };

    let mut relegated_parents = 0;
    for season in 2026..2036u32 {
        upper.season = season;
        lower.season = season;
        // A deliberately unkind order for the parents: they finish low half of the time.
        let mut order_top = shuffled(&upper.participant_ids, u64::from(season));
        let order_low = shuffled(&lower.participant_ids, u64::from(season) + 77);
        if season % 2 == 0 {
            order_top.retain(|club| club != "rm" && club != "ce");
            order_top.push("rm".into());
            order_top.push("ce".into());
        }
        let mut u = upper.clone();
        let mut l = lower.clone();
        u.standings = order_top
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut e = StandingEntry::new(c.clone());
                e.points = 1000 - i as u32;
                e
            })
            .collect();
        l.standings = order_low
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut e = StandingEntry::new(c.clone());
                e.points = 1000 - i as u32;
                e
            })
            .collect();
        let outcome = plan_ruled_ladder(&[u, l], &mut AwayWins);

        upper.participant_ids = outcome.participants[0].clone();
        lower.participant_ids = outcome.participants[1].clone();
        upper.rules.withdrawn_sides = outcome.withdrawn[0].clone();
        lower.rules.withdrawn_sides = outcome.withdrawn[1].clone();
        relegated_parents += outcome.movements[0]
            .relegated()
            .filter(|club| *club == "rm" || *club == "ce" || *club == "rs")
            .count();

        let all: Vec<&String> = upper
            .participant_ids
            .iter()
            .chain(&lower.participant_ids)
            .chain(lower.rules.withdrawn_sides.iter().map(|w| &w.team_id))
            .chain(upper.rules.withdrawn_sides.iter().map(|w| &w.team_id))
            .collect();
        let unique: HashSet<&&String> = all.iter().collect();
        assert_eq!(
            all.len(),
            total,
            "season {season}: a club was lost or doubled"
        );
        assert_eq!(
            unique.len(),
            total,
            "season {season}: a club is registered twice"
        );
        assert_eq!(
            upper.participant_ids.len(),
            12,
            "season {season}: the top division changed size"
        );
        for link in &links {
            let parent_in_top = upper.participant_ids.contains(&link.parent_id);
            let reserve_in_top = upper.participant_ids.contains(&link.team_id);
            assert!(
                !reserve_in_top,
                "season {season}: {} reached the top division",
                link.team_id
            );
            let reserve_in_second = lower.participant_ids.contains(&link.team_id);
            let parent_in_second = lower.participant_ids.contains(&link.parent_id);
            assert!(
                !(reserve_in_second && (parent_in_second || !parent_in_top && !parent_in_second)),
                "season {season}: {} shares a division with, or is above, {}",
                link.team_id,
                link.parent_id
            );
        }
    }
    assert!(
        relegated_parents > 0,
        "the scenario never relegated a parent, so it tested nothing"
    );
}
