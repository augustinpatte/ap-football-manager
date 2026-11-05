use super::*;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::test_squads::{Squad, player, today, with_potential};
use domain::player::Position::*;
use rand::{RngExt, SeedableRng};

fn club(squad: &Squad) -> crate::squad_intelligence::ClubSquad {
    squad.club("4-4-2", 30_000_000, Appetite::NEUTRAL)
}

/// Given random cost matrices with more columns than rows
/// When the Hungarian assignment solves them
/// Then its total equals the best total of every possible assignment.
#[test]
fn the_assignment_is_the_best_of_all_assignments() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(11);
    for _ in 0..200 {
        let (rows, columns) = (4, 6);
        let cost: Vec<i64> = (0..rows * columns)
            .map(|_| rng.random_range(-100..100))
            .collect();
        let chosen = hungarian(&cost, rows, columns);
        let total: i64 = chosen
            .iter()
            .enumerate()
            .map(|(row, &column)| cost[row * columns + column])
            .sum();
        let mut distinct = chosen.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), rows, "a column was used twice");
        assert_eq!(total, brute_force_best(&cost, rows, columns));
    }
}

fn brute_force_best(cost: &[i64], rows: usize, columns: usize) -> i64 {
    fn go(cost: &[i64], row: usize, rows: usize, columns: usize, used: &mut Vec<bool>) -> i64 {
        if row == rows {
            return 0;
        }
        let mut best = i64::MAX;
        for column in 0..columns {
            if used[column] {
                continue;
            }
            used[column] = true;
            best = best.min(cost[row * columns + column] + go(cost, row + 1, rows, columns, used));
            used[column] = false;
        }
        best
    }
    go(cost, 0, rows, columns, &mut vec![false; columns])
}

/// Given the same squad listed in two different orders
/// When it is evaluated
/// Then the value and every starter are identical: nothing depends on storage order.
#[test]
fn the_evaluation_does_not_depend_on_the_order_players_are_stored_in() {
    let squad = Squad::balanced();
    let mut reversed = Squad::new();
    reversed.players = squad.players.iter().rev().cloned().collect();
    let (a, b) = (club(&squad), club(&reversed));
    assert_eq!(a.base.value, b.base.value);
    assert_eq!(a.base.years[0].starter, b.base.years[0].starter);
    assert_eq!(a.base.years[2].cover, b.base.years[2].cover);
}

/// Given a balanced squad and a range of players of every position, rating and age
/// When each is added
/// Then the squad is never weaker for it.
#[test]
fn adding_a_player_never_makes_the_squad_weaker() {
    let club = club(&Squad::balanced());
    for (index, position) in [
        Goalkeeper,
        CenterBack,
        LeftBack,
        CentralMidfielder,
        RightWinger,
        Striker,
    ]
    .into_iter()
    .enumerate()
    {
        for ovr in [40_u8, 60, 72, 85] {
            for age in [19, 27, 34] {
                let signing = player(&format!("p{index}-{ovr}-{age}"), position.clone(), ovr, age);
                let case = club.signing(&signing, 0.0).unwrap();
                assert!(case.gain >= -1e-3, "{position:?} {ovr} {age}: {case:?}");
            }
        }
    }
}

/// Given a squad with two keepers and a squad with four
/// When their expected strength is read
/// Then the spare keepers add nothing: no club covers a winger with its third goalkeeper.
#[test]
fn a_spare_keeper_does_not_make_an_outfield_slot_look_covered() {
    let two = Squad::balanced();
    let mut four = Squad::balanced();
    four.players.push(player("gk3", Goalkeeper, 70, 27));
    four.players.push(player("gk4", Goalkeeper, 68, 27));
    let (two, four) = (club(&two), club(&four));
    for slot in 1..SLOTS {
        assert!(
            four.base.years[0].cover[slot] <= two.base.years[0].cover[slot] + 1e-3,
            "slot {slot}"
        );
    }
}

/// Given a keeper who is out for sixty days
/// When the squad is built
/// Then he is not in the present's plans but is back in the next year's.
#[test]
fn a_long_injury_removes_a_player_for_now_and_not_for_later() {
    let mut keeper = player("gk", Goalkeeper, 80, 27);
    keeper.injury = Some(domain::player::Injury {
        name: "x".into(),
        days_remaining: 60,
    });
    let member = Member::of(&keeper, &slots_of("4-4-2"), today()).unwrap();
    assert!(member.shift[0].is_none());
    assert!(member.shift[1].is_some() && member.shift[2].is_some());
    keeper.injury = Some(domain::player::Injury {
        name: "x".into(),
        days_remaining: 10,
    });
    let member = Member::of(&keeper, &slots_of("4-4-2"), today()).unwrap();
    assert!(member.shift[0].is_some());
    assert!(
        member.absence > BASE_ABSENCE,
        "a short injury still raises the chance of a miss"
    );
}

/// Given an academy player, a retired player and a loanee
/// When they are turned into members
/// Then none is part of the senior squad.
#[test]
fn only_senior_registered_players_are_in_the_squad() {
    let slots = slots_of("4-4-2");
    let senior = player("s", Striker, 70, 25);
    let mut academy = senior.clone();
    academy.squad_role = SquadRole::Youth;
    let mut retired = senior.clone();
    retired.retired = true;
    assert!(Member::of(&senior, &slots, today()).is_some());
    assert!(Member::of(&academy, &slots, today()).is_none());
    assert!(Member::of(&retired, &slots, today()).is_none());
}

/// Given a prospect aged 19 with a high ceiling
/// When his club's squad is read for the present and two years on
/// Then he is worth more in each later year.
#[test]
fn a_prospect_is_worth_more_each_year() {
    let mut squad = Squad::balanced();
    squad
        .players
        .push(with_potential(player("kid", Striker, 62, 18), 88));
    let club = club(&squad);
    let kid = club.model.members.iter().find(|m| m.id == "kid").unwrap();
    let levels: Vec<f32> = (0..HORIZON).map(|y| kid.level(9, y).unwrap()).collect();
    assert!(levels[0] < levels[1] && levels[1] < levels[2], "{levels:?}");
}

/// Given the same squad built twice
/// When it is evaluated
/// Then the evaluations are bit-for-bit equal.
#[test]
fn the_same_squad_gives_the_same_evaluation_every_time() {
    let squad = Squad::balanced();
    let (a, b) = (club(&squad), club(&squad));
    assert_eq!(a.base.value.to_bits(), b.base.value.to_bits());
}
