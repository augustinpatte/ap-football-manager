//! The administrative step between an agreed paid transfer and its registration.
//!
//! The clubs and the player can agree on a day, but a registration still has to be filed and
//! cleared. With the window open this used to take no time at all: the player left the day the
//! selling club said yes. [`deal_registration_date`] is the one place that decides when an agreed
//! deal is registered, so the manager's purchases, the manager's sales and the preview that
//! promises a date cannot disagree.
//!
//! The delay is a function of the deal, never a constant and never random: a stable hash of the
//! player, the two clubs and the day picks within a range that grows with the fee, with crossing
//! a border and with a structured payment. It is therefore the same on a reload and under
//! fast-forward.

use super::*;
use crate::stable_hash::stable_hash;

/// Below this a fee is a small deal that a club can file the same day.
const SMALL_FEE: u64 = 500_000;
/// From this a fee is a large deal with more paperwork and more people to sign it off.
const LARGE_FEE: u64 = 5_000_000;
/// Extra days for a move between countries (international clearance).
const INTERNATIONAL_EXTRA_DAYS: i64 = 1;
/// Extra days for a fee in instalments or with a sell-on clause (the contract has to be drawn up).
const STRUCTURED_EXTRA_DAYS: i64 = 1;

/// Everything about a deal that makes it quicker or slower to register.
pub(crate) struct DealShape<'a> {
    pub(crate) fee: u64,
    pub(crate) international: bool,
    pub(crate) terms: &'a TransferTerms,
}

/// Inclusive range of days a deal of this size takes before the extras.
fn base_range(fee: u64) -> (i64, i64) {
    if fee < SMALL_FEE {
        (0, 1)
    } else if fee < LARGE_FEE {
        (1, 2)
    } else {
        (2, 3)
    }
}

/// Days of administration for a deal, picked by `key` inside the range its shape allows.
pub(crate) fn processing_days(key: &str, shape: &DealShape<'_>) -> i64 {
    let (low, high) = base_range(shape.fee);
    let spread = u64::try_from(high - low + 1).unwrap_or(1);
    let picked =
        low + i64::try_from(stable_hash(key.as_bytes(), 0x7265_6769) % spread).unwrap_or(0);
    let international = if shape.international {
        INTERNATIONAL_EXTRA_DAYS
    } else {
        0
    };
    let structured = if shape.terms.instalments > 1 || shape.terms.sell_on_pct > 0 {
        STRUCTURED_EXTRA_DAYS
    } else {
        0
    };
    picked + international + structured
}

/// When an agreed deal for `player` to `buyer_team_id` is registered.
///
/// With the window shut it is the day the window opens, as before. With it open it is today plus
/// the deal's administration, never later than the closing day: a deal agreed on deadline day
/// completes that day, because there is no day left to wait. A window that does not say how long
/// it has left (only hand-built state: an open window always does) adds no wait.
pub(crate) fn deal_registration_date(
    game: &Game,
    player: &Player,
    buyer_team_id: &str,
    fee: u64,
    terms: &TransferTerms,
) -> Result<NaiveDate, String> {
    let date = transfer_registration_date(game)?;
    let today = game.clock.current_date.date_naive();
    if date != today {
        return Ok(date);
    }
    let seller_team_id = player.team_id.as_deref().unwrap_or_default();
    let country_of = |team_id: &str| {
        game.teams
            .iter()
            .find(|team| team.id == team_id)
            .map(|team| team.country.as_str())
    };
    let shape = DealShape {
        fee,
        international: country_of(buyer_team_id) != country_of(seller_team_id),
        terms,
    };
    let key = format!("{}|{buyer_team_id}|{seller_team_id}|{today}", player.id);
    let left = game
        .season_context
        .transfer_window
        .days_remaining
        .unwrap_or(0)
        .max(0);
    Ok(today + Duration::days(processing_days(&key, &shape).min(left)))
}

/// When an agreed loan of `player` to `borrower_team_id` starts: the same administration as a
/// permanent move, quicker for a plain loan (a loan with a purchase option has a contract to draw
/// up, so it takes as long as a small fee does).
pub(crate) fn loan_deal_registration_date(
    game: &Game,
    player: &Player,
    borrower_team_id: &str,
    has_buy_option: bool,
) -> Result<NaiveDate, String> {
    let fee = if has_buy_option { SMALL_FEE } else { 0 };
    deal_registration_date(
        game,
        player,
        borrower_team_id,
        fee,
        &TransferTerms::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> TransferTerms {
        TransferTerms::default()
    }

    fn days_over_many_keys(shape: &DealShape<'_>) -> Vec<i64> {
        (0..200)
            .map(|i| processing_days(&format!("player-{i}|buyer|seller|2026-08-01"), shape))
            .collect()
    }

    /// Given a small domestic deal
    /// When many are processed
    /// Then some are same-day and some take a day, none more.
    #[test]
    fn a_small_domestic_deal_is_often_same_day_and_never_slow() {
        let terms = plain();
        let days = days_over_many_keys(&DealShape {
            fee: 100_000,
            international: false,
            terms: &terms,
        });
        assert!(days.contains(&0) && days.contains(&1));
        assert!(days.iter().all(|d| (0..=1).contains(d)));
    }

    /// Given a large deal
    /// When many are processed
    /// Then none is same-day.
    #[test]
    fn a_large_deal_is_never_instant() {
        let terms = plain();
        let days = days_over_many_keys(&DealShape {
            fee: 40_000_000,
            international: false,
            terms: &terms,
        });
        assert!(days.iter().all(|d| (2..=3).contains(d)));
    }

    /// Given the same deal with a border to cross and with instalments
    /// When compared with the plain domestic one
    /// Then each adds a day, both add two.
    #[test]
    fn crossing_a_border_and_a_structured_fee_each_add_a_day() {
        let plain_terms = plain();
        let structured = TransferTerms {
            instalments: 3,
            ..plain()
        };
        let key = "player-1|buyer|seller|2026-08-01";
        let base = processing_days(
            key,
            &DealShape {
                fee: 2_000_000,
                international: false,
                terms: &plain_terms,
            },
        );
        let abroad = processing_days(
            key,
            &DealShape {
                fee: 2_000_000,
                international: true,
                terms: &plain_terms,
            },
        );
        let both = processing_days(
            key,
            &DealShape {
                fee: 2_000_000,
                international: true,
                terms: &structured,
            },
        );
        assert_eq!((abroad - base, both - base), (1, 2));
    }

    /// Given the same key
    /// When asked twice
    /// Then the answer is the same.
    #[test]
    fn the_same_deal_always_takes_the_same_time() {
        let terms = plain();
        let shape = DealShape {
            fee: 9_000_000,
            international: true,
            terms: &terms,
        };
        assert_eq!(processing_days("k", &shape), processing_days("k", &shape));
    }
}
