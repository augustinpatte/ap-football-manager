//! Players moving in and out of the club, read from each player's own movement ledger.
//!
//! One pass over the players serves three sections: the transfers (who came and went, for what),
//! the contracts (renewals and releases, with the terms before and after) and the cause the
//! squad section attaches to an arrival or a departure.

use std::collections::BTreeMap;

use domain::contract_ledger::ReleaseReason;
use domain::finance::CashKind;
use domain::player::{Player, PlayerMovementEntry, PlayerMovementKind};

use crate::game::Game;

use super::finances::journal_has;
use super::tracker::PreRun;
use super::types::{
    ContractChangeKind, ContractLine, ContractsReport, Listed, TransferKind, TransferLine,
    TransfersReport,
};

/// What the ledger says explains a player's arrival or departure.
#[derive(Debug, Clone)]
pub(super) struct MoveCause {
    pub kind: PlayerMovementKind,
    pub date: String,
    pub counterparty_name: Option<String>,
    pub fee: Option<u64>,
}

pub(super) struct Movements {
    pub transfers: TransfersReport,
    pub contracts: ContractsReport,
    /// The latest movement of each player involving the club in the run, by player id.
    pub causes: BTreeMap<String, MoveCause>,
}

/// How a movement reads from the club's side, when it is a transfer of a player.
fn transfer_kind(entry: &PlayerMovementEntry, club_id: &str) -> Option<TransferKind> {
    let arrives = entry.to_team_id.as_deref() == Some(club_id);
    let leaves = entry.from_team_id.as_deref() == Some(club_id);
    match (&entry.kind, arrives, leaves) {
        (PlayerMovementKind::PermanentTransfer | PlayerMovementKind::LoanToBuy, true, false) => {
            Some(TransferKind::Purchase)
        }
        (PlayerMovementKind::PermanentTransfer | PlayerMovementKind::LoanToBuy, false, true) => {
            Some(TransferKind::Sale)
        }
        (PlayerMovementKind::LoanStart, true, false) => Some(TransferKind::LoanIn),
        (PlayerMovementKind::LoanStart, false, true) => Some(TransferKind::LoanOut),
        (PlayerMovementKind::LoanReturn, true, false) => Some(TransferKind::LoanReturned),
        (PlayerMovementKind::LoanReturn, false, true) => Some(TransferKind::LoanEnded),
        (PlayerMovementKind::FreeAgentSigning, true, _) => Some(TransferKind::FreeSigning),
        _ => None,
    }
}

/// The payment a recorded fee stands for, as the journal would hold it: kind and signed amount.
fn expected_post(kind: TransferKind, fee: u64) -> Option<(CashKind, i64)> {
    let amount = i64::try_from(fee).ok()?;
    match kind {
        TransferKind::Purchase => Some((CashKind::TransferFeeOut, -amount)),
        TransferKind::Sale => Some((CashKind::TransferFeeIn, amount)),
        TransferKind::LoanIn => Some((CashKind::LoanFee, -amount)),
        TransferKind::LoanOut => Some((CashKind::LoanFee, amount)),
        TransferKind::LoanReturned | TransferKind::LoanEnded | TransferKind::FreeSigning => None,
    }
}

fn transfer_line(
    game: &Game,
    club_id: &str,
    player: &Player,
    entry: &PlayerMovementEntry,
) -> Option<TransferLine> {
    let kind = transfer_kind(entry, club_id)?;
    let arrives = entry.to_team_id.as_deref() == Some(club_id);
    let (counterparty_id, counterparty_name) = if arrives {
        (&entry.from_team_id, &entry.from_team_name)
    } else {
        (&entry.to_team_id, &entry.to_team_name)
    };
    let fee_in_journal = entry
        .fee
        .filter(|fee| *fee > 0)
        .and_then(|fee| expected_post(kind, fee))
        .map(|(cash_kind, amount)| journal_has(game, club_id, cash_kind, &entry.date, amount));
    Some(TransferLine {
        date: entry.date.clone(),
        kind,
        player_id: player.id.clone(),
        player_name: player.match_name.clone(),
        fee: entry.fee,
        counterparty_id: counterparty_id.clone(),
        counterparty_name: counterparty_name.clone(),
        loan_end_date: entry.loan_end_date.clone(),
        via_purchase_option: entry.kind == PlayerMovementKind::LoanToBuy,
        fee_in_journal,
    })
}

/// The terms of the contract in force before the entry at `index`, from the same ledger.
fn contract_before(
    entries: &[PlayerMovementEntry],
    index: usize,
) -> Option<&domain::contract_ledger::ContractRecord> {
    entries[..index]
        .iter()
        .rev()
        .find_map(|entry| entry.contract.as_ref())
}

fn contract_line(
    player: &Player,
    entries: &[PlayerMovementEntry],
    index: usize,
) -> Option<ContractLine> {
    let entry = &entries[index];
    let kind = match (&entry.kind, entry.release_reason) {
        (PlayerMovementKind::Renewal, _) => ContractChangeKind::Renewal,
        (PlayerMovementKind::Released, Some(ReleaseReason::Expired)) => ContractChangeKind::Expired,
        (PlayerMovementKind::Released, Some(ReleaseReason::Terminated)) => {
            ContractChangeKind::Terminated
        }
        (PlayerMovementKind::Released, None) => ContractChangeKind::Released,
        _ => return None,
    };
    let before = contract_before(entries, index);
    let after = entry.contract.as_ref();
    Some(ContractLine {
        date: entry.date.clone(),
        kind,
        player_id: player.id.clone(),
        player_name: player.match_name.clone(),
        weekly_wage_before: before.map(|record| record.weekly_wage),
        weekly_wage_after: after.map(|record| record.weekly_wage),
        end_before: before.and_then(|record| record.end.clone()),
        end_after: after.and_then(|record| record.end.clone()),
    })
}

fn involves(entry: &PlayerMovementEntry, club_id: &str) -> bool {
    entry.from_team_id.as_deref() == Some(club_id) || entry.to_team_id.as_deref() == Some(club_id)
}

pub(super) fn movements_of(
    game: &Game,
    club_id: &str,
    from: &str,
    until: &str,
    pre_run: &PreRun,
) -> Movements {
    let mut moves = Vec::new();
    let mut changes = Vec::new();
    let mut causes = BTreeMap::new();
    for player in &game.players {
        let entries: &[PlayerMovementEntry] = &player.movement_history;
        let mut before_the_run = pre_run.ledger_entries.get(&player.id).copied().unwrap_or(0);
        for (index, entry) in entries.iter().enumerate() {
            if entry.date.as_str() >= until
                || entry.date.as_str() < from
                || !involves(entry, club_id)
            {
                continue;
            }
            // The first of the first day's entries were recorded before the run began.
            if entry.date.as_str() == from && before_the_run > 0 {
                before_the_run -= 1;
                continue;
            }
            if let Some(line) = transfer_line(game, club_id, player, entry) {
                moves.push(line);
            }
            if let Some(line) = contract_line(player, entries, index) {
                changes.push(line);
            }
            // The latest movement of the run explains where he is now.
            causes.insert(player.id.clone(), cause_of(game, club_id, entry));
        }
    }
    moves.sort_by(|a, b| (&a.date, &a.player_id).cmp(&(&b.date, &b.player_id)));
    changes.sort_by(|a, b| (&a.date, &a.player_id).cmp(&(&b.date, &b.player_id)));
    Movements {
        transfers: transfers_report(moves),
        contracts: contracts_report(changes),
        causes,
    }
}

fn cause_of(game: &Game, club_id: &str, entry: &PlayerMovementEntry) -> MoveCause {
    let arrives = entry.to_team_id.as_deref() == Some(club_id);
    let (name, id) = if arrives {
        (&entry.from_team_name, &entry.from_team_id)
    } else {
        (&entry.to_team_name, &entry.to_team_id)
    };
    MoveCause {
        kind: entry.kind.clone(),
        date: entry.date.clone(),
        counterparty_name: name
            .clone()
            .or_else(|| id.as_deref().map(|id| game.team_name_or_id(id))),
        fee: entry.fee,
    }
}

fn transfers_report(moves: Vec<TransferLine>) -> TransfersReport {
    let mut report = TransfersReport::default();
    for line in &moves {
        match line.kind {
            TransferKind::Purchase => {
                report.purchases += 1;
                report.fees_paid += line.fee.unwrap_or(0);
            }
            TransferKind::Sale => {
                report.sales += 1;
                report.fees_received += line.fee.unwrap_or(0);
            }
            TransferKind::LoanIn => report.loans_in += 1,
            TransferKind::LoanOut => report.loans_out += 1,
            TransferKind::FreeSigning => report.free_signings += 1,
            TransferKind::LoanReturned | TransferKind::LoanEnded => {}
        }
    }
    report.moves = Listed::capped(moves);
    report
}

fn contracts_report(changes: Vec<ContractLine>) -> ContractsReport {
    let mut report = ContractsReport::default();
    for line in &changes {
        match line.kind {
            ContractChangeKind::Renewal => report.renewed += 1,
            ContractChangeKind::Expired => report.expired += 1,
            ContractChangeKind::Terminated => report.terminated += 1,
            ContractChangeKind::Released => report.released += 1,
        }
    }
    report.changes = Listed::capped(changes);
    report
}
