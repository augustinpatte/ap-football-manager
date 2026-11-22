//! Why a club's player is, or is not, approached by the market.
//!
//! Four questions decide whether a buyer opens an approach, and they are kept apart:
//!
//! * **Appeal**: how visible a player is from what he *is* (ability, ceiling, age), set against the
//!   world's senior players. This module owns it ([`AppealBench`]).
//! * **Opportunity**: what makes him a bargain right now: listed, a contract running out, low
//!   morale (`bids::incoming_interest_score`). A player can be a bargain without being good.
//! * **Buyer interest and affordability**: whether *this* buyer needs him, would pay that for him
//!   and can carry it: judged per buyer in the sweep, for a user's player exactly as for an AI
//!   club's, once he is visible on appeal alone.
//! * **Seller willingness**: a user's player is the user's to sell, so the approach is made and the
//!   user answers; an AI seller's is `willing_to_sell`.
//!
//! The old single score ran the first two together and ignored the rest, so a healthy, unlisted star
//! on a long contract could earn at most 20 points against a bar of 35: no buyer ever looked at him.
//!
//! This module also holds the funnel the harness reads: for the user's players, how many candidates
//! the sweep scored, how many it shortlisted, and where each buyer turned the rest away. That part is
//! measurement only, like the participation counters in `market`: nothing counted feeds a decision.

use domain::player::{Player, SquadRole};
use std::sync::atomic::{AtomicU64, Ordering};

/// One question the sweep asks about a user-owned target, in the order it asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Funnel {
    /// A player of any club scored for the shortlist.
    PlayersScored,
    /// A player of the user's club scored.
    UserPlayersScored,
    /// ...dropped because his score was under the shortlist threshold.
    UserBelowThreshold,
    /// ...kept on the shortlist.
    UserShortlisted,
    /// ...kept on the shortlist for his sporting appeal alone.
    UserShortlistedOnAppeal,
    /// Buyers that shopped on a day, summed over days.
    BuyersShopping,
    /// A buyer weighed a shortlisted user player.
    PairsWeighed,
    /// ...it was the buyer's own player, or he had moved today.
    RejectedMoved,
    /// ...the buyer was already borrowing him.
    RejectedLoanTarget,
    /// ...the inbox throttle: one club per player, a cap a day, a club in cooldown or in talks.
    RejectedThrottle,
    /// ...the buyer has no sporting reason (stature, need, depth).
    RejectedNoInterest,
    /// ...the club cannot use him: no starting need, or he adds too little to the side.
    RejectedNoCase,
    /// ...the club is overstretched or would not spend that much on one player.
    RejectedSpending,
    /// ...the fee is more than the buyer would pay for him.
    RejectedPrice,
    /// ...the fee is beyond the buyer's transfer budget or cash.
    RejectedBudget,
    /// ...the player would not agree terms with the buyer.
    RejectedTerms,
    /// ...the buyer cannot carry his wage.
    RejectedWage,
    /// ...the buyer's books cannot carry the deal.
    RejectedBooks,
    /// ...the pair passed every test.
    Passed,
    /// An incoming permanent offer was opened to the user.
    OfferOpened,
    /// An incoming loan offer was opened to the user.
    LoanOfferOpened,
}

pub const FUNNEL_STAGES: usize = 21;

/// The names of the funnel's counters, in order, for the harness's report.
pub const FUNNEL_NAMES: [&str; FUNNEL_STAGES] = [
    "players_scored",
    "user_players_scored",
    "user_below_threshold",
    "user_shortlisted",
    "user_shortlisted_on_appeal",
    "buyers_shopping",
    "pairs_weighed",
    "rejected_moved",
    "rejected_loan_target",
    "rejected_throttle",
    "rejected_no_interest",
    "rejected_no_case",
    "rejected_spending",
    "rejected_price",
    "rejected_budget",
    "rejected_terms",
    "rejected_wage",
    "rejected_books",
    "passed",
    "offer_opened",
    "loan_offer_opened",
];

static FUNNEL: [AtomicU64; FUNNEL_STAGES] = [const { AtomicU64::new(0) }; FUNNEL_STAGES];

pub(super) fn count(stage: Funnel) {
    FUNNEL[stage as usize].fetch_add(1, Ordering::Relaxed);
}

/// The funnel's counters since the last [`reset_market_funnel`], in [`FUNNEL_NAMES`] order.
pub fn market_funnel() -> [u64; FUNNEL_STAGES] {
    std::array::from_fn(|index| FUNNEL[index].load(Ordering::Relaxed))
}

pub fn reset_market_funnel() {
    for counter in &FUNNEL {
        counter.store(0, Ordering::Relaxed);
    }
}

/// The shortlist's bar, shared by the opportunity score and appeal.
pub(super) const VISIBLE_APPEAL: i32 = 35;

/// A player is visible on appeal from this share of the world's seniors upward (the top fifth).
const VISIBLE_FROM_SHARE: f64 = 0.80;

/// Appeal at the visible share, and how many points the very best add on top of it.
const APPEAL_FLOOR: i32 = VISIBLE_APPEAL;
const APPEAL_SPAN: f64 = 25.0;

/// What a player is worth to a buyer judged on ability alone, in tenths of a rating point (the
/// shared notion of quality: his rating and the credited part of his ceiling).
fn quality_tenths(ovr: u8, potential: u8, age: i32) -> i32 {
    (crate::value_scale::quality(ovr, potential, age) * 10.0).round() as i32
}

/// Visibility fades with age: a buyer looks past a player near the end of his career.
fn age_discount(age: i32) -> i32 {
    match age {
        ..=31 => 0,
        32..=33 => 10,
        34..=35 => 25,
        _ => 45,
    }
}

/// The world's senior players by quality, against which one player's appeal is read. Built once a
/// sweep; a player's appeal does not depend on who is buying.
pub(super) struct AppealBench {
    qualities: Vec<i32>,
}

impl AppealBench {
    pub(super) fn new<'a>(seniors: impl Iterator<Item = (&'a Player, i32)>) -> Self {
        let mut qualities: Vec<i32> = seniors
            .filter(|(player, _)| {
                player.team_id.is_some()
                    && !player.retired
                    && player.squad_role == SquadRole::Senior
            })
            .map(|(player, age)| quality_tenths(player.ovr, player.potential, age))
            .collect();
        qualities.sort_unstable();
        Self { qualities }
    }

    /// How visible a player is to the market from what he is, 0 (not at all) to about 60. Zero
    /// below the top fifth of the world's seniors; from there the bar of 35 to 60 for the best,
    /// less the age discount. Nothing about his contract, listing or morale enters.
    pub(super) fn appeal(&self, player: &Player, age: i32) -> i32 {
        if self.qualities.is_empty() || player.squad_role != SquadRole::Senior {
            return 0;
        }
        let quality = quality_tenths(player.ovr, player.potential, age);
        let below = self.qualities.partition_point(|q| *q < quality);
        let share = below as f64 / self.qualities.len() as f64;
        if share < VISIBLE_FROM_SHARE {
            return 0;
        }
        let points = APPEAL_FLOOR as f64
            + (share - VISIBLE_FROM_SHARE) / (1.0 - VISIBLE_FROM_SHARE) * APPEAL_SPAN;
        (points.round() as i32 - age_discount(age)).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{Player, PlayerAttributes, Position};

    fn player(ovr: u8, potential: u8) -> Player {
        let attributes = PlayerAttributes {
            pace: 50,
            stamina: 50,
            strength: 50,
            agility: 50,
            passing: 50,
            shooting: 50,
            tackling: 50,
            dribbling: 50,
            defending: 50,
            positioning: 50,
            vision: 50,
            decisions: 50,
            composure: 50,
            aggression: 50,
            teamwork: 50,
            leadership: 50,
            handling: 50,
            reflexes: 50,
            aerial: 50,
        };
        let mut p = Player::new(
            format!("p-{ovr}-{potential}"),
            "P".into(),
            "P".into(),
            "1999-01-01".into(),
            "GB".into(),
            Position::Striker,
            attributes,
        );
        p.ovr = ovr;
        p.potential = potential;
        p.team_id = Some("club".into());
        p
    }

    /// A world of 1,000 seniors, 20 at every rating from 50 to 99, all 27.
    fn bench() -> AppealBench {
        let world: Vec<Player> = (50..=99u8)
            .flat_map(|o| (0..20).map(move |_| player(o, o)))
            .collect();
        AppealBench::new(world.iter().map(|p| (p, 27)))
    }

    /// Given a world of seniors,
    /// When a star in his prime is read for appeal (no listing, contract or morale is even an input),
    /// Then he clears the shortlist's bar; and an ordinary player does not.
    #[test]
    fn a_star_is_visible_on_ability_alone_and_an_ordinary_player_is_not() {
        let bench = bench();

        assert!(bench.appeal(&player(92, 92), 27) >= VISIBLE_APPEAL);
        assert_eq!(bench.appeal(&player(70, 70), 27), 0);
    }

    /// Given the same player at 27 and at 36,
    /// When appeal is read,
    /// Then the veteran is no longer visible: a buyer looks past the end of a career.
    #[test]
    fn visibility_fades_with_age() {
        let bench = bench();
        let star = player(92, 92);

        assert!(bench.appeal(&star, 27) >= VISIBLE_APPEAL);
        assert!(bench.appeal(&star, 36) < VISIBLE_APPEAL);
        assert!(bench.appeal(&star, 33) < bench.appeal(&star, 27));
    }

    /// Given a teenager far from his ceiling and a veteran of the same current rating,
    /// When appeal is read,
    /// Then the teenager is the more visible: he is bought for what he will be.
    #[test]
    fn a_ceiling_counts_for_a_teenager_and_not_for_a_veteran() {
        let bench = bench();

        assert!(bench.appeal(&player(80, 99), 19) >= VISIBLE_APPEAL);
        assert_eq!(bench.appeal(&player(80, 99), 29), 0);
    }

    /// Given players of rising rating,
    /// When appeal is read,
    /// Then it never falls as ability rises.
    #[test]
    fn appeal_never_falls_as_ability_rises() {
        let bench = bench();

        let appeals: Vec<i32> = (50..=99).map(|o| bench.appeal(&player(o, o), 27)).collect();

        assert!(appeals.windows(2).all(|w| w[0] <= w[1]), "{appeals:?}");
        assert!(appeals.last() > appeals.first());
    }

    /// Given an academy player of high quality,
    /// When appeal is read,
    /// Then it is zero: the market approaches seniors.
    #[test]
    fn an_academy_player_has_no_appeal() {
        let bench = bench();
        let mut prospect = player(92, 95);
        prospect.squad_role = SquadRole::Youth;

        assert_eq!(bench.appeal(&prospect, 18), 0);
    }

    /// Given an empty world,
    /// When appeal is read,
    /// Then it is zero rather than a division by nothing.
    #[test]
    fn an_empty_bench_gives_no_appeal() {
        let bench = AppealBench::new(std::iter::empty());

        assert_eq!(bench.appeal(&player(99, 99), 27), 0);
    }

    /// Given the funnel's stages and names,
    /// When they are listed,
    /// Then there is one name per stage and the last stage indexes the last counter.
    #[test]
    fn every_stage_has_a_name_and_a_counter() {
        assert_eq!(FUNNEL_NAMES.len(), FUNNEL_STAGES);
        assert_eq!(Funnel::LoanOfferOpened as usize, FUNNEL_STAGES - 1);
    }
}
