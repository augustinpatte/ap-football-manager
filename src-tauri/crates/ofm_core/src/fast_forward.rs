//! Advancing the career to a chosen date, one ordinary day at a time.
//!
//! There is no second simulation here. A fast-forward is the loop a player would run by hand —
//! [`turn::process_day_with_capture`] on a quiet day, [`matchday::play_user_matchday_with_capture`]
//! on a day their club plays, [`end_of_season::advance_to_next_season`] when the season is over —
//! with the questions a player asks between clicks answered by code: *is there anything I must
//! look at before I carry on?* Because every day goes through the same functions in the same
//! order, a fast-forward and the same number of single days, started from the same snapshot, end
//! in the same world (`season-harness/tests/fast_forward.rs` holds that to account).
//!
//! The loop is driven one [`FastForward::step`] at a time, with the caller deciding how to hold
//! the game between steps. Each step is one whole day, so a stop — a decision, a cancel, an error —
//! always lands between days and never inside one: what a save sees is a world a player could have
//! reached by clicking.
//!
//! ## What stops it
//!
//! [`InterruptPolicy`] chooses how much the loop may decide for the player:
//!
//! * [`InterruptPolicy::StopAtNextMatch`] stops *before* the user's next fixture, and for anything
//!   [`InterruptPolicy::StopForDecisions`] stops for.
//! * [`InterruptPolicy::StopForDecisions`] plays the user's matches itself (with the lineup and
//!   tactics already set, as the existing "delegate" mode does) and stops when something new needs
//!   the player's approval: an incoming offer, an assistant proposal, a squad problem the host
//!   reports, a transfer deadline with talks still open, the end of the season.
//! * [`InterruptPolicy::ToTargetDate`] never stops for a decision. It plays the matches, rolls the
//!   season over, and records in the report what it passed — so nothing is skipped *silently*.
//!
//! Only a firing, an error, a cancel, the day limit and the target stop every policy.
//!
//! Decisions that were already open when the run began are *acknowledged*: the player saw them
//! (the host shows the blocker list before it starts, as for a single day) and chose to carry on,
//! so they do not stop the run again. Only what appears afterwards does.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use domain::player::{LoanOfferStatus, TransferOfferStatus};
use domain::season::TransferWindowStatus;
use domain::stats::StatsState;
use serde::{Deserialize, Serialize};

use crate::end_of_season::{self, EndOfSeasonSummary};
use crate::game::Game;
use crate::{matchday, turn};

/// How far ahead a request may reach. Two seasons: past that the calendar is not generated yet
/// and a request is a mistake, not a plan.
pub const MAX_FAST_FORWARD_DAYS: u32 = 800;

/// How much the fast-forward may decide for the player. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptPolicy {
    StopAtNextMatch,
    #[default]
    StopForDecisions,
    ToTargetDate,
}

/// What the player asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastForwardRequest {
    /// `YYYY-MM-DD`. The run ends on the day *before* it is played: the clock reads this date and
    /// nothing dated this day has happened yet, as after any "Continue".
    pub target_date: String,
    #[serde(default)]
    pub policy: InterruptPolicy,
}

/// The kinds of thing that need the player before the run can carry on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// An action-required blocker computed by the host (squad crisis, key contract...).
    Blocker,
    /// A proposal the assistant coach put to the player and that awaits an answer.
    DelegationProposal,
    /// A club's pending bid for one of the user's players.
    IncomingTransferOffer,
    /// A club's pending loan request for one of the user's players.
    IncomingLoanOffer,
    /// A free agent answered the manager's offer with terms that require an answer.
    FreeAgentCounterOffer,
    /// The window closes today and talks involving the user's club are still open: they end
    /// unanswered when the day is played.
    TransferDeadline,
}

/// One thing awaiting the player. `id` plus `params` is its identity: the same blocker with
/// different players named is a different decision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PendingDecision {
    pub kind: DecisionKind,
    pub id: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// Why the run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StopReason {
    TargetReached,
    /// The user's club plays on the date the clock reads; nothing of that day has been played.
    NextMatch,
    /// Something new needs the player; nothing of the day the clock reads has been played.
    Decisions {
        decisions: Vec<PendingDecision>,
    },
    /// The user's season is over and waits for the player to roll it over.
    SeasonComplete,
    /// The board dismissed the manager during the last day played.
    ManagerFired,
    /// The host asked to stop; the run ended between two days.
    Cancelled,
    /// [`MAX_FAST_FORWARD_DAYS`] days were played without reaching the target.
    DayLimit,
    /// A day could not be played. Every earlier day stands. `error` is a translation key.
    Error {
        error: String,
    },
}

/// Something a [`InterruptPolicy::ToTargetDate`] run went past that a stopping policy would have
/// stopped for. The report carries these so that nothing is skipped without a trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassedDecision {
    pub date: String,
    pub decision: PendingDecision,
}

/// Where a run stands, for a progress display. Cheap to build: no clone of the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastForwardProgress {
    pub start_date: String,
    pub current_date: String,
    pub target_date: String,
    pub days_done: u32,
    pub days_total: u32,
    pub user_matches_played: u32,
    pub seasons_rolled_over: u32,
}

/// What a finished run did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastForwardSummary {
    pub progress: FastForwardProgress,
    pub stop: StopReason,
    /// One per season the run rolled over ([`InterruptPolicy::ToTargetDate`] only).
    pub season_summaries: Vec<EndOfSeasonSummary>,
    /// What a [`InterruptPolicy::ToTargetDate`] run went past. Always empty for the other
    /// policies, which stop instead.
    pub passed_decisions: Vec<PassedDecision>,
}

/// An open run. Holds only what the loop needs between days; the game lives with the caller.
#[derive(Debug)]
pub struct FastForward {
    policy: InterruptPolicy,
    start_date: String,
    target_date: String,
    target: NaiveDate,
    days_total: u32,
    days_done: u32,
    was_employed: bool,
    acknowledged: BTreeSet<PendingDecision>,
    reported_passed: BTreeSet<PendingDecision>,
    user_matches_played: u32,
    season_summaries: Vec<EndOfSeasonSummary>,
    passed_decisions: Vec<PassedDecision>,
}

fn parse_day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn day_text(game: &Game) -> String {
    game.clock.current_date.format("%Y-%m-%d").to_string()
}

impl FastForward {
    /// Validates the request against the game and records what is already open as acknowledged.
    /// `host_decisions` are the decisions only the host can compute (blockers); they are asked
    /// once here, for the baseline, and again after each day by [`FastForward::step`].
    pub fn begin(
        game: &Game,
        request: &FastForwardRequest,
        host_decisions: &mut dyn FnMut(&Game) -> Vec<PendingDecision>,
    ) -> Result<Self, String> {
        let start = parse_day(&day_text(game))
            .ok_or_else(|| "be.error.fastForward.invalidDate".to_string())?;
        let target = parse_day(&request.target_date)
            .ok_or_else(|| "be.error.fastForward.invalidDate".to_string())?;
        if target <= start {
            return Err("be.error.fastForward.targetNotInFuture".to_string());
        }
        let days_total = u32::try_from((target - start).num_days())
            .map_err(|_| "be.error.fastForward.targetTooFar".to_string())?;
        if days_total > MAX_FAST_FORWARD_DAYS {
            return Err("be.error.fastForward.targetTooFar".to_string());
        }
        let mut acknowledged: BTreeSet<PendingDecision> =
            pending_decisions(game).into_iter().collect();
        acknowledged.extend(host_decisions(game));
        Ok(Self {
            policy: request.policy,
            start_date: start.format("%Y-%m-%d").to_string(),
            target_date: target.format("%Y-%m-%d").to_string(),
            target,
            days_total,
            days_done: 0,
            was_employed: game.manager.team_id.is_some(),
            acknowledged,
            reported_passed: BTreeSet::new(),
            user_matches_played: 0,
            season_summaries: Vec::new(),
            passed_decisions: Vec::new(),
        })
    }

    pub fn start_date(&self) -> &str {
        &self.start_date
    }

    pub fn progress(&self, game: &Game) -> FastForwardProgress {
        FastForwardProgress {
            start_date: self.start_date.clone(),
            current_date: day_text(game),
            target_date: self.target_date.clone(),
            days_done: self.days_done,
            days_total: self.days_total,
            user_matches_played: self.user_matches_played,
            seasons_rolled_over: u32::try_from(self.season_summaries.len()).unwrap_or(u32::MAX),
        }
    }

    pub fn finish(self, game: &Game, stop: StopReason) -> FastForwardSummary {
        FastForwardSummary {
            progress: self.progress(game),
            stop,
            season_summaries: self.season_summaries,
            passed_decisions: self.passed_decisions,
        }
    }

    /// Plays at most one day. `None` means the run goes on; `Some` is the reason it ended, and
    /// the game is then between two days, with nothing of the day the clock reads played.
    pub fn step(
        &mut self,
        game: &mut Game,
        host_decisions: &mut dyn FnMut(&Game) -> Vec<PendingDecision>,
        on_capture: &mut dyn FnMut(StatsState),
    ) -> Option<StopReason> {
        let today = day_text(game);
        if parse_day(&today).is_some_and(|date| date >= self.target) {
            return Some(StopReason::TargetReached);
        }
        if self.days_done >= MAX_FAST_FORWARD_DAYS {
            return Some(StopReason::DayLimit);
        }
        if let Some(stop) = self.attend_to_decisions(game, &today, host_decisions) {
            return Some(stop);
        }
        if let Some(stop) = self.attend_to_season_end(game) {
            return Some(stop);
        }
        let fixture = matchday::scheduled_user_fixture(game, &today);
        if fixture.is_some() && self.policy == InterruptPolicy::StopAtNextMatch {
            return Some(StopReason::NextMatch);
        }
        if let Err(error) = self.play_day(game, &today, fixture, on_capture) {
            return Some(StopReason::Error { error });
        }
        self.days_done += 1;
        if self.was_employed && game.manager.team_id.is_none() {
            return Some(StopReason::ManagerFired);
        }
        None
    }

    /// The decisions that appeared since the run began, before today is played. A stopping policy
    /// stops on them; the target-date policy writes them in the report and goes on.
    fn attend_to_decisions(
        &mut self,
        game: &Game,
        today: &str,
        host_decisions: &mut dyn FnMut(&Game) -> Vec<PendingDecision>,
    ) -> Option<StopReason> {
        if !self.was_employed {
            return None;
        }
        let mut open = pending_decisions(game);
        open.extend(transfer_deadline_decision(game, today));
        // The host's list (blockers) is the costly one: the target-date policy never stops for
        // it, and it is not asked for there.
        if self.policy != InterruptPolicy::ToTargetDate {
            open.extend(host_decisions(game));
        }
        let new: Vec<PendingDecision> = open
            .into_iter()
            .filter(|decision| !self.acknowledged.contains(decision))
            .collect();
        if new.is_empty() {
            return None;
        }
        if self.policy == InterruptPolicy::ToTargetDate {
            for decision in new {
                if self.reported_passed.insert(decision.clone()) {
                    self.passed_decisions.push(PassedDecision {
                        date: today.to_string(),
                        decision,
                    });
                }
            }
            return None;
        }
        // Acknowledged from here on: resuming after the stop is the player's "carry on".
        self.acknowledged.extend(new.iter().cloned());
        Some(StopReason::Decisions { decisions: new })
    }

    /// The season is over: a stopping policy hands the rollover to the player, who has an
    /// awards screen to read; the target-date policy does it with the game's own rollover.
    fn attend_to_season_end(&mut self, game: &mut Game) -> Option<StopReason> {
        if !self.was_employed || !end_of_season::is_season_complete(game) {
            return None;
        }
        if self.policy != InterruptPolicy::ToTargetDate {
            return Some(StopReason::SeasonComplete);
        }
        match end_of_season::advance_to_next_season(game) {
            Ok(summary) => {
                self.season_summaries.push(summary);
                if game.manager.team_id.is_none() {
                    return Some(StopReason::ManagerFired);
                }
                None
            }
            Err(error) => Some(StopReason::Error { error }),
        }
    }

    /// The day itself: exactly what a single "Continue" does, in `delegate` mode on a match day.
    fn play_day(
        &mut self,
        game: &mut Game,
        today: &str,
        fixture: Option<(Option<usize>, usize)>,
        on_capture: &mut dyn FnMut(StatsState),
    ) -> Result<(), String> {
        // Before anything is played, so a round drawn by today's results is known to be new.
        turn::begin_day(game);
        match fixture {
            Some((competition_index, fixture_index)) => {
                matchday::play_user_matchday_with_capture(
                    game,
                    competition_index,
                    fixture_index,
                    &mut |capture| on_capture(capture),
                )?;
                self.user_matches_played += 1;
            }
            None => turn::process_day_with_capture(game, &mut |capture| on_capture(capture)),
        }
        log::debug!("[fast_forward] played {today}");
        Ok(())
    }
}

/// Everything awaiting the player that the game itself can see. The host adds what only it can
/// compute (blockers) through the `host_decisions` it hands to [`FastForward`].
pub fn pending_decisions(game: &Game) -> Vec<PendingDecision> {
    let Some(user_team_id) = game.manager.team_id.as_deref() else {
        return Vec::new();
    };
    let mut decisions: Vec<PendingDecision> = game
        .delegation
        .proposals
        .iter()
        .map(|proposal| PendingDecision {
            kind: DecisionKind::DelegationProposal,
            id: proposal.id.clone(),
            params: BTreeMap::new(),
        })
        .collect();
    for player in game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(user_team_id))
    {
        for offer in &player.transfer_offers {
            if offer.status == TransferOfferStatus::Pending && offer.from_team_id != user_team_id {
                decisions.push(offer_decision(
                    DecisionKind::IncomingTransferOffer,
                    &offer.id,
                    &player.id,
                ));
            }
        }
        for offer in &player.loan_offers {
            if offer.status == LoanOfferStatus::Pending && offer.from_team_id != user_team_id {
                decisions.push(offer_decision(
                    DecisionKind::IncomingLoanOffer,
                    &offer.id,
                    &player.id,
                ));
            }
        }
    }
    for player in &game.players {
        for talk in player.morale_core.free_agent_talks.iter().filter(|talk| {
            talk.made_by_manager
                && talk.status == domain::free_agent_talk::FreeAgentTalkStatus::CounterOffered
        }) {
            decisions.push(PendingDecision {
                kind: DecisionKind::FreeAgentCounterOffer,
                id: talk.id.clone(),
                params: BTreeMap::from([
                    ("player_id".to_string(), player.id.clone()),
                    (
                        "expires_on".to_string(),
                        talk.counter_expires_on.clone().unwrap_or_default(),
                    ),
                ]),
            });
        }
    }
    decisions
}

fn offer_decision(kind: DecisionKind, offer_id: &str, player_id: &str) -> PendingDecision {
    PendingDecision {
        kind,
        id: offer_id.to_string(),
        params: BTreeMap::from([("player_id".to_string(), player_id.to_string())]),
    }
}

/// On the closing day, talks the user is party to — their own bids included — end unanswered when
/// the day is played (`transfers::expire_negotiations_at_deadline`). Arriving at that checkpoint
/// with any open is the last moment the player can answer them.
fn transfer_deadline_decision(game: &Game, today: &str) -> Option<PendingDecision> {
    if game.season_context.transfer_window.status != TransferWindowStatus::DeadlineDay {
        return None;
    }
    let user_team_id = game.manager.team_id.as_deref()?;
    let open = game
        .players
        .iter()
        .map(|player| {
            let owned = player.team_id.as_deref() == Some(user_team_id);
            let transfers = player.transfer_offers.iter().filter(|offer| {
                offer.status == TransferOfferStatus::Pending
                    && offer.date != today
                    && (owned || offer.from_team_id == user_team_id)
            });
            let loans = player.loan_offers.iter().filter(|offer| {
                offer.status == LoanOfferStatus::Pending
                    && offer.date != today
                    && (owned || offer.from_team_id == user_team_id)
            });
            transfers.count() + loans.count()
        })
        .sum::<usize>();
    (open > 0).then(|| PendingDecision {
        kind: DecisionKind::TransferDeadline,
        id: today.to_string(),
        params: BTreeMap::from([("open_negotiations".to_string(), open.to_string())]),
    })
}

/// Plays a whole run, holding the game for its length. For callers with no lock to release
/// between days — the season harness, a benchmark, a test. The application drives
/// [`FastForward::step`] itself so that other work can interleave between days.
pub fn run(
    game: &mut Game,
    request: &FastForwardRequest,
    host_decisions: &mut dyn FnMut(&Game) -> Vec<PendingDecision>,
    on_capture: &mut dyn FnMut(StatsState),
) -> Result<FastForwardSummary, String> {
    let mut run = FastForward::begin(game, request, host_decisions)?;
    let stop = loop {
        if let Some(stop) = run.step(game, host_decisions, on_capture) {
            break stop;
        }
    };
    Ok(run.finish(game, stop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::free_agent_talk::{FreeAgentTalk, FreeAgentTalkStatus};
    use domain::manager::Manager;
    use domain::player::{Player, PlayerAttributes, Position};
    use domain::team::Team;

    use crate::clock::GameClock;

    fn no_host(_: &Game) -> Vec<PendingDecision> {
        Vec::new()
    }

    fn game_on(date: (i32, u32, u32)) -> Game {
        let clock = GameClock::new(
            Utc.with_ymd_and_hms(date.0, date.1, date.2, 12, 0, 0)
                .unwrap(),
        );
        let mut manager = Manager::new(
            "mgr".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.hire("team1".into());
        let team = Team::new(
            "team1".into(),
            "Test FC".into(),
            "TST".into(),
            "England".into(),
            "Town".into(),
            "Ground".into(),
            10_000,
        );
        Game::new(clock, manager, vec![team], vec![], vec![], vec![])
    }

    fn request(target: &str, policy: InterruptPolicy) -> FastForwardRequest {
        FastForwardRequest {
            target_date: target.to_string(),
            policy,
        }
    }

    fn player_attributes() -> PlayerAttributes {
        PlayerAttributes {
            pace: 60,
            stamina: 60,
            strength: 60,
            agility: 60,
            passing: 60,
            shooting: 60,
            tackling: 60,
            dribbling: 60,
            defending: 60,
            positioning: 60,
            vision: 60,
            decisions: 60,
            composure: 60,
            aggression: 60,
            teamwork: 60,
            leadership: 60,
            handling: 30,
            reflexes: 30,
            aerial: 60,
        }
    }

    /// Given a target that is today, in the past or not a date, when the run begins, then it is
    /// refused with a key and nothing is played.
    #[test]
    fn a_target_that_is_not_in_the_future_is_refused() {
        let game = game_on((2025, 6, 15));
        for target in ["2025-06-15", "2025-06-01"] {
            let error = FastForward::begin(
                &game,
                &request(target, InterruptPolicy::ToTargetDate),
                &mut no_host,
            )
            .unwrap_err();
            assert_eq!(error, "be.error.fastForward.targetNotInFuture");
        }
        let error = FastForward::begin(
            &game,
            &request("soon", InterruptPolicy::ToTargetDate),
            &mut no_host,
        )
        .unwrap_err();
        assert_eq!(error, "be.error.fastForward.invalidDate");
    }

    /// Given a target further than the calendar reaches, when the run begins, then it is refused.
    #[test]
    fn a_target_beyond_the_limit_is_refused() {
        let game = game_on((2025, 6, 15));
        let error = FastForward::begin(
            &game,
            &request("2028-06-15", InterruptPolicy::ToTargetDate),
            &mut no_host,
        )
        .unwrap_err();
        assert_eq!(error, "be.error.fastForward.targetTooFar");
    }

    /// Given a quiet world, when a run reaches its target, then the clock reads the target, one
    /// day was played per day crossed and the stop is `TargetReached`.
    #[test]
    fn a_quiet_run_plays_one_day_per_day_and_stops_on_the_target() {
        let mut game = game_on((2025, 6, 15));
        let summary = run(
            &mut game,
            &request("2025-06-22", InterruptPolicy::ToTargetDate),
            &mut no_host,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(summary.stop, StopReason::TargetReached);
        assert_eq!(summary.progress.days_done, 7);
        assert_eq!(summary.progress.current_date, "2025-06-22");
        assert_eq!(day_text(&game), "2025-06-22");
    }

    /// Given a host decision that appears on the third day, when the policy stops for decisions,
    /// then the run ends there with that decision, between days; and when the same decision was
    /// open at the start, it is acknowledged and the run goes on.
    #[test]
    fn a_new_decision_stops_but_one_open_at_the_start_does_not() {
        let blocker = |game: &Game| {
            let late = game
                .clock
                .current_date
                .format("%Y-%m-%d")
                .to_string()
                .as_str()
                >= "2025-06-18";
            if late {
                vec![PendingDecision {
                    kind: DecisionKind::Blocker,
                    id: "injured_xi".into(),
                    params: BTreeMap::new(),
                }]
            } else {
                Vec::new()
            }
        };

        let mut game = game_on((2025, 6, 15));
        let summary = run(
            &mut game,
            &request("2025-06-30", InterruptPolicy::StopForDecisions),
            &mut { blocker },
            &mut |_| {},
        )
        .unwrap();
        assert!(
            matches!(summary.stop, StopReason::Decisions { ref decisions }
            if decisions.len() == 1 && decisions[0].id == "injured_xi")
        );
        assert_eq!(summary.progress.current_date, "2025-06-18");

        let mut game = game_on((2025, 6, 19));
        let summary = run(
            &mut game,
            &request("2025-06-30", InterruptPolicy::StopForDecisions),
            &mut { blocker },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(summary.stop, StopReason::TargetReached);
    }

    /// Given the target-date policy and a decision that appears, when the run passes it, then it
    /// does not stop and the report names it once.
    #[test]
    fn the_target_date_policy_reports_what_it_passed_instead_of_stopping() {
        let mut game = game_on((2025, 6, 15));
        let proposal = domain::delegation::Proposal {
            id: "p1".into(),
            duty: domain::delegation::Duty::Training,
            created: "2025-06-15".into(),
            action: domain::delegation::ProposalAction::RestPlayers { player_ids: vec![] },
            title_key: "t".into(),
            params: vec![],
            reason_key: "r".into(),
        };
        let mut reported = 0;
        let mut host = |_: &Game| {
            reported += 1;
            Vec::new()
        };
        let mut run_state = FastForward::begin(
            &game,
            &request("2025-06-20", InterruptPolicy::ToTargetDate),
            &mut host,
        )
        .unwrap();
        game.delegation.proposals.push(proposal);
        let stop = loop {
            if let Some(stop) = run_state.step(&mut game, &mut no_host, &mut |_| {}) {
                break stop;
            }
        };
        let summary = run_state.finish(&game, stop);
        assert_eq!(summary.stop, StopReason::TargetReached);
        assert_eq!(summary.passed_decisions.len(), 1);
        assert_eq!(summary.passed_decisions[0].decision.id, "p1");
    }

    /// Given a player counter-offers while a decision-mode advance is running, when the next
    /// boundary is reached, then the run stops before any further day can let the counter lapse.
    #[test]
    fn a_free_agent_counter_offer_stops_an_advance_for_decisions() {
        let mut game = game_on((2025, 6, 15));
        let mut run_state = FastForward::begin(
            &game,
            &request("2025-06-20", InterruptPolicy::StopForDecisions),
            &mut no_host,
        )
        .unwrap();
        let mut player = Player::new(
            "free-agent".into(),
            "Free".into(),
            "Agent".into(),
            "1995-01-01".into(),
            "England".into(),
            Position::Midfielder,
            player_attributes(),
        );
        player.morale_core.free_agent_talks.push(FreeAgentTalk {
            id: "counter-1".into(),
            club_id: "team1".into(),
            status: FreeAgentTalkStatus::CounterOffered,
            made_by_manager: true,
            counter_expires_on: Some("2025-06-18".into()),
            ..FreeAgentTalk::default()
        });
        game.players.push(player);

        let stop = run_state.step(&mut game, &mut no_host, &mut |_| {});

        assert!(matches!(stop, Some(StopReason::Decisions { decisions })
            if decisions.iter().any(|decision|
                decision.kind == DecisionKind::FreeAgentCounterOffer
                    && decision.id == "counter-1")));
        assert_eq!(day_text(&game), "2025-06-15");
    }

    /// Given a user match on the first day, when the policy is to stop at the next match, then
    /// nothing is played and the stop says so.
    #[test]
    fn stop_at_next_match_stops_before_the_matchday() {
        use domain::league::{Fixture, FixtureCompetition, FixtureStatus, League};
        let mut game = game_on((2025, 6, 15));
        let mut league = League::new(
            "league".into(),
            "League".into(),
            2025,
            &["team1".to_string(), "team2".to_string()],
        );
        league.fixtures.push(Fixture {
            id: "f1".into(),
            competition_id: "league".into(),
            date: "2025-06-17".into(),
            matchday: 1,
            home_team_id: "team1".into(),
            away_team_id: "team2".into(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            result: None,
            ..Default::default()
        });
        game.competitions = vec![league];
        let summary = run(
            &mut game,
            &request("2025-06-30", InterruptPolicy::StopAtNextMatch),
            &mut no_host,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(summary.stop, StopReason::NextMatch);
        assert_eq!(summary.progress.current_date, "2025-06-17");
        assert_eq!(summary.progress.days_done, 2);
    }
}
