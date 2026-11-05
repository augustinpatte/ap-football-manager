//! An AI club reading its own players as things it might sell, through Squad Intelligence.
//!
//! What a player is to his club ([`PlayerStatus`]) decides whether it will sell him and what it asks.
//! It used to be read from his rank among the players of the same broad position: the second of two
//! goalkeepers was always "first cover", whoever the first was, and a club's best forward was a
//! starter even when a forward as good sat on its bench. Here it is read from what the sale costs
//! the side the club fields, once the club has bought the cheapest replacement it could:
//!
//! * a player nobody of his standard replaces, even on the cheap market, is **indispensable**;
//! * one whose absence is clearly felt is a **starter**; one whose absence is a little felt is
//!   **rotation** or **backup**;
//! * one the club would not miss is **surplus**, and **unwanted** if he is also dear (or the club's
//!   books are in trouble).
//!
//! A club also sells an important player when the offer is exceptional and it can put the money to
//! work: [`rational_sale`].

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::NaiveDate;
use domain::player::Player;
use domain::team::Team;

use super::ClubProfile;
use super::realism::PlayerStatus;
use crate::game::Game;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::{REINVESTMENT_RATIO, STARTER_LOSS, SURPLUS_LOSS, SaleCase};
use crate::squad_intelligence::needs::NeedKind;

/// A sale costing at least this much makes a player rotation.
const ROTATION_LOSS: f32 = 0.06;

/// A wage this many per cent of the wage budget makes a spare costly.
const COSTLY_WAGE_PCT: i64 = 4;

/// A fee this many per cent of a player's value is an exceptional offer.
const EXCEPTIONAL_FEE_PCT: u64 = 125;

/// Whether an asking price is an exceptional one for a player of this value.
pub(super) fn is_exceptional(fee: u64, value: u64) -> bool {
    fee.saturating_mul(100) >= value.saturating_mul(EXCEPTIONAL_FEE_PCT)
}

/// A prospect this young, with this much to grow, is brought on rather than counted as a spare.
const PROSPECT_AGE: i32 = 22;
const PROSPECT_ROOM: i32 = 8;

/// What a player is to his club, from what his sale would cost the side.
pub(super) fn status_from_sale(
    case: &SaleCase,
    player: &Player,
    age: i32,
    wage_share_pct: i64,
    club_in_trouble: bool,
) -> PlayerStatus {
    if case.critical {
        return PlayerStatus::Indispensable;
    }
    if case.net_loss >= STARTER_LOSS {
        return PlayerStatus::Starter;
    }
    if case.net_loss >= ROTATION_LOSS {
        return PlayerStatus::Rotation;
    }
    if case.net_loss >= SURPLUS_LOSS {
        return PlayerStatus::Backup;
    }
    // Not missed. A young player with room to grow is a prospect to be brought on, not a spare.
    let prospect =
        age <= PROSPECT_AGE && i32::from(player.potential) - i32::from(player.ovr) >= PROSPECT_ROOM;
    if prospect {
        return PlayerStatus::Backup;
    }
    if age >= 24 && (wage_share_pct >= COSTLY_WAGE_PCT || club_in_trouble) {
        return PlayerStatus::Unwanted;
    }
    PlayerStatus::Surplus
}

/// The status of each senior of a club, from its own squad. `in_trouble` is the club's books.
pub(super) fn club_statuses_by_impact(
    squad: &ClubSquad,
    club_id: &str,
    players: &[&Player],
    wage_budget: i64,
    in_trouble: bool,
    age_of: &dyn Fn(&Player) -> i32,
) -> HashMap<String, PlayerStatus> {
    let sales = squad.sales(club_id);
    let cases: HashMap<&str, &SaleCase> =
        sales.iter().map(|(id, case)| (id.as_str(), case)).collect();
    let mut statuses = HashMap::new();
    for player in players {
        let Some(case) = cases.get(player.id.as_str()) else {
            continue;
        };
        let wage_share_pct = if wage_budget > 0 {
            i64::from(player.wage()) * 100 / wage_budget
        } else {
            0
        };
        statuses.insert(
            player.id.clone(),
            status_from_sale(case, player, age_of(player), wage_share_pct, in_trouble),
        );
    }
    statuses
}

/// Sellers' squads, built on demand and kept for the sweep.
pub(super) struct SaleView {
    squads: RefCell<HashMap<String, ClubSquad>>,
}

impl SaleView {
    pub(super) fn new() -> Self {
        Self {
            squads: RefCell::new(HashMap::new()),
        }
    }

    /// Whether the seller would let `player` go for `fee` although he is not for sale on the usual
    /// terms: the offer is exceptional, he is not indispensable, and the seller has weaknesses
    /// elsewhere that the money would mend by a good deal more than he is worth to the side.
    pub(super) fn rational_sale(
        &self,
        game: &Game,
        seller: &Seller<'_>,
        player: &Player,
        fee: u64,
    ) -> bool {
        if !is_exceptional(fee, player.market_value) {
            return false;
        }
        let mut squads = self.squads.borrow_mut();
        let seller_squad = squads.entry(seller.team.id.clone()).or_insert_with(|| {
            ClubSquad::from_players(
                seller.squad.iter().map(|&index| &game.players[index]),
                seller.team,
                seller.today,
                Appetite::of(&seller.profile),
            )
        });
        let case = seller_squad.sale(&player.id);
        if case.critical {
            return false;
        }
        reinvestment_gain(seller_squad, &player.id) >= REINVESTMENT_RATIO * case.net_loss
    }
}

/// The club that would sell: its row, where its players stand in `game.players`, its strategy.
pub(super) struct Seller<'a> {
    pub team: &'a Team,
    pub squad: &'a [usize],
    pub profile: ClubProfile,
    pub today: NaiveDate,
}

/// What the club could gain by mending its two worst weaknesses other than the sold player's own
/// slot: the weakest starter slots and the weakest cover, in strength points.
pub(crate) fn reinvestment_gain(squad: &ClubSquad, sold_player: &str) -> f32 {
    let sold_slot = squad.base.years[0].starter_index.iter().position(|index| {
        index.is_some_and(|i| {
            squad
                .model
                .members
                .get(i)
                .is_some_and(|member| member.id == sold_player)
        })
    });
    squad
        .needs()
        .into_iter()
        .filter(|need| need.kind == NeedKind::Starter && Some(need.slot) != sold_slot)
        .take(2)
        .map(|need| need.severity / crate::squad_intelligence::model::SLOTS as f32)
        .sum()
}
