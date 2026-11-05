//! Squad Intelligence: one shared reading of a squad, for every club that manages one.
//!
//! The AI clubs' transfer market, free-agent planning, renewals and sales, and the assistant who
//! works for the manager, all ask the same questions: *what is this squad short of, what would this
//! player add, what would it cost to lose him?* Each used to answer from its own group counts and
//! rating bars, which agreed with each other nowhere (a club "needed" a third goalkeeper because he
//! was better than its second). This module answers them once, from the team the club would
//! actually field.
//!
//! * [`model`] — the squad as a side playing a formation: who starts where (the lineup picker's own
//!   walk and the game's own positional fit), who covers whom, expected strength with absences,
//!   today and in one and two seasons.
//! * [`needs`] — where that side falls short of the club's own standard, slot by slot.
//! * [`impact`] — the marginal value of one signing, priced, and the cost of one sale.
//! * [`appetite`] — a club's tendency (contender, developing, constrained) as a tilt on horizon and
//!   price, never a script.
//! * [`perception`] — what an assistant can tell about a player he has not signed: a bounded,
//!   deterministic error that shrinks smoothly with his rating.
//!
//! The AI clubs read the world as it is; the assistant reads it through [`perception`]. The
//! decision policies that use the answers (the market's, the board's, the manager's delegation
//! modes) live with their callers: this module decides nothing and changes nothing.

pub(crate) mod appetite;
pub(crate) mod horizon;
pub(crate) mod impact;
pub(crate) mod model;
pub(crate) mod needs;
pub(crate) mod perception;

#[cfg(test)]
mod scenarios;
#[cfg(test)]
pub(crate) mod test_squads;

use chrono::NaiveDate;
use domain::player::Player;
use domain::team::Team;

use appetite::Appetite;
use impact::{Budget, Price, SaleCase, SigningCase, assess_sale, assess_signing};
use model::{Evaluation, Member, SquadModel};
use needs::Need;

/// A club's squad, assessed: the model, what it is worth as it stands, and the club's appetite.
/// Built once per decision (or once per market sweep for a club) and asked many questions.
#[derive(Debug, Clone)]
pub(crate) struct ClubSquad {
    pub model: SquadModel,
    pub base: Evaluation,
    pub appetite: Appetite,
    pub budget: Budget,
    today: NaiveDate,
}

impl ClubSquad {
    /// The squad of `team` made of `players` (any other club's players are ignored by the caller,
    /// not here: pass the club's own).
    pub(crate) fn from_players<'a>(
        players: impl Iterator<Item = &'a Player>,
        team: &Team,
        today: NaiveDate,
        appetite: Appetite,
    ) -> Self {
        let model = SquadModel::build(players, &team.formation, today, appetite.weights);
        let base = model.evaluate_now();
        Self {
            model,
            base,
            appetite,
            budget: Budget {
                cash: team.transfer_budget.min(team.finance).max(0),
                weekly_wage_budget: team.wage_budget,
            },
            today,
        }
    }

    /// The squad of the club `team_id` in `game`.
    pub(crate) fn of(game: &crate::game::Game, team_id: &str, appetite: Appetite) -> Option<Self> {
        let team = game.teams.iter().find(|team| team.id == team_id)?;
        let today = game.clock.current_date.date_naive();
        Some(Self::from_players(
            game.players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(team_id)),
            team,
            today,
            appetite,
        ))
    }

    /// What the squad is short of, most severe first.
    pub(crate) fn needs(&self) -> Vec<Need> {
        needs::needs(&self.model, &self.base)
    }

    /// What signing `player` at `price` would add, as this club reads him (`perceived_error`
    /// rating points off his true level; zero for a club that sees the world as it is).
    pub(crate) fn signing(
        &self,
        player: &Player,
        price: Price,
        perceived_error: f32,
    ) -> Option<SigningCase> {
        let candidate = Member::candidate(player, &self.model.slots, self.today, perceived_error)?;
        Some(self.signing_of(&candidate, price))
    }

    /// The same for a player already turned into a [`Member`].
    pub(crate) fn signing_of(&self, candidate: &Member, price: Price) -> SigningCase {
        assess_signing(
            &self.model,
            &self.base,
            candidate,
            price,
            self.budget,
            &self.appetite,
        )
    }

    /// What losing `player_id` would cost.
    pub(crate) fn sale(&self, player_id: &str) -> SaleCase {
        assess_sale(&self.model, &self.base, player_id)
    }
}
