//! One authoritative result per command a manager sends.
//!
//! A command that moves money or players (a loan offer, a bid, an answer to an offer) can be
//! delivered twice: a double click that outruns the disabled button, a retry after an answer was
//! lost on its way to the screen, a duplicated IPC message. Running it twice must not give two
//! outcomes, and the second must not be an error that contradicts the first success — the report
//! that started this module was a loan that was active under an error message.
//!
//! The screen names each user action with a request id. The first delivery runs under the game
//! lock and its outcome is recorded here, under the same lock; a repeat of the same id replays the
//! recorded outcome without touching the game, and [`StateManager::transaction_status`] answers
//! "did this one happen?" for a caller that never received the answer.
//!
//! Outcomes live in memory only. A save does not need them: what a command did is in the game
//! (the offer rows, the registration dates), and a request id is only meaningful for the few
//! seconds in which a screen might repeat it.

use crate::game::Game;
use crate::state::StateManager;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, VecDeque};

/// How many recent request ids are remembered. Far more than a person can send in the seconds a
/// repeat is plausible, and small enough that the ledger costs nothing.
const REMEMBERED_REQUESTS: usize = 256;

#[derive(Debug, Clone)]
enum Recorded {
    Committed(serde_json::Value),
    Refused(String),
}

/// What is known of a request id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
pub enum TransactionStatus {
    /// Never seen: it was not delivered, or it carried no id. Nothing it asked for happened.
    Unknown,
    /// It ran and changed the game.
    Committed,
    /// It ran and was refused; the game is as it was. Carries the refusal key.
    Refused(String),
}

/// The recent outcomes, oldest first, bounded by [`REMEMBERED_REQUESTS`].
#[derive(Debug, Default)]
pub(crate) struct TransactionLedger {
    order: VecDeque<String>,
    outcomes: HashMap<String, Recorded>,
}

impl TransactionLedger {
    fn recall(&self, request_id: &str) -> Option<&Recorded> {
        self.outcomes.get(request_id)
    }

    fn record(&mut self, request_id: &str, outcome: Recorded) {
        if self
            .outcomes
            .insert(request_id.to_string(), outcome)
            .is_none()
        {
            self.order.push_back(request_id.to_string());
        }
        while self.order.len() > REMEMBERED_REQUESTS {
            if let Some(oldest) = self.order.pop_front() {
                self.outcomes.remove(&oldest);
            }
        }
    }

    pub(crate) fn status(&self, request_id: &str) -> TransactionStatus {
        match self.recall(request_id) {
            None => TransactionStatus::Unknown,
            Some(Recorded::Committed(_)) => TransactionStatus::Committed,
            Some(Recorded::Refused(error)) => TransactionStatus::Refused(error.clone()),
        }
    }
}

/// Runs `change` on a scratch copy of the game and keeps the copy only if it succeeds, so a
/// refusal discovered after the change has started leaves nothing behind. For operations whose
/// own validation cannot all run before their first write.
fn all_or_nothing<T>(
    game: &mut Game,
    change: impl FnOnce(&mut Game) -> Result<T, String>,
) -> Result<T, String> {
    let mut scratch = game.clone();
    let value = change(&mut scratch)?;
    *game = scratch;
    Ok(value)
}

impl StateManager {
    /// Applies `change` to the active game as one transaction: read, change and commit happen under
    /// one hold of the game lock, a refusal at any point leaves the game untouched, and a repeat of
    /// `request_id` replays the first outcome instead of running again. Without an id (an agent
    /// calling the tool directly) it is the same atomic change with no replay.
    ///
    /// `None` when there is no active game.
    pub fn run_transaction<T>(
        &self,
        request_id: Option<&str>,
        change: impl FnOnce(&mut Game) -> Result<T, String>,
    ) -> Option<Result<T, String>>
    where
        T: Serialize + DeserializeOwned,
    {
        self.update_game(|game| {
            let Some(request_id) = request_id else {
                return all_or_nothing(game, change);
            };
            if let Some(recorded) = self.with_ledger(|ledger| ledger.recall(request_id).cloned()) {
                return replay(recorded);
            }
            let result = all_or_nothing(game, change);
            let recorded = match &result {
                Ok(value) => serde_json::to_value(value).map(Recorded::Committed).ok(),
                Err(error) => Some(Recorded::Refused(error.clone())),
            };
            if let Some(recorded) = recorded {
                self.with_ledger(|ledger| ledger.record(request_id, recorded));
            }
            result
        })
    }

    /// What became of a request, for a screen that did not receive the answer.
    pub fn transaction_status(&self, request_id: &str) -> TransactionStatus {
        self.with_ledger(|ledger| ledger.status(request_id))
    }
}

fn replay<T: DeserializeOwned>(recorded: Recorded) -> Result<T, String> {
    match recorded {
        Recorded::Refused(error) => Err(error),
        Recorded::Committed(value) => serde_json::from_value(value)
            .map_err(|_| "be.error.transactions.outcomeUnreadable".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn game() -> Game {
        let clock =
            crate::clock::GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        Game::new(clock, manager, vec![], vec![], vec![], vec![])
    }

    fn state() -> StateManager {
        let state = StateManager::new();
        state.set_game(game());
        state
    }

    fn messages(state: &StateManager) -> usize {
        state.get_game(|game| game.messages.len()).unwrap()
    }

    fn post_one_message(game: &mut Game) -> Result<u32, String> {
        game.messages.push(domain::message::InboxMessage::new(
            format!("m{}", game.messages.len()),
            "subject".to_string(),
            "body".to_string(),
            "sender".to_string(),
            "2026-08-01".to_string(),
        ));
        Ok(7)
    }

    /// Given a request id that has already run
    /// When the same id is delivered again
    /// Then the first outcome is replayed and the change does not run a second time.
    #[test]
    fn a_repeated_request_id_replays_the_first_outcome_without_running_again() {
        let state = state();
        let runs = AtomicUsize::new(0);
        let first = state
            .run_transaction(Some("req-1"), |game| {
                runs.fetch_add(1, Ordering::SeqCst);
                post_one_message(game)
            })
            .unwrap();
        let second = state
            .run_transaction(Some("req-1"), |game| {
                runs.fetch_add(1, Ordering::SeqCst);
                post_one_message(game)
            })
            .unwrap();

        assert_eq!((first, second), (Ok(7), Ok(7)));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(messages(&state), 1);
        assert_eq!(
            state.transaction_status("req-1"),
            TransactionStatus::Committed
        );
    }

    /// Given a request that was refused
    /// When the same id is delivered again, even though the game would now allow it
    /// Then the refusal is replayed: one request has one answer.
    #[test]
    fn a_refused_request_keeps_its_refusal_when_repeated() {
        let state = state();
        let refused = state
            .run_transaction::<u32>(Some("req-2"), |_| Err("be.error.nope".to_string()))
            .unwrap();
        let repeated = state
            .run_transaction(Some("req-2"), post_one_message)
            .unwrap();

        assert_eq!(refused, Err("be.error.nope".to_string()));
        assert_eq!(repeated, Err("be.error.nope".to_string()));
        assert_eq!(messages(&state), 0);
        assert_eq!(
            state.transaction_status("req-2"),
            TransactionStatus::Refused("be.error.nope".to_string())
        );
    }

    /// Given a change that writes and then fails
    /// When it runs
    /// Then nothing it wrote is kept.
    #[test]
    fn a_change_that_fails_after_writing_leaves_the_game_untouched() {
        let state = state();
        let result = state
            .run_transaction::<u32>(None, |game| {
                post_one_message(game)?;
                Err("be.error.late".to_string())
            })
            .unwrap();

        assert_eq!(result, Err("be.error.late".to_string()));
        assert_eq!(messages(&state), 0);
    }

    /// Given the same id delivered by many threads at once
    /// When they race
    /// Then exactly one runs and every thread gets the same answer.
    #[test]
    fn concurrent_deliveries_of_one_request_id_run_once() {
        let state = Arc::new(state());
        let runs = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let (state, runs) = (Arc::clone(&state), Arc::clone(&runs));
                std::thread::spawn(move || {
                    state
                        .run_transaction(Some("req-3"), |game| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            post_one_message(game)
                        })
                        .unwrap()
                })
            })
            .collect();
        let answers: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert!(answers.iter().all(|answer| *answer == Ok(7)));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(messages(&state), 1);
    }

    /// Given a transaction running and another command changing the game at the same time
    /// When both finish
    /// Then both changes are in the game: a transaction does not overwrite what others commit.
    #[test]
    fn a_transaction_does_not_lose_a_concurrent_change() {
        let state = Arc::new(state());
        let writer = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                for _ in 0..300 {
                    state.update_game(|game| {
                        post_one_message(game).ok();
                    });
                }
            })
        };
        for index in 0..300 {
            let id = format!("req-stress-{index}");
            state
                .run_transaction(Some(&id), post_one_message)
                .unwrap()
                .unwrap();
        }
        writer.join().unwrap();

        assert_eq!(messages(&state), 600);
    }

    /// Given an id nobody has sent
    /// When its status is asked
    /// Then it is unknown, which means nothing it asked for happened.
    #[test]
    fn an_unseen_request_id_is_unknown() {
        assert_eq!(
            state().transaction_status("never"),
            TransactionStatus::Unknown
        );
    }

    /// Given more requests than the ledger remembers
    /// When the oldest is asked about
    /// Then it has been forgotten, and the newest has not.
    #[test]
    fn the_ledger_forgets_the_oldest_requests() {
        let state = state();
        for index in 0..(REMEMBERED_REQUESTS + 5) {
            let id = format!("req-{index}");
            state
                .run_transaction(Some(&id), |_| Ok::<u32, String>(1))
                .unwrap()
                .unwrap();
        }

        assert_eq!(
            state.transaction_status("req-0"),
            TransactionStatus::Unknown
        );
        assert_eq!(
            state.transaction_status(&format!("req-{}", REMEMBERED_REQUESTS + 4)),
            TransactionStatus::Committed
        );
    }

    /// Given a career that is closed
    /// When the same id arrives in the next career
    /// Then it is a new request.
    #[test]
    fn clearing_the_game_forgets_request_ids() {
        let state = state();
        state
            .run_transaction(Some("req-4"), post_one_message)
            .unwrap()
            .unwrap();
        state.clear_game();
        state.set_game(game());

        assert_eq!(
            state.transaction_status("req-4"),
            TransactionStatus::Unknown
        );
    }

    /// Given no active game
    /// When a transaction is attempted
    /// Then it reports there is no game rather than running.
    #[test]
    fn a_transaction_without_a_game_reports_none() {
        let state = StateManager::new();
        assert!(
            state
                .run_transaction(Some("x"), |_| Ok::<u32, String>(1))
                .is_none()
        );
    }
}
