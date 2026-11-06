//! Realism of the AI market: where clubs shop, what a player is to his club, what he costs and
//! how the closing of the window changes all of it.
//!
//! Everything here is a small, bounded function of facts the game already holds. Nothing is
//! stored: a player's status is worked out on the day it is needed, and no rule is a quota — the
//! geography of a deal is a weight on a club's own ranking of its options, never a share to meet.
//!
//! Three notions of price are kept apart, because mixing them is how negotiations drift:
//!
//! 1. the **value** (`Player::market_value`): what the game thinks he is worth;
//! 2. the **asking price** ([`asking_fee`]): what his club wants, from his status, his contract,
//!    his potential and the days left in the window;
//! 3. the **ceiling** ([`buyer_ceiling`]): the most a buyer will pay, from how much he needs him.
//!
//! Each is a sum of bounded terms on top of the value, computed afresh from the facts each time:
//! nothing is multiplied onto a previous result, so a long negotiation cannot compound them.

use crate::squad_floor::{STARTERS_PER_GROUP, group_index};
use domain::player::{Player, SquadRole};
use std::collections::HashMap;

/// Seniors a club wants in each group, `[GK, DEF, MID, FWD]` (the same plan `ai_squad_sales` sells
/// down to).
pub(crate) const DESIRED_DEPTH: [usize; 4] = [3, 7, 7, 5];

/// Where in the window the market stands, by days left until it closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WindowPhase {
    /// More than forty days left: clubs are looking round.
    Early,
    /// Up to the last week: the ordinary rhythm.
    Mid,
    /// Seven days or fewer.
    LastWeek,
    /// The last three days (two days left, or one): "J-2", "J-1".
    Last72h,
    /// The closing day itself.
    DeadlineDay,
}

impl WindowPhase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            WindowPhase::Early => "early",
            WindowPhase::Mid => "mid",
            WindowPhase::LastWeek => "last_week",
            WindowPhase::Last72h => "last_72h",
            WindowPhase::DeadlineDay => "deadline_day",
        }
    }
}

impl PlayerStatus {
    pub(crate) fn name(self) -> &'static str {
        match self {
            PlayerStatus::Indispensable => "indispensable",
            PlayerStatus::Starter => "starter",
            PlayerStatus::Rotation => "rotation",
            PlayerStatus::Backup => "backup",
            PlayerStatus::Surplus => "surplus",
            PlayerStatus::Unwanted => "unwanted",
        }
    }
}

pub(crate) fn window_phase(days_remaining: Option<i64>) -> WindowPhase {
    match days_remaining {
        None => WindowPhase::Mid,
        Some(days) if days <= 0 => WindowPhase::DeadlineDay,
        Some(1..=2) => WindowPhase::Last72h,
        Some(3..=7) => WindowPhase::LastWeek,
        Some(8..=40) => WindowPhase::Mid,
        Some(_) => WindowPhase::Early,
    }
}

impl WindowPhase {
    /// How hard the closing presses, 0 (nothing: the early and middle of the window) to 1 (the
    /// last day). The one number every time-dependent term below is scaled by.
    pub(crate) fn pressure(self) -> f64 {
        match self {
            WindowPhase::Early => 0.0,
            WindowPhase::Mid => 0.0,
            WindowPhase::LastWeek => 0.35,
            WindowPhase::Last72h => 0.65,
            WindowPhase::DeadlineDay => 1.0,
        }
    }

    /// The factor on how many deals and loans the whole market may close in a day. It only raises
    /// a ceiling: a club with nothing to buy still buys nothing.
    pub(crate) fn activity_factor(self) -> f64 {
        1.0 + 1.2 * self.pressure()
    }

    /// Days a refused club waits before it asks again, shortened as the window closes.
    pub(crate) fn rebid_cooldown_days(self, base: i64) -> i64 {
        match self {
            WindowPhase::Early | WindowPhase::Mid => base,
            WindowPhase::LastWeek => base / 2,
            WindowPhase::Last72h => base / 4,
            WindowPhase::DeadlineDay => base / 10,
        }
        .max(1)
    }
}

/// What a player is to his club today. Worked out from the squad, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PlayerStatus {
    /// A starter whom the bench cannot replace.
    Indispensable,
    Starter,
    /// First cover, or a starter of the future being brought on.
    Rotation,
    Backup,
    /// Deeper than the club needs.
    Surplus,
    /// Surplus and costly for the books, or not wanted by the squad.
    Unwanted,
}

/// The facts a status is read from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StatusFacts {
    /// 0 is the best in his position group.
    pub rank_in_group: usize,
    pub group: usize,
    pub in_starting_xi: bool,
    /// Rating minus the first cover's rating in his group (negative when he is behind him).
    pub gap_to_cover: i32,
    pub appearances: u32,
    pub age: i32,
    /// His wage as a share of the club's wage budget (both weekly), per cent.
    pub wage_share_pct: i64,
    /// The club is over its wage budget or in debt.
    pub club_in_trouble: bool,
    pub potential_gap: i32,
    /// Out for weeks: the club cannot count on him now.
    pub long_injury: bool,
}

pub(crate) fn classify_status(facts: &StatusFacts) -> PlayerStatus {
    let starters = STARTERS_PER_GROUP[facts.group];
    let is_starter = facts.in_starting_xi || facts.rank_in_group < starters;
    let gap_for_indispensable = if facts.group == 0 { 5 } else { 6 };
    if is_starter {
        if facts.long_injury {
            return PlayerStatus::Rotation;
        }
        if facts.rank_in_group < starters && facts.gap_to_cover >= gap_for_indispensable {
            return PlayerStatus::Indispensable;
        }
        return PlayerStatus::Starter;
    }
    let beyond_plan = facts.rank_in_group >= DESIRED_DEPTH[facts.group];
    let costly = facts.wage_share_pct >= 4;
    if beyond_plan && (costly || facts.club_in_trouble) && facts.age >= 24 {
        return PlayerStatus::Unwanted;
    }
    if beyond_plan {
        return PlayerStatus::Surplus;
    }
    // Behind the plan's cover and not used: an older player who has not played is as good as
    // surplus; a young one with room to grow is a prospect to be brought on, not a spare.
    if facts.rank_in_group >= starters + 2 {
        let unused = facts.appearances <= 2;
        if unused && facts.age >= 28 && facts.potential_gap <= 2 {
            return PlayerStatus::Surplus;
        }
        return PlayerStatus::Backup;
    }
    PlayerStatus::Rotation
}

/// The status of every senior of one club, by player id. `wage_budget` and `in_trouble` are the
/// club's; `age_of` reads a player's age on the day.
pub(crate) fn club_statuses(
    players: &[&Player],
    starting_xi: &[String],
    wage_budget: i64,
    in_trouble: bool,
    age_of: &dyn Fn(&Player) -> i32,
) -> HashMap<String, PlayerStatus> {
    let mut out = HashMap::new();
    for (group, &starters) in STARTERS_PER_GROUP.iter().enumerate() {
        let mut members: Vec<&Player> = players
            .iter()
            .copied()
            .filter(|p| p.squad_role == SquadRole::Senior && group_index(&p.position) == group)
            .collect();
        members.sort_by(|a, b| {
            b.ovr
                .cmp(&a.ovr)
                .then_with(|| b.date_of_birth.cmp(&a.date_of_birth))
                .then_with(|| a.id.cmp(&b.id))
        });
        let first_cover = members.get(starters).map_or(0, |p| i32::from(p.ovr));
        for (rank, player) in members.iter().enumerate() {
            let wage_share_pct = if wage_budget > 0 {
                i64::from(player.wage()) * 100 / wage_budget
            } else {
                0
            };
            let facts = StatusFacts {
                rank_in_group: rank,
                group,
                in_starting_xi: starting_xi.iter().any(|id| id == &player.id),
                gap_to_cover: i32::from(player.ovr) - first_cover,
                appearances: player.stats.appearances,
                age: age_of(player),
                wage_share_pct,
                club_in_trouble: in_trouble,
                potential_gap: i32::from(player.potential) - i32::from(player.ovr),
                long_injury: player
                    .injury
                    .as_ref()
                    .is_some_and(|injury| injury.days_remaining >= LONG_INJURY_DAYS),
            };
            out.insert(player.id.clone(), classify_status(&facts));
        }
    }
    out
}

/// An injury of this many days or more takes a player out of the club's plans for the window.
pub(crate) const LONG_INJURY_DAYS: u32 = 45;

// --- Selling -----------------------------------------------------------------------------------

/// Whether a club puts this player in the market on its own initiative, or lets him go to a buyer
/// who asks. A listed player always is for sale; the rest depend on what he is to the club, how
/// much a sale is worth to it and how near the window's end is.
pub(crate) fn willing_to_sell(
    status: PlayerStatus,
    listed: bool,
    interest_score: i32,
    phase: WindowPhase,
    contract_days_left: Option<i64>,
) -> bool {
    if listed {
        return true;
    }
    // A player whose contract ends within two months and who is not the club's mainstay is going
    // for nothing in a few weeks: in the last week the club takes a fee rather than lose him.
    if phase >= WindowPhase::LastWeek
        && contract_days_left.is_some_and(|days| days <= 60)
        && status > PlayerStatus::Starter
    {
        return true;
    }
    match status {
        PlayerStatus::Indispensable => false,
        // A starter goes only for a bargain, or on the last day when the buyer has made it one.
        PlayerStatus::Starter => interest_score > 75,
        PlayerStatus::Rotation => interest_score > 60,
        PlayerStatus::Backup => interest_score > 45 || phase >= WindowPhase::LastWeek,
        PlayerStatus::Surplus | PlayerStatus::Unwanted => true,
    }
}

/// Everything about a player's sale that can move his asking price.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AskingFacts {
    pub value: u64,
    pub status: PlayerStatus,
    pub phase: WindowPhase,
    pub contract_days_left: Option<i64>,
    pub listed: bool,
    pub age: i32,
    pub potential_gap: i32,
    pub morale: u8,
    pub seller_in_trouble: bool,
    /// The seller has just been relegated and is over the wage budget of its new division.
    pub seller_cutting_costs: bool,
    /// The seller has nobody to replace him in his group beyond the squad floor.
    pub replacement_hard: bool,
}

/// What a spare is worth less than his value on an ordinary day, and how much more he is worth
/// less at the very close. A spare with years on his contract is still an asset: the discount
/// that matters grows as the time to sell him runs out (and, separately, as his contract does).
const SURPLUS_BASE_DISCOUNT: f64 = 0.06;
const UNWANTED_BASE_DISCOUNT: f64 = 0.10;
const SURPLUS_CLOSING_DISCOUNT: f64 = 0.26;
const UNWANTED_CLOSING_DISCOUNT: f64 = 0.36;

pub(crate) const ASKING_MIN: f64 = 0.50;
pub(crate) const ASKING_MAX: f64 = 1.60;

/// What the seller wants, as a multiple of the value, bounded to [`ASKING_MIN`]..[`ASKING_MAX`].
pub(crate) fn asking_multiplier(facts: &AskingFacts) -> f64 {
    let pressure = facts.phase.pressure();
    let mut m = 1.0;
    // Listing a player and calling him a spare are two names for one decision: the club wants him
    // gone. The deeper of the two discounts applies in full and half of the other, so a listed
    // surplus is not charged twice for the same reason.
    let listing_discount = if facts.listed { 0.10 } else { 0.0 };
    let status_discount = match facts.status {
        PlayerStatus::Backup => 0.05,
        PlayerStatus::Surplus => SURPLUS_BASE_DISCOUNT,
        PlayerStatus::Unwanted => UNWANTED_BASE_DISCOUNT,
        _ => 0.0,
    };
    m -= f64::max(listing_discount, status_discount)
        + 0.5 * f64::min(listing_discount, status_discount);
    m += match facts.status {
        PlayerStatus::Indispensable => 0.25,
        PlayerStatus::Starter => 0.12,
        _ => 0.0,
    };
    // A contract about to run out is a player the club will lose for nothing.
    if let Some(days) = facts.contract_days_left {
        m += if days <= 60 {
            -0.30
        } else if days <= 180 {
            -0.18
        } else if days <= 365 {
            -0.06
        } else if days >= 1095 && facts.status <= PlayerStatus::Starter {
            0.06
        } else {
            0.0
        };
    }
    if facts.age <= 23 && facts.potential_gap >= 8 {
        m += f64::from(facts.potential_gap.min(20)) / 200.0;
    }
    if facts.morale <= 40 {
        m -= 0.04;
    }
    if facts.seller_in_trouble {
        m -= 0.06;
    }
    // The window closing. A player the club cannot replace costs more the less time it has to
    // find someone; a spare costs less the less time it has to sell him; a contract that ends
    // soon makes a sale now worth more than a free departure later.
    m += match facts.status {
        PlayerStatus::Indispensable if facts.replacement_hard => 0.20 * pressure,
        PlayerStatus::Indispensable => 0.06 * pressure,
        PlayerStatus::Starter => 0.04 * pressure,
        PlayerStatus::Rotation => 0.0,
        PlayerStatus::Backup => -0.04 * pressure,
        PlayerStatus::Surplus => -SURPLUS_CLOSING_DISCOUNT * pressure,
        PlayerStatus::Unwanted => -UNWANTED_CLOSING_DISCOUNT * pressure,
    };
    // A club that has just come down and is over its new budget sells a little cheaper.
    if facts.seller_cutting_costs {
        m -= 0.04;
    }
    if facts.contract_days_left.is_some_and(|days| days <= 60) {
        m -= 0.10 * pressure;
    }
    m.clamp(ASKING_MIN, ASKING_MAX)
}

pub(crate) fn asking_fee(facts: &AskingFacts) -> u64 {
    ((facts.value as f64) * asking_multiplier(facts)).round() as u64
}

// --- Buying ------------------------------------------------------------------------------------

/// What a buyer weighs in deciding how much a target is worth to *him*.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CeilingFacts {
    pub value: u64,
    /// Target's rating minus the buyer's first cover in his group (the player he would replace).
    pub improvement: i32,
    /// The buyer is below the planning depth in the group.
    pub thin: bool,
    pub age: i32,
    pub phase: WindowPhase,
    /// How many acceptable alternatives the buyer has in the same group (0 means he is the only
    /// option left).
    pub alternatives: usize,
    /// Cash the buyer can spend: the lesser of his transfer budget and his finances.
    pub budget: i64,
    /// What the board adds to a valuation for a club that has just come up and must be ready for
    /// its new division (0 for everybody else).
    pub ambition: f64,
}

pub(crate) const CEILING_MIN: f64 = 0.60;
pub(crate) const CEILING_MAX: f64 = 1.50;

pub(crate) fn ceiling_multiplier(facts: &CeilingFacts) -> f64 {
    let pressure = facts.phase.pressure();
    let mut m = 0.95 + 0.02 * f64::from(facts.improvement.clamp(0, 12));
    // A buyer with time to spare holds out for a lower price, and gives in as the window closes:
    // the reason the best deals wait for the last days, and a club short of players waits less.
    let waiting = match facts.phase {
        WindowPhase::Early => 0.18,
        WindowPhase::Mid => 0.12,
        WindowPhase::LastWeek => 0.03,
        WindowPhase::Last72h => 0.0,
        WindowPhase::DeadlineDay => -0.03,
    };
    m -= if facts.thin { waiting * 0.4 } else { waiting };
    if facts.thin {
        m += 0.10 + 0.10 * pressure;
    }
    m += facts.ambition;
    if facts.age >= 30 {
        m -= (0.03 * f64::from(facts.age - 29)).min(0.20);
    }
    if facts.alternatives == 0 {
        m += 0.05 * (0.5 + pressure);
    }
    m.clamp(CEILING_MIN, CEILING_MAX)
}

/// The most the buyer pays: his valuation, and never more than he has.
pub(crate) fn buyer_ceiling(facts: &CeilingFacts) -> u64 {
    let valued = ((facts.value as f64) * ceiling_multiplier(facts)).round() as u64;
    valued.min(facts.budget.max(0) as u64)
}

/// The fee a deal is done at, if there is one. At or under the ceiling the seller gets his price;
/// within [`HAGGLE_PCT`] of it the buyer gets him at his ceiling; beyond that there is no deal.
pub(crate) fn agreed_fee(asking: u64, ceiling: u64) -> Option<u64> {
    if asking <= ceiling {
        Some(asking)
    } else if u128::from(ceiling) * 100 >= u128::from(asking) * (100 - HAGGLE_PCT) {
        Some(ceiling)
    } else {
        None
    }
}

/// How far under his price a seller will come to close (per cent).
pub(crate) const HAGGLE_PCT: u128 = 8;

// --- Geography ---------------------------------------------------------------------------------

/// What decides how much a buyer likes a target's country.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GeoFacts {
    pub same_country: bool,
    pub buyer_reputation: u32,
    /// Rating gain over the buyer's first cover (a thin group counts as a sizeable gain).
    pub improvement: i32,
    pub fee: u64,
    pub buyer_budget: i64,
    /// The target's market value: a cheap player is not worth a scouting trip across a border.
    pub value: u64,
}

pub(crate) const GEO_MIN: f64 = 0.40;
pub(crate) const GEO_MAX: f64 = 1.50;

/// A weight on a target's appeal, 1.0 for a player from the buyer's own country. Abroad it is
/// below 1 for a small club (scouting, agents, wages that do not carry over), approaches 1 for a
/// big one, and a clear upgrade lifts it past 1: a player who is much better than what the buyer
/// has is worth the trip to anyone who can afford him. It weights the ranking; it excludes nothing.
pub(crate) fn geographic_affinity(facts: &GeoFacts) -> f64 {
    if facts.same_country {
        return 1.0;
    }
    let prestige = ((f64::from(facts.buyer_reputation) - 500.0) / 350.0).clamp(0.0, 1.0);
    let base = (0.30 + 0.45 * prestige) * foreign_visibility(facts.value);
    let pull = 0.30 * (f64::from(facts.improvement.clamp(0, 15)) / 15.0);
    let share = if facts.buyer_budget > 0 {
        facts.fee as f64 / facts.buyer_budget as f64
    } else {
        1.0
    };
    // A fee that eats most of the budget leaves nothing for the extra cost of a foreign deal.
    let drag = if share > 0.6 {
        0.15 * (1.0 - prestige)
    } else {
        0.0
    };
    (base + pull - drag).clamp(GEO_MIN, GEO_MAX)
}

/// How visible a foreign player is to a buyer, by what he is worth: a player under £300 000 is
/// seen by few scouts outside his own country (0.6), one above £3 million by everybody (1.0),
/// in between on a log scale.
pub(crate) fn foreign_visibility(value: u64) -> f64 {
    const LOW: f64 = 300_000.0;
    const HIGH: f64 = 3_000_000.0;
    let v = (value.max(1) as f64).clamp(LOW, HIGH);
    0.6 + 0.4 * (v.ln() - LOW.ln()) / (HIGH.ln() - LOW.ln())
}

/// A buyer's ranking of a target it could sign: the club's own interest in him, plus what he adds
/// to the squad, weighted by geography.
pub(crate) fn target_rank(interest_score: i32, improvement: i32, affinity: f64) -> f64 {
    (f64::from(interest_score) + 4.0 * f64::from(improvement.clamp(0, 15))) * affinity
}

/// Whether a club with no hole in its squad goes shopping today. In the first weeks of the window
/// most wait (nothing presses, and prices fall later); from then on all look. `draw` is a stable
/// per-club-per-day number.
pub(crate) fn shops_without_pressing_need(phase: WindowPhase, draw: u64) -> bool {
    let share = match phase {
        WindowPhase::Early => 35,
        _ => 100,
    };
    draw % 100 < share
}

/// Whether every club, not only those on their review day, is in the market: the last three days.
pub(crate) fn whole_market_is_open(phase: WindowPhase) -> bool {
    phase >= WindowPhase::Last72h
}

/// A foreign player worth less than this is not worth a trip across the border for a squad filler.
pub(crate) const CROSS_BORDER_FILLER_VALUE: u64 = 500_000;
/// The rating gain that does justify one.
pub(crate) const CROSS_BORDER_MIN_GAIN: i32 = 4;

/// Whether a buyer would look abroad for this player: not a cheap squad filler, unless he is a
/// clear upgrade or the buyer has nobody in the position at all. A home player is never refused here.
pub(crate) fn worth_the_border(
    same_country: bool,
    value: u64,
    improvement: i32,
    thin: bool,
) -> bool {
    same_country
        || thin
        || value >= CROSS_BORDER_FILLER_VALUE
        || improvement >= CROSS_BORDER_MIN_GAIN
}

/// Whether a group is below the depth the floor plus the planning margin ask for.
pub(crate) fn group_is_thin(levels: &[u8], group: usize) -> bool {
    use crate::squad_floor::{MIN_PLAYERS_PER_GROUP, PLANNING_MARGIN_PER_GROUP};
    levels.len() < MIN_PLAYERS_PER_GROUP[group].1 + PLANNING_MARGIN_PER_GROUP
}

// --- Needs -------------------------------------------------------------------------------------

/// A club short of players in a group that the floor plus the planning margin ask for.
pub(crate) fn has_unmet_need(depths: &[usize; 4]) -> bool {
    use crate::squad_floor::{MIN_PLAYERS_PER_GROUP, PLANNING_MARGIN_PER_GROUP};
    (0..4).any(|g| depths[g] < MIN_PLAYERS_PER_GROUP[g].1 + PLANNING_MARGIN_PER_GROUP)
}

/// A club that cannot carry the signing of a fee player: over its wage budget by a fifth, or in
/// debt. It sells or lends before it buys.
/// The line past which a club is overstretched, drawn at `limit_pct` of the wage budget.
pub(crate) fn is_overstretched_at(
    wage_bill: i64,
    wage_budget: i64,
    finance: i64,
    limit_pct: i64,
) -> bool {
    finance < 0 || (wage_budget > 0 && wage_bill * 100 > wage_budget * limit_pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking(status: PlayerStatus, phase: WindowPhase) -> AskingFacts {
        AskingFacts {
            value: 10_000_000,
            status,
            phase,
            contract_days_left: Some(900),
            listed: false,
            age: 27,
            potential_gap: 0,
            morale: 70,
            seller_in_trouble: false,
            seller_cutting_costs: false,
            replacement_hard: true,
        }
    }

    #[test]
    fn phases_follow_the_days_left() {
        assert_eq!(window_phase(Some(0)), WindowPhase::DeadlineDay);
        assert_eq!(window_phase(Some(1)), WindowPhase::Last72h);
        assert_eq!(window_phase(Some(2)), WindowPhase::Last72h);
        assert_eq!(window_phase(Some(3)), WindowPhase::LastWeek);
        assert_eq!(window_phase(Some(7)), WindowPhase::LastWeek);
        assert_eq!(window_phase(Some(8)), WindowPhase::Mid);
        assert_eq!(window_phase(Some(41)), WindowPhase::Early);
        assert_eq!(window_phase(None), WindowPhase::Mid);
        assert_eq!(window_phase(Some(-5)), WindowPhase::DeadlineDay);
    }

    #[test]
    fn pressure_only_rises_toward_the_deadline() {
        let phases = [
            WindowPhase::Early,
            WindowPhase::Mid,
            WindowPhase::LastWeek,
            WindowPhase::Last72h,
            WindowPhase::DeadlineDay,
        ];
        for pair in phases.windows(2) {
            assert!(pair[0].pressure() <= pair[1].pressure());
            assert!(pair[0].activity_factor() <= pair[1].activity_factor());
            assert!(pair[0].rebid_cooldown_days(30) >= pair[1].rebid_cooldown_days(30));
        }
        // The first half of the window is the baseline; the last week is not.
        assert_eq!(WindowPhase::Mid.pressure(), 0.0);
        assert!(WindowPhase::LastWeek.pressure() > 0.0);
        assert!(WindowPhase::DeadlineDay.activity_factor() > 2.0);
        assert_eq!(WindowPhase::DeadlineDay.rebid_cooldown_days(1), 1);
    }

    #[test]
    fn an_indispensable_player_costs_more_as_the_window_closes() {
        let early = asking_multiplier(&asking(PlayerStatus::Indispensable, WindowPhase::Mid));
        let late = asking_multiplier(&asking(PlayerStatus::Indispensable, WindowPhase::Last72h));
        assert!(late > early + 0.1, "{early} -> {late}");
        // An easily replaced one rises far less.
        let mut easy = asking(PlayerStatus::Indispensable, WindowPhase::Last72h);
        easy.replacement_hard = false;
        assert!(asking_multiplier(&easy) < late);
    }

    #[test]
    fn rotation_price_is_stable_through_the_window() {
        let early = asking_multiplier(&asking(PlayerStatus::Rotation, WindowPhase::Early));
        let late = asking_multiplier(&asking(PlayerStatus::Rotation, WindowPhase::DeadlineDay));
        assert!((early - late).abs() < 1e-9);
    }

    #[test]
    fn surplus_and_unwanted_get_cheaper_toward_the_deadline() {
        for status in [PlayerStatus::Surplus, PlayerStatus::Unwanted] {
            let early = asking_multiplier(&asking(status, WindowPhase::Early));
            let late = asking_multiplier(&asking(status, WindowPhase::DeadlineDay));
            assert!(late < early - 0.1, "{status:?}: {early} -> {late}");
        }
        let unwanted = asking_multiplier(&asking(PlayerStatus::Unwanted, WindowPhase::Mid));
        let surplus = asking_multiplier(&asking(PlayerStatus::Surplus, WindowPhase::Mid));
        assert!(unwanted < surplus);
    }

    #[test]
    fn an_expiring_contract_is_priced_in() {
        let mut expiring = asking(PlayerStatus::Rotation, WindowPhase::Last72h);
        expiring.contract_days_left = Some(30);
        let mut long = expiring;
        long.contract_days_left = Some(900);
        assert!(asking_multiplier(&expiring) < asking_multiplier(&long) - 0.3);
    }

    #[test]
    fn asking_price_is_bounded_whatever_the_facts() {
        let statuses = [
            PlayerStatus::Indispensable,
            PlayerStatus::Starter,
            PlayerStatus::Rotation,
            PlayerStatus::Backup,
            PlayerStatus::Surplus,
            PlayerStatus::Unwanted,
        ];
        let phases = [
            WindowPhase::Early,
            WindowPhase::Mid,
            WindowPhase::LastWeek,
            WindowPhase::Last72h,
            WindowPhase::DeadlineDay,
        ];
        for status in statuses {
            for phase in phases {
                for days in [None, Some(-10), Some(0), Some(59), Some(61), Some(5000)] {
                    for (age, gap, morale, trouble, listed, hard) in [
                        (16, 40, 0, true, true, false),
                        (40, -30, 100, false, false, true),
                        (23, 20, 40, true, false, true),
                    ] {
                        let facts = AskingFacts {
                            value: 0,
                            status,
                            phase,
                            contract_days_left: days,
                            listed,
                            age,
                            potential_gap: gap,
                            morale,
                            seller_in_trouble: trouble,
                            seller_cutting_costs: false,
                            replacement_hard: hard,
                        };
                        let m = asking_multiplier(&facts);
                        assert!(m.is_finite() && (ASKING_MIN..=ASKING_MAX).contains(&m));
                        assert_eq!(asking_fee(&facts), 0, "a free player is not charged for");
                        let big = AskingFacts {
                            value: u64::from(u32::MAX) * 1_000,
                            ..facts
                        };
                        let fee = asking_fee(&big);
                        assert!(fee > 0 && fee < u64::MAX / 2);
                    }
                }
            }
        }
    }

    #[test]
    fn the_ceiling_is_bounded_and_capped_by_cash() {
        let facts = CeilingFacts {
            value: 10_000_000,
            improvement: 100,
            thin: true,
            age: 18,
            phase: WindowPhase::DeadlineDay,
            alternatives: 0,
            budget: i64::MAX,
            ambition: 0.0,
        };
        let m = ceiling_multiplier(&facts);
        assert!((CEILING_MIN..=CEILING_MAX).contains(&m));
        let poor = CeilingFacts {
            budget: 3_000_000,
            ..facts
        };
        assert_eq!(buyer_ceiling(&poor), 3_000_000);
        let broke = CeilingFacts {
            budget: -5,
            ..facts
        };
        assert_eq!(buyer_ceiling(&broke), 0);
        let worst = CeilingFacts {
            improvement: -50,
            thin: false,
            age: 99,
            alternatives: 9,
            phase: WindowPhase::Early,
            ..facts
        };
        let m = ceiling_multiplier(&worst);
        assert!((CEILING_MIN..0.8).contains(&m), "{m}");
    }

    #[test]
    fn a_deal_closes_between_the_ceiling_and_the_price() {
        assert_eq!(agreed_fee(100, 120), Some(100));
        assert_eq!(agreed_fee(100, 100), Some(100));
        assert_eq!(agreed_fee(100, 95), Some(95));
        assert_eq!(agreed_fee(100, 92), Some(92));
        assert_eq!(agreed_fee(100, 91), None);
        assert_eq!(agreed_fee(0, 0), Some(0));
        assert_eq!(agreed_fee(u64::MAX, 0), None);
    }

    #[test]
    fn geography_weighs_without_deciding() {
        let big = |improvement| GeoFacts {
            same_country: false,
            buyer_reputation: 900,
            improvement,
            fee: 10,
            buyer_budget: 1_000,
            value: 5_000_000,
        };
        let small = |improvement| GeoFacts {
            buyer_reputation: 450,
            ..big(improvement)
        };
        // Home is always 1.
        assert_eq!(
            geographic_affinity(&GeoFacts {
                same_country: true,
                ..big(0)
            }),
            1.0
        );
        // A big club hardly minds the border; a small one does.
        assert!(geographic_affinity(&big(0)) > geographic_affinity(&small(0)) + 0.2);
        assert!(geographic_affinity(&small(0)) < 0.7);
        // A clear upgrade is worth the trip for the big club, not (quite) for the small one.
        assert!(geographic_affinity(&big(15)) > 1.0);
        assert!(geographic_affinity(&small(4)) < 1.0);
        // Never outside its bounds.
        for improvement in [-100, 0, 7, 100] {
            for reputation in [0, 450, 900, 5_000] {
                let a = geographic_affinity(&GeoFacts {
                    buyer_reputation: reputation,
                    ..big(improvement)
                });
                assert!((GEO_MIN..=GEO_MAX).contains(&a));
            }
        }
    }

    #[test]
    fn a_zero_budget_does_not_divide() {
        let a = geographic_affinity(&GeoFacts {
            same_country: false,
            buyer_reputation: 500,
            improvement: 3,
            fee: 1,
            buyer_budget: 0,
            value: 0,
        });
        assert!(a.is_finite());
    }

    #[test]
    fn selling_willingness_follows_status() {
        use PlayerStatus::*;
        let mid = WindowPhase::Mid;
        assert!(!willing_to_sell(
            Indispensable,
            false,
            100,
            WindowPhase::DeadlineDay,
            Some(900)
        ));
        assert!(willing_to_sell(Indispensable, true, 0, mid, Some(900)));
        assert!(!willing_to_sell(Starter, false, 70, mid, Some(900)));
        assert!(willing_to_sell(Starter, false, 80, mid, Some(900)));
        assert!(willing_to_sell(Surplus, false, 0, mid, Some(900)));
        assert!(willing_to_sell(Unwanted, false, 0, mid, Some(900)));
        assert!(!willing_to_sell(Backup, false, 20, mid, Some(900)));
        assert!(willing_to_sell(
            Backup,
            false,
            20,
            WindowPhase::LastWeek,
            Some(900)
        ));
    }

    #[test]
    fn status_reads_the_squad() {
        let base = StatusFacts {
            rank_in_group: 0,
            group: 1,
            in_starting_xi: true,
            gap_to_cover: 10,
            appearances: 20,
            age: 27,
            wage_share_pct: 1,
            club_in_trouble: false,
            potential_gap: 0,
            long_injury: false,
        };
        assert_eq!(classify_status(&base), PlayerStatus::Indispensable);
        assert_eq!(
            classify_status(&StatusFacts {
                gap_to_cover: 2,
                ..base
            }),
            PlayerStatus::Starter
        );
        assert_eq!(
            classify_status(&StatusFacts {
                long_injury: true,
                ..base
            }),
            PlayerStatus::Rotation
        );
        let spare = StatusFacts {
            rank_in_group: 8,
            in_starting_xi: false,
            gap_to_cover: -15,
            appearances: 0,
            ..base
        };
        assert_eq!(classify_status(&spare), PlayerStatus::Surplus);
        assert_eq!(
            classify_status(&StatusFacts {
                wage_share_pct: 6,
                ..spare
            }),
            PlayerStatus::Unwanted
        );
        // A prospect behind the plan is not discarded for being young and costly.
        assert_eq!(
            classify_status(&StatusFacts {
                age: 19,
                wage_share_pct: 6,
                ..spare
            }),
            PlayerStatus::Surplus
        );
        let first_cover = StatusFacts {
            rank_in_group: 4,
            gap_to_cover: 0,
            ..spare
        };
        assert_eq!(classify_status(&first_cover), PlayerStatus::Rotation);
    }

    #[test]
    fn patience_fades_toward_the_close() {
        let shoppers = |phase| {
            (0..1_000)
                .filter(|&d| shops_without_pressing_need(phase, d))
                .count()
        };
        assert_eq!(shoppers(WindowPhase::Early), 350);
        assert_eq!(shoppers(WindowPhase::Mid), 1_000);
        assert!(!whole_market_is_open(WindowPhase::LastWeek));
        assert!(whole_market_is_open(WindowPhase::Last72h));
        assert_eq!(shoppers(WindowPhase::LastWeek), 1_000);
        assert_eq!(shoppers(WindowPhase::DeadlineDay), 1_000);
    }

    #[test]
    fn needs_and_overstretch() {
        assert!(has_unmet_need(&[1, 6, 6, 4]));
        assert!(!has_unmet_need(&[3, 6, 6, 4]));
        assert!(is_overstretched_at(121, 100, 5, 120));
        assert!(!is_overstretched_at(119, 100, 5, 120));
        assert!(is_overstretched_at(10, 100, -1, 120));
        assert!(!is_overstretched_at(500, 0, 5, 120));
    }
}

#[cfg(test)]
mod mission12_tests {
    use super::*;

    #[test]
    fn a_cheap_foreign_filler_is_not_worth_the_border_but_an_upgrade_or_a_hole_is() {
        assert!(worth_the_border(true, 100_000, 0, false));
        assert!(!worth_the_border(false, 100_000, 1, false));
        assert!(worth_the_border(false, 100_000, 4, false));
        assert!(worth_the_border(false, 100_000, 0, true));
        assert!(worth_the_border(false, 500_000, 0, false));
    }

    #[test]
    fn a_mainstay_is_kept_but_an_expiring_spare_goes_in_the_last_week() {
        let late = WindowPhase::LastWeek;
        assert!(!willing_to_sell(
            PlayerStatus::Starter,
            false,
            30,
            late,
            Some(40)
        ));
        assert!(willing_to_sell(
            PlayerStatus::Rotation,
            false,
            30,
            late,
            Some(40)
        ));
        assert!(!willing_to_sell(
            PlayerStatus::Rotation,
            false,
            30,
            WindowPhase::Mid,
            Some(40)
        ));
        assert!(!willing_to_sell(
            PlayerStatus::Rotation,
            false,
            30,
            late,
            Some(400)
        ));
    }

    fn spare(
        status: PlayerStatus,
        listed: bool,
        phase: WindowPhase,
        days: Option<i64>,
    ) -> AskingFacts {
        AskingFacts {
            value: 1_000_000,
            status,
            phase,
            contract_days_left: days,
            listed,
            age: 27,
            potential_gap: 0,
            morale: 70,
            seller_in_trouble: false,
            seller_cutting_costs: false,
            replacement_hard: false,
        }
    }

    #[test]
    fn a_listed_surplus_with_years_to_run_is_not_charged_twice_for_being_spare() {
        let m = asking_multiplier(&spare(
            PlayerStatus::Surplus,
            true,
            WindowPhase::Mid,
            Some(900),
        ));
        // 10 % listed + half of 6 %: 87 %, not the 78 % of the two discounts added.
        assert!((m - 0.87).abs() < 1e-9, "{m}");
    }

    #[test]
    fn the_last_day_does_not_cut_every_price() {
        for status in [
            PlayerStatus::Indispensable,
            PlayerStatus::Starter,
            PlayerStatus::Rotation,
        ] {
            let mid = asking_multiplier(&spare(status, false, WindowPhase::Mid, Some(900)));
            let last =
                asking_multiplier(&spare(status, false, WindowPhase::DeadlineDay, Some(900)));
            assert!(last >= mid, "{status:?}: {last} < {mid}");
        }
        let spare_mid = asking_multiplier(&spare(
            PlayerStatus::Surplus,
            false,
            WindowPhase::Mid,
            Some(900),
        ));
        let spare_last = asking_multiplier(&spare(
            PlayerStatus::Surplus,
            false,
            WindowPhase::DeadlineDay,
            Some(900),
        ));
        assert!(spare_last < spare_mid - 0.2);
    }

    #[test]
    fn a_relegated_seller_over_budget_asks_a_little_less() {
        let base = spare(PlayerStatus::Rotation, false, WindowPhase::Mid, Some(900));
        let cutting = AskingFacts {
            seller_cutting_costs: true,
            ..base
        };
        assert!((asking_multiplier(&base) - asking_multiplier(&cutting) - 0.04).abs() < 1e-9);
    }

    #[test]
    fn a_cheap_player_is_less_visible_abroad_and_a_star_is_seen_by_all() {
        assert!((foreign_visibility(0) - 0.6).abs() < 1e-9);
        assert!((foreign_visibility(300_000) - 0.6).abs() < 1e-9);
        assert!((foreign_visibility(3_000_000) - 1.0).abs() < 1e-9);
        assert!((foreign_visibility(u64::MAX) - 1.0).abs() < 1e-9);
        assert!(foreign_visibility(1_000_000) > 0.6 && foreign_visibility(1_000_000) < 1.0);
    }

    #[test]
    fn ambition_lifts_a_promoted_clubs_valuation_within_the_bounds() {
        let base = CeilingFacts {
            value: 1_000_000,
            improvement: 4,
            thin: false,
            age: 25,
            phase: WindowPhase::Mid,
            alternatives: 1,
            budget: i64::MAX,
            ambition: 0.0,
        };
        let promoted = CeilingFacts {
            ambition: 0.04,
            ..base
        };
        assert!((ceiling_multiplier(&promoted) - ceiling_multiplier(&base) - 0.04).abs() < 1e-9);
        let huge = CeilingFacts {
            ambition: 10.0,
            ..base
        };
        assert!(ceiling_multiplier(&huge) <= CEILING_MAX);
    }

    #[test]
    fn a_relegated_club_is_overstretched_at_its_budget_not_a_fifth_above() {
        assert!(!is_overstretched_at(110, 100, 1, 120));
        assert!(is_overstretched_at(110, 100, 1, 100));
        assert!(is_overstretched_at(1, 100, -1, 120), "debt always counts");
    }
}
