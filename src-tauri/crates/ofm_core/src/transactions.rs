//! One authoritative result per command a manager sends.
//!
//! A command that moves money or players (a loan offer, a bid, an answer to an offer) can be
//! delivered twice: a double click that outruns the disabled button, a retry after an answer was
//! lost on its way to the screen, a duplicated IPC message. Running it twice must not give two
//! outcomes, and the second must not be an error that contradicts the first success — the report
//! that started this module was a loan that was active under an error message.
//!
//! The screen names each user action with a request id. The ledger knows a request from the moment
//! its command arrives (`InProgress`), not only once it has finished: a screen that lost the answer
//! and asks while the command is still queued behind the game lock or still running must be told
//! "not finished", never "never happened". The first delivery runs under the game lock and its
//! outcome is recorded under the same lock; a repeat of the id replays the recorded outcome
//! without touching the game.
//!
//! A request id that was never seen cannot be assumed never to arrive: a delayed delivery may still
//! be on its way. [`StateManager::settle_transaction`] therefore closes an unseen id (`Abandoned`):
//! afterwards a late delivery is refused, so "it did not run" is a fact and not a guess, and the
//! screen may retry with a new id.
//!
//! A request id also belongs to one career and one operation. Every command carries the career token
//! of the game the screen was looking at; a command for a career that is no longer installed is
//! refused before it writes, and the ledger is emptied when the installed career changes.
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
/// repeat is plausible, and small enough that the ledger costs nothing. A request still in progress
/// is never forgotten to make room.
const REMEMBERED_REQUESTS: usize = 256;

const ERR_CAREER_CHANGED: &str = "be.error.transactions.careerChanged";
const ERR_REQUEST_ABANDONED: &str = "be.error.transactions.requestAbandoned";
const ERR_REQUEST_ID_REUSED: &str = "be.error.transactions.requestIdReused";
const ERR_OUTCOME_UNREADABLE: &str = "be.error.transactions.outcomeUnreadable";

/// A command's request id and the career the screen believed it was acting in.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransactionRequest<'a> {
    /// `None` for a caller that sends no id (an agent calling a tool directly): atomic, no replay.
    pub id: Option<&'a str>,
    /// `Game::career_token` the screen was given. `None` skips the check (agents, tests).
    pub career_token: Option<u64>,
}

impl<'a> TransactionRequest<'a> {
    pub fn new(id: Option<&'a str>, career_token: Option<u64>) -> Self {
        Self { id, career_token }
    }

    /// The request a screen sent with its command, or none for a caller that sent no stamp.
    pub fn from_stamp(stamp: Option<&'a RequestStamp>) -> Self {
        stamp.map_or_else(Self::default, |stamp| {
            Self::new(Some(&stamp.id), stamp.career_token)
        })
    }
}

/// What a screen sends with a command: the id of the user action and the career it was looking at.
/// One object so that a command carries both or neither.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStamp {
    pub id: String,
    /// `Game::career_token` of the game the screen holds.
    #[serde(default)]
    pub career_token: Option<u64>,
}

impl<'a> From<Option<&'a str>> for TransactionRequest<'a> {
    fn from(id: Option<&'a str>) -> Self {
        Self::new(id, None)
    }
}

#[derive(Debug, Clone)]
enum Entry {
    /// The command has arrived and has not finished (queued for the game lock, or running).
    InProgress,
    Committed(serde_json::Value),
    Refused(String),
    /// Closed by [`StateManager::settle_transaction`] before it ever ran; a late delivery is refused.
    Abandoned,
}

#[derive(Debug, Clone)]
struct Slot {
    operation: String,
    entry: Entry,
}

/// What is known of a request id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
pub enum TransactionStatus {
    /// Never seen. It may still be delivered; only `settle_transaction` makes this final.
    Unknown,
    /// It has arrived and has not finished. Ask again; do not retry and do not report failure.
    InProgress,
    /// It ran and changed the game.
    Committed,
    /// It ran and was refused; the game is as it was. Carries the refusal key.
    Refused(String),
    /// It never ran and now never will under this id: a retry must use a new id.
    Abandoned,
    /// The career the screen asks about is not the installed one. Nothing can be said about the
    /// request, and it can no longer change the current career.
    CareerChanged,
}

/// The recent requests, oldest first, bounded by [`REMEMBERED_REQUESTS`].
#[derive(Debug, Default)]
pub(crate) struct TransactionLedger {
    /// `Game::career_token` of the installed career; 0 when none.
    career: u64,
    order: VecDeque<String>,
    slots: HashMap<String, Slot>,
}

impl TransactionLedger {
    /// A ledger for a newly installed career: nothing from the previous one survives.
    pub(crate) fn for_career(career: u64) -> Self {
        Self {
            career,
            ..Self::default()
        }
    }

    fn insert(&mut self, request_id: &str, operation: &str, entry: Entry) {
        let slot = Slot {
            operation: operation.to_string(),
            entry,
        };
        if self.slots.insert(request_id.to_string(), slot).is_none() {
            self.order.push_back(request_id.to_string());
        }
        self.make_room();
    }

    fn make_room(&mut self) {
        while self.order.len() > REMEMBERED_REQUESTS {
            let Some(position) = self
                .order
                .iter()
                .position(|id| !matches!(self.slots[id].entry, Entry::InProgress))
            else {
                return;
            };
            if let Some(oldest) = self.order.remove(position) {
                self.slots.remove(&oldest);
            }
        }
    }

    /// Notes that a command has arrived. Refuses an id that was closed, or that names another operation.
    fn begin(&mut self, request_id: &str, operation: &str) -> Result<(), String> {
        if self.slots.contains_key(request_id) {
            return self.finished(request_id, operation).map(|_| ());
        }
        self.insert(request_id, operation, Entry::InProgress);
        Ok(())
    }

    /// The recorded outcome to replay, if the request has finished.
    fn finished(&self, request_id: &str, operation: &str) -> Result<Option<Entry>, String> {
        match self.slots.get(request_id) {
            Some(Slot {
                entry: Entry::Abandoned,
                ..
            }) => Err(ERR_REQUEST_ABANDONED.to_string()),
            Some(slot) if slot.operation != operation => Err(ERR_REQUEST_ID_REUSED.to_string()),
            Some(Slot {
                entry: entry @ (Entry::Committed(_) | Entry::Refused(_)),
                ..
            }) => Ok(Some(entry.clone())),
            Some(Slot {
                entry: Entry::InProgress,
                ..
            })
            | None => Ok(None),
        }
    }

    /// Takes back a mark whose command never finished (no game, a panic, a wrong career).
    fn withdraw_if_in_progress(&mut self, request_id: &str) {
        if matches!(
            self.slots.get(request_id).map(|slot| &slot.entry),
            Some(Entry::InProgress)
        ) {
            self.slots.remove(request_id);
            self.order.retain(|id| id != request_id);
        }
    }

    fn status_in(&self, career_token: Option<u64>, request_id: &str) -> TransactionStatus {
        if career_token.is_some_and(|token| token != self.career) {
            return TransactionStatus::CareerChanged;
        }
        match self.slots.get(request_id).map(|slot| &slot.entry) {
            None => TransactionStatus::Unknown,
            Some(Entry::InProgress) => TransactionStatus::InProgress,
            Some(Entry::Committed(_)) => TransactionStatus::Committed,
            Some(Entry::Refused(error)) => TransactionStatus::Refused(error.clone()),
            Some(Entry::Abandoned) => TransactionStatus::Abandoned,
        }
    }

    fn settle(&mut self, career_token: Option<u64>, request_id: &str) -> TransactionStatus {
        let status = self.status_in(career_token, request_id);
        if status == TransactionStatus::Unknown {
            self.insert(request_id, "", Entry::Abandoned);
            return TransactionStatus::Abandoned;
        }
        status
    }
}

/// Takes back the in-progress mark of a command that leaves without an outcome, however it leaves.
struct PendingMark<'a> {
    state: &'a StateManager,
    request_id: &'a str,
}

impl Drop for PendingMark<'_> {
    fn drop(&mut self) {
        self.state
            .with_ledger(|ledger| ledger.withdraw_if_in_progress(self.request_id));
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

/// What a transaction answered.
#[derive(Debug)]
pub struct TransactionReceipt<T> {
    pub value: T,
    /// The game as the screen shows it, taken under the same hold of the game lock as the commit, so
    /// no other command's change can be in it and none of this one's can be missing. `None` when the
    /// answer is a replay: the game has moved on and no snapshot of that moment was kept.
    pub snapshot: Option<Game>,
}

impl StateManager {
    /// Applies `change` to the active game as one transaction: read, change and commit happen under
    /// one hold of the game lock, a refusal at any point leaves the game untouched, and a repeat of
    /// the request id replays the first outcome instead of running again. Without an id (an agent
    /// calling the tool directly) it is the same atomic change with no replay.
    ///
    /// `operation` names the command: an id may not be reused for another one. A request for a
    /// career other than the installed one is refused before anything is written.
    ///
    /// `None` when there is no active game.
    pub fn run_transaction<'a, T>(
        &self,
        request: impl Into<TransactionRequest<'a>>,
        operation: &str,
        change: impl FnOnce(&mut Game) -> Result<T, String>,
    ) -> Option<Result<TransactionReceipt<T>, String>>
    where
        T: Serialize + DeserializeOwned,
    {
        let request = request.into();
        // The mark goes up before the wait for the game lock: a command queued behind a long one is
        // already "in progress" to anyone asking about it.
        let _pending = match request.id {
            Some(id) => {
                if let Err(refusal) = self.with_ledger(|ledger| ledger.begin(id, operation)) {
                    return Some(Err(refusal));
                }
                Some(PendingMark {
                    state: self,
                    request_id: id,
                })
            }
            None => None,
        };
        // A refusal or a replay changes nothing, so it must not advance the revision either.
        let committed = |answer: &Result<TransactionReceipt<T>, String>| matches!(answer, Ok(receipt) if receipt.snapshot.is_some());
        self.update_game_when(
            |game| {
                if request
                    .career_token
                    .is_some_and(|token| token != game.career_token)
                {
                    return Err(ERR_CAREER_CHANGED.to_string());
                }
                let Some(request_id) = request.id else {
                    let value = all_or_nothing(game, change)?;
                    return Ok(TransactionReceipt {
                        value,
                        snapshot: Some(game.ui_clone()),
                    });
                };
                match self.with_ledger(|ledger| ledger.finished(request_id, operation))? {
                    Some(finished) => replay(finished).map(|value| TransactionReceipt {
                        value,
                        snapshot: None,
                    }),
                    None => {
                        let result = all_or_nothing(game, change);
                        self.with_ledger(|ledger| {
                            ledger.insert(request_id, operation, recorded(&result))
                        });
                        result.map(|value| TransactionReceipt {
                            value,
                            snapshot: Some(game.ui_clone()),
                        })
                    }
                }
            },
            committed,
        )
    }

    /// What became of a request, for a screen that did not receive the answer. Read-only.
    pub fn transaction_status(
        &self,
        request_id: &str,
        career_token: Option<u64>,
    ) -> TransactionStatus {
        self.with_ledger(|ledger| ledger.status_in(career_token, request_id))
    }

    /// Like [`Self::transaction_status`], but an id never seen is closed: the answer `Abandoned`
    /// means it did not run and a late delivery of it will be refused. Every other answer is what
    /// `transaction_status` would say, and changes nothing.
    pub fn settle_transaction(
        &self,
        request_id: &str,
        career_token: Option<u64>,
    ) -> TransactionStatus {
        self.with_ledger(|ledger| ledger.settle(career_token, request_id))
    }
}

/// What to record for a finished command. An outcome that cannot be written as JSON still counts as
/// a commit (the game did change); a repeat then reports it unreadable instead of running again.
fn recorded<T: Serialize>(result: &Result<T, String>) -> Entry {
    match result {
        Ok(value) => {
            Entry::Committed(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
        }
        Err(error) => Entry::Refused(error.clone()),
    }
}

fn replay<T: DeserializeOwned>(finished: Entry) -> Result<T, String> {
    match finished {
        Entry::Refused(error) => Err(error),
        Entry::Committed(value) => {
            serde_json::from_value(value).map_err(|_| ERR_OUTCOME_UNREADABLE.to_string())
        }
        Entry::InProgress | Entry::Abandoned => Err(ERR_OUTCOME_UNREADABLE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    const OP: &str = "post_message";

    fn game_of_career(career_token: u64) -> Game {
        let clock =
            crate::clock::GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        let mut game = Game::new(clock, manager, vec![], vec![], vec![], vec![]);
        game.career_token = career_token;
        game
    }

    fn state_in_career(career_token: u64) -> StateManager {
        let state = StateManager::new();
        state.set_game(game_of_career(career_token));
        state
    }

    fn state() -> StateManager {
        state_in_career(0)
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

    fn value<T>(result: Option<Result<TransactionReceipt<T>, String>>) -> Result<T, String> {
        result.unwrap().map(|receipt| receipt.value)
    }

    fn run(state: &StateManager, id: &str) -> Result<u32, String> {
        value(state.run_transaction(Some(id), OP, post_one_message))
    }

    /// Given a request id that has already run
    /// When the same id is delivered again
    /// Then the first outcome is replayed and the change does not run a second time.
    #[test]
    fn a_repeated_request_id_replays_the_first_outcome_without_running_again() {
        let state = state();
        let runs = AtomicUsize::new(0);
        let deliver = || {
            value(state.run_transaction(Some("req-1"), OP, |game| {
                runs.fetch_add(1, Ordering::SeqCst);
                post_one_message(game)
            }))
        };
        let (first, second) = (deliver(), deliver());

        assert_eq!((first, second), (Ok(7), Ok(7)));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(messages(&state), 1);
        assert_eq!(
            state.transaction_status("req-1", None),
            TransactionStatus::Committed
        );
    }

    /// Given a request that was refused
    /// When the same id is delivered again, even though the game would now allow it
    /// Then the refusal is replayed: one request has one answer.
    #[test]
    fn a_refused_request_keeps_its_refusal_when_repeated() {
        let state = state();
        let refused = value(
            state.run_transaction::<u32>(Some("req-2"), OP, |_| Err("be.error.nope".to_string())),
        );
        let repeated = run(&state, "req-2");

        assert_eq!(refused, Err("be.error.nope".to_string()));
        assert_eq!(repeated, Err("be.error.nope".to_string()));
        assert_eq!(messages(&state), 0);
        assert_eq!(
            state.transaction_status("req-2", None),
            TransactionStatus::Refused("be.error.nope".to_string())
        );
    }

    /// Given a change that writes and then fails
    /// When it runs
    /// Then nothing it wrote is kept.
    #[test]
    fn a_change_that_fails_after_writing_leaves_the_game_untouched() {
        let state = state();
        let result = value(state.run_transaction::<u32>(None, OP, |game| {
            post_one_message(game)?;
            Err("be.error.late".to_string())
        }));

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
                    value(state.run_transaction(Some("req-3"), OP, |game| {
                        runs.fetch_add(1, Ordering::SeqCst);
                        post_one_message(game)
                    }))
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
            run(&state, &format!("req-stress-{index}")).unwrap();
        }
        writer.join().unwrap();

        assert_eq!(messages(&state), 600);
    }

    /// Given an id nobody has sent
    /// When its status is asked
    /// Then it is unknown, and asking changes nothing: the id can still run afterwards.
    #[test]
    fn an_unseen_request_id_is_unknown_and_asking_does_not_close_it() {
        let state = state();
        assert_eq!(
            state.transaction_status("never", None),
            TransactionStatus::Unknown
        );
        assert_eq!(run(&state, "never"), Ok(7));
    }

    /// Given more requests than the ledger remembers
    /// When the oldest is asked about
    /// Then it has been forgotten, and the newest has not.
    #[test]
    fn the_ledger_forgets_the_oldest_requests() {
        let state = state();
        for index in 0..(REMEMBERED_REQUESTS + 5) {
            run(&state, &format!("req-{index}")).unwrap();
        }

        assert_eq!(
            state.transaction_status("req-0", None),
            TransactionStatus::Unknown
        );
        assert_eq!(
            state.transaction_status(&format!("req-{}", REMEMBERED_REQUESTS + 4), None),
            TransactionStatus::Committed
        );
    }

    /// Given a career that is closed
    /// When the same id arrives in the next career
    /// Then it is a new request.
    #[test]
    fn clearing_the_game_forgets_request_ids() {
        let state = state();
        run(&state, "req-4").unwrap();
        state.clear_game();
        state.set_game(game_of_career(0));

        assert_eq!(
            state.transaction_status("req-4", None),
            TransactionStatus::Unknown
        );
    }

    /// Given a committed outcome that cannot be read back as the type a repeat asks for
    /// When the same id is delivered again
    /// Then the repeat says the earlier result is unreadable (it did happen: nothing runs again)
    #[test]
    fn a_committed_outcome_that_cannot_be_read_back_is_reported_not_rerun() {
        let state = state();
        let runs = AtomicUsize::new(0);
        value(state.run_transaction(Some("req-5"), OP, |game| {
            runs.fetch_add(1, Ordering::SeqCst);
            post_one_message(game)
        }))
        .unwrap();

        // Asked for as a different type: the recorded number is not a string.
        let repeat = value(state.run_transaction::<String>(Some("req-5"), OP, |_| {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }));

        assert_eq!(repeat, Err(ERR_OUTCOME_UNREADABLE.to_string()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(messages(&state), 1);
    }

    /// Given no active game
    /// When a transaction is attempted
    /// Then it reports there is no game rather than running, and leaves no mark behind.
    #[test]
    fn a_transaction_without_a_game_reports_none_and_leaves_no_mark() {
        let state = StateManager::new();
        assert!(
            state
                .run_transaction(Some("x"), OP, |_| Ok::<u32, String>(1))
                .is_none()
        );
        assert_eq!(
            state.transaction_status("x", None),
            TransactionStatus::Unknown
        );
    }

    /// Runs `body` while a transaction with `id` is executing: `body` is called from inside the
    /// change, with the game lock held, so its own status calls see exactly what a screen sees
    /// while the backend is busy.
    fn status_while_running(state: &StateManager, id: &str) -> TransactionStatus {
        let seen = std::sync::Mutex::new(None);
        value(state.run_transaction(Some(id), OP, |game| {
            *seen.lock().unwrap() = Some(state.transaction_status(id, None));
            post_one_message(game)
        }))
        .unwrap();
        seen.into_inner().unwrap().unwrap()
    }

    /// Given a transport failure while the transaction is still executing
    /// When the screen asks for the request's status before the backend has committed
    /// Then the answer is "in progress", not "unknown", and the transaction then commits.
    #[test]
    fn a_status_asked_while_the_request_runs_says_in_progress_and_it_then_commits() {
        let state = state();

        assert_eq!(
            status_while_running(&state, "req-slow"),
            TransactionStatus::InProgress
        );
        assert_eq!(
            state.transaction_status("req-slow", None),
            TransactionStatus::Committed
        );
        assert_eq!(messages(&state), 1);
    }

    /// Given a command queued behind a long-running one (it has arrived but has not got the game lock)
    /// When the screen asks for its status from another thread
    /// Then the answer is "in progress", although nothing of it has run yet.
    #[test]
    fn a_request_waiting_for_the_game_lock_is_already_in_progress() {
        let state = Arc::new(state());
        let (holding_tx, holding_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let blocker = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                state.update_game(|_| {
                    holding_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                });
            })
        };
        holding_rx.recv().unwrap();
        let queued = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || run(&state, "req-queued"))
        };
        // The queued command marks itself before it blocks on the game lock.
        let mut status = TransactionStatus::Unknown;
        for _ in 0..500 {
            status = state.transaction_status("req-queued", None);
            if status != TransactionStatus::Unknown {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        release_tx.send(()).unwrap();
        blocker.join().unwrap();
        let answer = queued.join().unwrap();

        assert_eq!(status, TransactionStatus::InProgress);
        assert_eq!(answer, Ok(7));
        assert_eq!(
            state.transaction_status("req-queued", None),
            TransactionStatus::Committed
        );
    }

    /// Given a repeat of an id that is still running (the screen retried with the same id)
    /// When both deliveries finish
    /// Then the change ran once and both got the same answer.
    #[test]
    fn a_retry_that_overlaps_the_original_run_does_not_run_twice() {
        let state = Arc::new(state());
        let runs = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let deliver = |state: Arc<StateManager>, runs: Arc<AtomicUsize>, barrier: Arc<Barrier>| {
            std::thread::spawn(move || {
                barrier.wait();
                value(state.run_transaction(Some("req-overlap"), OP, |game| {
                    runs.fetch_add(1, Ordering::SeqCst);
                    post_one_message(game)
                }))
            })
        };
        let first = deliver(state.clone(), runs.clone(), barrier.clone());
        let second = deliver(state.clone(), runs.clone(), barrier.clone());

        assert_eq!(first.join().unwrap(), Ok(7));
        assert_eq!(second.join().unwrap(), Ok(7));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    /// Given a transport failure and a status check that finds the id unseen
    /// When the screen settles the id and the command is then delivered late
    /// Then the late delivery is refused and changes nothing: "it did not run" stays true.
    #[test]
    fn a_settled_unseen_id_refuses_a_late_delivery() {
        let state = state();

        assert_eq!(
            state.settle_transaction("req-late", None),
            TransactionStatus::Abandoned
        );
        let late = run(&state, "req-late");

        assert_eq!(late, Err(ERR_REQUEST_ABANDONED.to_string()));
        assert_eq!(messages(&state), 0);
        assert_eq!(
            state.transaction_status("req-late", None),
            TransactionStatus::Abandoned
        );
    }

    /// Given a request that has finished or is running
    /// When it is settled
    /// Then settling reports what is known and closes nothing.
    #[test]
    fn settling_a_known_request_reports_it_and_does_not_close_it() {
        let state = state();
        run(&state, "req-done").unwrap();

        assert_eq!(
            state.settle_transaction("req-done", None),
            TransactionStatus::Committed
        );
        assert_eq!(run(&state, "req-done"), Ok(7));
        assert_eq!(messages(&state), 1);
        let during = std::sync::Mutex::new(None);
        value(state.run_transaction(Some("req-busy"), OP, |game| {
            *during.lock().unwrap() = Some(state.settle_transaction("req-busy", None));
            post_one_message(game)
        }))
        .unwrap();
        assert_eq!(
            during.into_inner().unwrap(),
            Some(TransactionStatus::InProgress)
        );
        assert_eq!(
            state.transaction_status("req-busy", None),
            TransactionStatus::Committed
        );
    }

    /// Given an id used for one operation
    /// When another operation sends the same id
    /// Then it is refused and the first outcome is untouched.
    #[test]
    fn a_request_id_cannot_be_reused_for_another_operation() {
        let state = state();
        run(&state, "req-op").unwrap();
        let other = value(
            state.run_transaction(Some("req-op"), "other_operation", |_| Ok::<u32, String>(99)),
        );

        assert_eq!(other, Err(ERR_REQUEST_ID_REUSED.to_string()));
        assert_eq!(run(&state, "req-op"), Ok(7));
        assert_eq!(messages(&state), 1);
    }

    /// Given a screen that holds the token of a career that has since been replaced
    /// When it sends a command
    /// Then the command is refused before it writes anything into the new career.
    #[test]
    fn a_command_for_another_career_is_refused_and_writes_nothing() {
        let state = state_in_career(1);
        state.set_game(game_of_career(2));

        let result = value(state.run_transaction(
            TransactionRequest::new(Some("req-old"), Some(1)),
            OP,
            post_one_message,
        ));

        assert_eq!(result, Err(ERR_CAREER_CHANGED.to_string()));
        assert_eq!(messages(&state), 0);
        assert_eq!(
            state.transaction_status("req-old", Some(2)),
            TransactionStatus::Unknown,
            "a refused stale command leaves no mark in the new career"
        );
    }

    /// Given a request that committed in career 1
    /// When career 2 is installed and the screen asks about that id with career 1's token
    /// Then it is told the career changed, never "committed" or "unknown"; and with career 2's
    /// token the same id is unknown.
    #[test]
    fn a_status_from_an_earlier_career_never_confirms_anything_in_the_current_one() {
        let state = state_in_career(1);
        value(state.run_transaction(
            TransactionRequest::new(Some("req-career"), Some(1)),
            OP,
            post_one_message,
        ))
        .unwrap();
        assert_eq!(
            state.transaction_status("req-career", Some(1)),
            TransactionStatus::Committed
        );

        state.set_game(game_of_career(2));

        assert_eq!(
            state.transaction_status("req-career", Some(1)),
            TransactionStatus::CareerChanged
        );
        assert_eq!(
            state.settle_transaction("req-career", Some(1)),
            TransactionStatus::CareerChanged
        );
        assert_eq!(
            state.transaction_status("req-career", Some(2)),
            TransactionStatus::Unknown
        );
    }

    /// Given the same career re-saved as a newer state of itself
    /// When the game is replaced with the same token
    /// Then the ledger is kept: a replacement is not a new career.
    #[test]
    fn replacing_the_game_with_the_same_career_keeps_the_ledger() {
        let state = state_in_career(3);
        run(&state, "req-same").unwrap();
        state.set_game(game_of_career(3));

        assert_eq!(
            state.transaction_status("req-same", Some(3)),
            TransactionStatus::Committed
        );
    }

    /// Given a career closed and the same id used in the next one
    /// When a status is asked with the old token
    /// Then the cleared state answers "career changed", not "unknown".
    #[test]
    fn a_cleared_career_answers_career_changed_to_its_old_token() {
        let state = state_in_career(4);
        run(&state, "req-clear").unwrap();
        state.clear_game();

        assert_eq!(
            state.transaction_status("req-clear", Some(4)),
            TransactionStatus::CareerChanged
        );
    }

    /// Given a command whose change panics
    /// When its mark is left behind
    /// Then the mark is taken back: the request is not in progress forever, and the game lock is
    /// not the ledger's lock.
    #[test]
    fn a_command_that_panics_leaves_no_in_progress_mark() {
        let state = Arc::new(state());
        let crashing = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                state.run_transaction::<u32>(Some("req-panic"), OP, |_| panic!("boom"))
            })
        };
        assert!(crashing.join().is_err());

        assert_eq!(
            state.transaction_status("req-panic", None),
            TransactionStatus::Unknown
        );
    }

    /// Given more finished requests than the ledger remembers while one request is still running
    /// When the ledger makes room
    /// Then the running request is not forgotten.
    #[test]
    fn a_request_in_progress_is_never_forgotten_to_make_room() {
        let state = state();
        let seen = std::sync::Mutex::new(None);
        value(state.run_transaction(Some("req-long"), OP, |game| {
            for index in 0..(REMEMBERED_REQUESTS + 10) {
                state.with_ledger(|ledger| {
                    ledger.insert(
                        &format!("filler-{index}"),
                        OP,
                        Entry::Refused(String::new()),
                    )
                });
            }
            *seen.lock().unwrap() = Some(state.transaction_status("req-long", None));
            post_one_message(game)
        }))
        .unwrap();

        assert_eq!(
            seen.into_inner().unwrap(),
            Some(TransactionStatus::InProgress)
        );
    }

    /// Given a commit and another command waiting for the game lock
    /// When the transaction returns
    /// Then its snapshot shows its own change and none of the waiting command's: it was taken
    /// under the same lock hold as the commit, not read afterwards.
    #[test]
    fn the_snapshot_belongs_to_the_commit_not_to_a_later_state() {
        let state = Arc::new(state());
        let (waiting_tx, waiting_rx) = mpsc::channel();
        let writer = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                waiting_rx.recv().unwrap();
                state.update_game(|game| {
                    post_one_message(game).ok();
                });
            })
        };
        let receipt = state
            .run_transaction(Some("req-snap"), OP, |game| {
                waiting_tx.send(()).unwrap();
                // Give the writer every chance to run before the transaction releases the lock.
                std::thread::sleep(Duration::from_millis(50));
                post_one_message(game)
            })
            .unwrap()
            .unwrap();
        writer.join().unwrap();

        assert_eq!(receipt.snapshot.unwrap().messages.len(), 1);
        assert_eq!(messages(&state), 2);
    }

    /// Given a request that already committed
    /// When it is delivered again
    /// Then the answer carries no snapshot: the game has moved on and none was kept.
    #[test]
    fn a_replay_carries_no_snapshot() {
        let state = state();
        let first = state
            .run_transaction(Some("req-replay"), OP, post_one_message)
            .unwrap()
            .unwrap();
        let second = state
            .run_transaction(Some("req-replay"), OP, post_one_message)
            .unwrap()
            .unwrap();

        assert!(first.snapshot.is_some());
        assert!(second.snapshot.is_none());
    }
    /// Given two transfer commands A then B on one career, both successful
    /// When their answers are put in the order a screen may receive them (B, then A)
    /// Then both commits are in the game, each answer carries the revision of the state it shows,
    /// and the later commit's revision is the higher one, so a screen can tell which to keep.
    #[test]
    fn two_commits_answered_out_of_order_are_ordered_by_the_revision_they_carry() {
        let state = state_in_career(3);
        // What a refresh asked for just before A commits sees: it must not look as new as A's answer.
        let read_before_a = state.get_game(|game| game.state_revision).unwrap();
        let a = state
            .run_transaction(Some("req-a"), OP, post_one_message)
            .unwrap()
            .unwrap();
        let b = state
            .run_transaction(Some("req-b"), OP, post_one_message)
            .unwrap()
            .unwrap();

        let (a_shown, b_shown) = (a.snapshot.unwrap(), b.snapshot.unwrap());

        assert_eq!(messages(&state), 2, "both transactions are committed");
        assert!(
            read_before_a < a_shown.state_revision,
            "an answer is stamped with the revision of the state it shows, not the one before it"
        );
        assert!(a_shown.state_revision < b_shown.state_revision);
        assert_eq!(a_shown.messages.len(), 1);
        assert_eq!(b_shown.messages.len(), 2);
        assert_eq!(a_shown.career_token, b_shown.career_token);
    }

    /// Given a committed request and another writer that has moved the game on
    /// When the request is delivered again
    /// Then the replay carries no snapshot (so no older revision can travel with it), nothing is
    /// written again, and the game still holds one copy of the first commit and the later writer's.
    #[test]
    fn a_replay_carries_no_snapshot_that_could_be_older_than_the_game() {
        let state = state_in_career(3);
        let first = state
            .run_transaction(Some("req-r"), OP, post_one_message)
            .unwrap()
            .unwrap();
        state.update_game(|game| post_one_message(game).ok());

        let revision_after_second_writer = state.get_game(|game| game.state_revision).unwrap();
        let replayed = state
            .run_transaction(Some("req-r"), OP, post_one_message)
            .unwrap()
            .unwrap();

        assert!(first.snapshot.unwrap().state_revision < revision_after_second_writer);
        assert!(replayed.snapshot.is_none());
        assert_eq!(messages(&state), 2);
    }

    /// Given eight threads each committing 25 transactions
    /// When every answer is collected
    /// Then no two answers share a revision: each commit has its own place in the order.
    #[test]
    fn concurrent_commits_never_share_a_revision() {
        let state = Arc::new(state_in_career(3));
        let workers: Vec<_> = (0..8)
            .map(|worker| {
                let state = Arc::clone(&state);
                std::thread::spawn(move || {
                    (0..25)
                        .map(|n| {
                            state
                                .run_transaction(
                                    Some(format!("req-{worker}-{n}").as_str()),
                                    OP,
                                    post_one_message,
                                )
                                .unwrap()
                                .unwrap()
                                .snapshot
                                .unwrap()
                                .state_revision
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut revisions: Vec<u64> = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect();
        revisions.sort_unstable();
        let distinct = revisions.len();
        revisions.dedup();

        assert_eq!(distinct, 200);
        assert_eq!(revisions.len(), 200, "a revision was handed out twice");
        assert_eq!(messages(&state), 200);
    }
    /// Given a revision the screen holds
    /// When a transaction is refused, or a committed one is replayed
    /// Then the revision has not moved: only a commit advances it, so a refusal leaves the game
    /// byte for byte as it was.
    #[test]
    fn a_refusal_and_a_replay_leave_the_revision_where_it_was() {
        let state = state_in_career(3);
        let committed = state
            .run_transaction(Some("req-ok"), OP, post_one_message)
            .unwrap()
            .unwrap();
        let after_commit = state.get_game(|game| game.state_revision).unwrap();

        let refused =
            state.run_transaction::<u32>(Some("req-no"), OP, |_| Err("be.error.nope".into()));
        let replayed = state.run_transaction(Some("req-ok"), OP, post_one_message);
        let career_refused = state.run_transaction(
            TransactionRequest::new(Some("req-old"), Some(99)),
            OP,
            post_one_message,
        );

        assert!(refused.unwrap().is_err());
        assert!(replayed.unwrap().is_ok());
        assert!(career_refused.unwrap().is_err());
        assert_eq!(committed.snapshot.unwrap().state_revision, after_commit);
        assert_eq!(
            state.get_game(|game| game.state_revision),
            Some(after_commit)
        );
    }
}
