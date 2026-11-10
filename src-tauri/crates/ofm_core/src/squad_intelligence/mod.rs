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
pub(crate) mod assignment;
pub(crate) mod horizon;
pub(crate) mod impact;
pub(crate) mod lineup_gap;
pub(crate) mod model;
pub(crate) mod needs;
pub(crate) mod perception;

#[cfg(test)]
mod scenarios;
#[cfg(test)]
pub(crate) mod test_squads;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use chrono::NaiveDate;
use domain::player::Player;
use domain::team::Team;

use appetite::Appetite;
use impact::{Budget, Marginal, SaleCase, assess_sale, assess_signing};
use model::{Availability, Evaluation, Member, SquadModel};
use needs::Need;

type Sales = Arc<Vec<(String, SaleCase)>>;
type SalesCache = HashMap<String, (u64, Sales)>;

/// The sale cases of each club's latest squad, by club id: an answer is a pure function of the
/// squad's fingerprint, so a stale entry is never read, only replaced.
static SALES_CACHE: LazyLock<Mutex<SalesCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// What each (squad, player) pair adds, by the fingerprints of both. A pure function of its key, so
/// an entry is never stale, only unused; emptied when it grows past [`MARGINAL_CACHE_LIMIT`].
static MARGINAL_CACHE: LazyLock<Mutex<HashMap<(u64, u64), Marginal>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Entries the marginal cache holds before it starts again (a few tens of megabytes at most).
const MARGINAL_CACHE_LIMIT: usize = 400_000;

/// A club's squad, assessed: the model, what it is worth as it stands, and the club's appetite.
/// Built once per decision (or once per market sweep for a club) and asked many questions.
#[derive(Debug, Clone)]
pub(crate) struct ClubSquad {
    pub model: SquadModel,
    /// [`SquadModel::fingerprint`], taken once: the squad does not change under a `ClubSquad`.
    fingerprint: u64,
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
        Self::build(players, team, today, appetite, Availability::Today)
    }

    /// The squad as planning sees it ([`Availability::Planning`]): for what a player is worth to
    /// his club (status, listing, renewal, sale), which should not change with every knock.
    pub(crate) fn for_planning<'a>(
        players: impl Iterator<Item = &'a Player>,
        team: &Team,
        today: NaiveDate,
        appetite: Appetite,
    ) -> Self {
        Self::build(players, team, today, appetite, Availability::Planning)
    }

    fn build<'a>(
        players: impl Iterator<Item = &'a Player>,
        team: &Team,
        today: NaiveDate,
        appetite: Appetite,
        availability: Availability,
    ) -> Self {
        let model = SquadModel::build(
            players,
            &team.formation,
            today,
            appetite.weights,
            availability,
        );
        let base = model.evaluate_now();
        Self {
            fingerprint: model.fingerprint(),
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

    /// The squad of the club `team_id` in `game`, as it is today.
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

    /// The squad of the club `team_id` in `game`, as planning sees it.
    pub(crate) fn of_for_planning(
        game: &crate::game::Game,
        team_id: &str,
        appetite: Appetite,
    ) -> Option<Self> {
        let team = game.teams.iter().find(|team| team.id == team_id)?;
        let today = game.clock.current_date.date_naive();
        Some(Self::for_planning(
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

    /// What signing `player` would add, as this club reads him (`perceived_error` rating points
    /// off his true level; zero for a club that sees the world as it is). Price it with
    /// [`Marginal::priced`]. Kept for as long as neither the squad nor the player changes: a
    /// market sweep asks the same club about the same players every day of a window.
    pub(crate) fn signing(&self, player: &Player, perceived_error: f32) -> Option<Marginal> {
        let candidate = Member::candidate(player, &self.model.slots, self.today, perceived_error)?;
        let key = (self.fingerprint, SquadModel::fingerprint_of(&candidate));
        {
            let cache = MARGINAL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(known) = cache.get(&key) {
                return Some(*known);
            }
        }
        let marginal = self.signing_of(&candidate);
        let mut cache = MARGINAL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= MARGINAL_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, marginal);
        Some(marginal)
    }

    /// The same for a player already turned into a [`Member`].
    pub(crate) fn signing_of(&self, candidate: &Member) -> Marginal {
        assess_signing(&self.model, &self.base, candidate)
    }

    /// Measurement only: where the model and the game's own picker would place `player`, and whom
    /// he would displace, for the harness's market trace. Never feeds a decision.
    pub(crate) fn placement(&self, player: &Player) -> Option<model::Placement> {
        let candidate = Member::candidate(player, &self.model.slots, self.today, 0.0)?;
        Some(self.model.placement_of(&candidate))
    }

    /// What `player` would add to this season's side alone: the gain of a loan, which is over before
    /// the future matters.
    pub(crate) fn present_gain(&self, player: &Player) -> f32 {
        let Some(candidate) = Member::candidate(player, &self.model.slots, self.today, 0.0) else {
            return 0.0;
        };
        self.signing_of(&candidate).now_gain
    }

    /// Whether `player` would start for this club today, in the side the model (and so the lineup
    /// picker) fields: a loan for his minutes is only as good as the games he is given.
    pub(crate) fn starts_today(&self, player: &Player) -> bool {
        Member::candidate(player, &self.model.slots, self.today, 0.0)
            .is_some_and(|candidate| self.signing_of(&candidate).starts_at.is_some())
    }

    /// Set a marginal value against a price, for this club.
    pub(crate) fn price(&self, marginal: &Marginal, price: impact::Price) -> impact::SigningCase {
        marginal.priced(price, self.budget, &self.appetite)
    }

    /// What losing each senior would cost, kept for as long as the squad is the same. Asking for
    /// all 26 of a club's players is 26 evaluations of the side; a club's squad does not change
    /// from one day to the next, so the answer is kept by club until its fingerprint moves.
    pub(crate) fn sales(&self, club_id: &str) -> Arc<Vec<(String, SaleCase)>> {
        let mut cache = SALES_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((kept, sales)) = cache.get(club_id)
            && *kept == self.fingerprint
        {
            return Arc::clone(sales);
        }
        let sales: Sales = Arc::new(
            self.model
                .members
                .iter()
                .filter(|member| !member.loaned_in)
                .map(|member| (member.id.clone(), self.sale(&member.id)))
                .collect(),
        );
        cache.insert(club_id.to_string(), (self.fingerprint, Arc::clone(&sales)));
        sales
    }

    /// What losing `player_id` would cost.
    pub(crate) fn sale(&self, player_id: &str) -> SaleCase {
        assess_sale(&self.model, &self.base, player_id)
    }
}
