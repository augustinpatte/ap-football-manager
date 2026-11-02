//! Delegated recruitment: the assistant signs players for the squad's needs.
//!
//! There is no second transfer engine here. The assistant picks a target the squad needs, and then
//! does what the manager does — makes a bid through `transfers::make_transfer_bid_with_terms`, so
//! the selling club's valuation, the counter-offers, the rounds, the cool-down after a break-off,
//! the board's wage policy, the squad floor and the **player's own consent** all apply to him
//! exactly as they would to the manager.
//!
//! What the assistant adds is restraint:
//!
//! * **He acts on a need**, never on appeal: a position the assistant's own review calls weak, and
//!   a player who would really improve it (or fill the thin group).
//! * **Every bid goes through [`authorize`] first**, at the fee and wage that bid commits to. Within
//!   the manager's ceilings and with the duty set to act alone, he bids; otherwise the bid becomes a
//!   proposal and nothing is spent. A ceiling of zero (the default) means every signing is a
//!   proposal.
//! * **One round per visit.** A negotiation is carried over several visits, three days apart, as the
//!   manager would carry it over several days. Where the negotiation stands is the
//!   [`ActiveApproach`] kept in the delegation state, so a reload carries on.
//! * **He does not repeat himself.** A target who was refused, whose club would not sell, whom the
//!   manager declined or who was left unanswered is set aside for weeks. At most one new approach
//!   per visit, two open at once, three a month.
//! * **He sells nobody.** Nothing in this module lists, offers or releases a player.

use chrono::{Days, NaiveDate};
use domain::delegation::{
    ACTIVE_APPROACH_CAPACITY, ActiveApproach, DecisionOutcome, Duty, ProposalAction, ShelveReason,
};
use domain::player::{Player, Position, SquadRole};
use domain::transfer_terms::TransferTerms;

use super::assistant::{self, RecKind};
use super::delegation::{date, log, param, record, stamp};
use super::delegation_policy::{Authorization, Reason, Spend, authorize};
use crate::game::Game;
use crate::player_knowledge::{PlayerKnowledge, player_knowledge};
use crate::squad_floor::{STARTERS_PER_GROUP, group_index};
use crate::transfers::{
    TransferNegotiationDecision, TransferNegotiationOutcome, bid_is_possible, judge_in_game,
    make_transfer_bid_with_terms, round_transfer_fee,
};

/// Senior players above which the assistant recruits no more: the squad list is nearly full.
const SENIOR_SQUAD_CEILING: usize = 27;
/// New approaches in any thirty days.
const MAX_APPROACHES_PER_MONTH: usize = 3;
const PACING_WINDOW_DAYS: u64 = 30;
/// Days a negotiation may stay open before the assistant drops it.
const ACTIVE_LIFETIME_DAYS: u64 = 45;
/// Rounds of one negotiation before the assistant gives it up.
const MAX_ROUNDS: u8 = 5;
/// How much better than the first cover a player must be to be worth a fee, in rating points.
const MIN_GAIN: i32 = 3;
/// The assistant opens at this per cent of the price he reads the selling club to ask, and will not
/// go past `CEILING_PCT_OF_ASK`. The opening has to clear the club's counter-offer floor (88 % of
/// its price) or the talks end at once.
const OPENING_PCT_OF_ASK: u64 = 95;
const CEILING_PCT_OF_ASK: u64 = 110;
/// A club asking more than this per cent of the player's value is not worth chasing.
const OVERPRICED_PCT_OF_VALUE: u64 = 150;
/// One signing may take at most this per cent of what the club has to spend on transfers.
const MAX_BUDGET_SHARE_PCT: i64 = 60;
/// Candidates weighed in full (consent, affordability) after the cheap filters, best first.
const SHORTLIST: usize = 10;
/// Oldest he will buy, and the goalkeeper's age.
const MAX_AGE: i32 = 31;
const MAX_AGE_GOALKEEPER: i32 = 34;

// How long a target stays aside, by why.
const SHELVE_CLUB_REFUSED_DAYS: u64 = 60;
const SHELVE_PLAYER_REFUSED_DAYS: u64 = 90;
const SHELVE_NOT_POSSIBLE_DAYS: u64 = 30;
const SHELVE_DECLINED_DAYS: u64 = 90;
/// A proposal lives [`super::delegation::PROPOSAL_LIFETIME_DAYS`] days; the target stays aside a
/// fortnight longer, so a proposal that lapsed is not made again the morning after.
const SHELVE_PROPOSED_DAYS: u64 = 35;
const SHELVE_DONE_DAYS: u64 = 365;

/// A player the assistant wants, and what he may pay for him.
struct Target {
    player_id: String,
    need: String,
    opening_fee: u64,
    max_fee: u64,
}

fn after(game: &Game, days: u64) -> String {
    date(game)
        .checked_add_days(Days::new(days))
        .unwrap_or_else(|| date(game))
        .format("%Y-%m-%d")
        .to_string()
}

fn position_group(name: &str) -> Option<usize> {
    let position = match name {
        "Goalkeeper" => Position::Goalkeeper,
        "Defender" => Position::Defender,
        "Midfielder" => Position::Midfielder,
        "Forward" => Position::Forward,
        _ => return None,
    };
    Some(group_index(&position))
}

fn group_name(group: usize) -> &'static str {
    ["Goalkeeper", "Defender", "Midfielder", "Forward"][group.min(3)]
}

/// The groups the assistant's own review calls weak, by name, in the order he would deal with them.
fn needs(game: &Game, team_id: &str) -> Vec<usize> {
    let mut found: Vec<(u8, usize)> = assistant::recommendations(game, team_id)
        .iter()
        .filter(|rec| rec.kind == RecKind::WeakPosition)
        .filter_map(|rec| {
            let group = rec
                .params
                .iter()
                .find(|(key, _)| key == "position")
                .and_then(|(_, value)| position_group(value))?;
            Some((rec.severity, group))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, group)| group).collect()
}

/// The ratings of the club's registered, fit seniors by group, best first.
fn levels(game: &Game, team_id: &str) -> [Vec<u8>; 4] {
    let mut groups: [Vec<u8>; 4] = Default::default();
    for player in game.players.iter().filter(|p| {
        p.team_id.as_deref() == Some(team_id)
            && p.squad_role == SquadRole::Senior
            && p.active_loan.is_none()
            && !p.retired
    }) {
        groups[group_index(&player.natural_position)].push(player.ovr);
    }
    for group in &mut groups {
        group.sort_unstable_by(|a, b| b.cmp(a));
    }
    groups
}

fn age_of(player: &Player, today: NaiveDate) -> i32 {
    domain::age::age_on_or_unknown(today, &player.date_of_birth)
}

/// Whether the assistant may look at this player at all: someone else's, senior, known to the club
/// (his own division, or reported on by a scout), and nothing already in motion for him.
fn is_eligible(game: &Game, player: &Player, team_id: &str, today: &str) -> bool {
    let memory = &game.delegation.recruitment;
    player.team_id.as_deref().is_some_and(|club| club != team_id)
        && !player.retired
        && player.squad_role == SquadRole::Senior
        && player.active_loan.is_none()
        && !memory.is_shelved(&player.id, today)
        && !memory.is_active(&player.id)
        && matches!(
            player_knowledge(game, player),
            PlayerKnowledge::Known | PlayerKnowledge::Scouted { .. }
        )
        // The manager's own talks with him, or a club turned away lately, are not to be disturbed.
        && crate::transfers::latest_negotiation_with_club(player, team_id, date(game)).is_none()
        && !crate::transfers::player_has_pending_registration(player)
        && !game.delegation.proposals.iter().any(|proposal| {
            matches!(&proposal.action,
                ProposalAction::SignPlayer { player_id, .. } if *player_id == player.id)
        })
}

/// How good a buy he is, for ranking only: what he adds, what he may become, what he costs.
fn appeal(player: &Player, gain: i32, today: NaiveDate, budget: i64) -> i64 {
    let age = age_of(player, today);
    let youth = if age <= 23 {
        (i64::from(player.potential) - i64::from(player.ovr)) / 2
    } else {
        0
    };
    let ageing = i64::from((age - 29).max(0)) * 2;
    let cost_share = if budget > 0 {
        i64::try_from(player.market_value)
            .unwrap_or(i64::MAX)
            .saturating_mul(10)
            / budget
    } else {
        10
    };
    i64::from(gain) * 3 + youth + if player.transfer_listed { 6 } else { 0 } - ageing - cost_share
}

/// The best player to approach for the squad's most pressing need, if there is one worth a bid.
fn choose_target(game: &Game, team_id: &str) -> Option<Target> {
    let team = game.teams.iter().find(|t| t.id == team_id)?;
    let today_date = date(game);
    let today = stamp(game);
    let squad = levels(game, team_id);
    let seniors: usize = squad.iter().map(Vec::len).sum();
    if seniors >= SENIOR_SQUAD_CEILING {
        return None;
    }
    let budget = team.transfer_budget.min(team.finance);
    for group in needs(game, team_id) {
        let thin = crate::transfers::group_is_thin(&squad[group], group);
        let mut candidates: Vec<(i64, &Player, i32)> = game
            .players
            .iter()
            .filter(|p| group_index(&p.natural_position) == group)
            .filter(|p| is_eligible(game, p, team_id, &today))
            .filter(|p| {
                let age = age_of(p, today_date);
                age <= if group == 0 {
                    MAX_AGE_GOALKEEPER
                } else {
                    MAX_AGE
                }
            })
            .filter_map(|p| {
                let gain = crate::transfers::improvement_over_cover(&squad[group], group, p.ovr);
                // A real improvement, or a body for a group that has too few: never a player who
                // would sit behind the first cover.
                let weak_starter = squad[group]
                    .get(STARTERS_PER_GROUP[group].saturating_sub(1))
                    .is_none_or(|worst_starter| p.ovr > *worst_starter);
                (gain >= MIN_GAIN || (thin && weak_starter)).then_some((
                    appeal(p, gain, today_date, budget),
                    p,
                    gain,
                ))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        for (_, player, _) in candidates.into_iter().take(SHORTLIST) {
            if let Some(target) = weigh(game, team_id, player, group) {
                return Some(target);
            }
        }
    }
    None
}

/// Whether this player can actually be bid for and would come, and at what price.
fn weigh(game: &Game, team_id: &str, player: &Player, group: usize) -> Option<Target> {
    let team = game.teams.iter().find(|t| t.id == team_id)?;
    let seller = game
        .teams
        .iter()
        .find(|t| Some(t.id.as_str()) == player.team_id.as_deref())?;
    // The assistant's read of the price: what the selling club would ask of this club today. He
    // opens a little under it, which draws a counter rather than a flat no, and pays up to a tenth
    // over it. A club that wants much more than the player is worth is not worth the chase.
    let ask = crate::transfers::minimum_acceptable_fee_at(
        date(game),
        player,
        seller,
        team,
        crate::transfers::window_phase(game.season_context.transfer_window.days_remaining),
    );
    let worth = player.market_value.max(1);
    if ask.saturating_mul(100) > worth.saturating_mul(OVERPRICED_PCT_OF_VALUE) {
        return None;
    }
    let opening_fee = round_transfer_fee(ask.saturating_mul(OPENING_PCT_OF_ASK) / 100);
    let max_fee = round_transfer_fee(ask.saturating_mul(CEILING_PCT_OF_ASK) / 100)
        .min(crate::transfers::transfer_fee_ceiling(team));
    let share_cap = team.transfer_budget.saturating_mul(MAX_BUDGET_SHARE_PCT) / 100;
    if opening_fee == 0
        || opening_fee > max_fee
        || i64::try_from(opening_fee).ok()? > share_cap
        || bid_is_possible(game, &player.id, opening_fee, &TransferTerms::default()).is_err()
    {
        return None;
    }
    // He has to be someone who would come to this club: a refusal no wage can answer ends it
    // before a bid is spent on it.
    let judgement = judge_in_game(game, player, team_id, &TransferTerms::default()).ok()?;
    if judgement.interest() == crate::contracts::InterestLevel::Unlikely
        && judgement.counter.is_none()
    {
        return None;
    }
    Some(Target {
        player_id: player.id.clone(),
        need: group_name(group).to_string(),
        opening_fee,
        max_fee,
    })
}

/// What a bid at `fee` on these terms would commit the club to, for [`authorize`]. The wage is the
/// one he is to be paid (the one asked for, or the buyer's standard), the length likewise.
fn commitment_of(
    game: &Game,
    team_id: &str,
    player_id: &str,
    fee: u64,
    terms: &TransferTerms,
) -> Option<Spend> {
    let player = game.players.iter().find(|p| p.id == player_id)?;
    let judgement = judge_in_game(game, player, team_id, terms).ok()?;
    Some(Spend::Transfer {
        fee: i64::try_from(fee).ok()?,
        weekly_wage: judgement.terms.weekly_wage,
        years: u8::try_from(judgement.terms.contract_years).unwrap_or(u8::MAX),
    })
}

fn terms_of(approach: &ActiveApproach) -> TransferTerms {
    TransferTerms {
        player_weekly_wage: approach.wage,
        player_contract_years: approach.years,
        ..TransferTerms::default()
    }
}

/// The weekly rhythm of the duty: carry on the open negotiations, then, if there is room and a
/// need, open one more.
pub(super) fn player_recruitment(game: &mut Game, team_id: &str) {
    let today = stamp(game);
    game.delegation.recruitment.forget_expired(&today);
    drop_forgotten_negotiations(game);
    if !crate::transfers::transfer_window_is_open(game) {
        return;
    }
    let open: Vec<ActiveApproach> = game.delegation.recruitment.active.clone();
    for approach in open {
        step(game, team_id, approach);
    }
    let memory = &game.delegation.recruitment;
    let since = after_back(game, PACING_WINDOW_DAYS);
    if memory.active.len() >= ACTIVE_APPROACH_CAPACITY
        || memory.approaches_since(&since) >= MAX_APPROACHES_PER_MONTH
    {
        return;
    }
    let Some(target) = choose_target(game, team_id) else {
        return;
    };
    let approach = ActiveApproach {
        player_id: target.player_id.clone(),
        started: today.clone(),
        need: target.need.clone(),
        max_fee: target.max_fee,
        next_fee: target.opening_fee,
        wage: 0,
        years: 0,
        rounds: 0,
    };
    game.delegation.recruitment.active.push(approach.clone());
    game.delegation
        .recruitment
        .record_approach(&target.player_id, &today);
    step(game, team_id, approach);
}

/// A negotiation that has been open for weeks without an end (the window shut on it, a season went
/// by) is dropped rather than carried on stale: the player is set aside for a month.
fn drop_forgotten_negotiations(game: &mut Game) {
    let since = after_back(game, ACTIVE_LIFETIME_DAYS);
    let stale: Vec<String> = game
        .delegation
        .recruitment
        .active
        .iter()
        .filter(|approach| approach.started.as_str() < since.as_str())
        .map(|approach| approach.player_id.clone())
        .collect();
    let until = after(game, SHELVE_NOT_POSSIBLE_DAYS);
    for player_id in stale {
        game.delegation
            .recruitment
            .shelve(&player_id, &until, ShelveReason::NotPossible);
        game.delegation.recruitment.finish(&player_id);
    }
}

fn after_back(game: &Game, days: u64) -> String {
    date(game)
        .checked_sub_days(Days::new(days))
        .unwrap_or_else(|| date(game))
        .format("%Y-%m-%d")
        .to_string()
}

/// One round of one negotiation.
fn step(game: &mut Game, team_id: &str, mut approach: ActiveApproach) {
    let Some(player) = game
        .players
        .iter()
        .find(|p| p.id == approach.player_id)
        .cloned()
    else {
        game.delegation.recruitment.finish(&approach.player_id);
        return;
    };
    let name = player.match_name.clone();
    if player.team_id.as_deref() == Some(team_id) {
        finish_signed(game, &approach, &name, None);
        return;
    }
    if crate::transfers::player_has_pending_registration(&player) {
        // An agreement waiting for the window, or a loan: nothing to do but wait.
        return;
    }
    if approach.rounds >= MAX_ROUNDS {
        give_up(game, &approach, &name, ShelveReason::ClubRefused);
        return;
    }
    let terms = terms_of(&approach);
    let Some(spend) = commitment_of(
        game,
        team_id,
        &approach.player_id,
        approach.next_fee,
        &terms,
    ) else {
        give_up(game, &approach, &name, ShelveReason::NotPossible);
        return;
    };
    // Past what he judges the player worth: the club asks more than the assistant would ever pay
    // on his own account, so the manager is asked instead.
    let beyond_judgement = approach.next_fee > approach.max_fee;
    let authorization = authorize(&game.delegation, Duty::PlayerRecruitment, spend);
    match authorization {
        Authorization::NotDelegated => {
            game.delegation.recruitment.finish(&approach.player_id);
        }
        Authorization::NeedsApproval(reason) => {
            propose(game, &approach, &name, spend, reason, beyond_judgement);
        }
        Authorization::Auto if beyond_judgement => {
            propose(game, &approach, &name, spend, Reason::FeeAboveLimit, true);
        }
        Authorization::Auto => bid(game, &mut approach, &name),
    }
}

fn propose(
    game: &mut Game,
    approach: &ActiveApproach,
    name: &str,
    spend: Spend,
    reason: Reason,
    beyond_judgement: bool,
) {
    let Spend::Transfer {
        fee,
        weekly_wage,
        years,
    } = spend
    else {
        return;
    };
    let today = stamp(game);
    let reason_key = match reason {
        Reason::ApprovalRequired if !beyond_judgement => "delegation.reason.recruitNeed",
        _ => "delegation.reason.recruitAboveLimit",
    };
    game.delegation.propose(
        Duty::PlayerRecruitment,
        &today,
        ProposalAction::SignPlayer {
            player_id: approach.player_id.clone(),
            fee: u64::try_from(fee).unwrap_or(0),
            wage: weekly_wage,
            years,
        },
        "delegation.proposal.signPlayer",
        vec![
            param("player", name),
            param("position", &approach.need),
            param("fee", fee),
            param("wage", weekly_wage),
            param("years", years),
        ],
        reason_key,
    );
    let until = after(game, SHELVE_PROPOSED_DAYS);
    game.delegation
        .recruitment
        .shelve(&approach.player_id, &until, ShelveReason::ProposalLapsed);
    game.delegation.recruitment.finish(&approach.player_id);
}

fn bid(game: &mut Game, approach: &mut ActiveApproach, name: &str) {
    let result = make_transfer_bid_with_terms(
        game,
        &approach.player_id,
        approach.next_fee,
        terms_of(approach),
    );
    approach.rounds = approach.rounds.saturating_add(1);
    match result {
        Err(_) => give_up(game, approach, name, ShelveReason::NotPossible),
        Ok(outcome) => apply_outcome(game, approach, name, &outcome),
    }
}

fn apply_outcome(
    game: &mut Game,
    approach: &mut ActiveApproach,
    name: &str,
    outcome: &TransferNegotiationOutcome,
) {
    match outcome.decision {
        TransferNegotiationDecision::Accepted => {
            finish_signed(game, approach, name, outcome.registration_date.as_deref());
        }
        TransferNegotiationDecision::CounterOffer => {
            approach.next_fee = outcome.suggested_fee.unwrap_or(approach.next_fee);
            game.delegation.recruitment.replace(approach.clone());
        }
        TransferNegotiationDecision::Rejected if outcome.is_terminal => {
            give_up(game, approach, name, ShelveReason::ClubRefused);
        }
        TransferNegotiationDecision::Rejected => match &outcome.player_demand {
            // The clubs agree; the player wants other terms. They are put to the authorisation
            // like any other, on the next visit.
            Some(demand) => {
                approach.wage = demand.weekly_wage;
                approach.years = u8::try_from(demand.contract_years).unwrap_or(0);
                game.delegation.recruitment.replace(approach.clone());
            }
            None => give_up(game, approach, name, ShelveReason::PlayerRefused),
        },
    }
}

fn finish_signed(
    game: &mut Game,
    approach: &ActiveApproach,
    name: &str,
    registers_on: Option<&str>,
) {
    let until = after(game, SHELVE_DONE_DAYS);
    game.delegation
        .recruitment
        .shelve(&approach.player_id, &until, ShelveReason::Done);
    game.delegation.recruitment.finish(&approach.player_id);
    let (key, mut params) = match registers_on {
        Some(day) => ("delegation.log.recruitScheduled", vec![param("date", day)]),
        None => ("delegation.log.recruitSigned", Vec::new()),
    };
    params.push(param("player", name));
    params.push(param("fee", approach.next_fee));
    params.push(param("position", &approach.need));
    log(game, Duty::PlayerRecruitment, key, params);
}

fn give_up(game: &mut Game, approach: &ActiveApproach, name: &str, reason: ShelveReason) {
    let days = match reason {
        ShelveReason::PlayerRefused => SHELVE_PLAYER_REFUSED_DAYS,
        ShelveReason::NotPossible => SHELVE_NOT_POSSIBLE_DAYS,
        ShelveReason::ManagerDeclined => SHELVE_DECLINED_DAYS,
        ShelveReason::ClubRefused | ShelveReason::ProposalLapsed | ShelveReason::Done => {
            SHELVE_CLUB_REFUSED_DAYS
        }
    };
    let until = after(game, days);
    game.delegation
        .recruitment
        .shelve(&approach.player_id, &until, reason);
    game.delegation.recruitment.finish(&approach.player_id);
    record(
        game,
        Duty::PlayerRecruitment,
        match reason {
            ShelveReason::ClubRefused => "delegation.log.recruitClubRefused",
            ShelveReason::PlayerRefused => "delegation.log.recruitPlayerRefused",
            _ => "delegation.log.recruitNotPossible",
        },
        vec![param("player", name), param("position", &approach.need)],
        DecisionOutcome::LeftToManager,
    );
}

/// The manager said no to a signing: the assistant sets the player aside for a good while.
pub(super) fn on_declined(game: &mut Game, player_id: &str) {
    let until = after(game, SHELVE_DECLINED_DAYS);
    game.delegation
        .recruitment
        .shelve(player_id, &until, ShelveReason::ManagerDeclined);
}

/// The manager said yes to a signing: the bid is made at the fee, wage and length proposed, with
/// everyone's say intact. A club that asks for more, or a player who asks for other terms, leaves
/// the negotiation open for the assistant to carry on (and to put to the manager again if it
/// goes past what he approved).
pub(super) fn sign_approved(
    game: &mut Game,
    player_id: &str,
    fee: u64,
    wage: u32,
    years: u8,
) -> Result<(), String> {
    let player = game
        .players
        .iter()
        .find(|p| p.id == player_id)
        .cloned()
        .ok_or("be.error.playerNotFound")?;
    let name = player.match_name.clone();
    let terms = TransferTerms {
        player_weekly_wage: wage,
        player_contract_years: years,
        ..TransferTerms::default()
    };
    let mut approach = ActiveApproach {
        player_id: player_id.to_string(),
        started: stamp(game),
        need: group_name(group_index(&player.natural_position)).to_string(),
        // What the manager approved is the ceiling: asked for more, the assistant asks again.
        max_fee: fee,
        next_fee: fee,
        wage,
        years,
        rounds: 0,
    };
    let outcome = make_transfer_bid_with_terms(game, player_id, fee, terms)?;
    approach.rounds = 1;
    match outcome.decision {
        TransferNegotiationDecision::Accepted => {
            finish_signed(game, &approach, &name, outcome.registration_date.as_deref());
            Ok(())
        }
        TransferNegotiationDecision::CounterOffer => {
            approach.next_fee = outcome.suggested_fee.unwrap_or(fee);
            game.delegation.recruitment.replace(approach);
            Ok(())
        }
        TransferNegotiationDecision::Rejected if outcome.is_terminal => {
            Err("be.error.delegation.signingRefusedByClub".to_string())
        }
        TransferNegotiationDecision::Rejected => match outcome.player_demand {
            Some(demand) => {
                approach.wage = demand.weekly_wage;
                approach.years = u8::try_from(demand.contract_years).unwrap_or(0);
                game.delegation.recruitment.replace(approach);
                Ok(())
            }
            None => Err("be.error.delegation.signingRefusedByPlayer".to_string()),
        },
    }
    .inspect_err(|_| {
        let until = after(game, SHELVE_CLUB_REFUSED_DAYS);
        game.delegation
            .recruitment
            .shelve(player_id, &until, ShelveReason::ClubRefused);
    })
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::{game, staff_member};
    use super::super::delegation::{resolve_proposal, run_after_day};
    use super::*;
    use chrono::Duration;
    use domain::delegation::{DelegationLimits, ProposalAction};
    use domain::season::TransferWindowStatus;

    fn member(id: &str, team: &str, position: Position, ovr: u8, value: u64) -> Player {
        let mut player = crate::world::test_fixtures::player_at(id, team, "2030-06-30");
        player.position = position.clone();
        player.natural_position = position;
        player.ovr = ovr;
        player.potential = ovr;
        player.market_value = value;
        player.stage_wage(8_000);
        player.condition = 90;
        player
    }

    /// "team1" is short of defenders (the five it has are far below the rest of the side);
    /// "team2" has a surplus of them, two of them good, and is a smaller club.
    fn game_needing_a_defender() -> Game {
        let mut g = game(Some(80), 40);
        g.players.clear();
        let add = |g: &mut Game,
                   team: &str,
                   prefix: &str,
                   position: Position,
                   ovrs: &[u8],
                   value: u64| {
            for (i, ovr) in ovrs.iter().enumerate() {
                g.players.push(member(
                    &format!("{prefix}{i}"),
                    team,
                    position.clone(),
                    *ovr,
                    value,
                ));
            }
        };
        add(
            &mut g,
            "team1",
            "k",
            Position::Goalkeeper,
            &[68, 64],
            800_000,
        );
        add(
            &mut g,
            "team1",
            "d",
            Position::Defender,
            &[50, 49, 48, 47, 46],
            300_000,
        );
        add(
            &mut g,
            "team1",
            "m",
            Position::Midfielder,
            &[74, 72, 70, 69, 68, 66],
            1_500_000,
        );
        add(
            &mut g,
            "team1",
            "f",
            Position::Forward,
            &[72, 70, 66],
            1_500_000,
        );
        add(
            &mut g,
            "team2",
            "ok",
            Position::Goalkeeper,
            &[60, 58, 56],
            400_000,
        );
        add(
            &mut g,
            "team2",
            "od",
            Position::Defender,
            &[68, 66],
            2_000_000,
        );
        add(
            &mut g,
            "team2",
            "ox",
            Position::Defender,
            &[54, 53, 52, 51],
            900_000,
        );
        add(
            &mut g,
            "team2",
            "om",
            Position::Midfielder,
            &[60; 6],
            400_000,
        );
        add(&mut g, "team2", "of", Position::Forward, &[60; 4], 400_000);
        g.season_context.transfer_window.status = TransferWindowStatus::Open;
        g.season_context.transfer_window.days_remaining = Some(30);
        for team in &mut g.teams {
            team.finance = 40_000_000;
            team.transfer_budget = 20_000_000;
            team.wage_budget = 80_000_000;
        }
        g.teams[0].starting_xi_ids = (0..11)
            .filter_map(|i| g.players.get(i).map(|p| p.id.clone()))
            .collect();
        g
    }

    fn delegate(g: &mut Game, confirm: bool, fee_ceiling: i64) {
        g.delegation.set(Duty::PlayerRecruitment, true, confirm);
        g.delegation.limits = DelegationLimits {
            max_transfer_fee: fee_ceiling,
            max_commitment: 20_000_000,
            ..DelegationLimits::default()
        };
    }

    fn visit(g: &mut Game) {
        run_after_day(g);
    }

    fn days_later(g: &mut Game, days: i64) {
        g.clock.current_date += Duration::days(days);
    }

    fn defenders_signed(g: &Game) -> Vec<String> {
        g.players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some("team1") && p.id.starts_with('o'))
            .filter(|p| p.position == Position::Defender)
            .map(|p| p.id.clone())
            .collect()
    }

    fn sign_proposals(g: &Game) -> Vec<(String, u64)> {
        g.delegation
            .proposals
            .iter()
            .filter_map(|p| match &p.action {
                ProposalAction::SignPlayer { player_id, fee, .. } => {
                    Some((player_id.clone(), *fee))
                }
                _ => None,
            })
            .collect()
    }

    /// Scenario 8. Given a squad short at the back and a fee ceiling the manager has set
    /// When the assistant visits over a few weeks
    /// Then he signs a defender through the ordinary negotiation, within the ceiling, and asks for nothing.
    #[test]
    fn the_assistant_signs_a_suitable_player_within_the_limits_he_was_given() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, false, 5_000_000);
        let cash = g.teams[0].finance;

        for _ in 0..8 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        let signed = defenders_signed(&g);
        assert_eq!(
            signed.len(),
            1,
            "one defender, not a shopping spree: {signed:?}"
        );
        assert!(g.teams[0].finance < cash, "he was paid for");
        assert!(cash - g.teams[0].finance <= 5_000_000, "within the ceiling");
        assert!(
            g.delegation.proposals.is_empty(),
            "nothing needed the manager"
        );
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.title_key == "delegation.log.recruitSigned")
        );
        assert!(
            g.delegation.recruitment.active.is_empty(),
            "the negotiation is closed"
        );
    }

    /// Scenario 9. Given a ceiling below the price of any good defender
    /// When the assistant visits
    /// Then he proposes the signing instead, and nothing is spent or moved.
    #[test]
    fn a_deal_above_his_limits_is_proposed_not_executed() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, false, 500_000);
        let cash = g.teams[0].finance;

        for _ in 0..4 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        let proposals = sign_proposals(&g);
        assert!(!proposals.is_empty(), "the manager is asked");
        assert!(defenders_signed(&g).is_empty());
        assert_eq!(g.teams[0].finance, cash);
        assert!(
            g.players.iter().all(|p| p.transfer_offers.is_empty()),
            "no bid was made"
        );
    }

    /// The default ceiling is zero. A manager who has handed the duty over but set no ceiling
    /// is never charged: every signing is a proposal.
    #[test]
    fn with_the_default_ceiling_nothing_is_ever_executed() {
        let mut g = game_needing_a_defender();
        g.delegation
            .apply_mode(domain::delegation::DelegationMode::Full);
        assert_eq!(g.delegation.limits.max_transfer_fee, 0);
        let cash = g.teams[0].finance;

        for _ in 0..10 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        assert!(defenders_signed(&g).is_empty());
        assert_eq!(g.teams[0].finance, cash);
        assert!(!sign_proposals(&g).is_empty());
    }

    /// When the duty asks to be consulted, even a cheap signing within every ceiling is a proposal.
    #[test]
    fn a_duty_that_asks_for_approval_proposes_even_within_the_ceiling() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, true, 50_000_000);

        visit(&mut g);

        assert!(!sign_proposals(&g).is_empty());
        assert!(defenders_signed(&g).is_empty());
    }

    /// When the duty is not delegated the assistant does not touch the market at all.
    #[test]
    fn an_undelegated_duty_does_nothing() {
        let mut g = game_needing_a_defender();
        g.delegation.limits.max_transfer_fee = 50_000_000;

        for _ in 0..4 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        assert!(g.delegation.proposals.is_empty());
        assert!(g.players.iter().all(|p| p.transfer_offers.is_empty()));
        assert!(g.delegation.recruitment.approaches.is_empty());
    }

    /// Scenario 10. Given a proposal the manager turns down
    /// When the assistant keeps visiting for two months
    /// Then he never puts the same player to him again.
    #[test]
    fn a_declined_proposal_is_not_repeated() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, true, 50_000_000);
        visit(&mut g);
        let (declined, _) = sign_proposals(&g).into_iter().next().expect("a proposal");
        let proposal_id = g.delegation.proposals[0].id.clone();

        resolve_proposal(&mut g, &proposal_id, false).unwrap();
        for _ in 0..20 {
            days_later(&mut g, 3);
            visit(&mut g);
        }

        assert!(
            sign_proposals(&g)
                .iter()
                .all(|(player, _)| *player != declined),
            "{declined} came back"
        );
        assert!(
            g.delegation.recruitment.is_shelved(&declined, &stamp(&g)) || {
                // Past the shelf the assistant may look again, but that is more than the two
                // months asked about: the window above is 60 days and the shelf is 90.
                false
            }
        );
    }

    /// Given an approved proposal
    /// When the manager says yes
    /// Then the bid goes through the ordinary negotiation: it may be signed, or left open for the
    ///      assistant to carry on, but the club is never charged more than was approved.
    #[test]
    fn an_approved_proposal_is_bid_through_the_normal_negotiation() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, true, 50_000_000);
        visit(&mut g);
        let (player, fee) = sign_proposals(&g).into_iter().next().expect("a proposal");
        let proposal_id = g.delegation.proposals[0].id.clone();
        let cash = g.teams[0].finance;

        resolve_proposal(&mut g, &proposal_id, true).unwrap();

        let moved = g
            .players
            .iter()
            .find(|p| p.id == player)
            .is_some_and(|p| p.team_id.as_deref() == Some("team1"));
        let negotiating = g.delegation.recruitment.is_active(&player);
        assert!(moved || negotiating, "signed, or still being negotiated");
        assert!(cash - g.teams[0].finance <= i64::try_from(fee).unwrap());
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.outcome == DecisionOutcome::Approved)
        );
    }

    /// The assistant sells nobody and lists nobody, whatever the mode.
    #[test]
    fn the_assistant_never_sells_or_lists_a_player() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, false, 50_000_000);
        let squad_before: Vec<(String, bool, bool)> = g
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
            .map(|p| (p.id.clone(), p.transfer_listed, p.loan_listed))
            .collect();

        for _ in 0..12 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        for (id, listed, loan_listed) in squad_before {
            let now = g.players.iter().find(|p| p.id == id).unwrap();
            assert_eq!(now.team_id.as_deref(), Some("team1"), "{id} was sold");
            assert_eq!(
                (now.transfer_listed, now.loan_listed),
                (listed, loan_listed)
            );
        }
    }

    /// Given a squad with no need
    /// When the assistant visits
    /// Then he does not shop.
    #[test]
    fn a_squad_without_a_need_buys_nobody() {
        let mut g = game_needing_a_defender();
        for p in g.players.iter_mut().filter(|p| p.id.starts_with('d')) {
            p.ovr = 72;
        }
        delegate(&mut g, false, 50_000_000);

        for _ in 0..4 {
            visit(&mut g);
            days_later(&mut g, 3);
        }

        assert!(g.delegation.recruitment.approaches.is_empty());
        assert!(g.delegation.proposals.is_empty());
    }

    /// Without an assistant in post nothing runs, as for every other duty.
    #[test]
    fn nothing_is_recruited_without_an_assistant() {
        let mut g = game_needing_a_defender();
        g.staff
            .retain(|s| s.post() != domain::staff::StaffPost::AssistantManager);
        delegate(&mut g, false, 50_000_000);
        let _ = staff_member;

        visit(&mut g);

        assert!(g.delegation.recruitment.approaches.is_empty());
    }

    /// The memory is plain data: it survives a round trip through JSON with every field set.
    #[test]
    fn the_recruitment_memory_round_trips() {
        let mut g = game_needing_a_defender();
        delegate(&mut g, true, 50_000_000);
        visit(&mut g);
        g.delegation.recruitment.replace(ActiveApproach {
            player_id: "od0".into(),
            started: "2026-08-01".into(),
            need: "Defender".into(),
            max_fee: 3_000_000,
            next_fee: 2_200_000,
            wage: 9_500,
            years: 4,
            rounds: 2,
        });
        let json = serde_json::to_string(&g.delegation).unwrap();
        let back: domain::delegation::DelegationState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g.delegation);
        assert!(!back.recruitment.shelved.is_empty());
    }
}
