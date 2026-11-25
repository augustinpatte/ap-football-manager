//! The report a simulation hands the manager when it ends.
//!
//! A fast-forward plays days the manager did not click through; this module says what happened in
//! them. It owns no ledger and repeats no simulation: it reads the records the game already keeps
//! (fixtures, each player's movement ledger, the cash journal, the delegation log, the inbox) and
//! the few things only the run could see (which club the manager held on which days, and the
//! injuries seen while the run played, kept by [`ReportTracker`]).
//!
//! * **Only what happened.** Every line is a recorded event, or the difference between the squad
//!   before and after, with the ledger's own explanation attached when it has one and none when
//!   it has not. No fee is computed and no cause is inferred.
//! * **The days actually played.** A run that stops early reports the days it played, not the days
//!   it was asked for. A manager dismissed on the way, or moved to another club, gets one entry
//!   per club, each covering only its own days.
//! * **Bounded.** Each list is capped at [`MAX_LISTED`] with its true total beside it; the totals
//!   are exact however long the run was.
//! * **Cheap.** Building it reads the game once and the game is never cloned: a run costs the
//!   tracker's one pass over the club's players per day and one pass over the records at the end.

mod delegation;
mod events;
mod finances;
mod matches;
mod movements;
mod squad;
mod tracker;
mod types;

#[cfg(test)]
mod tests;

pub use tracker::ReportTracker;
pub use types::*;

use crate::game::Game;

/// How many items of a list a report carries. The totals beside them are exact.
const MAX_LISTED: usize = 400;

/// Closes the run's watch and reads the report out of the game as it stands after the last day.
pub fn build_report(game: &Game, mut tracker: ReportTracker) -> SimulationReport {
    tracker.close_open_stretch(game);
    let clubs = tracker
        .stretches
        .iter()
        .filter_map(|stretch| {
            let close = stretch.close.as_ref()?;
            let (matches, knockouts) = matches::matches_of(
                game,
                &stretch.club_id,
                &stretch.from,
                &close.until,
                &stretch.pre_run,
            );
            let movements = movements::movements_of(
                game,
                &stretch.club_id,
                &stretch.from,
                &close.until,
                &stretch.pre_run,
            );
            Some(ClubReport {
                club_id: stretch.club_id.clone(),
                club_name: stretch.club_name.clone(),
                from: stretch.from.clone(),
                until: close.until.clone(),
                days_played: stretch.days_played,
                ended: close.ended,
                matches,
                finances: finances::finances_of(
                    game,
                    &stretch.club_id,
                    &stretch.from,
                    &close.until,
                    stretch.opening_cash,
                    close.closing_cash,
                    &stretch.pre_run,
                ),
                squad: squad::squad_of(game, stretch, close, &movements.causes),
                delegation: delegation::delegation_of(
                    game,
                    &stretch.from,
                    &close.until,
                    &stretch.pre_run,
                ),
                events: events::events_of(game, stretch, &close.until, close.ended, knockouts),
                transfers: movements.transfers,
                contracts: movements.contracts,
            })
        })
        .collect();
    SimulationReport { clubs }
}
