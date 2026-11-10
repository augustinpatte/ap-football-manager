//! Delegated listing: putting the players the squad does not need on the transfer list, and taking
//! them off again.
//!
//! The assistant reads "who could this club sell?" from the same ranking an AI club lists from
//! (`ai_squad_sales::ranked_sellable`): the players the side the club fields would hardly miss, the
//! ones leaving anyway, the ones whose sale funds a stronger side. He applies the manager's policy to
//! it:
//!
//! * a **protected** player is never listed, and a player the manager listed himself (or took off the
//!   list again) is never touched: what the manager did by hand stands;
//! * with **Total authority** he lists at most [`ASSISTANT_LISTING_CAPACITY`] players, once a week,
//!   keeping the squad above the floor; he takes his own listings off when the player turns out to
//!   be needed, and pays off a player nobody bought when the books call for it;
//! * otherwise he **proposes** the listing, one at a time, with the reasons; a refused one is not
//!   raised again for months.

use std::collections::HashSet;

use chrono::NaiveDate;
use domain::delegation::{
    ASSISTANT_LISTING_CAPACITY, DecisionOutcome, Duty, ProposalAction, ShelveReason,
};

use super::delegation::{date, log, param, record, stamp};
use super::delegation_policy::{Authorization, authorize_sale};
use super::delegation_recruitment::Eye;
use crate::ai_squad_sales::{ListingCandidate, ranked_sellable, release_unsold_surplus};
use crate::game::Game;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::STARTER_LOSS;

/// Days between two reviews of the squad for players to list.
const LISTING_REVIEW_DAYS: i64 = 7;

/// Days a player the manager took off the list again is not listed again.
const SHELVE_DELISTED_DAYS: u64 = 90;

fn days_after(game: &Game, days: u64) -> String {
    date(game)
        .checked_add_days(chrono::Days::new(days))
        .unwrap_or_else(|| date(game))
        .format("%Y-%m-%d")
        .to_string()
}

fn reviewed_recently(game: &Game, today: NaiveDate) -> bool {
    game.delegation
        .sales
        .last_listing_review
        .as_deref()
        .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .is_some_and(|last| (today - last).num_days() < LISTING_REVIEW_DAYS)
}

/// The weekly review: tidy the assistant's own listings, pay off what nobody bought, and list or
/// propose listing what the squad does not need.
pub(super) fn review_listings(game: &mut Game, team_id: &str, eye: Eye<'_>) {
    let today = date(game);
    if reviewed_recently(game, today) {
        return;
    }
    game.delegation.sales.last_listing_review = Some(stamp(game));
    let previously_listed = tidy_own_listings(game, team_id, eye);
    if game.delegation.limits.finance_bound {
        release_what_nobody_bought(game, team_id, &previously_listed, today);
    }
    list_what_is_not_needed(game, team_id);
}

/// What the assistant listed last time and still has listed. A player the manager took off the list
/// is the manager's decision: set aside, and forgotten. A player who has since become needed (or
/// been protected) comes off the list. Returns the players still listed by the assistant.
fn tidy_own_listings(game: &mut Game, team_id: &str, eye: Eye<'_>) -> HashSet<String> {
    let mine: Vec<String> = game.delegation.sales.listed.clone();
    let mut still_listed = HashSet::new();
    for id in mine {
        let at_club = game
            .players
            .iter()
            .any(|p| p.id == id && p.team_id.as_deref() == Some(team_id));
        let listed = game.players.iter().any(|p| p.id == id && p.transfer_listed);
        if !at_club {
            game.delegation.sales.listed.retain(|held| *held != id);
            continue;
        }
        if !listed {
            // The manager took him off the list himself: his call stands.
            let until = days_after(game, SHELVE_DELISTED_DAYS);
            game.delegation
                .sales
                .shelve(&id, &until, ShelveReason::ManagerDeclined);
            game.delegation.sales.listed.retain(|held| *held != id);
            continue;
        }
        let needed = game.delegation.is_protected(&id)
            || net_loss_of(game, team_id, eye, &id) >= STARTER_LOSS;
        if needed {
            delist(game, &id);
        } else {
            still_listed.insert(id);
        }
    }
    still_listed
}

fn net_loss_of(game: &Game, team_id: &str, eye: Eye<'_>, player_id: &str) -> f32 {
    let appetite =
        Appetite::of(&crate::transfers::profile_of(game, team_id)).seen_by_assistant(eye.rating);
    crate::squad_intelligence::ClubSquad::of_for_planning(game, team_id, appetite)
        .map_or(0.0, |squad| squad.sale(player_id).net_loss)
}

fn delist(game: &mut Game, player_id: &str) {
    let name = game
        .players
        .iter_mut()
        .find(|p| p.id == player_id)
        .map(|p| {
            p.transfer_listed = false;
            p.full_name.clone()
        })
        .unwrap_or_default();
    game.delegation
        .sales
        .listed
        .retain(|held| held != player_id);
    log(
        game,
        Duty::Offers,
        "delegation.log.delisted",
        vec![param("player", name)],
    );
}

/// Under Total authority, and only when the books call for it (the club is well over its wage budget,
/// the severance is a small share of its cash), pays off a listed player nobody bought. The same
/// rule an AI club follows, applied to a player the assistant himself listed.
fn release_what_nobody_bought(
    game: &mut Game,
    team_id: &str,
    previously_listed: &HashSet<String>,
    today: NaiveDate,
) {
    let Some(team) = game.teams.iter().find(|t| t.id == team_id).cloned() else {
        return;
    };
    let bill = crate::finances::calc_wages(game, team_id);
    let Some(player_id) = release_unsold_surplus(game, &team, bill, previously_listed, today)
    else {
        return;
    };
    game.delegation
        .sales
        .listed
        .retain(|held| *held != player_id);
    let name = game
        .players
        .iter()
        .find(|p| p.id == player_id)
        .map(|p| p.full_name.clone())
        .unwrap_or_default();
    log(
        game,
        Duty::Offers,
        "delegation.log.released",
        vec![param("player", name), param("bill", bill)],
    );
}

/// What may be listed: not protected, not already listed, not set aside, not mid-registration, and
/// the squad stays above the floor without him.
fn may_list(game: &Game, candidate: &ListingCandidate, today: &str) -> bool {
    let player = &game.players[candidate.index];
    !player.transfer_listed
        && !game.delegation.is_protected(&player.id)
        && !game.delegation.sales.is_shelved(&player.id, today)
        && !crate::transfers::player_has_pending_registration(player)
        && crate::squad_floor::departure_would_leave_short(game, &player.id).is_none()
}

fn list_what_is_not_needed(game: &mut Game, team_id: &str) {
    let today_date = date(game);
    let today = stamp(game);
    let Some(team) = game.teams.iter().find(|t| t.id == team_id).cloned() else {
        return;
    };
    let bill = crate::finances::calc_wages(game, team_id);
    let held = game
        .delegation
        .sales
        .listed
        .iter()
        .filter(|id| {
            game.players
                .iter()
                .any(|p| p.id == **id && p.transfer_listed)
        })
        .count();
    let room = ASSISTANT_LISTING_CAPACITY.saturating_sub(held);
    if room == 0 {
        return;
    }
    let wanted: Vec<ListingCandidate> = ranked_sellable(game, &team, bill, today_date)
        .into_iter()
        .filter(|candidate| may_list(game, candidate, &today))
        .take(room)
        .collect();
    for candidate in wanted {
        let player_id = game.players[candidate.index].id.clone();
        match authorize_sale(&game.delegation, Duty::Offers) {
            Authorization::Auto => list(game, &player_id, &candidate),
            Authorization::NeedsApproval(_) => {
                propose_listing(game, &player_id, &candidate, &today)
            }
            Authorization::NotDelegated => {}
        }
    }
}

fn describe(game: &Game, candidate: &ListingCandidate) -> Vec<(String, String)> {
    let player = &game.players[candidate.index];
    vec![
        param("player", &player.full_name),
        param("player_id", &player.id),
        param("why", candidate.reason.name()),
        param(
            "loss",
            (candidate.net_loss * crate::squad_intelligence::model::SLOTS as f32).round() as i32,
        ),
        param("value", player.market_value),
        param("wage", player.wage()),
    ]
}

fn list(game: &mut Game, player_id: &str, candidate: &ListingCandidate) {
    let params = describe(game, candidate);
    if let Some(player) = game.players.iter_mut().find(|p| p.id == player_id) {
        player.transfer_listed = true;
    }
    game.delegation.sales.listed.push(player_id.to_string());
    let key = match candidate.reason.name() {
        "funds" => "delegation.log.listedFunds",
        "books" => "delegation.log.listedBooks",
        "leaving" => "delegation.log.listedLeaving",
        _ => "delegation.log.listedSurplus",
    };
    log(game, Duty::Offers, key, params);
}

fn propose_listing(game: &mut Game, player_id: &str, candidate: &ListingCandidate, today: &str) {
    let params = describe(game, candidate);
    let reason_key = match candidate.reason.name() {
        "funds" => "delegation.reason.listFunds",
        "books" => "delegation.reason.listBooks",
        "leaving" => "delegation.reason.listLeaving",
        _ => "delegation.reason.listSurplus",
    };
    game.delegation.propose(
        Duty::Offers,
        today,
        ProposalAction::ListForSale {
            player_id: player_id.to_string(),
        },
        "delegation.proposal.listPlayer",
        params,
        reason_key,
    );
}

/// The manager approved listing a player: he is listed (and the assistant may take him off again
/// if the squad changes), unless he has been protected since or the squad could not spare him.
pub(super) fn list_approved(game: &mut Game, player_id: &str) -> Result<(), String> {
    if game.delegation.is_protected(player_id) {
        return Err("be.error.delegation.playerProtected".to_string());
    }
    crate::squad_floor::ensure_departure_keeps_floor(game, player_id)?;
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == player_id)
        .ok_or("be.error.playerNotFound")?;
    // Listed on the manager's approval, not on the assistant's own: it is the manager's listing,
    // and the assistant never takes it off.
    player.transfer_listed = true;
    let name = player.full_name.clone();
    record(
        game,
        Duty::Offers,
        "delegation.log.listedApproved",
        vec![param("player", name)],
        DecisionOutcome::Approved,
    );
    Ok(())
}
