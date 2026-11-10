//! The squad scenarios the layer exists for, as football situations with named players.
//!
//! Each test names the situation (Given), the question put to the layer (When) and the sporting
//! answer a club would give (Then). They run against the pure model: the transfer market, the
//! assistant and the AI's sales read the same answers and have their own tests.

use domain::player::Position::{self, *};

use super::ClubSquad;
use super::appetite::Appetite;
use super::impact::{Price, Reason, SigningCase};
use super::needs::NeedKind;
use super::test_squads::{Squad, player, with_potential};

const CASH: i64 = 30_000_000;

fn comfortable() -> Appetite {
    Appetite::NEUTRAL
}

fn contender() -> Appetite {
    Appetite {
        weights: [0.70, 0.22, 0.08],
        price: 0.48,
    }
}

fn price(fee: u64) -> Price {
    Price {
        fee,
        weekly_wage: 20_000,
    }
}

fn case(club: &ClubSquad, position: Position, ovr: u8, age: i32, fee: u64) -> SigningCase {
    let candidate = player(&format!("cand-{position:?}-{ovr}"), position, ovr, age);
    let marginal = club.signing(&candidate, 0.0).expect("a senior");
    club.price(&marginal, price(fee))
}

fn group_of(position: &Position) -> usize {
    crate::squad_floor::group_index(position)
}

/// Two keepers of 85 and 83 and a first striker of 61 with nobody behind him
fn keepers_but_no_strikers() -> Squad {
    Squad::new()
        .add(Goalkeeper, &[85, 83])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[74, 72, 68, 66])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(CentralMidfielder, &[74, 72, 68, 66])
        .add(Striker, &[61])
}

/// Given a club with two good keepers and a striker of 61 with no cover, €30 million, and three
/// players on the market: a keeper of 88 for €25 million, a striker of 78 for €22 million and a
/// striker of 74 for €12 million
/// When the signings are compared
/// Then the keeper is not worth signing at all, both strikers are, and the one that is better value
/// depends on how the club spends: a club that holds its cash takes the cheaper striker, a
/// contender the better one.
#[test]
fn a_striker_is_needed_and_a_third_goalkeeper_is_not() {
    let squad = keepers_but_no_strikers();
    let club = squad.club("4-4-2", CASH, comfortable());
    let keeper = case(&club, Goalkeeper, 88, 27, 25_000_000);
    let striker_d = case(&club, Striker, 78, 27, 22_000_000);
    let striker_e = case(&club, Striker, 74, 27, 12_000_000);
    println!("keeper {keeper:?}\nD {striker_d:?}\nE {striker_e:?}");
    assert!(!keeper.is_worth_it(), "{keeper:?}");
    assert!(striker_d.is_worth_it() && striker_e.is_worth_it());
    assert!(striker_d.gain() > keeper.gain() * 3.0);

    let careful = squad.club(
        "4-4-2",
        CASH,
        Appetite {
            weights: [0.55, 0.30, 0.15],
            price: 1.8,
        },
    );
    let d = case(&careful, Striker, 78, 27, 22_000_000);
    let e = case(&careful, Striker, 74, 27, 12_000_000);
    assert!(
        e.utility > d.utility,
        "a careful club prefers E: {e:?} {d:?}"
    );

    let rich = squad.club("4-4-2", CASH, contender());
    let d = case(&rich, Striker, 78, 27, 22_000_000);
    let e = case(&rich, Striker, 74, 27, 12_000_000);
    assert!(
        d.utility > e.utility,
        "a contender takes the better striker"
    );
}

/// Given the same club
/// When its needs are read
/// Then the first is a striker slot, not a goalkeeper one.
#[test]
fn the_squads_first_need_is_the_weak_striker_slot() {
    let club = keepers_but_no_strikers().club("4-4-2", CASH, comfortable());
    let needs = club.needs();
    println!("{needs:?}");
    let first = needs.first().expect("a need");
    assert_eq!(group_of(&first.position), 3, "{first:?}");
    assert!(needs.iter().all(|need| need.group != 0), "{needs:?}");
}

/// Given a club with excellent forwards and poor centre-backs
/// When a centre-back of 74 and a third forward of 80 are weighed
/// Then the defender is worth far more: the forward would be a substitute.
#[test]
fn a_defensive_weakness_beats_one_more_attacker() {
    let squad = Squad::new()
        .add(Goalkeeper, &[75, 66])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[58, 55, 52])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(CentralMidfielder, &[74, 72, 68, 66])
        .add(Striker, &[84, 82, 76]);
    let club = squad.club("4-4-2", CASH, comfortable());
    let centre_back = case(&club, CenterBack, 74, 27, 8_000_000);
    let forward = case(&club, Striker, 80, 27, 8_000_000);
    println!("cb {centre_back:?}\nst {forward:?}");
    assert!(centre_back.is_worth_it());
    assert!(
        centre_back.gain() > forward.gain() * 2.0,
        "{centre_back:?} {forward:?}"
    );
    assert!(centre_back.utility > forward.utility + 0.5);
    let first = club.needs().into_iter().next().expect("a need");
    assert_eq!(first.position, CenterBack);
}

/// Given a strong eleven with only fourteen reliable seniors (no cover in most lines)
/// When an ordinary squad player of 66 is weighed, and then a star of 75 for the eleven
/// Then the bench player is worth signing: his value is depth, not a place in the eleven.
#[test]
fn a_thin_bench_makes_an_ordinary_squad_player_worth_signing() {
    let thin = Squad::new()
        .add(Goalkeeper, &[78])
        .add(LeftBack, &[74, 68])
        .add(RightBack, &[74])
        .add(CenterBack, &[76, 75, 70])
        .add(LeftMidfielder, &[74])
        .add(RightMidfielder, &[74])
        .add(CentralMidfielder, &[76, 75, 66])
        .add(Striker, &[78, 76]);
    assert_eq!(thin.players.len(), 14);
    let club = thin.club("4-4-2", CASH, comfortable());
    let cover = case(&club, CenterBack, 66, 27, 500_000);
    println!("{cover:?}");
    assert!(cover.marginal.gain >= super::impact::MIN_GAIN, "{cover:?}");
    assert_eq!(cover.marginal.reason, Reason::Depth);
    assert!(cover.marginal.starts_at.is_none());
    let keeper_cover = case(&club, Goalkeeper, 66, 27, 500_000);
    assert!(
        keeper_cover.is_worth_it(),
        "a second keeper is cover too: {keeper_cover:?}"
    );
}

/// Given the same eleven with a full bench
/// When the same player of 66 is weighed
/// Then he adds almost nothing: depth is valued where it is missing, not everywhere.
#[test]
fn a_full_bench_makes_the_same_player_redundant() {
    let club = Squad::balanced().club("4-4-2", CASH, comfortable());
    let cover = case(&club, CenterBack, 66, 27, 500_000);
    assert!(!cover.is_worth_it(), "{cover:?}");
}

/// Given a balanced squad whose best striker is 35 and about to retire
/// When a striker of 76 aged 24 is weighed, and the same player for a squad whose striker is 27
/// Then he is clearly worth signing for the first club and only marginally for the second, and the
/// first club's needs already list the succession.
#[test]
fn an_ageing_striker_makes_a_successor_worth_signing() {
    let veteran = Squad::new()
        .add(Goalkeeper, &[74, 66])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[74, 72, 68, 66])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(CentralMidfielder, &[74, 72, 68, 66])
        .add_aged(Striker, &[(77, 35), (73, 27), (70, 26)]);
    let prime = Squad::new()
        .add(Goalkeeper, &[74, 66])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[74, 72, 68, 66])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(CentralMidfielder, &[74, 72, 68, 66])
        .add_aged(Striker, &[(77, 27), (73, 27), (70, 26)]);
    let old = case(
        &veteran.club("4-4-2", CASH, comfortable()),
        Striker,
        76,
        24,
        6_000_000,
    );
    let young = case(
        &prime.club("4-4-2", CASH, comfortable()),
        Striker,
        76,
        24,
        6_000_000,
    );
    println!("with a veteran {old:?}\nwith a prime striker {young:?}");
    assert!(old.is_worth_it());
    // For the club with a prime striker he is a marginal upgrade the price all but eats; for the
    // one losing its first choice he is clearly worth the money.
    assert!(young.utility < 0.05, "{young:?}");
    assert!(old.utility > young.utility + 0.15, "{old:?} {young:?}");
    assert!(old.gain() > young.gain() * 1.5);
    let needs_of = |squad: &Squad| squad.club("4-4-2", CASH, comfortable()).needs();
    assert!(
        needs_of(&veteran)
            .iter()
            .any(|n| n.kind == NeedKind::Succession && n.group == 3)
    );
    assert!(
        needs_of(&prime)
            .iter()
            .all(|n| n.kind != NeedKind::Succession)
    );
}

/// Given a club that plays 4-4-2 with two strikers and one that plays 4-5-1 with one, both holding
/// the same two strikers of 78 and 62
/// When the club changes formation
/// Then its need follows: the second striker is a need in 4-4-2 and not in 4-5-1.
#[test]
fn needs_follow_the_formation_the_club_plays() {
    let squad = Squad::new()
        .add(Goalkeeper, &[75, 66])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[74, 72, 68, 66])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(CentralMidfielder, &[74, 72, 68, 66])
        .add(Striker, &[78, 62]);
    let two_up = squad.club("4-4-2", CASH, comfortable());
    let one_up = squad.club("4-5-1", CASH, comfortable());
    let striker_need = |club: &ClubSquad| {
        club.needs()
            .into_iter()
            .filter(|need| need.group == 3 && need.kind == NeedKind::Starter)
            .map(|need| need.severity)
            .fold(0.0_f32, f32::max)
    };
    println!(
        "442 {} 451 {}",
        striker_need(&two_up),
        striker_need(&one_up)
    );
    assert!(striker_need(&two_up) > striker_need(&one_up) + 2.0);
    let signing_442 = case(&two_up, Striker, 76, 27, 5_000_000);
    let signing_451 = case(&one_up, Striker, 76, 27, 5_000_000);
    assert!(
        signing_442.gain() > signing_451.gain() * 2.0,
        "{signing_442:?} {signing_451:?}"
    );
}

/// Given a club whose only keeper is 80 and who is then lost (sold, or out for the season)
/// When the squad is re-read
/// Then the keeper slot is at once its largest need, and a keeper of 66 is worth signing.
#[test]
fn losing_the_starting_keeper_changes_the_priorities_at_once() {
    let before = Squad::balanced();
    let mut after = Squad::balanced();
    after
        .players
        .retain(|p| p.ovr != 74 || p.natural_position != Goalkeeper);
    let club_before = before.club("4-4-2", CASH, comfortable());
    let club_after = after.club("4-4-2", CASH, comfortable());
    let first_after = club_after.needs().into_iter().next().expect("a need");
    println!("{:?}", club_after.needs());
    assert_eq!(first_after.group, 0, "{first_after:?}");
    assert!(
        club_before
            .needs()
            .iter()
            .all(|need| need.group != 0 || need.severity < 6.0)
    );
    let keeper = case(&club_after, Goalkeeper, 70, 28, 2_000_000);
    assert!(keeper.is_worth_it() && keeper.marginal.reason == Reason::Starter);
}

/// Given a serious injury to the starting keeper (out for sixty days)
/// When the squad is read
/// Then it is treated like a loss for the present: the club needs a keeper now.
#[test]
fn a_long_injury_to_the_keeper_counts_as_a_loss_for_the_present() {
    let mut squad = Squad::balanced();
    for p in &mut squad.players {
        if p.natural_position == Goalkeeper && p.ovr == 74 {
            p.injury = Some(domain::player::Injury {
                name: "x".into(),
                days_remaining: 60,
            });
        }
    }
    let club = squad.club("4-4-2", CASH, comfortable());
    let keeper = case(&club, Goalkeeper, 70, 28, 2_000_000);
    assert!(keeper.is_worth_it(), "{keeper:?}");
}

/// Given a club with three talented young midfielders who will be 74 within a year, and a
/// veteran of 71 on the market
/// When the veteran is weighed for a developing club and for a contender
/// Then he is worth little to either: the present is covered and the future is already in the
/// squad. A midfielder that is clearly better than the young ones is still a signing.
#[test]
fn a_club_with_rising_midfielders_does_not_buy_one_that_blocks_them() {
    let youngsters = Squad::new()
        .add(Goalkeeper, &[75, 66])
        .add(LeftBack, &[72, 66])
        .add(RightBack, &[72, 66])
        .add(CenterBack, &[74, 72, 68, 66])
        .add(LeftMidfielder, &[72, 66])
        .add(RightMidfielder, &[72, 66])
        .add(Striker, &[76, 73, 66]);
    let mut players = youngsters.players;
    for (i, ovr) in [66_u8, 65, 64, 63].into_iter().enumerate() {
        players.push(with_potential(
            player(&format!("kid{i}"), CentralMidfielder, ovr, 19),
            80,
        ));
    }
    let squad = Squad { players };
    let developing = Appetite {
        weights: [0.40, 0.35, 0.25],
        price: 1.2,
    };
    let club = squad.club("4-4-2", CASH, developing);
    let veteran = case(&club, CentralMidfielder, 71, 29, 6_000_000);
    println!("{veteran:?}");
    assert!(!veteran.is_worth_it(), "{veteran:?}");
    let star = case(&club, CentralMidfielder, 84, 27, 6_000_000);
    assert!(star.is_worth_it());
}

/// Given a club with keepers of 85, 83 and 55, and one with keepers of 85, 45 and 40
/// When the cost of selling its second keeper (the first club) and its only good one (the second) is
/// read
/// Then selling the second keeper costs little and is not critical (a cheap cover can be signed behind
/// the first), and selling the only good keeper is critical: nobody replaces him, even on the cheap
/// market. The difference between selling a spare and selling the side.
#[test]
fn selling_a_second_keeper_costs_little_and_selling_the_only_good_one_is_critical() {
    let keepers = |ratings: &[u8]| {
        let mut squad = Squad::balanced();
        squad.players.retain(|p| p.natural_position != Goalkeeper);
        squad.add(Goalkeeper, ratings)
    };
    // The keeper who is `rank`-th best (0 the first) of the club's natural keepers.
    let sale_of = |squad: &Squad, rank: usize| {
        let club = squad.club("4-4-2", CASH, comfortable());
        let mut keepers: Vec<_> = club
            .model
            .members
            .iter()
            .filter(|m| m.natural_keeper)
            .collect();
        keepers.sort_by_key(|keeper| std::cmp::Reverse(keeper.ovr));
        let keeper_id = &keepers[rank].id;
        let loss = (club.base.value - club.model.evaluate_without(keeper_id).value).max(0.0);
        (club.sale(keeper_id), loss)
    };
    let (second_of_two, _) = sale_of(&keepers(&[85, 83, 55]), 1);
    let (only_good_one, loss) = sale_of(&keepers(&[85, 45, 40]), 0);
    println!("{second_of_two:?} {only_good_one:?}");
    assert!(!second_of_two.critical, "{second_of_two:?}");
    assert!(only_good_one.critical, "{only_good_one:?}");
    assert!(only_good_one.net_loss > second_of_two.net_loss * 2.0);
    // A cheap replacement is never as good as the player: the cap is applied.
    assert!(only_good_one.net_loss < loss - 0.02);
    assert!(only_good_one.net_loss > 0.5 * loss);
}

/// Given two clubs with the same eleven of the same ratings, one of 24-year-olds and one of
/// 33-year-olds, and a prospect of 60 who will be 78 in two seasons
/// When each prices him
/// Then the ageing club values him more: its own starters will have declined or retired by the time
/// he arrives, which is what a club "planning a squad over time" means.
#[test]
fn an_ageing_squad_values_a_prospect_more_than_a_young_one_does() {
    let squad_aged = |age: i32| {
        let mut squad = Squad::new();
        for (position, ratings) in [
            (Goalkeeper, &[74u8, 66][..]),
            (LeftBack, &[72, 66]),
            (RightBack, &[72, 66]),
            (CenterBack, &[74, 72, 68, 66]),
            (LeftMidfielder, &[72, 66]),
            (RightMidfielder, &[72, 66]),
            (CentralMidfielder, &[74, 72, 68, 66]),
            (Striker, &[75, 73, 67]),
        ] {
            let aged: Vec<(u8, i32)> = ratings.iter().map(|&ovr| (ovr, age)).collect();
            squad = squad.add_aged(position, &aged);
        }
        squad.club("4-4-2", CASH, comfortable())
    };
    let prospect = with_potential(player("prospect", Striker, 60, 19), 80);
    let worth = |club: &ClubSquad| club.signing(&prospect, 0.0).expect("a senior").gain;
    let (young, ageing) = (worth(&squad_aged(24)), worth(&squad_aged(33)));
    assert!(ageing > young, "ageing {ageing} against young {young}");
}
