//! Authored national cups: the draw, the round names and the dates of the later rounds.

use super::*;
use chrono::TimeZone;
use domain::competition_rules::{DrawMode, MonthDay};
use std::collections::HashSet;

fn clubs(count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("c{n:02}")).collect()
}

fn authored_cup(count: usize, salt: u64) -> League {
    let team_ids = clubs(count);
    let mut cup = League::new("cup".into(), "Cup".into(), 2026, &team_ids);
    cup.kind = CompetitionType::Cup;
    cup.rules.format = CompetitionFormat::Knockout;
    cup.rules.cup.draw = DrawMode::Random;
    cup.rules.cup.draw_salt = salt;
    cup.rules.knockout_matches_per_day = 64;
    cup.standings.clear();
    let start = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
    seed_knockout_round(&mut cup, &team_ids, start, FixtureCompetition::Cup);
    cup
}

fn play_round_home_wins(cup: &mut League) {
    for fixture in cup
        .fixtures
        .iter_mut()
        .filter(|fixture| fixture.status == FixtureStatus::Scheduled)
    {
        fixture.status = FixtureStatus::Completed;
        fixture.result = Some(domain::league::MatchResult {
            home_goals: 1,
            ..Default::default()
        });
    }
    advance_knockout_competition_round(cup);
}

fn pairings(cup: &League) -> Vec<(String, String)> {
    cup.fixtures
        .iter()
        .map(|fixture| (fixture.home_team_id.clone(), fixture.away_team_id.clone()))
        .collect()
}

#[test]
fn an_uneven_field_is_a_round_of_the_bracket_above_it_and_the_best_seeds_wait() {
    let cup = authored_cup(36, 1);
    assert_eq!(cup.knockout_rounds[0].name, "Round of 64");
    assert_eq!(cup.fixtures.len(), 4, "36 clubs: 28 byes and 4 ties");
    let byes = &cup.knockout_rounds[0].bye_team_ids;
    assert_eq!(byes.len(), 28);
    assert_eq!(byes[..], clubs(36)[..28], "the 28 best seeds wait");
    let playing: HashSet<&str> = cup
        .fixtures
        .iter()
        .flat_map(|fixture| [fixture.home_team_id.as_str(), fixture.away_team_id.as_str()])
        .collect();
    assert_eq!(playing.len(), 8);
    assert!(clubs(36)[28..].iter().all(|club| playing.contains(club.as_str())));
}

#[test]
fn the_draw_is_fixed_by_the_salt_and_changes_with_it() {
    assert_eq!(pairings(&authored_cup(40, 7)), pairings(&authored_cup(40, 7)));
    assert_ne!(pairings(&authored_cup(40, 7)), pairings(&authored_cup(40, 8)));
}

#[test]
fn a_sequential_cup_still_pairs_the_clubs_in_the_order_listed() {
    let mut cup = authored_cup(8, 0);
    cup.rules.cup.draw = DrawMode::Sequential;
    cup.fixtures.clear();
    cup.knockout_rounds.clear();
    let team_ids = clubs(8);
    seed_knockout_round(&mut cup, &team_ids, Utc::now(), FixtureCompetition::Cup);
    assert_eq!(pairings(&cup)[0], ("c01".to_string(), "c02".to_string()));
}

#[test]
fn a_cup_plays_down_to_one_champion_and_its_rounds_follow_the_authored_dates() {
    let mut cup = authored_cup(36, 3);
    cup.rules.cup.round_dates = vec![
        MonthDay { month: 5, day: 22 },
        MonthDay { month: 4, day: 21 },
        MonthDay { month: 2, day: 3 },
        MonthDay { month: 1, day: 13 },
        MonthDay { month: 12, day: 9 },
    ];
    let mut names = vec![cup.knockout_rounds[0].name.clone()];
    for _ in 0..8 {
        if cup.knockout_rounds.last().is_some_and(|round| round.name == "Final")
            && cup.fixtures.iter().all(|f| f.status == FixtureStatus::Completed)
        {
            break;
        }
        play_round_home_wins(&mut cup);
        if let Some(last) = cup.knockout_rounds.last()
            && names.last() != Some(&last.name)
        {
            names.push(last.name.clone());
        }
    }
    assert_eq!(
        names,
        ["Round of 64", "Round of 32", "Round of 16", "Quarterfinal", "Semifinal", "Final"]
    );
    let date_of = |round: usize| -> String {
        let id = &cup.knockout_rounds[round].fixture_ids[0];
        cup.fixtures.iter().find(|f| &f.id == id).unwrap().date.clone()
    };
    assert_eq!(date_of(1), "2026-12-09", "round of 32");
    assert_eq!(date_of(2), "2027-01-13", "round of 16");
    assert_eq!(date_of(3), "2027-02-03", "quarter-finals");
    assert_eq!(date_of(4), "2027-04-21", "semi-finals");
    assert_eq!(date_of(5), "2027-05-22", "final");
    assert!(cup.fixtures.iter().all(|f| f.status == FixtureStatus::Completed));
    let final_id = &cup.knockout_rounds[5].fixture_ids[0];
    let final_tie = cup.fixtures.iter().find(|f| &f.id == final_id).unwrap();
    assert!(final_tie.advancing_team_id().is_some());
}
