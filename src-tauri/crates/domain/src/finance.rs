use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;

use crate::team::FinancialTransactionKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CashPost {
    pub id: String,
    pub club_id: String,
    /// Positive is income, negative is expense. Integer euros.
    pub amount: i64,
    pub kind: CashKind,
    /// Game-clock date (`YYYY-MM-DD`), never wall-clock.
    pub date: String,
}

/// Chart of accounts. Additive — never `#[serde(other)]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CashKind {
    OpeningBalance,
    PlayerWages,
    StaffWages,
    Matchday,
    Sponsorship,
    CommercialCampaign,
    PrizeMoney,
    TransferFeeOut,
    TransferFeeIn,
    /// Paid to a player on joining, on top of his wage.
    SigningBonus,
    LoanFee,
    ContractTermination,
    BoardSupport,
    Facilities,
    Upkeep,
    Other,
}

impl CashKind {
    pub fn counts_toward_season_totals(self) -> bool {
        !matches!(
            self,
            Self::OpeningBalance | Self::TransferFeeIn | Self::TransferFeeOut | Self::LoanFee
        )
    }

    pub fn from_legacy_ledger(kind: FinancialTransactionKind) -> Self {
        match kind {
            FinancialTransactionKind::PrizeMoney => Self::PrizeMoney,
            FinancialTransactionKind::ContractTermination => Self::ContractTermination,
            FinancialTransactionKind::BoardSupport => Self::BoardSupport,
            FinancialTransactionKind::CommercialCampaign => Self::CommercialCampaign,
        }
    }
}

/// Append-only cash journal. `Clone` is a pointer bump; writers copy-on-write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CashJournal {
    posts: Arc<Vec<CashPost>>,
    clubs: Arc<HashSet<String>>,
}

impl Deref for CashJournal {
    type Target = [CashPost];

    fn deref(&self) -> &[CashPost] {
        &self.posts
    }
}

/// Prefix of the id of a post that stands for many (see [`CashJournal::fold_before`]).
pub const FOLDED_ID_PREFIX: &str = "fold:";

impl CashJournal {
    /// Replaces every post dated before `cutoff` (`YYYY-MM-DD`) by one post per club, kind and month,
    /// and returns the ids of the posts it created.
    ///
    /// A club's cash is the sum of its posts, and a fold changes no sum: each created post carries
    /// the exact total of the posts it replaces, in its first day of the month, so a club's balance,
    /// and its total by kind and month, are what they were. What is lost is the single payment: after
    /// a fold a month before the cutoff is one weekly-wages line, not four. An opening balance is
    /// kept as it is (it is the club's starting point, and there is one), and a post that is
    /// itself a fold is never folded again, so the operation is idempotent. The kept posts stay in
    /// their order and the created ones follow them, which is the order a reload from storage gives.
    pub fn fold_before(&mut self, cutoff: &str) -> Vec<String> {
        use std::collections::BTreeMap;
        let foldable = |post: &CashPost| {
            post.date.as_str() < cutoff
                && post.kind != CashKind::OpeningBalance
                && !post.id.starts_with(FOLDED_ID_PREFIX)
        };
        if !self.posts.iter().any(foldable) {
            return Vec::new();
        }
        let mut totals: BTreeMap<(String, String, String), (CashKind, i128)> = BTreeMap::new();
        let mut kept = Vec::with_capacity(self.posts.len());
        for post in self.posts.iter() {
            if foldable(post) {
                let month = post.date.get(..7).unwrap_or(&post.date).to_string();
                let kind_name = format!("{:?}", post.kind);
                let entry = totals
                    .entry((post.club_id.clone(), kind_name, month))
                    .or_insert((post.kind, 0));
                entry.1 += i128::from(post.amount);
            } else {
                kept.push(post.clone());
            }
        }
        let mut created = Vec::with_capacity(totals.len());
        for ((club_id, kind_name, month), (kind, total)) in totals {
            // A month's total of one club and kind is far inside i64: it is a few hundred posts of
            // wages. A total that were not is kept as it is rather than wrapped.
            let Ok(amount) = i64::try_from(total) else {
                continue;
            };
            let id = format!("{FOLDED_ID_PREFIX}{club_id}:{kind_name}:{month}");
            created.push(id.clone());
            kept.push(CashPost {
                id,
                club_id,
                amount,
                kind,
                date: format!("{month}-01"),
            });
        }
        *self = Self::from_vec(kept);
        created
    }

    pub fn from_vec(posts: Vec<CashPost>) -> Self {
        let clubs = posts.iter().map(|post| post.club_id.clone()).collect();
        Self {
            posts: Arc::new(posts),
            clubs: Arc::new(clubs),
        }
    }

    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.posts, &other.posts)
    }

    pub fn contains_club(&self, club_id: &str) -> bool {
        self.clubs.contains(club_id)
    }

    pub fn extend(&mut self, posts: impl IntoIterator<Item = CashPost>) {
        let extra: Vec<CashPost> = posts.into_iter().collect();
        if extra.is_empty() {
            return;
        }
        let clubs = Arc::make_mut(&mut self.clubs);
        for post in &extra {
            clubs.insert(post.club_id.clone());
        }
        Arc::make_mut(&mut self.posts).extend(extra);
    }

    /// Sum of posts for one club. Folded in `i128` so prefix order cannot panic.
    pub fn cash_for(&self, club_id: &str) -> i64 {
        let sum = self
            .iter()
            .filter(|post| post.club_id == club_id)
            .fold(0i128, |acc, post| acc + i128::from(post.amount));
        i64::try_from(sum).expect("journal overflow is a bug: post_all rejected it")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cash_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&CashKind::TransferFeeOut).unwrap();
        assert_eq!(json, "\"transfer_fee_out\"");
        let back: CashKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CashKind::TransferFeeOut);
    }

    #[test]
    fn unknown_kind_fails_to_deserialize() {
        let err = serde_json::from_str::<CashKind>("\"not_a_kind\"").unwrap_err();
        assert!(err.to_string().contains("unknown variant"));
    }

    fn dated(id: &str, club: &str, amount: i64, kind: CashKind, date: &str) -> CashPost {
        CashPost {
            id: id.to_string(),
            club_id: club.to_string(),
            amount,
            kind,
            date: date.to_string(),
        }
    }

    fn sample_journal() -> CashJournal {
        CashJournal::from_vec(vec![
            dated("o1", "a", 1_000, CashKind::OpeningBalance, "2030-07-01"),
            dated("w1", "a", -100, CashKind::PlayerWages, "2030-08-04"),
            dated("w2", "a", -100, CashKind::PlayerWages, "2030-08-11"),
            dated("m1", "a", 250, CashKind::Matchday, "2030-08-12"),
            dated("w3", "a", -100, CashKind::PlayerWages, "2030-09-01"),
            dated("b1", "b", -40, CashKind::PlayerWages, "2030-08-05"),
            dated("w9", "a", -100, CashKind::PlayerWages, "2032-03-01"),
        ])
    }

    /// Given a journal with several clubs, kinds and months, when posts before a cutoff are folded,
    /// then every club's balance is exactly what it was.
    #[test]
    fn a_fold_changes_no_club_balance() {
        let mut journal = sample_journal();
        let before = (journal.cash_for("a"), journal.cash_for("b"));

        let created = journal.fold_before("2031-01-01");

        assert!(!created.is_empty());
        assert_eq!((journal.cash_for("a"), journal.cash_for("b")), before);
    }

    /// Given the same journal, when it is folded, then the posts become one per club, kind and
    /// month (wages of August are one post of -200, September's another), the opening balance
    /// and the posts after the cutoff are kept as they are, and the journal is smaller.
    #[test]
    fn a_fold_leaves_one_post_per_club_kind_and_month() {
        let mut journal = sample_journal();
        let length_before = journal.len();

        let mut created = journal.fold_before("2031-01-01");
        created.sort();

        assert_eq!(
            created,
            vec![
                "fold:a:Matchday:2030-08",
                "fold:a:PlayerWages:2030-08",
                "fold:a:PlayerWages:2030-09",
                "fold:b:PlayerWages:2030-08",
            ]
        );
        let august = journal
            .iter()
            .find(|p| p.id == "fold:a:PlayerWages:2030-08")
            .unwrap();
        assert_eq!((august.amount, august.date.as_str()), (-200, "2030-08-01"));
        assert!(
            journal.iter().any(|p| p.id == "o1"),
            "the opening balance is kept"
        );
        assert!(
            journal.iter().any(|p| p.id == "w9"),
            "a post after the cutoff is kept"
        );
        assert!(journal.len() < length_before);
    }

    /// Given a journal that has been folded, when the same cutoff (or a later one) is folded again,
    /// then nothing changes: a fold is never folded, so the operation can run at every rollover.
    #[test]
    fn folding_twice_changes_nothing_the_second_time() {
        let mut journal = sample_journal();
        journal.fold_before("2031-01-01");
        let once = journal.clone();

        assert!(journal.fold_before("2031-01-01").is_empty());
        assert!(
            journal
                .fold_before("2035-01-01")
                .iter()
                .all(|id| id == "w9" || id.starts_with("fold:"))
        );
        assert_eq!(journal.cash_for("a"), once.cash_for("a"));
    }

    /// Given a journal with nothing older than the cutoff, when it is folded, then it is untouched
    /// (and does not share a new allocation for nothing).
    #[test]
    fn folding_nothing_leaves_the_journal_as_it_was() {
        let mut journal = sample_journal();
        let copy = journal.clone();
        assert!(journal.fold_before("2020-01-01").is_empty());
        assert!(journal.shares_storage_with(&copy));
    }

    fn post(id: &str, amount: i64) -> CashPost {
        CashPost {
            id: id.to_string(),
            club_id: "alpha".to_string(),
            amount,
            kind: CashKind::Other,
            date: "2026-02-16".to_string(),
        }
    }

    #[test]
    fn cash_for_does_not_depend_on_prefix_order() {
        let credits_first =
            CashJournal::from_vec(vec![post("1", i64::MAX), post("2", 1), post("3", -1)]);
        let debit_first =
            CashJournal::from_vec(vec![post("1", i64::MAX), post("3", -1), post("2", 1)]);
        assert_eq!(credits_first.cash_for("alpha"), i64::MAX);
        assert_eq!(debit_first.cash_for("alpha"), i64::MAX);
    }
}
