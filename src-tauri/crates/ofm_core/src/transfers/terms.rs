//! What a permanent transfer carries beyond its fee: instalments, a clause on the next sale, a
//! signing bonus and the player's own say.
//!
//! Money rules, in one place:
//! * **Budget envelope.** The buyer's transfer budget is debited the whole fee plus any bonus the
//!   day the deal completes — it is a commitment. The seller's envelope is credited as each
//!   payment actually arrives.
//! * **Cash.** Only what is due moves: the first instalment and the bonus on completion, each
//!   later instalment on its due date (see [`process_transfer_obligations`]).
//! * **Sell-on.** Settled in full from the selling club's cash when the player is next sold, in
//!   the same atomic posting as the fee.

use super::affordability::FeeBlock;
use super::*;
use crate::finances::{CashKind, PostRequest, post_all, validate_posts};
use chrono::Months;
use domain::transfer_terms::{MAX_INSTALMENTS, MAX_SELL_ON_PCT, TransferObligation, TransferTerms};

pub const ERR_INVALID_DEAL_TERMS: &str = "be.error.transfers.invalidDealTerms";
pub const ERR_SELL_ON_UNAFFORDABLE: &str = "be.error.transfers.sellOnUnaffordable";
/// A club that asks to be paid over several years wants a little more for the wait.
const DEFERRAL_PREMIUM_PCT_PER_EXTRA_INSTALMENT: u64 = 4;
/// A seller gives up one per cent of the asking price for every this-many per cent of sell-on.
const SELL_ON_PCT_PER_DISCOUNT_PCT: u64 = 3;
/// A player will not drop this far down the reputation ladder without something in return.
const STEP_DOWN_MARGIN: u32 = 100;
/// A bonus of this many weeks of his usual wage is a real sweetener to a player.
const SWEETENER_BONUS_WEEKS: u64 = 26;

pub(crate) fn validate_terms(terms: &TransferTerms) -> Result<(), String> {
    if terms.instalments == 0
        || terms.instalments > MAX_INSTALMENTS
        || terms.sell_on_pct > MAX_SELL_ON_PCT
    {
        return Err(ERR_INVALID_DEAL_TERMS.to_string());
    }
    Ok(())
}

/// The share of the fee in each later instalment, and the (larger or equal) first one that takes
/// the rounding remainder, so the parts always add up to the fee.
pub(crate) fn instalment_split(fee: u64, instalments: u8) -> (u64, u64) {
    let n = u64::from(instalments.max(1));
    let share = fee / n;
    (fee - share * (n - 1), share)
}

/// Cash the buyer pays on completion: the first instalment and the bonus.
pub(crate) fn cash_due_on_completion(fee: u64, terms: &TransferTerms) -> u64 {
    instalment_split(fee, terms.instalments)
        .0
        .saturating_add(terms.signing_bonus)
}

/// Percentage a seller applies to its minimum price for these terms: instalments cost it a
/// premium, a sell-on clause earns the buyer a discount.
pub(crate) fn seller_price_pct(terms: &TransferTerms) -> u64 {
    let premium =
        DEFERRAL_PREMIUM_PCT_PER_EXTRA_INSTALMENT * u64::from(terms.instalments.saturating_sub(1));
    let discount = u64::from(terms.sell_on_pct) / SELL_ON_PCT_PER_DISCOUNT_PCT;
    100 + premium - discount
}

/// Whether `team` can carry these terms: the cash due on completion within its cash, and the
/// whole fee and bonus within its transfer budget. Plain terms ask exactly what a plain fee asks.
pub(crate) fn fund_block(team: &Team, fee: u64, terms: &TransferTerms) -> Result<(), FeeBlock> {
    let commitment = fee.saturating_add(terms.signing_bonus);
    let cash = cash_due_on_completion(fee, terms);
    let (Ok(commitment), Ok(cash)) = (i64::try_from(commitment), i64::try_from(cash)) else {
        return Err(FeeBlock::InsufficientFunds);
    };
    if team.finance < cash {
        return Err(FeeBlock::InsufficientFunds);
    }
    if team.transfer_budget < commitment {
        return Err(FeeBlock::TransferBudgetTooLow);
    }
    Ok(())
}

/// A player who has no use for a promise cannot be given one. Checked when the bid is made, so
/// the refusal comes before any club has agreed to anything.
pub(crate) fn ensure_promise_is_wanted(
    player: &Player,
    terms: &TransferTerms,
    today: NaiveDate,
) -> Result<(), String> {
    if terms.promise_stepping_stone && !wants_stepping_stone(player, today) {
        return Err(ERR_PROMISE_NOT_WANTED.to_string());
    }
    Ok(())
}

/// Whether the player himself turns the move down: an ambitious young player will not step down
/// to a clearly lesser club unless the terms give him something — the promise of a way out, or a
/// bonus worth half a year of his usual wage.
pub(crate) fn player_refuses_move(
    player: &Player,
    owner: &Team,
    buyer: &Team,
    terms: &TransferTerms,
    today: NaiveDate,
) -> bool {
    if !wants_stepping_stone(player, today) {
        return false;
    }
    let steps_down = buyer.reputation.saturating_add(STEP_DOWN_MARGIN) < owner.reputation;
    if !steps_down {
        return false;
    }
    let weekly = u64::from(crate::contracts::reference_player_wage(player));
    let bonus_is_a_sweetener = terms.signing_bonus > 0
        && terms.signing_bonus >= weekly.saturating_mul(SWEETENER_BONUS_WEEKS);
    !(terms.promise_stepping_stone || bonus_is_a_sweetener)
}

/// A completed transfer's money-relevant facts, bundled so the settlement steps share one view.
#[derive(Clone, Copy)]
pub(super) struct Deal<'a> {
    pub player_id: &'a str,
    pub to_team_id: &'a str,
    pub from_team_id: &'a str,
    pub fee: u64,
    pub terms: &'a TransferTerms,
    pub date: NaiveDate,
}

/// What was settled at completion, for the caller to finish the books.
pub(super) struct Settlement {
    pub first_instalment: u64,
    /// `(obligation id, beneficiary club, amount)` of every sell-on clause this sale triggered.
    pub sell_on_paid: Vec<(String, String, u64)>,
}

/// Posts every cash movement of a completed transfer in one atomic step: the first instalment,
/// the bonus, and any sell-on clause the sale triggers.
pub(super) fn settle_cash(game: &mut Game, deal: &Deal) -> Result<Settlement, String> {
    let Deal {
        player_id,
        to_team_id,
        from_team_id,
        fee,
        terms,
        date,
    } = *deal;
    let to_i64 = |value: u64| {
        i64::try_from(value).map_err(|_| "be.error.finance.amountOverflow".to_string())
    };
    let (first, _) = instalment_split(fee, terms.instalments);
    let mut requests = vec![
        PostRequest::new(to_team_id, -to_i64(first)?, CashKind::TransferFeeOut, date),
        PostRequest::new(from_team_id, to_i64(first)?, CashKind::TransferFeeIn, date),
    ];
    if terms.signing_bonus > 0 {
        requests.push(PostRequest::new(
            to_team_id,
            -to_i64(terms.signing_bonus)?,
            CashKind::SigningBonus,
            date,
        ));
    }
    let sell_on_paid = sell_on_due(game, player_id, from_team_id, fee);
    for (_, beneficiary, owed) in sell_on_paid.iter().filter(|(_, _, owed)| *owed > 0) {
        requests.push(PostRequest::new(
            from_team_id,
            -to_i64(*owed)?,
            CashKind::TransferFeeOut,
            date,
        ));
        requests.push(PostRequest::new(
            beneficiary.as_str(),
            to_i64(*owed)?,
            CashKind::TransferFeeIn,
            date,
        ));
    }
    ensure_sale_clauses_payable(game, player_id, from_team_id, fee, terms)?;
    post_all(game, &requests)?;
    Ok(Settlement {
        first_instalment: first,
        sell_on_paid,
    })
}

/// Adjusts the transfer-budget envelopes for a settled transfer; see the module notes.
pub(super) fn apply_budgets(game: &mut Game, deal: &Deal, settlement: &Settlement) {
    let Deal {
        to_team_id,
        from_team_id,
        fee,
        terms,
        ..
    } = *deal;
    let signed = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
    let sell_on_total: u64 = settlement
        .sell_on_paid
        .iter()
        .map(|(_, _, owed)| owed)
        .sum();
    if let Some(buyer) = game.teams.iter_mut().find(|team| team.id == to_team_id) {
        buyer.transfer_budget -= signed(fee.saturating_add(terms.signing_bonus));
    }
    if let Some(seller) = game.teams.iter_mut().find(|team| team.id == from_team_id) {
        seller.transfer_budget += signed(settlement.first_instalment) - signed(sell_on_total);
    }
    for (_, beneficiary, owed) in &settlement.sell_on_paid {
        if let Some(team) = game.teams.iter_mut().find(|team| &team.id == beneficiary) {
            team.transfer_budget += signed(*owed);
        }
    }
}

/// Settles the clauses the sale triggered, schedules the instalments still to come, records a
/// clause the buyer granted, and gives the player the promise he was made.
pub(super) fn record_obligations(game: &mut Game, deal: &Deal, settlement: &Settlement) {
    let Deal {
        player_id,
        to_team_id,
        from_team_id,
        fee,
        terms,
        date,
    } = *deal;
    let today = date.format("%Y-%m-%d").to_string();
    for obligation in &mut game.transfer_obligations {
        if let TransferObligation::SellOn { id, settled_on, .. } = obligation
            && settlement
                .sell_on_paid
                .iter()
                .any(|(paid, _, _)| paid == id)
        {
            *settled_on = Some(today.clone());
        }
    }
    let (_, share) = instalment_split(fee, terms.instalments);
    for number in 2..=terms.instalments {
        let Some(due) = date.checked_add_months(Months::new(12 * u32::from(number - 1))) else {
            continue;
        };
        game.transfer_obligations
            .push(TransferObligation::Instalment {
                id: Uuid::new_v4().to_string(),
                player_id: player_id.to_string(),
                payer_team_id: to_team_id.to_string(),
                payee_team_id: from_team_id.to_string(),
                amount: share,
                due: due.format("%Y-%m-%d").to_string(),
                number,
                of: terms.instalments,
                paid_on: None,
            });
    }
    if terms.sell_on_pct > 0 {
        game.transfer_obligations.push(TransferObligation::SellOn {
            id: Uuid::new_v4().to_string(),
            player_id: player_id.to_string(),
            holder_team_id: to_team_id.to_string(),
            beneficiary_team_id: from_team_id.to_string(),
            pct: terms.sell_on_pct,
            settled_on: None,
        });
    }
    if terms.promise_stepping_stone {
        let buyer = game
            .teams
            .iter()
            .find(|team| team.id == to_team_id)
            .cloned();
        if let (Some(buyer), Some(player)) = (
            buyer,
            game.players
                .iter_mut()
                .find(|player| player.id == player_id),
        ) {
            player.morale_core.stepping_stone_promise = make_promise(&buyer, date);
        }
    }
}

/// How long a settled obligation is kept before it is dropped from the save.
const SETTLED_RETENTION_DAYS: i64 = 120;

/// Pays every instalment that has fallen due, once. Each is marked paid in the same step that
/// moves its money, so a second run the same day (or a reload in between) pays nothing twice. A
/// payment whose club no longer exists is voided rather than retried for ever.
pub fn process_transfer_obligations(game: &mut Game) {
    let date = game.clock.current_date.date_naive();
    let today = date.format("%Y-%m-%d").to_string();
    let user_team_id = game.manager.team_id.clone();
    let mut notices = Vec::new();

    for index in 0..game.transfer_obligations.len() {
        let TransferObligation::Instalment {
            player_id,
            payer_team_id,
            payee_team_id,
            amount,
            due,
            number,
            of,
            paid_on: None,
            ..
        } = game.transfer_obligations[index].clone()
        else {
            continue;
        };
        if NaiveDate::parse_from_str(&due, "%Y-%m-%d").is_ok_and(|day| day > date) {
            continue;
        }
        let both_clubs_exist = [&payer_team_id, &payee_team_id]
            .iter()
            .all(|id| game.teams.iter().any(|team| &team.id == *id));
        let Ok(cents) = i64::try_from(amount) else {
            continue;
        };
        let paid = both_clubs_exist
            && post_all(
                game,
                &[
                    PostRequest::new(&payer_team_id, -cents, CashKind::TransferFeeOut, date),
                    PostRequest::new(&payee_team_id, cents, CashKind::TransferFeeIn, date),
                ],
            )
            .is_ok();
        if let TransferObligation::Instalment { paid_on, .. } =
            &mut game.transfer_obligations[index]
        {
            // Voided and paid both close it: a payment between clubs that are gone cannot recur.
            *paid_on = (paid || !both_clubs_exist).then(|| today.clone());
        }
        if paid {
            if let Some(payee) = game.teams.iter_mut().find(|team| team.id == payee_team_id) {
                payee.transfer_budget += cents;
            }
            if user_team_id.as_deref() == Some(payer_team_id.as_str())
                || user_team_id.as_deref() == Some(payee_team_id.as_str())
            {
                notices.push((player_id, payer_team_id, payee_team_id, amount, number, of));
            }
        }
    }

    for (player_id, payer, payee, amount, number, of) in notices {
        let user_pays = user_team_id.as_deref() == Some(payer.as_str());
        let message = crate::messages::transfer_instalment_message(
            &player_id,
            &game
                .players
                .iter()
                .find(|player| player.id == player_id)
                .map_or_else(|| player_id.clone(), |player| player.full_name.clone()),
            &game.team_name_or_id(if user_pays { &payee } else { &payer }),
            amount,
            number,
            of,
            user_pays,
            &today,
        );
        crate::inbox::emit(game, message);
    }

    game.transfer_obligations.retain(|obligation| {
        let settled = match obligation {
            TransferObligation::Instalment { paid_on, .. } => paid_on.as_deref(),
            TransferObligation::SellOn { settled_on, .. } => settled_on.as_deref(),
        };
        settled
            .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
            .is_none_or(|day| (date - day).num_days() < SETTLED_RETENTION_DAYS)
    });
}

/// Same checks as the posting, with no mutation: the money due on completion can be posted.
pub(super) fn ensure_completion_postable(
    game: &Game,
    player_id: &str,
    buyer_id: &str,
    seller_id: &str,
    fee: u64,
    terms: &TransferTerms,
) -> Result<(), String> {
    ensure_sale_clauses_payable(game, player_id, seller_id, fee, terms)?;
    let date = game.clock.current_date.date_naive();
    let to_i64 = |value: u64| {
        i64::try_from(value).map_err(|_| "be.error.finance.amountOverflow".to_string())
    };
    let (first, _) = instalment_split(fee, terms.instalments);
    let mut requests = vec![
        PostRequest::new(buyer_id, -to_i64(first)?, CashKind::TransferFeeOut, date),
        PostRequest::new(seller_id, to_i64(first)?, CashKind::TransferFeeIn, date),
    ];
    if terms.signing_bonus > 0 {
        requests.push(PostRequest::new(
            buyer_id,
            -to_i64(terms.signing_bonus)?,
            CashKind::SigningBonus,
            date,
        ));
    }
    validate_posts(game, &requests)
}

/// A bonus paid on its own (a free agent's) has to fit the club's cash and transfer budget, which
/// is what a fee of the same size would have to.
pub fn ensure_bonus_affordable(team: &Team, bonus: u64) -> Result<(), String> {
    super::affordability::fee_block(team, bonus).map_err(|block| block.error_key().to_string())
}

/// Every sell-on clause this sale triggers: `(obligation id, beneficiary club, amount owed)`.
/// A clause binds the club that holds it, on the next sale of that player, for a share of the fee.
pub(super) fn sell_on_due(
    game: &Game,
    player_id: &str,
    from_team_id: &str,
    fee: u64,
) -> Vec<(String, String, u64)> {
    game.transfer_obligations
        .iter()
        .filter_map(|obligation| match obligation {
            TransferObligation::SellOn {
                id,
                player_id: clause_player,
                holder_team_id,
                beneficiary_team_id,
                pct,
                settled_on: None,
            } if clause_player == player_id && holder_team_id == from_team_id => Some((
                id.clone(),
                beneficiary_team_id.clone(),
                fee.saturating_mul(u64::from(*pct)) / 100,
            )),
            _ => None,
        })
        .collect()
}

/// Whether the selling club can pay the clauses this sale triggers: out of what it holds plus the
/// first instalment it is about to receive. Checked before a deal is marked agreed, so a refusal
/// cannot leave one agreed with the player still at his club.
pub(super) fn ensure_sale_clauses_payable(
    game: &Game,
    player_id: &str,
    from_team_id: &str,
    fee: u64,
    terms: &TransferTerms,
) -> Result<(), String> {
    let owed: u64 = sell_on_due(game, player_id, from_team_id, fee)
        .iter()
        .map(|(_, _, owed)| owed)
        .sum();
    if owed == 0 {
        return Ok(());
    }
    let cash = game
        .teams
        .iter()
        .find(|team| team.id == from_team_id)
        .map_or(0, |team| team.finance);
    let first = i64::try_from(instalment_split(fee, terms.instalments).0).unwrap_or(i64::MAX);
    let owed = i64::try_from(owed).unwrap_or(i64::MAX);
    if cash.saturating_add(first) < owed {
        return Err(ERR_SELL_ON_UNAFFORDABLE.to_string());
    }
    Ok(())
}
