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
