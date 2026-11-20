//! Transfer-window state and the registration gates that depend on it.
//!
//! A deal agreed while the window is shut is not a deal yet — it is an agreement waiting for
//! a registration date. These predicates are what the rest of the module asks before treating
//! a player as available.

use super::*;

pub(crate) fn has_pending_loan_registration(player: &domain::player::Player) -> bool {
    player
        .loan_offers
        .iter()
        .any(|offer| offer.status == LoanOfferStatus::PendingRegistration)
}
pub(crate) fn has_pending_transfer_registration(player: &domain::player::Player) -> bool {
    player
        .transfer_offers
        .iter()
        .any(|offer| offer.status == TransferOfferStatus::PendingRegistration)
}
pub(crate) fn player_has_active_or_pending_loan(player: &domain::player::Player) -> bool {
    player.active_loan.is_some() || has_pending_loan_registration(player)
}
/// The weekly wages `team_id` has agreed to pay in deals that are agreed and wait for their
/// registration date: a purchase at the wage its terms name (or the standard one a renewal is
/// judged by), a loan at the share of the wage it will carry.
///
/// Until a deal registers its player is not on the club's books, so a bill read from the squad
/// does not hold his wage: two approaches each judged against the same room could both be agreed
/// and both register, taking the club past its wage budget (the P4.6 delegation finding: 18,000 and
/// 16,000 a week into a room of 28,120). Reserving them here is derived from the offers, which are
/// saved with the players: it survives a save, falls away the moment a deal registers (the wage
/// is then in the bill), is voided or is withdrawn, and never cancels a deal that was agreed.
pub(crate) fn pending_wage_commitments(game: &Game, team_id: &str) -> i64 {
    let mut reserved = 0;
    for player in &game.players {
        for offer in &player.transfer_offers {
            if offer.status == TransferOfferStatus::PendingRegistration
                && offer.from_team_id == team_id
            {
                reserved += execution::buyers_agreed_terms(game, player, team_id, &offer.terms)
                    .map_or_else(|_| i64::from(player.wage()), |(_, wage, _)| i64::from(wage));
            }
        }
        for offer in &player.loan_offers {
            if offer.status == LoanOfferStatus::PendingRegistration && offer.from_team_id == team_id
            {
                reserved += loan_wage_share(player, offer.wage_contribution_pct);
            }
        }
    }
    reserved
}
#[cfg(test)]
/// The players `team_id` has agreed to buy that wait to register, other than `except_player_id`:
/// each will take a place on its senior list. Derived from the saved offers like the wage and the
/// fee. A player already registered at the club (a loan that becomes a purchase) takes none.
pub(crate) fn pending_incoming_purchases(
    game: &Game,
    team_id: &str,
    except_player_id: &str,
) -> usize {
    game.players
        .iter()
        .filter(|player| {
            player.id != except_player_id && player.team_id.as_deref() != Some(team_id)
        })
        .filter(|player| {
            player.transfer_offers.iter().any(|offer| {
                offer.status == TransferOfferStatus::PendingRegistration
                    && offer.from_team_id == team_id
            })
        })
        .count()
}
#[cfg(test)]

/// The players `team_id` has agreed to take on loan that wait to register, other than
/// `except_player_id`: each will take a place on its senior list exactly as a purchase does. A
/// player already registered at the club (a loan that has started) is counted by the squad
/// capacity itself, not here.
pub(crate) fn pending_incoming_loans(game: &Game, team_id: &str, except_player_id: &str) -> usize {
    game.players
        .iter()
        .filter(|player| {
            player.id != except_player_id && player.team_id.as_deref() != Some(team_id)
        })
        .filter(|player| {
            player.loan_offers.iter().any(|offer| {
                offer.status == LoanOfferStatus::PendingRegistration
                    && offer.from_team_id == team_id
            })
        })
        .count()
}
#[cfg(test)]

/// The club's own seniors who are out on loan elsewhere, other than `except_player_id`. Each
/// returns to the club's list and takes a place there, so the club cannot promise that place to a
/// signing while he is away: a club that lent a senior out and signed his replacement would be over
/// 30 when the loan ended. `except_player_id` is the player whose own registration is being
/// checked — a loan ending resumes the place it left, it does not take a second one.
pub(crate) fn loaned_out_seniors(game: &Game, team_id: &str, except_player_id: &str) -> usize {
    game.players
        .iter()
        .filter(|player| !player.retired && player.id != except_player_id)
        .filter(|player| player.squad_role == domain::player::SquadRole::Senior)
        .filter(|player| player.team_id.as_deref() != Some(team_id))
        .filter(|player| {
            player
                .active_loan
                .as_ref()
                .is_some_and(|loan| loan.parent_team_id == team_id)
        })
        .count()
}

/// The refusal when `team_id` has no place left on its senior list for `incoming_player_id` once
/// the places taken by purchases and loans agreed and waiting to register, and the places its own
/// seniors out on loan will return to, are set aside; `None` when he can be registered.
/// `incoming_player_id` is excepted from the reservation, so a deal that is registering does not
/// block itself through its own `PendingRegistration` offer.
///
/// Every senior signing route — an in-game purchase, a free-agent talk, a global-market signing,
/// an AI club filling its squad, an academy promotion and an AI loan — asks this, so a promised
/// place is respected everywhere a senior place is taken. A loan that waits to register reserves
/// its place as a purchase does: before that, two loans could each be promised the last one.
///
/// A senior out on loan reserves his place too. He is not registered today, so the list shows a
/// free place, but he returns to it when the loan ends; a club that filled the place meanwhile
/// would be over the limit on the day he comes back. The same rule is what keeps the ten-season
/// study's squads at or under 30.
pub(crate) fn signing_place_refusal(
    game: &Game,
    team_id: &str,
    incoming_player_id: &str,
) -> Option<String> {
    // One pass over the world, not four: this gate runs for every club's every signing, so it is
    // the hottest loop of an AI day. The team is compared once per player, and a player who
    // reserves nothing (almost all of them) costs no more than that comparison. The three counts
    // above (kept for the tests as the plain statement of the rule) are what each branch sums.
    let mut registered_seniors = 0;
    let mut promised = 0;
    for player in &game.players {
        if player.team_id.as_deref() == Some(team_id) {
            if !player.retired && player.squad_role == domain::player::SquadRole::Senior {
                registered_seniors += 1;
            }
            continue;
        }
        let reserved = places_reserved_by(player, team_id);
        if reserved > 0 && player.id != incoming_player_id {
            promised += reserved;
        }
    }
    let capacity = crate::contracts::SquadCapacity::from_counts(registered_seniors, 0, 0, 0);
    (capacity.free_places <= promised).then(|| capacity.full_error())
}

/// The places a player who is *not* registered at `team_id` holds for it: a purchase agreed and
/// waiting to register, a loan agreed and waiting to register, and, if he is one of its seniors out
/// on loan, the place he returns to. Each counts one. The incoming player's own exception is the
/// caller's.
fn places_reserved_by(player: &domain::player::Player, team_id: &str) -> usize {
    let pending_purchase = player.transfer_offers.iter().any(|offer| {
        offer.status == TransferOfferStatus::PendingRegistration && offer.from_team_id == team_id
    });
    let pending_loan = player.loan_offers.iter().any(|offer| {
        offer.status == LoanOfferStatus::PendingRegistration && offer.from_team_id == team_id
    });
    let loaned_out_senior = !player.retired
        && player.squad_role == domain::player::SquadRole::Senior
        && player
            .active_loan
            .as_ref()
            .is_some_and(|loan| loan.parent_team_id == team_id);
    usize::from(pending_purchase) + usize::from(pending_loan) + usize::from(loaned_out_senior)
}

/// The cash and transfer budget `team_id` has agreed to spend in purchases that wait for their
/// registration date. Derived from the saved offers like [`pending_wage_commitments`]: it survives
/// a save, ends the moment a deal registers (the money has then left the books) or is voided, and
/// a registration that fails for want of money is the notified failure it always was. Loans carry
/// no fee, so they reserve none. Read when a bid is made, never when one registers.
pub(crate) fn pending_fee_commitments(game: &Game, team_id: &str) -> terms::PendingFees {
    let mut reserved = terms::PendingFees::default();
    for player in &game.players {
        for offer in &player.transfer_offers {
            if offer.status == TransferOfferStatus::PendingRegistration
                && offer.from_team_id == team_id
            {
                let cash = terms::cash_due_on_completion(offer.fee, &offer.terms);
                let budget = offer.fee.saturating_add(offer.terms.signing_bonus);
                reserved.cash = reserved
                    .cash
                    .saturating_add(i64::try_from(cash).unwrap_or(i64::MAX));
                reserved.budget = reserved
                    .budget
                    .saturating_add(i64::try_from(budget).unwrap_or(i64::MAX));
            }
        }
    }
    reserved
}
pub(crate) fn player_has_pending_registration(player: &domain::player::Player) -> bool {
    player_has_active_or_pending_loan(player) || has_pending_transfer_registration(player)
}
pub(crate) fn transfer_window_is_open(game: &Game) -> bool {
    matches!(
        game.season_context.transfer_window.status,
        TransferWindowStatus::Open | TransferWindowStatus::DeadlineDay
    )
}
pub(crate) fn transfer_registration_date(game: &Game) -> Result<NaiveDate, String> {
    let current_date = game.clock.current_date.date_naive();
    if transfer_window_is_open(game) {
        return Ok(current_date);
    }

    let opens_on = game
        .season_context
        .transfer_window
        .opens_on
        .as_deref()
        .ok_or(ERR_TRANSFER_WINDOW_CLOSED)?;
    let registration_date = NaiveDate::parse_from_str(opens_on, "%Y-%m-%d")
        .map_err(|_| ERR_TRANSFER_WINDOW_CLOSED.to_string())?;

    if registration_date <= current_date {
        return Err(ERR_TRANSFER_WINDOW_CLOSED.to_string());
    }

    Ok(registration_date)
}
pub(crate) fn loan_registration_date(game: &Game) -> Result<NaiveDate, String> {
    transfer_registration_date(game)
}
