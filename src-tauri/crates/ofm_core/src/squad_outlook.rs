//! A club's squad as it will be in a year, not as it is today.
//!
//! The market, the free-agent planner and the renewal review all asked the same question of a squad
//! — "how deep and how good are we?" — about the squad of the day. A club whose best keeper is
//! 35 looked as well covered as one whose best keeper is 25, until the day he retired and the club
//! had to buy in a hurry. This module answers the question for the squad a year ahead: who is
//! expected to have left (retired, or a veteran on an expiring contract the club will not renew),
//! and what the others are expected to be rated (a prospect grows towards his potential, a
//! veteran declines). Those are the numbers a club's need is judged against, so a club buys,
//! signs and keeps with its succession in mind.
//!
//! The expectations are planning heuristics, not a forecast of the development model: a club
//! planning a squad does not know a prospect's dice. They are deliberately coarse and bounded.

use crate::squad_floor::{MIN_PLAYERS_PER_GROUP, PLANNING_MARGIN_PER_GROUP, STARTERS_PER_GROUP};

/// A retirement this likely at the next season's end counts as a departure for planning. The
/// chance table (`aging::retirement_base_chance`) crosses it at 35: a club plans around a
/// 34-year-old, the season before the odds are even.
const LIKELY_RETIREMENT_PCT: u32 = 40;

/// How many rating points a group's first choices may be expected to lose before the club
/// calls it a succession risk. Smaller drops are ordinary squad churn.
const SUCCESSION_DROP: f32 = 3.0;

/// Goalkeepers age later than outfield players, by this many years, in both decline and the
/// age at which a club stops renewing them.
const KEEPER_AGE_OFFSET: i32 = 3;

/// The age after which a club does not give a player another contract for being good, by group.
pub(crate) fn peak_age_limit(group: usize) -> i32 {
    32 + if group == 0 { KEEPER_AGE_OFFSET } else { 0 }
}

/// One senior as planning sees him.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlannedPlayer {
    pub ovr: u8,
    pub potential: u8,
    pub age: i32,
    pub group: usize,
    /// Days left on his contract; `None` is a contract with no end.
    pub contract_days: Option<i64>,
}

impl PlannedPlayer {
    /// A player as planning sees him on `current_date`.
    pub(crate) fn of(player: &domain::player::Player, current_date: chrono::NaiveDate) -> Self {
        Self {
            ovr: player.ovr,
            potential: player.potential,
            age: domain::age::age_on_or_unknown(current_date, &player.date_of_birth),
            group: crate::squad_floor::group_index(&player.position),
            contract_days: crate::contracts::contract_days_remaining(
                player.contract_end(),
                current_date,
            ),
        }
    }
}

/// Whether a player counts toward his club's squad today: a registered senior, not on loan, not
/// out for the season. The one definition both the market's view of a squad and the club's own
/// planning use, so the same club is never judged by two different counts.
pub(crate) fn counts_toward_squad(player: &domain::player::Player) -> bool {
    player.squad_role == domain::player::SquadRole::Senior
        && player.active_loan.is_none()
        && player
            .injury
            .as_ref()
            .is_none_or(|injury| injury.days_remaining < crate::transfers::LONG_INJURY_DAYS)
}

/// His rating a year from now, as a club would expect it.
pub(crate) fn expected_ovr_in_a_year(ovr: u8, potential: u8, age: i32, group: usize) -> u8 {
    // A keeper at 33 is where an outfielder is at 30.
    let age = age - if group == 0 { KEEPER_AGE_OFFSET } else { 0 };
    let room = f32::from(potential.saturating_sub(ovr));
    let share = match age {
        ..=20 => 0.40,
        21..=22 => 0.30,
        23..=24 => 0.20,
        25..=26 => 0.10,
        _ => 0.0,
    };
    let decline = match age {
        ..=30 => 0,
        31..=32 => 1,
        33..=34 => 2,
        _ => 3,
    };
    let grown = i32::from(ovr) + (room * share).round() as i32;
    (grown - decline).clamp(1, 100) as u8
}

/// A player's rating a year on, as a club would expect it.
pub(crate) fn expected_ovr_for(
    player: &domain::player::Player,
    current_date: chrono::NaiveDate,
) -> u8 {
    expected_ovr_in_a_year(
        player.ovr,
        player.potential,
        domain::age::age_on_or_unknown(current_date, &player.date_of_birth),
        crate::squad_floor::group_index(&player.position),
    )
}

/// Whether the club should plan without him a year from now: a likely retirement, or a veteran
/// past his group's renewal age whose contract runs out within the renewal horizon (the review
/// will not extend him unless the depth chart forces it, which is the very need being planned).
pub(crate) fn leaves_within_a_year(player: &PlannedPlayer) -> bool {
    if crate::aging::retirement_base_chance(player.age + 1) >= LIKELY_RETIREMENT_PCT {
        return true;
    }
    player.age > peak_age_limit(player.group)
        && player
            .contract_days
            .is_some_and(|days| days <= crate::ai_contracts::RENEWAL_HORIZON_DAYS)
}

/// The ratings of the seniors who stay, a year on, per position group, best first.
pub(crate) fn planned_levels(players: &[PlannedPlayer]) -> [Vec<u8>; 4] {
    let mut levels: [Vec<u8>; 4] = Default::default();
    for player in players.iter().filter(|p| !leaves_within_a_year(p)) {
        levels[player.group].push(expected_ovr_in_a_year(
            player.ovr,
            player.potential,
            player.age,
            player.group,
        ));
    }
    for group in &mut levels {
        group.sort_unstable_by(|a, b| b.cmp(a));
    }
    levels
}

fn mean_of_starters(levels: &[u8], group: usize) -> f32 {
    let starters = STARTERS_PER_GROUP[group];
    (0..starters)
        .map(|slot| f32::from(levels.get(slot).copied().unwrap_or(0)))
        .sum::<f32>()
        / starters as f32
}

/// How many rating points (rounded) the group's first choices are expected to lose in a year.
/// `current` and `planned` are best first.
pub(crate) fn starters_drop(current: &[u8], planned: &[u8], group: usize) -> i32 {
    (mean_of_starters(current, group) - mean_of_starters(planned, group)).round() as i32
}

/// Whether a group will be clearly worse or thinner a year from now than it is today: its first
/// choices lose [`SUCCESSION_DROP`] points between them, or departures take it under the depth a
/// club plans for. `current` and `planned` are best first.
pub(crate) fn succession_risk(current: &[u8], planned: &[u8], group: usize) -> bool {
    let planned_depth = MIN_PLAYERS_PER_GROUP[group].1 + PLANNING_MARGIN_PER_GROUP;
    let drops =
        mean_of_starters(current, group) - mean_of_starters(planned, group) >= SUCCESSION_DROP;
    let newly_thin = planned.len() < planned_depth && current.len() >= planned_depth;
    drops || newly_thin
}

/// One club's squad now and a year on, per position group, best first: `(current, planned)`.
/// `squad` is the club's players who count toward it ([`counts_toward_squad`]).
pub(crate) fn club_outlook<'a>(
    squad: impl Iterator<Item = &'a domain::player::Player>,
    current_date: chrono::NaiveDate,
) -> ([Vec<u8>; 4], [Vec<u8>; 4]) {
    let mut current: [Vec<u8>; 4] = Default::default();
    let mut planning = Vec::new();
    for player in squad {
        let planned = PlannedPlayer::of(player, current_date);
        current[planned.group].push(player.ovr);
        planning.push(planned);
    }
    for group in &mut current {
        group.sort_unstable_by(|a, b| b.cmp(a));
    }
    (current, planned_levels(&planning))
}

/// Whether a group may take on a successor: a succession replaces a departure, it does not stack a
/// group above the depth it plans for (plus the one the transition year carries). Without this a
/// club with an ageing keeper and a free-agent pool would keep signing keepers, each better than
/// the cover it just signed, until the squad is full.
pub(crate) fn has_room_for_a_successor(planned_len: usize, group: usize) -> bool {
    planned_len <= MIN_PLAYERS_PER_GROUP[group].1 + PLANNING_MARGIN_PER_GROUP
}

#[cfg(test)]
mod tests {
    use super::*;

    const GK: usize = 0;
    const MID: usize = 2;

    fn flat_attributes() -> domain::player::PlayerAttributes {
        domain::player::PlayerAttributes {
            pace: 60,
            stamina: 60,
            strength: 60,
            agility: 60,
            passing: 60,
            shooting: 60,
            tackling: 60,
            dribbling: 60,
            defending: 60,
            positioning: 60,
            vision: 60,
            decisions: 60,
            composure: 60,
            aggression: 60,
            teamwork: 60,
            leadership: 60,
            handling: 60,
            reflexes: 60,
            aerial: 60,
        }
    }

    fn senior(group: usize, ovr: u8, potential: u8, age: i32) -> PlannedPlayer {
        PlannedPlayer {
            ovr,
            potential,
            age,
            group,
            contract_days: Some(900),
        }
    }

    /// Given a 19-year-old with room to grow and a 30-year-old at his peak
    /// When their rating a year on is planned
    /// Then the prospect is expected to rise toward his potential and the veteran to hold.
    #[test]
    fn a_prospect_is_expected_to_grow_and_a_peak_player_to_hold() {
        assert_eq!(expected_ovr_in_a_year(60, 80, 19, MID), 68);
        assert_eq!(expected_ovr_in_a_year(75, 75, 30, MID), 75);
    }

    /// Given a prospect already at his potential
    /// When his rating is planned
    /// Then he is not expected to grow past it.
    #[test]
    fn nobody_is_expected_to_grow_past_his_potential() {
        assert_eq!(expected_ovr_in_a_year(70, 70, 19, MID), 70);
        assert_eq!(expected_ovr_in_a_year(72, 70, 19, MID), 72);
    }

    /// Given an outfield player and a keeper of the same age and rating
    /// When their rating a year on is planned
    /// Then the keeper holds up three years longer.
    #[test]
    fn a_keeper_declines_later_than_an_outfield_player() {
        assert_eq!(expected_ovr_in_a_year(80, 80, 33, MID), 78);
        assert_eq!(expected_ovr_in_a_year(80, 80, 33, GK), 80);
        assert_eq!(expected_ovr_in_a_year(80, 80, 37, GK), 78);
    }

    /// Given a 34-year-old and a 33-year-old, both with long contracts
    /// When the club asks who will have gone in a year
    /// Then the 34-year-old counts as gone (retirement is a coin toss by the season's end) and
    /// the 33-year-old does not.
    #[test]
    fn a_player_of_thirty_four_is_planned_around_and_one_of_thirty_three_is_not() {
        assert!(leaves_within_a_year(&senior(GK, 80, 80, 34)));
        assert!(!leaves_within_a_year(&senior(GK, 80, 80, 33)));
    }

    /// Given a 33-year-old outfield veteran and a 28-year-old, each on an expiring contract
    /// When the club asks who will have gone in a year
    /// Then the veteran is planned out (the review will not renew him for being good) and the
    /// 28-year-old, who will be renewed, is kept.
    #[test]
    fn a_veteran_on_an_expiring_contract_is_planned_out_and_a_prime_player_is_not() {
        let expiring = |age| PlannedPlayer {
            contract_days: Some(100),
            ..senior(MID, 75, 75, age)
        };
        assert!(leaves_within_a_year(&expiring(33)));
        assert!(!leaves_within_a_year(&expiring(28)));
    }

    /// Given a keeper of 33 on an expiring contract
    /// When the club asks who will have gone in a year
    /// Then he is kept: a keeper's renewal age is three years later than an outfield player's.
    #[test]
    fn a_keeper_on_an_expiring_contract_is_kept_longer() {
        let keeper = PlannedPlayer {
            contract_days: Some(100),
            ..senior(GK, 80, 80, 33)
        };
        assert!(!leaves_within_a_year(&keeper));
    }

    /// Given a keeper of 35 and a keeper of 28 and a prospect of 19
    /// When the squad a year on is planned
    /// Then the 35-year-old is not in it and the others are, best first, the prospect grown.
    #[test]
    fn planned_levels_drop_the_leavers_and_grow_the_prospect() {
        let levels = planned_levels(&[
            senior(GK, 80, 80, 35),
            senior(GK, 62, 62, 28),
            senior(GK, 55, 78, 19),
        ]);
        assert_eq!(levels[GK], vec![64, 62]);
    }

    /// Given a keeper of 35 rated 80 with a cover keeper of 62
    /// When the group is checked for succession risk
    /// Then it is at risk: the first choice drops 18 points.
    #[test]
    fn an_ageing_keeper_with_poor_cover_is_a_succession_risk() {
        let players = [senior(GK, 80, 80, 35), senior(GK, 62, 62, 28)];
        let planned = planned_levels(&players);
        assert!(succession_risk(&[80, 62], &planned[GK], GK));
    }

    /// Given a keeper of 26 and one of 28 (the control for the case above)
    /// When the group is checked for succession risk
    /// Then it is not: nobody is leaving or declining.
    #[test]
    fn a_keeper_in_his_prime_is_not_a_succession_risk() {
        let players = [
            senior(GK, 80, 80, 26),
            senior(GK, 62, 62, 28),
            senior(GK, 60, 60, 24),
        ];
        let planned = planned_levels(&players);
        assert!(!succession_risk(&[80, 62, 60], &planned[GK], GK));
    }

    /// Given a midfield of three prime players at 70, a veteran of 36 at 70, a 19-year-old rated 66
    /// with a potential of 80 and a bench player at 62
    /// When the group is checked for succession risk, with and without the 19-year-old
    /// Then with him there is none (he is expected to be 72, the group keeps its depth and its first
    /// choices), and without him there is: the group goes under the depth a club plans for. The
    /// pair is what proves it is his growth that removes the risk.
    #[test]
    fn a_ready_replacement_removes_the_risk() {
        let core = [
            senior(MID, 70, 70, 27),
            senior(MID, 70, 70, 28),
            senior(MID, 70, 70, 29),
            senior(MID, 70, 70, 36),
            senior(MID, 62, 62, 25),
        ];
        let prospect = senior(MID, 66, 80, 19);
        let with_him: Vec<_> = core.iter().copied().chain([prospect]).collect();
        let planned = planned_levels(&with_him);
        assert_eq!(planned[MID], vec![72, 70, 70, 70, 62]);
        assert!(!succession_risk(
            &[70, 70, 70, 70, 66, 62],
            &planned[MID],
            MID
        ));
        let planned = planned_levels(&core);
        assert!(succession_risk(&[70, 70, 70, 70, 62], &planned[MID], MID));
    }

    /// Given a midfield of six at 70 that loses two to retirement, so four remain at the same level
    /// When the group is checked for succession risk
    /// Then it is at risk and for depth alone: the first choices do not drop at all (the mean of
    /// the starters is 70 before and after), the group is simply under the five a club plans for.
    #[test]
    fn departures_that_leave_a_group_under_its_depth_are_a_risk() {
        let players = [
            senior(MID, 70, 70, 27),
            senior(MID, 70, 70, 28),
            senior(MID, 70, 70, 29),
            senior(MID, 70, 70, 30),
            senior(MID, 70, 70, 35),
            senior(MID, 70, 70, 36),
        ];
        let current = [70, 70, 70, 70, 70, 70];
        let planned = planned_levels(&players);
        assert_eq!(planned[MID].len(), 4);
        assert_eq!(starters_drop(&current, &planned[MID], MID), 0);
        assert!(succession_risk(&current, &planned[MID], MID));
    }

    /// Given veterans past the renewal age on a contract with no end and on one that ended yesterday
    /// When the club asks who will have gone in a year
    /// Then the one with no end stays (the expiry sweep never ends it) and the one whose contract
    /// has lapsed has gone.
    #[test]
    fn a_contract_with_no_end_stays_and_a_lapsed_one_has_gone() {
        let veteran = |contract_days| PlannedPlayer {
            contract_days,
            ..senior(MID, 75, 75, 33)
        };
        assert!(!leaves_within_a_year(&veteran(None)));
        assert!(leaves_within_a_year(&veteran(Some(-1))));
    }

    /// Given a group with room for its depth and one already at it
    /// When a successor is considered
    /// Then the club takes one more than it plans for and no more.
    #[test]
    fn a_group_takes_on_a_successor_only_up_to_one_above_its_depth() {
        // Midfield plans for five (four plus the margin).
        assert!(has_room_for_a_successor(4, MID));
        assert!(has_room_for_a_successor(5, MID));
        assert!(!has_room_for_a_successor(6, MID));
    }

    /// Given a club's players, one on loan, one out for 60 days, one out for 10 days, and an
    /// academy player
    /// When the squad that counts is read
    /// Then only the one out for ten days and the healthy senior count.
    #[test]
    fn a_loanee_a_long_injury_and_an_academy_player_do_not_count_toward_the_squad() {
        let mut healthy = domain::player::Player::new(
            "h".into(),
            "h".into(),
            "h".into(),
            "1998-01-01".into(),
            "England".into(),
            domain::player::Position::Midfielder,
            flat_attributes(),
        );
        healthy.squad_role = domain::player::SquadRole::Senior;
        let injured = |days| {
            let mut p = healthy.clone();
            p.injury = Some(domain::player::Injury {
                name: "x".into(),
                days_remaining: days,
            });
            p
        };
        let mut academy = healthy.clone();
        academy.squad_role = domain::player::SquadRole::Youth;
        let mut loanee = healthy.clone();
        loanee.active_loan = Some(domain::player::ActiveLoan {
            parent_team_id: "a".into(),
            loan_team_id: "b".into(),
            loan_team_name: None,
            start_date: "2026-01-01".into(),
            end_date: "2026-12-31".into(),
            wage_contribution_pct: 50,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        });
        assert!(counts_toward_squad(&healthy));
        assert!(counts_toward_squad(&injured(10)));
        assert!(!counts_toward_squad(&injured(60)));
        assert!(!counts_toward_squad(&academy));
        assert!(!counts_toward_squad(&loanee));
    }
}
