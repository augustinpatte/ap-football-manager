//! Who is in the squad after the run and was not before, and the reverse, with what the ledgers
//! say explains each change; and the injuries the run watched.

use std::collections::{BTreeMap, BTreeSet};

use crate::game::Game;

use super::movements::MoveCause;
use super::tracker::{InjurySeen, Stretch, StretchClose};
use super::types::{InjuryLine, Listed, SquadMove, SquadReport};

fn name_of(game: &Game, player_id: &str) -> String {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .map_or_else(|| player_id.to_string(), |player| player.match_name.clone())
}

fn squad_move(game: &Game, player_id: &str, causes: &BTreeMap<String, MoveCause>) -> SquadMove {
    let cause = causes.get(player_id);
    SquadMove {
        player_id: player_id.to_string(),
        player_name: name_of(game, player_id),
        cause: cause.map(|cause| cause.kind.clone()),
        date: cause.map(|cause| cause.date.clone()),
        counterparty_name: cause.and_then(|cause| cause.counterparty_name.clone()),
        fee: cause.and_then(|cause| cause.fee),
    }
}

fn moves(
    game: &Game,
    ids: impl Iterator<Item = String>,
    causes: &BTreeMap<String, MoveCause>,
) -> Listed<SquadMove> {
    let mut moves: Vec<SquadMove> = ids.map(|id| squad_move(game, &id, causes)).collect();
    moves.sort_by(|a, b| (&a.date, &a.player_id).cmp(&(&b.date, &b.player_id)));
    Listed::capped(moves)
}

fn injury_line(seen: &InjurySeen, squad_after: &BTreeSet<String>) -> InjuryLine {
    InjuryLine {
        player_id: seen.player_id.clone(),
        player_name: seen.player_name.clone(),
        injury: seen.injury.clone(),
        days_remaining_when_seen: seen.days_remaining,
        first_seen: seen.first_seen.clone(),
        recovered_by: seen.recovered_by.clone(),
        still_at_club: squad_after.contains(&seen.player_id),
    }
}

pub(super) fn squad_of(
    game: &Game,
    stretch: &Stretch,
    close: &StretchClose,
    causes: &BTreeMap<String, MoveCause>,
) -> SquadReport {
    let arrived = close.squad_after.difference(&stretch.squad_before).cloned();
    let left = stretch.squad_before.difference(&close.squad_after).cloned();
    let injuries = stretch
        .injuries
        .iter()
        .map(|seen| injury_line(seen, &close.squad_after))
        .collect();
    SquadReport {
        size_before: u32::try_from(stretch.squad_before.len()).unwrap_or(u32::MAX),
        size_after: u32::try_from(close.squad_after.len()).unwrap_or(u32::MAX),
        arrivals: moves(game, arrived, causes),
        departures: moves(game, left, causes),
        injuries: Listed::capped(injuries),
    }
}
