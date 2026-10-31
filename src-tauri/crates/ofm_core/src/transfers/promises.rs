//! The stepping-stone promise: a club lands a player partly by promising not to stand in the way
//! of a suitable move later. The promise is stored on the player (`morale_core`), so it survives
//! save/load, and it is judged on terms that can be checked against the game state.
//!
//! *Suitable* means a bidder at least [`MIN_REPUTATION_STEP_UP`] reputation points above the
//! promising club, offering at least [`MIN_FEE_PCT_OF_VALUE`] per cent of the player's market
//! value, while the promise is active (after [`PROTECTED_AFTER_DAYS`], before [`PROMISE_TERM_YEARS`]
//! run out). Refusing such an offer breaches it; selling to such a bidder fulfils it.

use crate::game::Game;
use chrono::{Days, Months, NaiveDate};
use domain::message::{InboxMessage, MessageCategory, MessagePriority};
use domain::player::{
    ContractRenewalState, Player, RenewalSessionOutcome, RenewalSessionStatus,
    SteppingStonePromise, SteppingStonePromiseStatus,
};
use domain::team::Team;
use std::collections::HashMap;

/// A club must out-reputation the promiser by this much to count as a step up.
pub const MIN_REPUTATION_STEP_UP: u32 = 50;
/// An offer below this share of market value is a lowball, not a move the promiser must allow.
pub const MIN_FEE_PCT_OF_VALUE: u8 = 80;
/// A player who has just arrived is not owed an immediate exit.
pub const PROTECTED_AFTER_DAYS: u64 = 180;
pub const PROMISE_TERM_YEARS: u32 = 3;
/// What he will give up in wages for the promise, per cent of what he would ask without it.
pub const WAGE_DISCOUNT_PCT: u32 = 10;
/// A breached promise closes his door to renewal talks for this long.
const BREACH_RENEWAL_BLOCK_DAYS: u64 = 180;
const BREACH_TRUST_LOSS: u8 = 30;
const BREACH_MORALE_LOSS: u8 = 15;
const LAPSE_TRUST_GAIN: u8 = 5;

pub const ERR_PROMISE_NOT_WANTED: &str = "be.error.contracts.promiseNotWanted";

fn fmt(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

/// Whether the promise means anything to him: a young player with room to grow, who expects
/// to outgrow the club. A veteran or a finished player has no use for it, so offering one to
/// him is refused rather than recorded as a commitment that buys nothing.
pub fn wants_stepping_stone(player: &Player, current_date: NaiveDate) -> bool {
    let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
    age <= 27 && i32::from(player.potential) >= i32::from(player.ovr) + 3
}

/// The wage he would ask once the promise is on the table.
pub fn discounted_wage(expected_wage: u32) -> u32 {
    let kept = 100 - WAGE_DISCOUNT_PCT;
    expected_wage.saturating_mul(kept).div_ceil(100)
}

pub(crate) fn make_promise(team: &Team, current_date: NaiveDate) -> Option<SteppingStonePromise> {
    let protected_from = current_date.checked_add_days(Days::new(PROTECTED_AFTER_DAYS))?;
    let expires_on = current_date.checked_add_months(Months::new(PROMISE_TERM_YEARS * 12))?;
    Some(SteppingStonePromise {
        club_id: team.id.clone(),
        made_on: fmt(current_date),
        protected_from: fmt(protected_from),
        expires_on: fmt(expires_on),
        min_buyer_reputation: team.reputation.saturating_add(MIN_REPUTATION_STEP_UP),
        min_fee_pct_of_value: MIN_FEE_PCT_OF_VALUE,
        status: SteppingStonePromiseStatus::Active,
        resolved_on: None,
    })
}

/// Whether the club is bound by the promise today, by the player's own record.
fn binding_promise<'a>(
    player: &'a Player,
    club_id: &str,
    today: NaiveDate,
) -> Option<&'a SteppingStonePromise> {
    let promise = player.morale_core.stepping_stone_promise.as_ref()?;
    let bound = promise.status == SteppingStonePromiseStatus::Active
        && promise.club_id == club_id
        && parse(&promise.protected_from).is_some_and(|from| today >= from)
        && parse(&promise.expires_on).is_some_and(|until| today <= until);
    bound.then_some(promise)
}

fn is_suitable_offer(
    promise: &SteppingStonePromise,
    buyer_reputation: u32,
    fee: u64,
    value: u64,
) -> bool {
    let floor = u128::from(value) * u128::from(promise.min_fee_pct_of_value) / 100;
    buyer_reputation >= promise.min_buyer_reputation && u128::from(fee) >= floor
}

fn resolve(player: &mut Player, status: SteppingStonePromiseStatus, today: NaiveDate) {
    if let Some(promise) = player.morale_core.stepping_stone_promise.as_mut() {
        promise.status = status;
        promise.resolved_on = Some(fmt(today));
    }
}

fn promise_message(
    kind: &str,
    player: &Player,
    club_name: &str,
    today: NaiveDate,
    urgent: bool,
) -> InboxMessage {
    let mut params = HashMap::new();
    params.insert("player".to_string(), player.full_name.clone());
    params.insert("team".to_string(), club_name.to_string());
    InboxMessage::new(
        format!("stepping_stone_{kind}_{}_{}", player.id, fmt(today)),
        String::new(),
        String::new(),
        String::new(),
        fmt(today),
    )
    .with_category(MessageCategory::Contract)
    .with_priority(if urgent {
        MessagePriority::Urgent
    } else {
        MessagePriority::Normal
    })
    .with_sender_role("")
    .with_i18n(
        &format!("be.msg.steppingStone.{kind}.subject"),
        &format!("be.msg.steppingStone.{kind}.body"),
        params,
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

/// The promise is broken: he loses trust and heart, and will not sit down to renewal talks for a
/// while. The block reuses the renewal session's own "manager blocked" state, so every renewal
/// path (his, the delegate's, the AI-assisted one) already respects it.
fn apply_breach(player: &mut Player, today: NaiveDate) {
    resolve(player, SteppingStonePromiseStatus::Breached, today);
    let core = &mut player.morale_core;
    core.manager_trust = core.manager_trust.saturating_sub(BREACH_TRUST_LOSS);
    player.morale = player.morale.saturating_sub(BREACH_MORALE_LOSS);
    let state = player
        .morale_core
        .renewal_state
        .get_or_insert_with(ContractRenewalState::default);
    state.status = RenewalSessionStatus::Blocked;
    state.last_outcome = Some(RenewalSessionOutcome::BlockedByManager);
    state.manager_blocked_until = today
        .checked_add_days(Days::new(BREACH_RENEWAL_BLOCK_DAYS))
        .map(fmt);
}

/// Called when the managing club turns down an incoming offer. A suitable offer, refused while
/// the promise binds, breaches it.
pub(crate) fn on_offer_refused(game: &mut Game, player_id: &str, buyer_team_id: &str, fee: u64) {
    let today = game.clock.current_date.date_naive();
    let Some(club_id) = game.manager.team_id.clone() else {
        return;
    };
    let buyer_reputation = game
        .teams
        .iter()
        .find(|team| team.id == buyer_team_id)
        .map_or(0, |team| team.reputation);
    let club_name = game.team_name_or_id(&club_id);
    let Some(player) = game
        .players
        .iter_mut()
        .find(|player| player.id == player_id)
    else {
        return;
    };
    let breached = binding_promise(player, &club_id, today).is_some_and(|promise| {
        is_suitable_offer(promise, buyer_reputation, fee, player.market_value)
    });
    if !breached {
        return;
    }
    apply_breach(player, today);
    let message = promise_message("breached", player, &club_name, today, true);
    game.messages.push(message);
}

/// Called once a transfer has completed. Selling a player to a club that meets the promise's
/// terms fulfils it; any other sale simply ends it.
pub(crate) fn on_player_sold(
    game: &mut Game,
    player_id: &str,
    from_team_id: &str,
    buyer_team_id: &str,
    fee: u64,
) {
    let today = game.clock.current_date.date_naive();
    let buyer_reputation = game
        .teams
        .iter()
        .find(|team| team.id == buyer_team_id)
        .map_or(0, |team| team.reputation);
    let club_name = game.team_name_or_id(from_team_id);
    let Some(player) = game
        .players
        .iter_mut()
        .find(|player| player.id == player_id)
    else {
        return;
    };
    let Some(promise) = player.morale_core.stepping_stone_promise.as_ref() else {
        return;
    };
    if promise.status != SteppingStonePromiseStatus::Active || promise.club_id != from_team_id {
        return;
    }
    let suitable = is_suitable_offer(promise, buyer_reputation, fee, player.market_value);
    let status = if suitable {
        SteppingStonePromiseStatus::Fulfilled
    } else {
        SteppingStonePromiseStatus::Lapsed
    };
    resolve(player, status, today);
    if suitable {
        let message = promise_message("fulfilled", player, &club_name, today, false);
        game.messages.push(message);
    }
}

/// Daily upkeep: a promise that ran out unbroken lapses to the club's credit; one whose player
/// has left the club by other means (release, loan buy-out) ends without effect. Idempotent: it
/// only ever moves an `Active` promise.
pub fn process_stepping_stone_promises(game: &mut Game) {
    let today = game.clock.current_date.date_naive();
    let names: HashMap<String, String> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), team.name.clone()))
        .collect();
    let mut messages = Vec::new();
    for player in &mut game.players {
        let Some(promise) = player.morale_core.stepping_stone_promise.as_ref() else {
            continue;
        };
        if promise.status != SteppingStonePromiseStatus::Active {
            continue;
        }
        let club_id = promise.club_id.clone();
        let expired = parse(&promise.expires_on).is_none_or(|until| today > until);
        let gone = player.team_id.as_deref() != Some(club_id.as_str())
            && player
                .active_loan
                .as_ref()
                .is_none_or(|loan| loan.parent_team_id != club_id);
        if expired && !gone {
            resolve(player, SteppingStonePromiseStatus::Lapsed, today);
            let core = &mut player.morale_core;
            core.manager_trust = core.manager_trust.saturating_add(LAPSE_TRUST_GAIN).min(100);
            let club_name = names.get(&club_id).cloned().unwrap_or(club_id);
            messages.push(promise_message("lapsed", player, &club_name, today, false));
        } else if gone {
            resolve(player, SteppingStonePromiseStatus::Lapsed, today);
        }
    }
    game.messages.extend(messages);
}
