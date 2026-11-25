//! The club's money over the run, read from the cash journal and the two cash balances.
//!
//! The journal is the game's one ledger of payments (the report keeps none of its own). The two
//! balances are the club's cash before the first day and after the last; the difference between
//! them and the sum of the journal's posts is reported, not hidden, so a payment made outside the
//! journal would show up as `unexplained`.

use std::collections::BTreeMap;

use domain::finance::{CashKind, FOLDED_ID_PREFIX};

use crate::game::Game;

use super::tracker::PreRun;
use super::types::{CashKindTotal, FinancesReport};

pub(super) fn finances_of(
    game: &Game,
    club_id: &str,
    from: &str,
    until: &str,
    opening_cash: i64,
    closing_cash: i64,
    pre_run: &PreRun,
) -> FinancesReport {
    let mut totals: BTreeMap<String, (CashKind, i64, i64, u32)> = BTreeMap::new();
    let mut detail_complete = true;
    // A fold writes its post on the first day of the month; one for a month the run started in
    // stands for days before the run too, so it cannot be told apart from the run's own.
    let month_start = format!("{}-01", from.get(..7).unwrap_or(from));
    // An opening balance is the cash a club had before its first post, written as a post so the
    // journal sums to the balance; it changes nothing, so it is neither income nor expense.
    for post in game
        .cash_journal
        .iter()
        .filter(|post| post.club_id == club_id && post.kind != CashKind::OpeningBalance)
    {
        if post.id.starts_with(FOLDED_ID_PREFIX)
            && post.date.as_str() >= month_start.as_str()
            && post.date.as_str() < until
        {
            detail_complete = false;
        }
        // Paid before the run began: already in the opening cash.
        if pre_run.posts.contains(&post.id)
            || post.date.as_str() < from
            || post.date.as_str() >= until
        {
            continue;
        }
        let entry = totals
            .entry(format!("{:?}", post.kind))
            .or_insert((post.kind, 0, 0, 0));
        if post.amount >= 0 {
            entry.1 += post.amount;
        } else {
            entry.2 += post.amount;
        }
        entry.3 += 1;
    }
    let mut by_kind: Vec<CashKindTotal> = totals
        .into_values()
        .map(|(kind, income, expense, posts)| CashKindTotal {
            kind,
            income,
            expense,
            net: income + expense,
            posts,
        })
        .collect();
    by_kind.sort_by_key(|total| std::cmp::Reverse(total.net.unsigned_abs()));
    let explained: i64 = by_kind.iter().map(|total| total.net).sum();
    let fees = |kind: CashKind| {
        by_kind
            .iter()
            .find(|total| total.kind == kind)
            .map_or(0, |total| total.net)
    };
    let change = closing_cash - opening_cash;
    FinancesReport {
        opening_cash,
        closing_cash,
        change,
        transfer_fees_out: fees(CashKind::TransferFeeOut),
        transfer_fees_in: fees(CashKind::TransferFeeIn),
        posts: by_kind.iter().map(|total| total.posts).sum(),
        unexplained: change - explained,
        detail_complete,
        by_kind,
    }
}

/// Whether the journal holds a post for `club_id` on `date` of exactly `amount` and `kind`.
pub(super) fn journal_has(
    game: &Game,
    club_id: &str,
    kind: CashKind,
    date: &str,
    amount: i64,
) -> bool {
    game.cash_journal.iter().any(|post| {
        post.club_id == club_id && post.kind == kind && post.date == date && post.amount == amount
    })
}
