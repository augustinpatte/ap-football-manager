//! An AI club reading the market through Squad Intelligence.
//!
//! The sweep used to ask, per target, whether he was rated above the buyer's first cover in his
//! broad group. A club with two keepers of 85 and 83 and a striker of 61 therefore wanted a keeper
//! of 88 more than a striker of 78. The buyer's view answers the question the club actually has:
//! what would this player add to the side it fields, priced against what the club can spend.
//!
//! One view is built per buyer per sweep, from its own squad and its own strategy profile; what a
//! target would add is worked out once and kept (it does not depend on the price), and the price
//! is applied as often as the haggling needs.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::NaiveDate;
use domain::player::Player;
use domain::team::Team;

use super::ClubProfile;
use crate::game::Game;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::{MIN_GAIN, Marginal, Price, SigningCase};

/// Where each club's players stand in `game.players`. The sweep never adds or removes a player, and
/// updates the two clubs of a completed deal itself.
pub(super) fn squad_indices_by_club(game: &Game) -> HashMap<String, Vec<usize>> {
    let mut by_club: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, player) in game.players.iter().enumerate() {
        if let Some(club) = player.team_id.as_deref() {
            by_club.entry(club.to_string()).or_default().push(index);
        }
    }
    by_club
}

/// One buyer's reading of the targets in front of it.
pub(super) struct BuyerView {
    squad: ClubSquad,
    /// What each target would add, by player id. `None`: he is not a senior the model can place.
    memo: RefCell<HashMap<String, Option<Marginal>>>,
}

impl BuyerView {
    pub(super) fn new(
        game: &Game,
        squad: &[usize],
        team: &Team,
        profile: ClubProfile,
        today: NaiveDate,
    ) -> Self {
        Self {
            squad: ClubSquad::from_players(
                squad.iter().map(|&index| &game.players[index]),
                team,
                today,
                Appetite::of(&profile),
            ),
            memo: RefCell::new(HashMap::new()),
        }
    }

    /// What `player` would add to this club's squad, before any price.
    pub(super) fn marginal(&self, player: &Player) -> Option<Marginal> {
        if let Some(known) = self.memo.borrow().get(&player.id) {
            return *known;
        }
        let marginal = self.squad.signing(player, 0.0);
        self.memo.borrow_mut().insert(player.id.clone(), marginal);
        marginal
    }

    /// The signing of `player` for `fee`, as this club values it. His wage is read as what he
    /// earns now, the figure a club can see.
    pub(super) fn appraise(&self, player: &Player, fee: u64) -> Option<SigningCase> {
        let marginal = self.marginal(player)?;
        Some(self.squad.price(
            &marginal,
            Price {
                fee,
                weekly_wage: player.wage(),
            },
        ))
    }

    /// The rating points a signing adds to the side (zero if he adds nothing).
    pub(super) fn points(&self, player: &Player) -> i32 {
        self.marginal(player).map_or(0, |m| m.points.max(0))
    }

    /// What a loan of `player` would add this season: a loan is for the present only.
    pub(super) fn gain_this_season(&self, player: &Player) -> f32 {
        self.squad.present_gain(player)
    }

    /// Whether a loan of `player` would make the side better, which is all a club asks of one.
    pub(super) fn worth_borrowing(&self, player: &Player) -> bool {
        self.gain_this_season(player) >= MIN_GAIN
    }
}
