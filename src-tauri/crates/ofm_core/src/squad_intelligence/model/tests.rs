use super::*;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::test_squads::{Squad, player, today, with_potential};
use domain::player::Position::*;
use rand::{RngExt, SeedableRng};

fn club(squad: &Squad) -> crate::squad_intelligence::ClubSquad {
    squad.club("4-4-2", 30_000_000, Appetite::NEUTRAL)
}

/// Given identical slot ratings for an outfielder and a natural goalkeeper
/// When their candidate fingerprints are calculated
/// Then the keeper's different outfield coverage cannot reuse the outfielder's cached appraisal.
#[test]
fn candidate_cache_distinguishes_a_goalkeeper_from_an_outfielder() {
    let club = club(&Squad::balanced());
    let candidate = player("candidate", Striker, 80, 25);
    let outfielder = Member::candidate(&candidate, &club.model.slots, today(), 0.0).unwrap();
    let mut goalkeeper = outfielder.clone();
    goalkeeper.natural_keeper = true;
    goalkeeper.refresh_levels();
    assert_ne!(outfielder.levels[0][1], goalkeeper.levels[0][1]);
    assert_ne!(
        SquadModel::fingerprint_of(&outfielder),
        SquadModel::fingerprint_of(&goalkeeper)
    );
    let mut changed_squad = club.model.clone();
    changed_squad.members[0].natural_keeper = !changed_squad.members[0].natural_keeper;
    changed_squad.members[0].refresh_levels();
    assert_ne!(club.model.fingerprint(), changed_squad.fingerprint());
}

/// Given equal candidates, one projected to decline and another projected to leave
/// When their candidate fingerprints are calculated
/// Then the absent season cannot reuse the declining player's cached appraisal.
#[test]
fn candidate_cache_distinguishes_decline_from_departure() {
    let club = club(&Squad::balanced());
    let candidate = player("candidate", Striker, 80, 25);
    let mut declining = Member::candidate(&candidate, &club.model.slots, today(), 0.0).unwrap();
    declining.shift[1] = Some(-1.0);
    declining.refresh_levels();
    let mut departed = declining.clone();
    departed.shift[1] = None;
    departed.refresh_levels();
    assert_ne!(declining.level(1, 1), departed.level(1, 1));
    assert_ne!(
        SquadModel::fingerprint_of(&declining),
        SquadModel::fingerprint_of(&departed)
    );
    let mut declining_squad = club.model.clone();
    declining_squad.members[0].shift[1] = Some(-1.0);
    let mut departed_squad = declining_squad.clone();
    departed_squad.members[0].shift[1] = None;
    assert_ne!(declining_squad.fingerprint(), departed_squad.fingerprint());
}

/// Given a formation that names the same position more than once
/// When a player's fit is read for its slots
/// Then every repetition carries the fit of the first one, and every distinct slot is rated on its
/// own: sharing the work never changes an answer.
#[test]
fn repeated_slots_share_their_fit_without_changing_it() {
    let p = player("fits", CenterBack, 77, 26);
    let slots = slots_of("4-4-2");
    let fit = fits(&p, &slots);
    for slot_index in 0..slots.len() {
        let first = slots[..slot_index]
            .iter()
            .position(|earlier| *earlier == slots[slot_index]);
        if let Some(first) = first {
            assert_eq!(
                fit[slot_index].to_bits(),
                fit[first].to_bits(),
                "slot {slot_index} repeats slot {first}"
            );
        } else {
            assert!(
                (1.0..=99.0).contains(&fit[slot_index]),
                "slot {slot_index} must be rated"
            );
        }
    }
}

/// Given a player out on loan, and the same player with no loan
/// When the club reads him as a possible signing
/// Then the loan neither keeps him out of the model nor changes how the signing is read: a loan is
/// a state the club can end, not the player's level. (In the borrower's own squad he is a member
/// marked `loaned_in`; see the tests on loaned-in players below.)
#[test]
fn a_loaned_player_is_read_as_the_signing_he_would_be() {
    let club = club(&Squad::balanced());
    let mut loaned = player("loaned", Striker, 80, 25);
    loaned.active_loan = Some(domain::player::ActiveLoan {
        parent_team_id: "parent".to_string(),
        loan_team_id: "club".to_string(),
        loan_team_name: None,
        start_date: "2025-07-01".to_string(),
        end_date: "2026-06-30".to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
    });
    let mut owned = loaned.clone();
    owned.active_loan = None;
    let from_loan = Member::candidate(&loaned, &club.model.slots, today(), 0.0)
        .expect("a loan can be made permanent");
    let from_owned = Member::candidate(&owned, &club.model.slots, today(), 0.0).unwrap();
    assert_eq!(
        SquadModel::fingerprint_of(&from_loan),
        SquadModel::fingerprint_of(&from_owned)
    );
    let in_the_squad = Member::of(&loaned, &club.model.slots, today(), Availability::Today)
        .expect("the borrower's own model holds a player on loan to it");
    assert!(in_the_squad.loaned_in);
    assert_eq!(
        in_the_squad.fit, from_owned.fit,
        "being on loan changes how long he is planned for, not how good he is"
    );
    assert_eq!(
        in_the_squad.shift,
        [Some(0.0), None, None],
        "this loan has ended"
    );
}

/// Given two otherwise identical veterans, one declining and one expected to retire
/// When their signings are appraised through the shared cache
/// Then each cached result matches an independently calculated appraisal.
#[test]
fn cached_signings_do_not_confuse_decline_with_retirement() {
    let club = club(&Squad::new());
    let mut declining = player("cache-decline", Striker, 2, 33);
    declining.ovr = 2;
    declining.potential = 2;
    let mut retiring = declining.clone();
    retiring.date_of_birth = "1992-01-01".to_string();
    let declining_member = Member::candidate(&declining, &club.model.slots, today(), 0.0).unwrap();
    let retiring_member = Member::candidate(&retiring, &club.model.slots, today(), 0.0).unwrap();
    let fresh_declining = club.signing_of(&declining_member);
    let fresh_retiring = club.signing_of(&retiring_member);
    assert_ne!(
        fresh_declining.gain.to_bits(),
        fresh_retiring.gain.to_bits()
    );
    assert_eq!(
        club.signing(&declining, 0.0).unwrap().gain.to_bits(),
        fresh_declining.gain.to_bits()
    );
    assert_eq!(
        club.signing(&retiring, 0.0).unwrap().gain.to_bits(),
        fresh_retiring.gain.to_bits()
    );
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
    let member = Member::of(&keeper, &slots_of("4-4-2"), today(), Availability::Today).unwrap();
    assert!(member.shift[0].is_none());
    assert!(member.shift[1].is_some() && member.shift[2].is_some());
    keeper.injury = Some(domain::player::Injury {
        name: "x".into(),
        days_remaining: 10,
    });
    let member = Member::of(&keeper, &slots_of("4-4-2"), today(), Availability::Today).unwrap();
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
    assert!(Member::of(&senior, &slots, today(), Availability::Today).is_some());
    assert!(Member::of(&academy, &slots, today(), Availability::Today).is_none());
    assert!(Member::of(&retired, &slots, today(), Availability::Today).is_none());
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

/// Given a squad and a player asked about twice, and then the squad after it loses its striker
/// When the answers are compared with a fresh evaluation
/// Then the kept answer is the fresh one, and a changed squad is never answered from the old one.
#[test]
fn a_kept_answer_is_the_fresh_answer_and_never_outlives_a_change_of_squad() {
    let squad = Squad::balanced();
    let club = club(&squad);
    let target = player("target", Striker, 80, 26);
    let asked = club.signing(&target, 0.0).unwrap();
    let again = club.signing(&target, 0.0).unwrap();
    let candidate = Member::candidate(&target, &club.model.slots, today(), 0.0).unwrap();
    let fresh = club.signing_of(&candidate);
    assert_eq!(asked.gain.to_bits(), fresh.gain.to_bits());
    assert_eq!(again.gain.to_bits(), fresh.gain.to_bits());

    let mut weaker = Squad::balanced();
    weaker
        .players
        .retain(|p| p.natural_position != Striker || p.ovr < 70);
    let weaker = self::club(&weaker);
    let after = weaker.signing(&target, 0.0).unwrap();
    assert!(
        after.gain > asked.gain,
        "a club short of strikers gains more from one"
    );
}

/// Given a squad with a deep bench and every member's own evaluation of the side without him
/// When the members who neither start nor cover anyone in any horizon year are looked at
/// Then the side without each of them is exactly the side with him (the claim that lets a whole
/// squad's sale costs be read without twenty-six evaluations), and there are some such members.
#[test]
fn a_player_who_plays_no_part_costs_exactly_nothing_to_lose() {
    let mut squad = Squad::balanced();
    for (i, ovr) in [60_u8, 59, 58, 57, 56].into_iter().enumerate() {
        squad
            .players
            .push(player(&format!("deep{i}"), CentralMidfielder, ovr, 27));
        squad
            .players
            .push(player(&format!("deepback{i}"), CenterBack, ovr, 27));
    }
    let club = club(&squad);
    let mut idle = 0;
    for (index, member) in club.model.members.iter().enumerate() {
        if club.base.involves(index) {
            continue;
        }
        idle += 1;
        let without = club.model.evaluate_without(&member.id);
        assert_eq!(
            without.value.to_bits(),
            club.base.value.to_bits(),
            "{}",
            member.id
        );
    }
    assert!(idle >= 3, "{idle} idle members");
}

/// Given every member of a squad
/// When the cost of losing him is read, with and without a cheap replacement coming in
/// Then the replacement never makes the loss bigger, and a loss under the surplus line is the
/// same number either way: the shortcut in `assess_sale` loses nothing.
#[test]
fn a_replacement_never_makes_a_loss_bigger() {
    let club = club(&Squad::balanced());
    for member in &club.model.members {
        let without = club.model.evaluate_without(&member.id);
        let loss = (club.base.value - without.value).max(0.0);
        let case = club.sale(&member.id);
        assert!(
            case.net_loss <= loss + 1e-4,
            "{}: {} vs {}",
            member.id,
            case.net_loss,
            loss
        );
        if loss < crate::squad_intelligence::impact::SURPLUS_LOSS {
            assert!(case.surplus);
        }
    }
}

/// Given many random squads (random positions, ratings, ages, injuries) and random candidates
/// When a candidate is added by the chain of moves and by a fresh solve of every year
/// Then the side's strength is the same, year by year, to the thousandth of a point: the chain is
/// exact. Where two elevens are equally strong the solver may pick either, and the cover read from
/// them differs; the chain keeps the squad's own side unless a move strictly improves it, so a
/// candidate who changes nothing in the eleven changes nothing there.
#[test]
fn adding_a_player_by_a_chain_is_exactly_a_fresh_solve() {
    use domain::player::Position;
    let positions = [
        Goalkeeper,
        LeftBack,
        RightBack,
        CenterBack,
        LeftMidfielder,
        RightMidfielder,
        CentralMidfielder,
        AttackingMidfielder,
        RightWinger,
        Striker,
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(2026);
    let mut worst_cover = 0.0_f32;
    let mut improving = 0;
    for case in 0..120 {
        let size = rng.random_range(13..=26);
        let mut squad = Squad::new();
        for i in 0..size {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            let ovr = rng.random_range(48..=86);
            let age = rng.random_range(17..=36);
            let mut p = player(&format!("c{case}p{i}"), position, ovr, age);
            if rng.random_range(0..10) == 0 {
                p.injury = Some(domain::player::Injury {
                    name: "x".into(),
                    days_remaining: rng.random_range(5..=90),
                });
            }
            squad.players.push(p);
        }
        let club = club(&squad);
        for k in 0..6 {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            let candidate = player(
                &format!("c{case}cand{k}"),
                position,
                rng.random_range(50..=90),
                rng.random_range(17..=35),
            );
            let Some(member) = Member::candidate(&candidate, &club.model.slots, today(), 0.0)
            else {
                continue;
            };
            let chain = club.model.evaluate_with(&member);
            let fresh = club.model.evaluate_with_fresh(&member);
            for year in 0..HORIZON {
                assert!(
                    (chain.years[year].xi - fresh.years[year].xi).abs() < 1e-3,
                    "case {case} cand {k} year {year}: chain {} fresh {}",
                    chain.years[year].xi,
                    fresh.years[year].xi
                );
            }
            if chain.years[0].xi > club.base.years[0].xi + 1e-3 {
                improving += 1;
            } else if chain.years[0].xi < club.base.years[0].xi + 1e-3 {
                // The eleven is untouched: its starters are the squad's own.
                assert_eq!(
                    chain.years[0].starter_index, club.base.years[0].starter_index,
                    "case {case} cand {k}"
                );
            }
            worst_cover = worst_cover.max((chain.value - fresh.value).abs());
        }
    }
    assert!(
        improving > 30,
        "the test must include real improvements: {improving}"
    );
    assert!(worst_cover < 0.5, "covers differ by {worst_cover}");
    println!("improving {improving}, worst cover difference {worst_cover}");
}

fn assert_same_evaluation(left: Evaluation, right: Evaluation, context: &str) {
    assert_eq!(
        left.value.to_bits(),
        right.value.to_bits(),
        "{context}: value {} against {}",
        left.value,
        right.value
    );
    for year in 0..HORIZON {
        let (a, b) = (left.years[year], right.years[year]);
        assert_eq!(a.xi.to_bits(), b.xi.to_bits(), "{context} year {year}: xi");
        assert_eq!(
            a.expected.to_bits(),
            b.expected.to_bits(),
            "{context} year {year}: expected"
        );
        assert_eq!(
            a.starter.map(f32::to_bits),
            b.starter.map(f32::to_bits),
            "{context} year {year}: starter"
        );
        assert_eq!(
            a.cover.map(f32::to_bits),
            b.cover.map(f32::to_bits),
            "{context} year {year}: cover"
        );
        assert_eq!(
            a.absence.map(f32::to_bits),
            b.absence.map(f32::to_bits),
            "{context} year {year}: absence"
        );
        assert_eq!(
            a.starter_index, b.starter_index,
            "{context} year {year}: starter index"
        );
        assert_eq!(
            a.cover_index, b.cover_index,
            "{context} year {year}: cover index"
        );
    }
}

/// Given a signing who is worse than every cover the squad already has, in every horizon year
/// When his appraisal reuses the squad's own view instead of pairing the bench again
/// Then every year is exactly what the full evaluation answers.
#[test]
fn a_signing_below_the_whole_cover_reuses_the_squads_own_view() {
    let club = club(&Squad::balanced());
    let weak = player("weak", Striker, 30, 25);
    let member = Member::candidate(&weak, &club.model.slots, today(), 0.0).unwrap();
    for year in 0..HORIZON {
        assert!(
            club.model.reuses_base_cover(year, &member),
            "year {year} must allow the reuse the test is about"
        );
    }
    let before = BASE_COVER_REUSES.load(std::sync::atomic::Ordering::Relaxed);
    let optimized = club.model.evaluate_with(&member);
    let after = BASE_COVER_REUSES.load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        after > before,
        "the shortcut must take part in the evaluation"
    );
    let full = club.model.evaluate_with_full(&member);
    assert_same_evaluation(optimized, full, "weak signing");
}

/// Given many random squads and random candidates
/// When a signing is evaluated with the unchanged-cover shortcut and without it
/// Then every year, index and rating is bit for bit the same: the shortcut only skips work.
#[test]
fn the_unchanged_cover_shortcut_answers_exactly_as_the_full_evaluation() {
    use domain::player::Position;
    let positions = [
        Goalkeeper,
        LeftBack,
        RightBack,
        CenterBack,
        LeftMidfielder,
        RightMidfielder,
        CentralMidfielder,
        AttackingMidfielder,
        RightWinger,
        Striker,
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(5150);
    let mut reused = 0;
    for case in 0..80 {
        let size = rng.random_range(12..=26);
        let mut squad = Squad::new();
        for i in 0..size {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            squad.players.push(player(
                &format!("s{case}p{i}"),
                position,
                rng.random_range(45..=88),
                rng.random_range(17..=36),
            ));
        }
        let club = club(&squad);
        for k in 0..5 {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            let candidate = player(
                &format!("s{case}cand{k}"),
                position,
                rng.random_range(20..=90),
                rng.random_range(17..=36),
            );
            let Some(member) = Member::candidate(&candidate, &club.model.slots, today(), 0.0)
            else {
                continue;
            };
            for year in 0..HORIZON {
                if club.model.reuses_base_cover(year, &member) {
                    reused += 1;
                }
            }
            let optimized = club.model.evaluate_with(&member);
            let full = club.model.evaluate_with_full(&member);
            assert_same_evaluation(optimized, full, &format!("case {case} candidate {k}"));
        }
    }
    assert!(
        reused > 30,
        "the test must exercise the shortcut: {reused} year views reused"
    );
}

/// Given a player with a short injury
/// When he is read for a signing (today) and for planning
/// Then today's reading raises his chance of missing a match and planning's does not: what he is
/// worth to his club does not change with every knock. A long injury is absent for the present in
/// both.
#[test]
fn planning_reads_a_short_injury_as_an_ordinary_risk_and_today_does_not() {
    let mut p = player("p", Striker, 75, 26);
    p.injury = Some(domain::player::Injury {
        name: "x".into(),
        days_remaining: 10,
    });
    let slots = slots_of("4-4-2");
    let today_member = Member::of(&p, &slots, today(), Availability::Today).unwrap();
    let planning = Member::of(&p, &slots, today(), Availability::Planning).unwrap();
    assert!(today_member.absence > BASE_ABSENCE);
    assert_eq!(planning.absence, BASE_ABSENCE);
    p.injury = Some(domain::player::Injury {
        name: "x".into(),
        days_remaining: 90,
    });
    for availability in [Availability::Today, Availability::Planning] {
        let member = Member::of(&p, &slots, today(), availability).unwrap();
        assert!(member.shift[0].is_none() && member.shift[1].is_some());
    }
}

/// Cost of the layer's operations on a world the size of the shipped one: 440 squads of 26, a
/// shortlist of 300 candidates each, and the sale analysis of every squad. Prints microseconds
/// per operation; asserts nothing (a timing is not a test). Run with
/// `cargo test --release -p ofm_core --lib squad_intelligence::model::tests::benchmark -- --ignored --nocapture`.
#[test]
#[ignore = "timing, not a test"]
fn benchmark_the_cost_of_one_sweep_in_a_world_of_440_clubs() {
    use domain::player::Position;
    let positions = [
        Goalkeeper,
        LeftBack,
        RightBack,
        CenterBack,
        CenterBack,
        LeftMidfielder,
        RightMidfielder,
        CentralMidfielder,
        CentralMidfielder,
        AttackingMidfielder,
        Striker,
        Striker,
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(7);
    let squads: Vec<Squad> = (0..440)
        .map(|club| {
            let mut squad = Squad::new();
            for i in 0..26 {
                let position: Position = positions[i % positions.len()].clone();
                squad.players.push(player(
                    &format!("k{club}p{i}"),
                    position,
                    rng.random_range(52..=80),
                    rng.random_range(18..=35),
                ));
            }
            squad
        })
        .collect();
    let targets: Vec<domain::player::Player> = (0..300)
        .map(|i| {
            player(
                &format!("t{i}"),
                positions[i % positions.len()].clone(),
                rng.random_range(55..=88),
                rng.random_range(18..=34),
            )
        })
        .collect();

    let started = std::time::Instant::now();
    let clubs: Vec<_> = squads.iter().map(club).collect();
    let build = started.elapsed();

    let started = std::time::Instant::now();
    let mut gain = 0.0_f32;
    for club in &clubs {
        for target in &targets[..30] {
            let member = Member::candidate(target, &club.model.slots, today(), 0.0).unwrap();
            gain += club.signing_of(&member).gain;
        }
    }
    let signings = started.elapsed();

    let started = std::time::Instant::now();
    let mut loss = 0.0_f32;
    for club in &clubs {
        for member in &club.model.members {
            loss += club.sale(&member.id).net_loss;
        }
    }
    let sales = started.elapsed();
    println!(
        "440 squads built in {build:?} ({:.0} us each); {} candidate evaluations in {signings:?} ({:.1} us each); {} sale analyses in {sales:?} ({:.1} us each) [{gain} {loss}]",
        build.as_micros() as f64 / 440.0,
        440 * 30,
        signings.as_micros() as f64 / (440.0 * 30.0),
        440 * 26,
        sales.as_micros() as f64 / (440.0 * 26.0),
    );
}

/// The cover as it was first written: eleven rounds, each taking the best remaining pairing of an
/// uncovered slot and a bench player with room.
fn cover_by_repeated_best_pair(
    squad: &[&Member],
    used: &[bool],
    year: usize,
) -> ([f32; SLOTS], [Option<usize>; SLOTS]) {
    let bench: Vec<usize> = (0..squad.len())
        .filter(|&index| !used[index] && squad[index].shift[year].is_some())
        .collect();
    let mut taken = vec![0_u8; bench.len()];
    let mut covered = [false; SLOTS];
    let mut cover = [UNCOVERED_FIT; SLOTS];
    let mut index_of = [None; SLOTS];
    for _ in 0..SLOTS {
        let mut best: Option<(usize, usize, f32)> = None;
        for slot in (0..SLOTS).filter(|&slot| !covered[slot]) {
            for (position, &index) in bench.iter().enumerate() {
                if taken[position] >= SLOTS_PER_COVER {
                    continue;
                }
                let Some(level) = squad[index].level(slot, year) else {
                    continue;
                };
                if best.is_none_or(|(_, _, best_level)| level > best_level) {
                    best = Some((slot, position, level));
                }
            }
        }
        let Some((slot, position, level)) = best else {
            break;
        };
        covered[slot] = true;
        taken[position] += 1;
        cover[slot] = level;
        index_of[slot] = Some(bench[position]);
    }
    (cover, index_of)
}

/// Given many random squads
/// When the cover is assigned by sorting the pairings once, and by taking the best remaining pairing
/// again and again
/// Then every slot has the same cover rated the same and the same player covering it: the faster
/// way is the same way.
#[test]
fn sorting_the_pairings_once_assigns_the_cover_exactly_as_picking_the_best_again_and_again() {
    use domain::player::Position;
    let positions = [
        Goalkeeper,
        LeftBack,
        RightBack,
        CenterBack,
        LeftMidfielder,
        RightMidfielder,
        CentralMidfielder,
        AttackingMidfielder,
        RightWinger,
        Striker,
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(99);
    for case in 0..200 {
        let size = rng.random_range(12..=30);
        let mut squad = Squad::new();
        for i in 0..size {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            squad.players.push(player(
                &format!("c{case}p{i}"),
                position,
                rng.random_range(45..=88),
                rng.random_range(17..=36),
            ));
        }
        let club = club(&squad);
        let refs: Vec<&Member> = club.model.members.iter().collect();
        for year in 0..HORIZON {
            let view = club.base.years[year];
            let used: Vec<bool> = (0..refs.len())
                .map(|index| view.starter_index.contains(&Some(index)))
                .collect();
            let (cover, index_of) = cover_by_repeated_best_pair(&refs, &used, year);
            assert_eq!(view.cover, cover, "case {case} year {year}");
            assert_eq!(view.cover_index, index_of, "case {case} year {year}");
        }
    }
}

/// Given slot levels that are fractional and often exactly equal
/// When the cover is assigned by sorting packed pairings once, and by taking the best remaining
/// pairing again and again
/// Then the same player covers each slot at the same rating: the packed order keeps the old
/// tie-break of slot before bench position.
#[test]
fn fractional_and_equal_cover_ratings_keep_the_pairing_order() {
    use domain::player::Position;
    let positions = [
        Goalkeeper,
        LeftBack,
        RightBack,
        CenterBack,
        LeftMidfielder,
        RightMidfielder,
        CentralMidfielder,
        AttackingMidfielder,
        RightWinger,
        Striker,
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(5151);
    for case in 0..60 {
        let size = rng.random_range(12..=30);
        let mut squad = Squad::new();
        for i in 0..size {
            let position: Position = positions[rng.random_range(0..positions.len())].clone();
            squad.players.push(player(
                &format!("f{case}p{i}"),
                position,
                rng.random_range(45..=88),
                rng.random_range(17..=36),
            ));
        }
        let mut club = club(&squad);
        for (i, member) in club.model.members.iter_mut().enumerate() {
            for year in 0..HORIZON {
                for slot in 0..SLOTS {
                    // Thirds, so exact equality and fractional differences both occur, and some
                    // levels sit under the emergency body's 25.
                    let step = (case * 31 + i * 17 + slot * 7 + year * 13) % 97;
                    member.levels[year][slot] = 18.0 + step as f32 / 3.0;
                }
            }
        }
        let refs: Vec<&Member> = club.model.members.iter().collect();
        for year in 0..HORIZON {
            let view = club.base.years[year];
            let used: Vec<bool> = (0..refs.len())
                .map(|index| view.starter_index.contains(&Some(index)))
                .collect();
            let (cover, index_of) = cover_by_repeated_best_pair(&refs, &used, year);
            let mut scratch = Scratch {
                used,
                ..Scratch::default()
            };
            let mut assigned = EMPTY_YEAR;
            assign_cover(&refs, year, &mut assigned, &mut scratch);
            assert_eq!(
                assigned.cover.map(f32::to_bits),
                cover.map(f32::to_bits),
                "case {case} year {year} cover"
            );
            assert_eq!(
                assigned.cover_index, index_of,
                "case {case} year {year} cover index"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// A player on loan to the club is part of the side it fields, for as long as the loan lasts
// ---------------------------------------------------------------------------

/// A copy of `signing` on loan to the club from `days_left` days before the loan ends.
fn on_loan_until(mut signing: domain::player::Player, days_left: i64) -> domain::player::Player {
    let end = today() + chrono::Duration::days(days_left);
    signing.active_loan = Some(domain::player::ActiveLoan {
        parent_team_id: "parent".to_string(),
        loan_team_id: "club".to_string(),
        loan_team_name: None,
        start_date: (today() - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string(),
        end_date: end.format("%Y-%m-%d").to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
    });
    signing.team_id = Some("club".to_string());
    signing
}

fn squad_with(loaned: domain::player::Player) -> crate::squad_intelligence::ClubSquad {
    let mut squad = Squad::balanced();
    squad.players.push(loaned);
    club(&squad)
}

/// Given a club whose best striker is 75 and a striker of 88 on loan to it for another season
/// When the squad model is built
/// Then the loaned striker is in the model and starts today, as the lineup picker will field him.
#[test]
fn a_player_on_loan_to_the_club_starts_for_it_in_the_model() {
    let club = squad_with(on_loan_until(player("star", Striker, 88, 26), 400));
    let index = club
        .model
        .members
        .iter()
        .position(|member| member.id == "star")
        .expect("the loaned player is a member of the borrower's squad");
    assert!(club.model.members[index].loaned_in);
    assert!(
        club.base.years[0].starter_index.contains(&Some(index)),
        "he is one of the eleven"
    );
}

/// Given loans that end in four months, in a year and a half, and in three years
/// When each loaned player is projected over the horizon
/// Then he is there for exactly the seasons the loan covers and gone after, whatever his contract
/// with the club that owns him says.
#[test]
fn a_loaned_player_is_in_the_plans_only_while_the_loan_lasts() {
    let there = |days_left: i64| {
        let club = squad_with(on_loan_until(player("star", Striker, 88, 26), days_left));
        club.model
            .members
            .iter()
            .find(|member| member.id == "star")
            .expect("a member")
            .shift
            .map(|shift| shift.is_some())
    };
    assert_eq!(there(120), [true, false, false]);
    assert_eq!(there(560), [true, true, false]);
    assert_eq!(there(1_100), [true, true, true]);
    // The edges: a loan with exactly a season left covers this one only; a day more covers the next.
    assert_eq!(there(365), [true, false, false]);
    assert_eq!(there(366), [true, true, false]);
    assert_eq!(there(730), [true, true, false]);
    assert_eq!(there(731), [true, true, true]);
}

/// Given a loan whose end date cannot be read
/// When the loaned player is projected
/// Then he is planned for the present only, never for seasons nobody can promise.
#[test]
fn a_loan_with_an_unreadable_end_is_planned_for_the_present_only() {
    let mut loaned = on_loan_until(player("star", Striker, 88, 26), 900);
    loaned.active_loan.as_mut().expect("a loan").end_date = "not a date".to_string();
    let club = squad_with(loaned);
    let shift = club
        .model
        .members
        .iter()
        .find(|member| member.id == "star")
        .expect("a member")
        .shift
        .map(|shift| shift.is_some());
    assert_eq!(shift, [true, false, false]);
}

/// Given a club with a player on loan to it
/// When the cost of every sale is read
/// Then the loaned player is not among those it could sell, and asking about him is the same
/// answer as for a player the club does not hold: he is not the club's to sell.
#[test]
fn a_club_cannot_sell_a_player_it_has_only_borrowed() {
    let club = squad_with(on_loan_until(player("star", Striker, 88, 26), 400));
    assert!(
        club.model
            .members
            .iter()
            .any(|m| m.id == "star" && m.loaned_in),
        "he is in the squad, so keeping him out of the sales is the guard's doing"
    );
    assert!(club.sales("club").iter().all(|(id, _)| id != "star"));
    let case = club.sale("star");
    assert!(case.surplus && case.loss == 0.0);
}

/// Given the same club with and without a loaned striker of 88
/// When the strikers' slots are read for need
/// Then a club that has borrowed the striker no longer has a starter need up front.
#[test]
fn a_borrowed_striker_closes_the_need_he_was_borrowed_for() {
    let mut weak = Squad::balanced();
    weak.players.retain(|p| p.natural_position != Striker);
    weak.players
        .extend([player("s1", Striker, 58, 27), player("s2", Striker, 57, 27)]);
    let without = club(&weak);
    let mut with = weak.players.clone();
    with.push(on_loan_until(player("star", Striker, 88, 26), 400));
    let with = club(&Squad { players: with });
    let starter_needs = |club: &crate::squad_intelligence::ClubSquad| {
        club.needs()
            .iter()
            .filter(|need| need.kind == crate::squad_intelligence::needs::NeedKind::Starter)
            .map(|need| need.severity)
            .sum::<f32>()
    };
    assert!(
        starter_needs(&with) < starter_needs(&without),
        "{} against {}",
        starter_needs(&with),
        starter_needs(&without)
    );
}
