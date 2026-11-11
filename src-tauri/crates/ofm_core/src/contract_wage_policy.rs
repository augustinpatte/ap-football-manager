use crate::contracts::RenewalFinancialProjection;
use crate::finances::{
    calc_cash_runway_weeks, calc_wages, player_weekly_wage_for_team, weekly_commitment_at_wage,
};
use crate::game::Game;
use domain::team::Team;

pub(crate) const WAGE_SOFT_CAP_PCT: i64 = 110;
/// What a contract the squad floor waives the board's policy for may add to the weekly bill, as a
/// share (per cent) of the wage a typical senior of the club is paid (its median). A club short
/// of a keeper needs a keeper, not the best-paid player on the market: the waiver once covered
/// any wage at all, so a short squad could buy its way a whole budget past the board's policy.
pub(crate) const FLOOR_WAIVER_SCALE_PCT: i64 = 150;
pub(crate) const LEGACY_OVER_BUDGET_GRACE_PCT: i64 = 3;
const LEGACY_OVER_BUDGET_GRACE_MIN: i64 = 25_000;
const ERR_PLAYER_HAS_NO_TEAM: &str = "be.error.contracts.playerHasNoTeam";

fn backend_error_with_param(key: &str, param_name: &str, param_value: i64) -> String {
    let param_value = param_value.to_string();
    let mut message = String::with_capacity(key.len() + param_name.len() + param_value.len() + 2);
    message.push_str(key);
    message.push('?');
    message.push_str(param_name);
    message.push('=');
    message.push_str(&param_value);
    message
}

fn projected_wage_bills(
    game: &Game,
    team_id: &str,
    player: &domain::player::Player,
    offered_wage: u32,
) -> (i64, i64) {
    let current_bill = calc_wages(game, team_id);
    let current_contribution = player_weekly_wage_for_team(player, team_id);
    let offered_contribution = weekly_commitment_at_wage(player, team_id, i64::from(offered_wage));
    (
        current_bill,
        current_bill - current_contribution + offered_contribution,
    )
}

pub fn project_contract_offer_financial_impact(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
) -> RenewalFinancialProjection {
    let (current_bill, projected_bill) = projected_wage_bills(game, &team.id, player, offered_wage);
    let wage_budget = team.wage_budget;
    let soft_cap = (wage_budget * WAGE_SOFT_CAP_PCT) / 100;

    let current_cash_runway_weeks = calc_cash_runway_weeks(team.finance, -current_bill);
    let projected_cash_runway_weeks = calc_cash_runway_weeks(team.finance, -projected_bill);

    RenewalFinancialProjection {
        current_annual_wage_bill: current_bill,
        projected_annual_wage_bill: projected_bill,
        annual_wage_budget: wage_budget,
        annual_soft_cap: soft_cap,
        current_weekly_wage_spend: current_bill,
        projected_weekly_wage_spend: projected_bill,
        current_cash_runway_weeks,
        projected_cash_runway_weeks,
        currently_over_budget: current_bill > wage_budget,
        policy_allows: wage_policy_allows_projection(team, current_bill, projected_bill),
    }
}

pub fn wage_policy_allows_projection(team: &Team, current_bill: i64, projected_bill: i64) -> bool {
    let soft_cap = (team.wage_budget * WAGE_SOFT_CAP_PCT) / 100;

    if current_bill <= team.wage_budget {
        return projected_bill <= soft_cap;
    }

    if projected_bill <= current_bill {
        return true;
    }

    let legacy_grace = std::cmp::max(
        (team.wage_budget * LEGACY_OVER_BUDGET_GRACE_PCT) / 100,
        LEGACY_OVER_BUDGET_GRACE_MIN,
    );

    projected_bill <= current_bill + legacy_grace
}

pub fn renewal_wage_policy_allows(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
) -> bool {
    let (current_bill, projected_bill) = projected_wage_bills(game, &team.id, player, offered_wage);
    wage_policy_allows_projection(team, current_bill, projected_bill)
}

/// What the board says to paying a player a wage, to keep him or to sign him.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WagePolicyVerdict {
    /// The wage bill stays within the board's policy.
    WithinPolicy,
    /// Over the policy, allowed because the club would be below the squad
    /// floor in his group without him.
    OverPolicyToKeepSquadFloor,
    /// Over the policy, refused.
    OverPolicy,
}

impl WagePolicyVerdict {
    pub fn permits(self) -> bool {
        self != Self::OverPolicy
    }

    /// The line a manager is shown when the board waived its policy, for a
    /// deal they struck themselves.
    pub(crate) fn waiver_feedback_detail_key(self) -> Option<&'static str> {
        (self == Self::OverPolicyToKeepSquadFloor)
            .then_some("playerProfile.renewalFeedbackAcceptedToKeepSquadFloorDetail")
    }
}

/// Whether the board lets `team` pay `player` `offered_wage`.
///
/// The one rule for every contract a club offers to keep or sign a player —
/// the manager's renewals and free-agent signings, the assistant's delegated
/// renewals, an AI club's renewals and the squad floor's top-up. The wage
/// policy holds, except when the club would be below the squad floor in the
/// player's group without him (counted as the floor counts): a club that cannot
/// put a side out has no wage bill worth protecting.
pub fn wage_policy_verdict(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
) -> WagePolicyVerdict {
    // The wages of deals the club has agreed and that wait to register are as good as paid: a
    // free agent signed on top of them would spend the room a purchase has already used. The
    // registration of one of those deals is judged by `purchase_wage_policy_verdict`, which does
    // not read them, so no deal is counted against itself.
    let committed_bill =
        calc_wages(game, &team.id) + crate::transfers::pending_wage_commitments(game, &team.id);
    wage_policy_verdict_at_bill(game, team, player, offered_wage, committed_bill)
}

/// [`wage_policy_verdict`] when the caller already knows the club's `current_bill`
/// ([`calc_wages`]): a sweep that judges many candidates for one club works it out once
/// instead of scanning every player in the world for each of them. The verdict is the same.
pub(crate) fn wage_policy_verdict_at_bill(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
    current_bill: i64,
) -> WagePolicyVerdict {
    let projected_bill = current_bill - player_weekly_wage_for_team(player, &team.id)
        + weekly_commitment_at_wage(player, &team.id, i64::from(offered_wage));
    verdict_for_bills(
        team,
        current_bill,
        projected_bill,
        || floor_waiver_ceiling(game, &team.id),
        || crate::squad_floor::club_needs_him_for_the_floor(game, &team.id, player),
    )
}

/// The most the squad-floor waiver lets one contract add to `team_id`'s weekly bill:
/// [`FLOOR_WAIVER_SCALE_PCT`] of the median wage of its seniors, or no limit at a club with no
/// wages to take a median of (nothing to measure a scale by).
pub(crate) fn floor_waiver_ceiling(game: &Game, team_id: &str) -> i64 {
    let mut wages: Vec<i64> = game
        .players
        .iter()
        .filter(|player| {
            player.team_id.as_deref() == Some(team_id)
                && player.squad_role == domain::player::SquadRole::Senior
        })
        .map(|player| player_weekly_wage_for_team(player, team_id))
        .filter(|wage| *wage > 0)
        .collect();
    if wages.is_empty() {
        return i64::MAX;
    }
    wages.sort_unstable();
    wages[wages.len() / 2] * FLOOR_WAIVER_SCALE_PCT / 100
}

/// The same verdict for a player *joining* `team` who is not on its wage bill yet,
/// though he may already be registered to it — a youngster taken into its academy.
/// His whole wage is new to the bill. [`wage_policy_verdict`] takes a registered
/// player's current wage off before adding the offer, which is right for a renewal
/// and would let a newcomer pay for himself.
pub(crate) fn joining_wage_policy_verdict(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
) -> WagePolicyVerdict {
    let current_bill = calc_wages(game, &team.id);
    let projected_bill =
        current_bill + weekly_commitment_at_wage(player, &team.id, i64::from(offered_wage));
    verdict_for_bills(
        team,
        current_bill,
        projected_bill,
        || floor_waiver_ceiling(game, &team.id),
        || crate::squad_floor::club_needs_him_for_the_floor(game, &team.id, player),
    )
}

/// The same verdict for a club *buying* the player, who will then be on its books at
/// the full `offered_wage`. A player already on loan at the buyer is counted at his
/// loan share today and at the whole wage after the purchase, which
/// [`wage_policy_verdict`] would not do (it keeps the loan split).
pub fn purchase_wage_policy_verdict(
    game: &Game,
    team: &Team,
    player: &domain::player::Player,
    offered_wage: u32,
) -> WagePolicyVerdict {
    BuyerWageFacts::of(game, &team.id).purchase_verdict(team, player, offered_wage)
}

/// What judging a buyer's purchases needs to know about the buyer: its wage bill and its
/// senior counts. A sweep that judges many players for one club (the AI market) works
/// these out once and asks for each verdict without scanning the world again; one
/// purchase goes through [`purchase_wage_policy_verdict`], which builds them itself.
pub(crate) struct BuyerWageFacts {
    current_bill: i64,
    seniors: [usize; 4],
    /// The most a floor waiver may add to the bill ([`floor_waiver_ceiling`]); no limit for a
    /// sweep that signs the cheapest player who will do and so cannot overpay for the floor.
    floor_waiver_ceiling: i64,
}

impl BuyerWageFacts {
    pub(crate) fn of(game: &Game, team_id: &str) -> Self {
        Self {
            floor_waiver_ceiling: floor_waiver_ceiling(game, team_id),
            ..Self::new(
                calc_wages(game, team_id),
                crate::squad_floor::senior_counts(game, team_id),
            )
        }
    }

    /// [`Self::of`] for a purchase being agreed now: the bill also holds the wages of the club's
    /// deals that are agreed and wait to register. Not for a registration, which is one of them.
    pub(crate) fn of_committed(game: &Game, team_id: &str) -> Self {
        Self {
            floor_waiver_ceiling: floor_waiver_ceiling(game, team_id),
            ..Self::new(
                calc_wages(game, team_id)
                    + crate::transfers::pending_wage_commitments(game, team_id),
                crate::squad_floor::senior_counts(game, team_id),
            )
        }
    }

    /// The weekly wage bill the verdicts are judged against.
    pub(crate) fn current_bill(&self) -> i64 {
        self.current_bill
    }

    /// For a caller that already holds the senior counts (the market sweep's depth map), and
    /// that chooses the cheapest player who will do for the floor itself: no limit on the waiver.
    pub(crate) fn new(current_bill: i64, seniors: [usize; 4]) -> Self {
        Self {
            current_bill,
            seniors,
            floor_waiver_ceiling: i64::MAX,
        }
    }

    pub(crate) fn purchase_verdict(
        &self,
        team: &Team,
        player: &domain::player::Player,
        offered_wage: u32,
    ) -> WagePolicyVerdict {
        let current_contribution = player_weekly_wage_for_team(player, &team.id);
        let projected_bill = self.current_bill - current_contribution + i64::from(offered_wage);
        verdict_for_bills(
            team,
            self.current_bill,
            projected_bill,
            || self.floor_waiver_ceiling,
            || crate::squad_floor::club_needs_him_given(self.seniors, &team.id, player),
        )
    }
}

/// The rule itself, once. A contract is within policy when the bill it makes stays within the
/// board's budget *and* the club's cash can carry the wage it adds
/// ([`SigningBooks::cash_carries_wage`](crate::ai_finance_guard::SigningBooks::cash_carries_wage)).
/// Otherwise the squad floor may waive the policy, for a wage no larger than the club's own pay
/// scale ([`floor_waiver_ceiling`]) and only where the club would be below the floor without
/// him: a club that cannot put a side out has no bill worth protecting, but it needs a player,
/// not the dearest one. Both of the waiver's questions are asked only when the policy says no.
fn verdict_for_bills(
    team: &Team,
    current_bill: i64,
    projected_bill: i64,
    floor_waiver_ceiling: impl FnOnce() -> i64,
    club_needs_him_for_the_floor: impl FnOnce() -> bool,
) -> WagePolicyVerdict {
    let added = projected_bill - current_bill;
    let carried = crate::ai_finance_guard::SigningBooks::estimated(team, current_bill)
        .cash_carries_wage(added);
    if carried && wage_policy_allows_projection(team, current_bill, projected_bill) {
        WagePolicyVerdict::WithinPolicy
    } else if added <= floor_waiver_ceiling() && club_needs_him_for_the_floor() {
        WagePolicyVerdict::OverPolicyToKeepSquadFloor
    } else {
        WagePolicyVerdict::OverPolicy
    }
}

pub fn renewal_wage_policy_error_message(team: &Team) -> String {
    backend_error_with_param(
        "be.error.contracts.boardWagePolicy",
        "budget",
        team.wage_budget,
    )
}

pub fn loan_borrower_wage_policy_error_message(borrower: &Team) -> String {
    backend_error_with_param(
        "be.error.transfers.loanBorrowerCannotAffordWages",
        "budget",
        borrower.wage_budget,
    )
}

pub fn project_renewal_financial_impact(
    game: &Game,
    player_id: &str,
    offered_wage: u32,
) -> Result<RenewalFinancialProjection, String> {
    let player = game
        .players
        .iter()
        .find(|player| player.id == player_id)
        .ok_or_else(|| "be.error.playerNotFound".to_string())?;
    let team_id = player
        .contract_club_id()
        .ok_or_else(|| ERR_PLAYER_HAS_NO_TEAM.to_string())?;
    let team = game
        .teams
        .iter()
        .find(|team| team.id == team_id)
        .ok_or_else(|| "be.error.teamNotFound".to_string())?;

    Ok(project_contract_offer_financial_impact(
        game,
        team,
        player,
        offered_wage,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::test_support::uniform_attributes;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{Player, Position};

    const MEDIAN_WAGE: u32 = 12_000;

    fn player(id: &str, position: Position, wage: u32, club: Option<&str>) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            position,
            uniform_attributes(60),
        );
        player.team_id = club.map(str::to_string);
        player.stage_contract_end(Some("2030-06-30".to_string()));
        player.stage_wage(wage);
        player
    }

    /// A club (`club`) whose seniors are `[goalkeepers, defenders, midfielders, forwards]`, every
    /// one on the same wage, with the given cash and a wage budget exactly equal to its bill.
    fn club(seniors: [usize; 4], cash: i64) -> Game {
        let mut team = Team::new(
            "club".to_string(),
            "Club".to_string(),
            "CLB".to_string(),
            "England".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            5_000,
        );
        team.finance = cash;
        let mut players = Vec::new();
        for (count, position, tag) in [
            (seniors[0], Position::Goalkeeper, "gk"),
            (seniors[1], Position::Defender, "df"),
            (seniors[2], Position::Midfielder, "mf"),
            (seniors[3], Position::Forward, "fw"),
        ] {
            for index in 0..count {
                players.push(player(
                    &format!("{tag}{index}"),
                    position.clone(),
                    MEDIAN_WAGE,
                    Some("club"),
                ));
            }
        }
        team.wage_budget = i64::from(MEDIAN_WAGE) * players.len() as i64;
        let mut manager = Manager::new(
            "m".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("club".to_string());
        Game::new(
            GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()),
            manager,
            vec![team],
            players,
            vec![],
            vec![],
        )
    }

    fn verdict(game: &Game, candidate: &Player, wage: u32) -> WagePolicyVerdict {
        wage_policy_verdict(game, &game.teams[0], candidate, wage)
    }

    /// Given a club with no forward at all, on its wage budget (so a wage a tenth above its pay
    ///   scale already breaks the soft cap), and a free forward
    /// When he asks a wage at the ceiling of the club's own pay scale, one above it, and ten times it
    /// Then the squad floor waives the board's policy for the first only: a club short of a
    ///      forward needs a forward, not the dearest one on the market.
    #[test]
    fn the_squad_floor_waives_the_policy_for_a_wage_on_the_clubs_scale_and_no_more() {
        let game = club([2, 4, 4, 0], 50_000_000);
        let forward = player("fa", Position::Forward, 0, None);
        let ceiling = (i64::from(MEDIAN_WAGE) * FLOOR_WAIVER_SCALE_PCT / 100) as u32;

        assert_eq!(
            verdict(&game, &forward, MEDIAN_WAGE),
            WagePolicyVerdict::WithinPolicy,
            "a wage on the pay scale fits the soft cap: nothing to waive"
        );
        assert_eq!(
            verdict(&game, &forward, ceiling),
            WagePolicyVerdict::OverPolicyToKeepSquadFloor,
            "the ceiling itself is allowed"
        );
        assert_eq!(
            verdict(&game, &forward, ceiling + 1),
            WagePolicyVerdict::OverPolicy
        );
        assert_eq!(
            verdict(&game, &forward, MEDIAN_WAGE * 10),
            WagePolicyVerdict::OverPolicy,
            "ten times the club's pay scale is not a floor signing"
        );
    }

    /// Given a club that is not short of a forward, on its wage budget
    /// When a forward asks a wage that breaks the soft cap
    /// Then there is nothing to waive for: the policy refuses.
    #[test]
    fn a_club_that_is_not_short_has_no_waiver() {
        let game = club([2, 5, 5, 4], 50_000_000);
        let forward = player("fa", Position::Forward, 0, None);

        assert_eq!(
            verdict(&game, &forward, MEDIAN_WAGE * 2),
            WagePolicyVerdict::OverPolicy
        );
    }

    /// A sound club with room in its budget, but only `cash` in the bank and a gate that does not
    /// pay its wages.
    fn sound_club_with_cash(cash: i64) -> Game {
        let mut game = club([3, 5, 5, 3], cash);
        game.teams[0].wage_budget *= 2;
        game
    }

    /// Given a club that earns far less than it pays, with room in its wage budget
    /// When a midfielder is signed on a wage the budget allows, with too little cash to meet a
    ///   season of the loss and then with plenty
    /// Then the board refuses the first and allows the second: a wage is paid every week of a
    ///      contract, and the budget does not say whether the bank can.
    #[test]
    fn a_wage_the_clubs_cash_cannot_carry_is_refused_though_the_budget_allows_it() {
        let midfielder = player("fa", Position::Midfielder, 0, None);

        assert_eq!(
            verdict(&sound_club_with_cash(1_000_000), &midfielder, 10_000),
            WagePolicyVerdict::OverPolicy
        );
        assert_eq!(
            verdict(&sound_club_with_cash(500_000_000), &midfielder, 10_000),
            WagePolicyVerdict::WithinPolicy
        );
    }

    /// Given a club in debt with room in its budget
    /// When it takes on a wage, and when it renews a player at the wage he is paid
    /// Then the first is refused and the second is not: a club in debt takes on no new weekly
    ///      cost, and keeping what it has is not one.
    #[test]
    fn a_club_in_debt_takes_on_no_new_wage_but_may_keep_a_player_at_his_own() {
        let game = sound_club_with_cash(-1);
        let midfielder = player("fa", Position::Midfielder, 0, None);
        let own = game.players.iter().find(|p| p.id == "mf0").unwrap().clone();

        assert_eq!(
            verdict(&game, &midfielder, 10_000),
            WagePolicyVerdict::OverPolicy
        );
        assert_eq!(
            verdict(&game, &own, MEDIAN_WAGE),
            WagePolicyVerdict::WithinPolicy
        );
    }

    /// Given a club short of a goalkeeper and in debt
    /// When a goalkeeper on its pay scale is signed
    /// Then the floor still lets it: a club that cannot put a side out is not held to its cash.
    #[test]
    fn a_club_in_debt_may_still_fill_its_squad_floor() {
        let game = club([1, 5, 5, 3], -1_000_000);
        let keeper = player("fa", Position::Goalkeeper, 0, None);

        assert_eq!(
            verdict(&game, &keeper, MEDIAN_WAGE),
            WagePolicyVerdict::OverPolicyToKeepSquadFloor
        );
    }

    /// Given a club with no player on a wage at all
    /// When the floor's ceiling is asked
    /// Then there is none: with no pay scale to measure by, the waiver cannot be bounded by one.
    #[test]
    fn a_club_with_no_wages_has_no_pay_scale_to_bound_the_floor_by() {
        let mut game = club([2, 4, 4, 0], 0);
        for p in &mut game.players {
            p.stage_wage(0);
        }

        assert_eq!(floor_waiver_ceiling(&game, "club"), i64::MAX);
    }
}
