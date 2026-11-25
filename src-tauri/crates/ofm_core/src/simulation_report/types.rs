//! The shapes of a simulation report. Everything here is read back from the game's own records
//! (fixtures, movement ledgers, the cash journal, the delegation log, the inbox) or is the
//! difference between two snapshots the run took itself; none of it is an estimate.
//!
//! Names (clubs, players, competitions) travel beside their ids so a screen can drill down by id
//! and still show something when the id no longer resolves. Anything a player reads that is not a
//! name or a number is a translation key.

use domain::delegation::{DecisionOutcome, Duty};
use domain::finance::CashKind;
use domain::player::PlayerMovementKind;
use serde::{Deserialize, Serialize};

/// A list the report keeps short. `total` is the true count, so a screen can say "and 140 more";
/// the items are the first `MAX_LISTED` in date order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Listed<T> {
    pub items: Vec<T>,
    pub total: u32,
}

impl<T> Default for Listed<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            total: 0,
        }
    }
}

impl<T> Listed<T> {
    pub(super) fn capped(mut items: Vec<T>) -> Self {
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        items.truncate(super::MAX_LISTED);
        Self { items, total }
    }
}

/// What the run produced: one entry per stretch in which the manager held a club.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SimulationReport {
    /// Oldest first. Empty when the manager had no club for the whole run.
    pub clubs: Vec<ClubReport>,
}

/// How a managed stretch ended.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StretchEnd {
    /// The manager still had the club when the run stopped.
    Continues,
    /// The board dismissed the manager during `last_day`.
    Dismissed,
    /// The manager's club changed to another one without a dismissal being recorded.
    Moved,
}

/// What happened to one club under this manager between two dates. `from` is the first day played
/// for it and `until` the first day not played for it, so the two dates bound exactly the days
/// the report covers (an early stop shortens `until`; it is never the requested target).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClubReport {
    pub club_id: String,
    pub club_name: String,
    pub from: String,
    pub until: String,
    pub days_played: u32,
    pub ended: StretchEnd,
    pub matches: MatchesReport,
    pub transfers: TransfersReport,
    pub contracts: ContractsReport,
    pub finances: FinancesReport,
    pub squad: SquadReport,
    pub delegation: DelegationReport,
    pub events: Vec<ReportEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutcome {
    Win,
    Draw,
    Loss,
}

/// One finished match of the club.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchLine {
    pub fixture_id: String,
    pub competition_id: String,
    pub competition_name: String,
    pub competition_name_key: Option<String>,
    pub date: String,
    pub at_home: bool,
    pub opponent_id: String,
    pub opponent_name: String,
    pub goals_for: u8,
    pub goals_against: u8,
    /// Set when a knockout tie went to a shootout: the shootout score, club first.
    pub penalties_for: Option<u8>,
    pub penalties_against: Option<u8>,
    /// By the goals of the match; a shootout is shown beside it, not folded into it.
    pub outcome: MatchOutcome,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchesReport {
    pub played: u32,
    pub won: u32,
    pub drawn: u32,
    pub lost: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub fixtures: Listed<MatchLine>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    /// A player bought, by a transfer or by exercising a loan's purchase option.
    Purchase,
    Sale,
    /// A player borrowed by the club.
    LoanIn,
    /// A player of the club lent out.
    LoanOut,
    /// A player of the club came back from a loan.
    LoanReturned,
    /// A borrowed player went back to his own club.
    LoanEnded,
    /// A free agent signed.
    FreeSigning,
}

/// One move of a player into or out of the club, as the player's own movement ledger records it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferLine {
    pub date: String,
    pub kind: TransferKind,
    pub player_id: String,
    pub player_name: String,
    /// The fee the ledger records; `None` when it records none (a free signing, a loan return).
    pub fee: Option<u64>,
    pub counterparty_id: Option<String>,
    pub counterparty_name: Option<String>,
    pub loan_end_date: Option<String>,
    /// Whether a purchase option was what made the move.
    pub via_purchase_option: bool,
    /// Whether the cash journal holds the payment this fee stands for (same club, date and amount).
    /// `None` when no fee is recorded. `Some(false)` is a discrepancy the screen must show.
    pub fee_in_journal: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransfersReport {
    pub purchases: u32,
    pub sales: u32,
    pub loans_in: u32,
    pub loans_out: u32,
    pub free_signings: u32,
    /// Sum of the fees the ledgers record on purchases (euros).
    pub fees_paid: u64,
    /// Sum of the fees the ledgers record on sales (euros).
    pub fees_received: u64,
    pub moves: Listed<TransferLine>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractChangeKind {
    Renewal,
    /// The contract ran out and was not renewed.
    Expired,
    /// The club ended it.
    Terminated,
    /// The ledger records a release and no reason for it.
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractLine {
    pub date: String,
    pub kind: ContractChangeKind,
    pub player_id: String,
    pub player_name: String,
    pub weekly_wage_before: Option<u32>,
    pub weekly_wage_after: Option<u32>,
    pub end_before: Option<String>,
    pub end_after: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractsReport {
    pub renewed: u32,
    pub expired: u32,
    pub terminated: u32,
    pub released: u32,
    pub changes: Listed<ContractLine>,
}

/// One cash kind's total over the run (euros; income positive, expense negative).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CashKindTotal {
    pub kind: CashKind,
    pub income: i64,
    pub expense: i64,
    pub net: i64,
    pub posts: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FinancesReport {
    /// The club's cash on the first day, before it was played, and after the last.
    pub opening_cash: i64,
    pub closing_cash: i64,
    pub change: i64,
    /// By kind of payment, the largest movement first.
    pub by_kind: Vec<CashKindTotal>,
    /// What the journal paid for players (negative) and received for them (positive).
    pub transfer_fees_out: i64,
    pub transfer_fees_in: i64,
    pub posts: u32,
    /// The part of `change` the journal does not explain: `change` less the sum of the posts of
    /// the run. Zero when the books reconcile.
    pub unexplained: i64,
    /// False when the interval contains folded posts (one line standing for a month): the totals
    /// are still exact, the single payments are not recoverable.
    pub detail_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SquadMove {
    pub player_id: String,
    pub player_name: String,
    /// The movement the ledger records for him in the run, if it records one. `None` is "the
    /// ledger does not say": he is in the squad (or gone from it) and nothing explains why.
    pub cause: Option<PlayerMovementKind>,
    pub date: Option<String>,
    pub counterparty_name: Option<String>,
    pub fee: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InjuryLine {
    pub player_id: String,
    pub player_name: String,
    /// The injury as the player carries it (`Injury::name`).
    pub injury: String,
    pub days_remaining_when_seen: u32,
    /// The first day on which the player was injured at the end of the day.
    pub first_seen: String,
    /// The first day on which he was no longer injured; `None` while he still is.
    pub recovered_by: Option<String>,
    /// Whether he still belongs to the club when the stretch ends.
    pub still_at_club: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SquadReport {
    /// Players registered to the club on the first day and after the last.
    pub size_before: u32,
    pub size_after: u32,
    pub arrivals: Listed<SquadMove>,
    pub departures: Listed<SquadMove>,
    pub injuries: Listed<InjuryLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationLine {
    pub date: String,
    pub duty: Duty,
    pub title_key: String,
    pub params: Vec<(String, String)>,
    pub outcome: DecisionOutcome,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DelegationReport {
    /// Done by the assistant within what the manager allowed.
    pub executed: u32,
    pub approved: u32,
    /// Proposed and refused by the manager, or refused by the rules.
    pub declined: u32,
    /// Left to the manager: the assistant could not or would not act alone.
    pub left_to_manager: u32,
    pub entries: Listed<DelegationLine>,
}

/// A moment worth the manager's attention, with the id of the record it comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportEvent {
    /// The board dismissed the manager; the club's stretch ends the same day.
    Dismissed { date: String },
    /// A season finished and the run rolled it over.
    SeasonCompleted {
        date: String,
        season: u32,
        league_name: String,
        position: u32,
        total_teams: u32,
        points: u32,
        division_move: String,
    },
    /// The club won a knockout tie.
    KnockoutAdvanced {
        date: String,
        fixture_id: String,
        competition_id: String,
        competition_name: String,
    },
    /// The club lost a knockout tie and is out of the competition.
    KnockoutEliminated {
        date: String,
        fixture_id: String,
        competition_id: String,
        competition_name: String,
    },
    /// A board directive reached the inbox.
    BoardMessage {
        date: String,
        message_id: String,
        subject_key: Option<String>,
        subject: String,
    },
}

impl ReportEvent {
    pub(super) fn date(&self) -> &str {
        match self {
            Self::Dismissed { date }
            | Self::SeasonCompleted { date, .. }
            | Self::KnockoutAdvanced { date, .. }
            | Self::KnockoutEliminated { date, .. }
            | Self::BoardMessage { date, .. } => date,
        }
    }
}
