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
use domain::player::{Player, Position};
use domain::team::Team;

use super::ClubProfile;
use crate::game::Game;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::{MIN_GAIN, Marginal, Price, SigningCase};

/// What a loan has to add to the side this season, in the squad model's rating points: twice the
/// floor under any transfer. A loan is a playing role, and the model's reading of one is exact
/// while a manager's selection is not: he misjudges a deputy by up to nine points, rests a tired
/// player, changes his mind. Taken at the transfer floor, a loan is a marginal starter who is on
/// the bench by the second month; the P4.7 long-career study found that the share of 60-day loans
/// that went idle (under 180 minutes) rose from 21 % to 41 % over ten seasons, almost all of them
/// arriving behind the borrower's starters, and that the model's own gain predicted it: loans
/// valued under 0.2 were idle 36 to 41 % of the time, loans over 0.4 from 0 to 19 %. At twice the
/// floor the same ten seasons play the same minutes on loan (a tenth fewer at three times) in a
/// quarter fewer loans, idle 9 to 25 %.
const LOAN_MIN_GAIN: f32 = 2.0 * MIN_GAIN;

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
    /// Each weak starting slot, with the rating points the model expects it to cost.
    starter_needs: Vec<(Position, f32)>,
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
        let squad = ClubSquad::from_players(
            squad
                .iter()
                .map(|&index| &game.players[index])
                .filter(|player| player.team_id.as_deref() == Some(team.id.as_str())),
            team,
            today,
            Appetite::of(&profile),
        );
        let starter_needs = squad
            .needs()
            .into_iter()
            .filter(|need| need.kind == crate::squad_intelligence::needs::NeedKind::Starter)
            .map(|need| (need.position, need.severity))
            .collect();
        Self {
            squad,
            starter_needs,
            memo: RefCell::new(HashMap::new()),
        }
    }

    /// A broad-group depth check cannot rule out a specialist for a weak starting slot. Both his
    /// natural position and any position he is trained to cover count.
    pub(super) fn has_starter_need_for(&self, player: &Player) -> bool {
        self.starter_need_severity_for(player).is_some()
    }

    /// The rating points the weak starting slot he would fill is expected to cost, if any.
    fn starter_need_severity_for(&self, player: &Player) -> Option<f32> {
        self.starter_needs
            .iter()
            .filter(|(position, _)| {
                *position == player.natural_position
                    || player.alternate_positions.contains(position)
            })
            .map(|(_, severity)| *severity)
            .max_by(f32::total_cmp)
    }

    /// What the club values a signing at, in rating points: the expected gain to the side, or the
    /// size of the weak starting slot he fixes. Expected points divide a specialist's contribution
    /// across the eleven, so a deep group can hide a hole in the side that is actually fielded;
    /// the slot's own severity keeps that improvement visible.
    pub(super) fn improvement(&self, player: &Player) -> i32 {
        let severity = self
            .starter_need_severity_for(player)
            .map_or(0, |severity| severity.round() as i32);
        self.points(player).max(severity)
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

    /// Measurement only: where the club's model and the game's own picker would place `player`
    /// over the horizon, for the harness's market trace. Never feeds a decision.
    pub(super) fn placement(
        &self,
        player: &Player,
    ) -> Option<crate::squad_intelligence::model::Placement> {
        self.squad.placement(player)
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
    ///
    /// Read from the marginal the sweep keeps for every target (the same one a purchase is priced
    /// on), so a borrower that finds nobody worth borrowing asks the model once per player, not
    /// once per player per day.
    pub(super) fn gain_this_season(&self, player: &Player) -> f32 {
        self.marginal(player)
            .map_or(0.0, |marginal| marginal.now_gain)
    }

    /// Whether `player` would start in the side the model expects this club to field.
    fn would_start(&self, player: &Player) -> bool {
        self.marginal(player)
            .is_some_and(|marginal| marginal.starts_at.is_some())
    }

    /// Whether a loan of `player` would make the side better by [`LOAN_MIN_GAIN`], which is what a
    /// club asks of one,
    /// and, for a player of the kind clubs lend for his minutes (`is_a_loan_for_minutes`, whatever
    /// the reason his own club listed him), would give them: he would start in the side the model
    /// expects to field. A prospect on the bench is the loan his club wanted to avoid, and a
    /// hard-sale listing of such a player is held to the same test by design.
    pub(super) fn worth_borrowing(&self, player: &Player, age: i32) -> bool {
        self.gain_this_season(player) >= LOAN_MIN_GAIN
            && (!super::loans::is_a_loan_for_minutes(player, age) || self.would_start(player))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a squad index captured before a player leaves the club
    /// When the buyer's view is rebuilt after the move
    /// Then the departed player is not included in the buyer's squad.
    #[test]
    fn a_departed_player_is_not_in_the_buyers_view() {
        let mut game = super::super::tests::acceptance_game("team1", false);
        let team = game
            .teams
            .iter()
            .find(|team| team.id == "team1")
            .unwrap()
            .clone();
        let player_id = game.players[0].id.clone();
        game.players[0].team_id = Some("team2".to_string());
        let view = BuyerView::new(
            &game,
            &[0],
            &team,
            ClubProfile::NEUTRAL,
            game.clock.current_date.date_naive(),
        );
        assert!(
            view.squad
                .model
                .members
                .iter()
                .all(|member| member.id != player_id)
        );
    }

    use crate::squad_intelligence::test_squads::{
        Squad, player as squad_player, team as squad_team, today,
    };
    use domain::player::SquadRole;

    /// The borrower's balanced squad (strikers of 75, 73 and 67 for a 4-4-2) in a game, and a
    /// prospect striker of `ovr`, aged 20 with room to grow, on the loan list of another club.
    fn borrower_and_prospect(ovr: u8) -> (Game, Team, Vec<usize>, Player) {
        use chrono::TimeZone;
        let mut prospect = squad_player("prospect", Position::Striker, ovr, 20);
        prospect.potential = ovr + 14;
        prospect.team_id = Some("parent".into());
        prospect.squad_role = SquadRole::Senior;
        let borrower = squad_team("4-4-2", 5_000_000, 600_000);
        let mut user = domain::manager::Manager::new(
            "user".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        user.hire("elsewhere".into());
        let clock = crate::clock::GameClock::new(
            chrono::Utc.from_utc_datetime(&today().and_hms_opt(12, 0, 0).unwrap()),
        );
        let mut game = Game::new(
            clock,
            user,
            vec![borrower.clone()],
            Squad::balanced().players,
            vec![],
            vec![],
        );
        game.players.push(prospect.clone());
        let squad: Vec<usize> = game
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.team_id.as_deref() == Some("club"))
            .map(|(i, _)| i)
            .collect();
        (game, borrower, squad, prospect)
    }

    /// Given a prospect who would start for the borrower, but by a margin of a point or two over
    /// the striker he displaces, and another who would start by a clear one
    /// When the club decides whether a loan is worth it
    /// Then it declines the first and takes the second: a loan is a playing role, and a manager's
    /// misjudgement of a deputy, a rest day or a change of mind takes it from a marginal starter.
    /// (The P4.7 study: loans the model valued under 0.2 points of the side were idle 36 to 41 %
    /// of the time, those over 0.4 from 0 to 19 %.)
    #[test]
    fn a_loan_for_minutes_needs_a_clear_margin_to_be_a_realistic_playing_role() {
        let verdict = |ovr: u8| {
            let (game, team, squad, prospect) = borrower_and_prospect(ovr);
            let view = BuyerView::new(&game, &squad, &team, ClubProfile::NEUTRAL, today());
            (
                view.gain_this_season(&prospect),
                view.worth_borrowing(&prospect, 20),
            )
        };
        let marginal = (60..=90)
            .map(|ovr| (ovr, verdict(ovr)))
            .find(|(_, (gain, _))| *gain >= MIN_GAIN && *gain < LOAN_MIN_GAIN)
            .expect("some rating adds a little");
        assert!(
            !marginal.1.1,
            "a marginal starter at {} was taken",
            marginal.0
        );
        let clear = (60..=95)
            .map(|ovr| (ovr, verdict(ovr)))
            .find(|(_, (gain, _))| *gain >= LOAN_MIN_GAIN)
            .expect("some rating adds a lot");
        assert!(clear.1.1, "a clear starter at {} was refused", clear.0);
    }
}
