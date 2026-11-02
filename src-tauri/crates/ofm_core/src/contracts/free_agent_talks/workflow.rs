//! The life of a free-agent negotiation: an offer is made, the player answers a few days later,
//! the club may be asked for other terms, and the whole thing ends in a signing, a refusal, a
//! withdrawal or a rival's signing.
//!
//! Three rules hold everywhere:
//! * **One path to a contract.** Every signing, whoever makes it, ends in [`sign_free_agent`],
//!   which refuses a player who already has a club and closes every other talk he had.
//! * **Money is checked twice.** Once when the offer is made, once when the player says yes: a
//!   club's books can change in the days between, and a signing it can no longer pay lapses
//!   rather than going through.
//! * **Time is bounded.** An offer is answered within [`MAX_RESPONSE_DAYS`]; a counter stands for
//!   [`COUNTER_STANDS_DAYS`]; nothing waits on the manager forever.

use super::appeal::{
    Appeal, AppealFacts, ClubSquad, OfferTerms, PlaceInSide, Verdict, ambition_pct, appeal_of,
    club_standing, counter_terms, days_unattached, player_standing,
};
use super::consts::*;
use super::*;
use crate::stable_hash::stable_hash;
use domain::free_agent_talk::{FreeAgentTalk, FreeAgentTalkReason, FreeAgentTalkStatus};

/// What an offer, a counter-answer or a withdrawal came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeAgentOfferDecision {
    /// With the player, who answers on `respond_on`.
    Pending,
    /// The player asks for other terms.
    CounterOffer,
    /// He has signed.
    Accepted,
    /// The talk is over.
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeAgentOfferOutcome {
    pub decision: FreeAgentOfferDecision,
    /// The talk as it now stands (closed, for a refusal).
    pub talk: Option<FreeAgentTalk>,
    pub respond_on: Option<String>,
    pub reason: Option<FreeAgentTalkReason>,
    pub suggested_wage: Option<u32>,
    pub suggested_years: Option<u32>,
}

impl FreeAgentOfferOutcome {
    fn of(decision: FreeAgentOfferDecision, talk: &FreeAgentTalk) -> Self {
        Self {
            decision,
            talk: Some(talk.clone()),
            respond_on: talk.respond_on.clone(),
            reason: talk.reason,
            suggested_wage: talk.counter_wage,
            suggested_years: talk.counter_years,
        }
    }
}

/// How the player would take an offer now, for the screen that asks before the manager commits.
/// Not a promise: a rival's better offer, or the days that pass, can change the answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeAgentInterest {
    pub verdict: InterestLevel,
    /// What weighs against the offer most, if anything does.
    pub main_obstacle: Option<FreeAgentTalkReason>,
    pub expected_wage: u32,
    pub expected_years: u32,
    pub place_in_side: PlaceInSide,
    pub days_unattached: i64,
    pub min_response_days: u64,
    pub max_response_days: u64,
    /// Other clubs with an open offer to him.
    pub rival_clubs: Vec<RivalInterest>,
    /// The manager's club's own talk with him, if one is open.
    pub open_talk: Option<FreeAgentTalk>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterestLevel {
    Keen,
    Hesitant,
    Unlikely,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RivalInterest {
    pub club_id: String,
    pub club_name: String,
}

fn manager_team(game: &Game) -> Result<Team, String> {
    let team_id = game
        .manager
        .team_id
        .as_deref()
        .ok_or(ERR_NO_TEAM_ASSIGNED.to_string())?;
    game.teams
        .iter()
        .find(|team| team.id == team_id)
        .cloned()
        .ok_or(ERR_MANAGED_TEAM_NOT_FOUND.to_string())
}

fn free_player_index(game: &Game, player_id: &str) -> Result<usize, String> {
    let index = game
        .players
        .iter()
        .position(|player| player.id == player_id)
        .ok_or(ERR_PLAYER_NOT_FOUND.to_string())?;
    let player = &game.players[index];
    if player.retired || player.team_id.is_some() {
        return Err(ERR_PLAYER_NOT_FREE_AGENT.to_string());
    }
    Ok(index)
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Days an answer takes: the same for the same talk on the same day, whatever the order in which
/// the game happens to look at it, so a save reloaded mid-talk is answered on the same day.
fn response_delay_days(player_id: &str, club_id: &str, today: NaiveDate, round: u8) -> u64 {
    let key = format!("{player_id}|{club_id}|{today}|{round}");
    let spread = MAX_RESPONSE_DAYS - MIN_RESPONSE_DAYS + 1;
    MIN_RESPONSE_DAYS + stable_hash(key.as_bytes(), 0x6672_6565) % spread
}

pub(super) fn new_talk(
    player_id: &str,
    club_id: &str,
    terms: &OfferTerms,
    today: NaiveDate,
    made_by_manager: bool,
) -> FreeAgentTalk {
    let day = format_date(today);
    FreeAgentTalk {
        id: format!("fat:{player_id}:{club_id}:{day}"),
        club_id: club_id.to_string(),
        status: FreeAgentTalkStatus::AwaitingResponse,
        weekly_wage: terms.weekly_wage,
        contract_years: terms.contract_years,
        signing_bonus: terms.signing_bonus,
        promise_stepping_stone: terms.promise_stepping_stone,
        round: 1,
        opened_on: day.clone(),
        updated_on: day,
        respond_on: Some(format_date(
            today + chrono::Days::new(response_delay_days(player_id, club_id, today, 1)),
        )),
        made_by_manager,
        ..FreeAgentTalk::default()
    }
}

fn close_talk(
    talk: &mut FreeAgentTalk,
    status: FreeAgentTalkStatus,
    reason: Option<FreeAgentTalkReason>,
    today: NaiveDate,
) {
    talk.status = status;
    talk.reason = reason;
    talk.respond_on = None;
    talk.counter_expires_on = None;
    talk.closed_on = Some(format_date(today));
    talk.updated_on = format_date(today);
}

fn terms_of(talk: &FreeAgentTalk) -> OfferTerms {
    OfferTerms {
        weekly_wage: talk.weekly_wage,
        contract_years: talk.contract_years,
        signing_bonus: talk.signing_bonus,
        promise_stepping_stone: talk.promise_stepping_stone,
    }
}

fn validate_terms(terms: &OfferTerms) -> Result<(), String> {
    let weeks = WEEKS_PER_YEAR * u64::from(terms.contract_years.max(1));
    if terms.weekly_wage == 0
        || terms.contract_years == 0
        || terms.contract_years > MAX_CONTRACT_YEARS
        || terms.signing_bonus > u64::from(terms.weekly_wage) * weeks
    {
        return Err(ERR_INVALID_OFFER.to_string());
    }
    Ok(())
}

/// The facts a player weighs an offer from `team` on.
pub(crate) fn facts_for(
    player: &Player,
    team: &Team,
    squad: &ClubSquad,
    terms: &OfferTerms,
    today: NaiveDate,
) -> AppealFacts {
    let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
    let wants_promise = crate::transfers::wants_stepping_stone(player, today);
    let mut expected = expected_wage(player, team, today);
    if terms.promise_stepping_stone && wants_promise {
        expected = crate::transfers::discounted_wage(expected);
    }
    AppealFacts {
        age,
        ambition: ambition_pct(age, player.ovr, player.potential),
        player_standing: player_standing(player.ovr, player.potential, age),
        club_standing: club_standing(team.reputation, squad.level()),
        place: squad.place_for(player),
        days_unattached: days_unattached(player, today),
        wants_stepping_stone: wants_promise,
        expected_wage: expected,
        expected_years: expected_contract_years(player, today),
        incumbent: None,
    }
}

/// The rules about money, held to when an offer is made and again when a player says yes.
fn validate_affordable(
    game: &Game,
    team: &Team,
    player: &Player,
    terms: &OfferTerms,
    today: NaiveDate,
) -> Result<(), String> {
    if !wage_policy_verdict(game, team, player, terms.weekly_wage).permits() {
        return Err(renewal_wage_policy_error_message(team));
    }
    if terms.signing_bonus > 0 {
        crate::transfers::ensure_bonus_affordable(team, terms.signing_bonus)?;
        let bonus = i64::try_from(terms.signing_bonus)
            .map_err(|_| "be.error.finance.amountOverflow".to_string())?;
        crate::finances::validate_posts(
            game,
            &[crate::finances::PostRequest::new(
                &team.id,
                -bonus,
                crate::finances::CashKind::SigningBonus,
                today,
            )],
        )?;
    }
    Ok(())
}

fn ensure_room_in_the_squad(game: &Game, club_id: &str) -> Result<(), String> {
    if ClubSquad::of(&game.players, club_id).seniors() >= MAX_SENIOR_SQUAD {
        return Err(ERR_SQUAD_FULL.to_string());
    }
    Ok(())
}

fn open_talk_of<'a>(player: &'a Player, club_id: &str) -> Option<&'a FreeAgentTalk> {
    player
        .morale_core
        .free_agent_talks
        .iter()
        .find(|talk| talk.club_id == club_id && talk.is_open())
}

fn open_talk_mut<'a>(player: &'a mut Player, club_id: &str) -> Option<&'a mut FreeAgentTalk> {
    player
        .morale_core
        .free_agent_talks
        .iter_mut()
        .find(|talk| talk.club_id == club_id && talk.is_open())
}

/// Whether `club_id` was turned away so firmly that it is not heard for a while: an insulting
/// offer, or patience run out.
fn is_in_the_cold(player: &Player, club_id: &str, today: NaiveDate) -> bool {
    player.morale_core.free_agent_talks.iter().any(|talk| {
        talk.club_id == club_id
            && talk.status == FreeAgentTalkStatus::Rejected
            && matches!(
                talk.reason,
                Some(FreeAgentTalkReason::OfferInsulting | FreeAgentTalkReason::TalksBrokenOff)
            )
            && talk
                .closed_on
                .as_deref()
                .and_then(parse_contract_date)
                .is_some_and(|closed| (today - closed).num_days() < BROKEN_OFF_COOLDOWN_DAYS)
    })
}

/// Talks the manager's club has open across the market.
fn open_manager_talks(game: &Game) -> usize {
    game.players
        .iter()
        .flat_map(|player| &player.morale_core.free_agent_talks)
        .filter(|talk| talk.made_by_manager && talk.is_open())
        .count()
}

/// Makes the manager's offer to a free agent. It does not sign him: it opens a talk he answers
/// within a few days (see [`process_free_agent_talks`]), unless the offer is so far below his
/// worth that he refuses it on the spot.
///
/// Refused outright, with a translation-key error, when the offer is malformed, the club cannot
/// pay it, its squad list is full, a talk is already open, the club was turned away recently, or
/// the manager has too many talks open.
pub fn submit_free_agent_offer(
    game: &mut Game,
    player_id: &str,
    terms: OfferTerms,
) -> Result<FreeAgentOfferOutcome, String> {
    let team = manager_team(game)?;
    let index = free_player_index(game, player_id)?;
    let today = game.clock.current_date.date_naive();
    validate_terms(&terms)?;
    if terms.promise_stepping_stone
        && !crate::transfers::wants_stepping_stone(&game.players[index], today)
    {
        return Err(crate::transfers::ERR_PROMISE_NOT_WANTED.to_string());
    }
    lapse_overdue_counters(&mut game.players[index], today);

    let existing = open_talk_of(&game.players[index], &team.id);
    if existing.is_some_and(|talk| talk.status == FreeAgentTalkStatus::AwaitingResponse) {
        return Err(ERR_TALK_ALREADY_OPEN.to_string());
    }
    if is_in_the_cold(&game.players[index], &team.id, today) {
        return Err(ERR_TALKS_BROKEN_OFF.to_string());
    }
    if existing.is_none() && open_manager_talks(game) >= MAX_OPEN_TALKS_PER_MANAGER {
        return Err(ERR_TOO_MANY_OPEN_TALKS.to_string());
    }
    validate_affordable(game, &team, &game.players[index], &terms, today)?;
    ensure_room_in_the_squad(game, &team.id)?;

    let squad = ClubSquad::of(&game.players, &team.id);
    let player = &game.players[index];
    let facts = facts_for(player, &team, &squad, &terms, today);
    let round = existing.map_or(1, |talk| talk.round.saturating_add(1));
    let insulting = is_insulting_wage_offer(
        reference_player_wage(player),
        facts.expected_wage,
        terms.worth_per_week(),
    );

    let mut talk = existing
        .cloned()
        .unwrap_or_else(|| new_talk(player_id, &team.id, &terms, today, true));
    talk.weekly_wage = terms.weekly_wage;
    talk.contract_years = terms.contract_years;
    talk.signing_bonus = terms.signing_bonus;
    talk.promise_stepping_stone = terms.promise_stepping_stone;
    talk.round = round;
    talk.status = FreeAgentTalkStatus::AwaitingResponse;
    talk.counter_wage = None;
    talk.counter_years = None;
    talk.counter_expires_on = None;
    talk.updated_on = format_date(today);
    talk.respond_on = Some(format_date(
        today + chrono::Days::new(response_delay_days(player_id, &team.id, today, round)),
    ));

    let decision = if insulting {
        close_talk(
            &mut talk,
            FreeAgentTalkStatus::Rejected,
            Some(FreeAgentTalkReason::OfferInsulting),
            today,
        );
        FreeAgentOfferDecision::Rejected
    } else if round > MAX_ROUNDS {
        close_talk(
            &mut talk,
            FreeAgentTalkStatus::Rejected,
            Some(FreeAgentTalkReason::TalksBrokenOff),
            today,
        );
        FreeAgentOfferDecision::Rejected
    } else {
        FreeAgentOfferDecision::Pending
    };
    let outcome = FreeAgentOfferOutcome::of(decision, &talk);
    store_talk(&mut game.players[index], talk);
    Ok(outcome)
}

/// Replaces the talk with the same id, or adds it.
fn store_talk(player: &mut Player, talk: FreeAgentTalk) {
    let talks = &mut player.morale_core.free_agent_talks;
    match talks.iter_mut().find(|existing| existing.id == talk.id) {
        Some(existing) => *existing = talk,
        None => talks.push(talk),
    }
}

/// The manager takes the terms the player asked for in a counter-offer. The player has already
/// proposed them, so he signs at once — provided the club can still pay them.
pub fn accept_free_agent_counter(
    game: &mut Game,
    player_id: &str,
) -> Result<FreeAgentOfferOutcome, String> {
    let team = manager_team(game)?;
    let index = free_player_index(game, player_id)?;
    let today = game.clock.current_date.date_naive();
    let talk = open_talk_of(&game.players[index], &team.id)
        .filter(|talk| talk.status == FreeAgentTalkStatus::CounterOffered)
        .cloned()
        .ok_or(ERR_NO_COUNTER_TO_ANSWER.to_string())?;
    let (Some(wage), Some(years)) = (talk.counter_wage, talk.counter_years) else {
        return Err(ERR_NO_COUNTER_TO_ANSWER.to_string());
    };
    if talk
        .counter_expires_on
        .as_deref()
        .and_then(parse_contract_date)
        .is_some_and(|expires| today > expires)
    {
        lapse_overdue_counters(&mut game.players[index], today);
        return Err(ERR_COUNTER_EXPIRED.to_string());
    }
    let terms = OfferTerms {
        weekly_wage: wage,
        contract_years: years,
        ..terms_of(&talk)
    };
    complete_signing(game, index, &team.id, &terms, true, today)?;
    let signed = game.players[index]
        .morale_core
        .free_agent_talks
        .iter()
        .find(|candidate| candidate.id == talk.id)
        .cloned()
        .unwrap_or(talk);
    Ok(FreeAgentOfferOutcome::of(
        FreeAgentOfferDecision::Accepted,
        &signed,
    ))
}

/// The manager takes the club's offer back, or lets a counter go.
pub fn withdraw_free_agent_offer(
    game: &mut Game,
    player_id: &str,
) -> Result<FreeAgentOfferOutcome, String> {
    let team = manager_team(game)?;
    let index = free_player_index(game, player_id)?;
    let today = game.clock.current_date.date_naive();
    let talk =
        open_talk_mut(&mut game.players[index], &team.id).ok_or(ERR_NO_OPEN_TALK.to_string())?;
    close_talk(
        talk,
        FreeAgentTalkStatus::Withdrawn,
        Some(FreeAgentTalkReason::ManagerWithdrew),
        today,
    );
    Ok(FreeAgentOfferOutcome::of(
        FreeAgentOfferDecision::Rejected,
        talk,
    ))
}

/// How a free agent would take `terms` from the manager's club right now.
pub fn assess_free_agent_interest(
    game: &Game,
    player_id: &str,
    terms: OfferTerms,
) -> Result<FreeAgentInterest, String> {
    let team = manager_team(game)?;
    let index = free_player_index(game, player_id)?;
    let today = game.clock.current_date.date_naive();
    let player = &game.players[index];
    let squad = ClubSquad::of(&game.players, &team.id);
    let facts = facts_for(player, &team, &squad, &terms, today);
    // Terms with no length yet (a screen still being filled in) are judged at the length he wants.
    let terms = OfferTerms {
        contract_years: if terms.contract_years == 0 {
            facts.expected_years
        } else {
            terms.contract_years
        },
        ..terms
    };
    let appeal = appeal_of(&facts, &terms);
    let verdict = match appeal.verdict() {
        Verdict::Accept => InterestLevel::Keen,
        Verdict::Counter => InterestLevel::Hesitant,
        Verdict::Refuse => InterestLevel::Unlikely,
    };
    Ok(FreeAgentInterest {
        verdict,
        main_obstacle: (appeal.score < ACCEPT_AT).then(|| appeal.main_obstacle()),
        expected_wage: facts.expected_wage,
        expected_years: facts.expected_years,
        place_in_side: facts.place,
        days_unattached: facts.days_unattached,
        min_response_days: MIN_RESPONSE_DAYS,
        max_response_days: MAX_RESPONSE_DAYS,
        rival_clubs: rival_clubs_of(game, player, &team.id),
        open_talk: open_talk_of(player, &team.id).cloned(),
    })
}

fn rival_clubs_of(game: &Game, player: &Player, own_club_id: &str) -> Vec<RivalInterest> {
    player
        .morale_core
        .free_agent_talks
        .iter()
        .filter(|talk| talk.is_open() && talk.club_id != own_club_id)
        .map(|talk| RivalInterest {
            club_id: talk.club_id.clone(),
            club_name: game.team_name_or_id(&talk.club_id),
        })
        .collect()
}

/// Talks that wait on the manager's answer: counters not yet answered.
pub fn free_agent_talks_awaiting_manager(game: &Game) -> usize {
    game.players
        .iter()
        .flat_map(|player| &player.morale_core.free_agent_talks)
        .filter(|talk| talk.made_by_manager && talk.status == FreeAgentTalkStatus::CounterOffered)
        .count()
}

/// Lets counters the manager did not answer in time lapse.
fn lapse_overdue_counters(player: &mut Player, today: NaiveDate) -> Vec<FreeAgentTalk> {
    let mut lapsed = Vec::new();
    for talk in &mut player.morale_core.free_agent_talks {
        let overdue = talk.status == FreeAgentTalkStatus::CounterOffered
            && talk
                .counter_expires_on
                .as_deref()
                .and_then(parse_contract_date)
                .is_some_and(|expires| today > expires);
        if overdue {
            close_talk(
                talk,
                FreeAgentTalkStatus::Lapsed,
                Some(FreeAgentTalkReason::CounterExpired),
                today,
            );
            lapsed.push(talk.clone());
        }
    }
    lapsed
}

/// The day's work on free-agent talks: players whose answer is due give it, unanswered counters
/// lapse, old closed talks are dropped, and rival clubs show interest in the best of the market.
/// Called once a day, after the contract expiries that put players on the market.
pub fn process_free_agent_talks(game: &mut Game) {
    let today = game.clock.current_date.date_naive();
    let with_talks: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| !player.morale_core.free_agent_talks.is_empty())
        .map(|(index, _)| index)
        .collect();
    for index in with_talks {
        for lapsed in lapse_overdue_counters(&mut game.players[index], today) {
            notify_talk_ended(game, index, &lapsed, today, None);
        }
        resolve_due_talks(game, index, today);
        prune_closed_talks(&mut game.players[index], today);
    }
    rivals::generate_rival_interest(game, today);
}

fn prune_closed_talks(player: &mut Player, today: NaiveDate) {
    player.morale_core.free_agent_talks.retain(|talk| {
        talk.is_open()
            || talk
                .closed_on
                .as_deref()
                .and_then(parse_contract_date)
                .is_none_or(|closed| (today - closed).num_days() <= CLOSED_TALK_RETENTION_DAYS)
    });
}

/// One player's answer, if one is due: he signs with the club whose offer he likes best, or each
/// offer that is due is answered on its own.
fn resolve_due_talks(game: &mut Game, index: usize, today: NaiveDate) {
    let day = format_date(today);
    let player = &game.players[index];
    if player.team_id.is_some() || player.retired {
        let player = &mut game.players[index];
        for talk in player
            .morale_core
            .free_agent_talks
            .iter_mut()
            .filter(|talk| talk.is_open())
        {
            close_talk(talk, FreeAgentTalkStatus::Lapsed, None, today);
        }
        return;
    }
    let is_due = |talk: &FreeAgentTalk| {
        talk.status == FreeAgentTalkStatus::AwaitingResponse
            && talk
                .respond_on
                .as_deref()
                .is_some_and(|due| due <= day.as_str())
    };
    if !player.morale_core.free_agent_talks.iter().any(is_due) {
        return;
    }

    // Every offer on his desk, best first: the player weighs them together.
    let mut ranked: Vec<(String, Appeal)> = player
        .morale_core
        .free_agent_talks
        .iter()
        .filter(|talk| {
            talk.status == FreeAgentTalkStatus::AwaitingResponse && talk.opened_on <= day
        })
        .filter_map(|talk| {
            let team = game.teams.iter().find(|team| team.id == talk.club_id)?;
            let squad = ClubSquad::of(&game.players, &team.id);
            let terms = terms_of(talk);
            let facts = facts_for(player, team, &squad, &terms, today);
            Some((talk.id.clone(), appeal_of(&facts, &terms)))
        })
        .collect();
    ranked.sort_by(|(left_id, left), (right_id, right)| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left_id.cmp(right_id))
    });

    for (talk_id, appeal) in &ranked {
        if appeal.verdict() != Verdict::Accept {
            break;
        }
        let Some(talk) = game.players[index]
            .morale_core
            .free_agent_talks
            .iter()
            .find(|talk| &talk.id == talk_id)
            .cloned()
        else {
            continue;
        };
        match complete_signing(
            game,
            index,
            &talk.club_id,
            &terms_of(&talk),
            talk.made_by_manager,
            today,
        ) {
            Ok(()) => return,
            Err(_) => {
                // The club can no longer pay what it offered: the offer lapses, the next is weighed.
                if let Some(talk) = game.players[index]
                    .morale_core
                    .free_agent_talks
                    .iter_mut()
                    .find(|talk| &talk.id == talk_id)
                {
                    close_talk(
                        talk,
                        FreeAgentTalkStatus::Lapsed,
                        Some(FreeAgentTalkReason::ClubCannotAfford),
                        today,
                    );
                    let ended = talk.clone();
                    notify_talk_ended(game, index, &ended, today, None);
                }
            }
        }
    }

    answer_due_offers(game, index, &ranked, today);
}

/// Nobody signed: each offer whose answer is due gets its own — a counter or a refusal.
fn answer_due_offers(game: &mut Game, index: usize, ranked: &[(String, Appeal)], today: NaiveDate) {
    let day = format_date(today);
    for (talk_id, appeal) in ranked {
        let Some(position) = game.players[index]
            .morale_core
            .free_agent_talks
            .iter()
            .position(|talk| {
                &talk.id == talk_id
                    && talk.status == FreeAgentTalkStatus::AwaitingResponse
                    && talk
                        .respond_on
                        .as_deref()
                        .is_some_and(|due| due <= day.as_str())
            })
        else {
            continue;
        };
        let talk = game.players[index].morale_core.free_agent_talks[position].clone();
        let counter = (appeal.verdict() == Verdict::Counter && talk.round < MAX_ROUNDS)
            .then(|| counter_for(game, index, &talk, today))
            .flatten();
        let ended = {
            let stored = &mut game.players[index].morale_core.free_agent_talks[position];
            match counter {
                Some((wage, years)) => {
                    stored.status = FreeAgentTalkStatus::CounterOffered;
                    stored.counter_wage = Some(wage);
                    stored.counter_years = Some(years);
                    stored.respond_on = None;
                    stored.reason = Some(appeal.main_obstacle());
                    stored.updated_on = day.clone();
                    stored.counter_expires_on =
                        Some(format_date(today + chrono::Days::new(COUNTER_STANDS_DAYS)));
                    None
                }
                None => {
                    // Out of patience only if he was willing to counter and the rounds ran out;
                    // a counter no terms could satisfy is a refusal for the reason that blocks it.
                    let reason = if appeal.verdict() == Verdict::Counter && talk.round >= MAX_ROUNDS
                    {
                        FreeAgentTalkReason::TalksBrokenOff
                    } else {
                        appeal.main_obstacle()
                    };
                    close_talk(stored, FreeAgentTalkStatus::Rejected, Some(reason), today);
                    Some(())
                }
            }
        };
        let talk = game.players[index].morale_core.free_agent_talks[position].clone();
        if ended.is_some() {
            notify_talk_ended(game, index, &talk, today, None);
        } else {
            notify_counter(game, index, &talk, today);
        }
    }
}

/// The terms he would sign on instead of the club's offer, if there are any.
fn counter_for(
    game: &Game,
    index: usize,
    talk: &FreeAgentTalk,
    today: NaiveDate,
) -> Option<(u32, u32)> {
    let team = game.teams.iter().find(|team| team.id == talk.club_id)?;
    let squad = ClubSquad::of(&game.players, &team.id);
    let terms = terms_of(talk);
    let facts = facts_for(&game.players[index], team, &squad, &terms, today);
    counter_terms(&facts, &terms)
}

/// Signs the player to `club_id` on `terms`, after checking the club can still pay for it and has
/// a place for him. Everything is checked before anything is changed.
fn complete_signing(
    game: &mut Game,
    index: usize,
    club_id: &str,
    terms: &OfferTerms,
    made_by_manager: bool,
    today: NaiveDate,
) -> Result<(), String> {
    let team = game
        .teams
        .iter()
        .find(|team| team.id == club_id)
        .cloned()
        .ok_or("be.error.teamNotFound".to_string())?;
    let player = game.players[index].clone();
    ensure_room_in_the_squad(game, club_id)?;
    validate_affordable(game, &team, &player, terms, today)?;
    if !made_by_manager && !rivals::club_can_carry_wage(game, &team, terms.weekly_wage) {
        return Err(renewal_wage_policy_error_message(&team));
    }

    sign_free_agent(
        game,
        index,
        &team,
        terms.weekly_wage,
        terms.contract_years,
        today,
    )?;
    if terms.signing_bonus > 0 {
        let bonus = i64::try_from(terms.signing_bonus)
            .map_err(|_| "be.error.finance.amountOverflow".to_string())?;
        crate::finances::post_all(
            game,
            &[crate::finances::PostRequest::new(
                &team.id,
                -bonus,
                crate::finances::CashKind::SigningBonus,
                today,
            )],
        )?;
        if let Some(club) = game.teams.iter_mut().find(|club| club.id == team.id) {
            club.transfer_budget -= bonus;
        }
    }
    if terms.promise_stepping_stone {
        game.players[index].morale_core.stepping_stone_promise =
            crate::transfers::make_promise(&team, today);
    }
    if made_by_manager {
        let player = &game.players[index];
        game.messages.push(free_agent_signed_message(
            &player.id,
            &player.full_name,
            &team.name,
            terms.contract_years,
            &format_date(today),
        ));
    }
    Ok(())
}

/// Closes the talks of a player who has just signed with `team`: the club's own ends in a
/// signing, every other club's is lost, and a manager whose offer lost is told by whom.
pub(crate) fn settle_talks_on_signing(
    game: &mut Game,
    index: usize,
    team: &Team,
    today: NaiveDate,
) {
    let mut lost_by_manager = Vec::new();
    for talk in &mut game.players[index].morale_core.free_agent_talks {
        if !talk.is_open() {
            continue;
        }
        if talk.club_id == team.id {
            close_talk(talk, FreeAgentTalkStatus::Signed, None, today);
        } else {
            close_talk(
                talk,
                FreeAgentTalkStatus::LostToRival,
                Some(FreeAgentTalkReason::BetterOfferElsewhere),
                today,
            );
            if talk.made_by_manager {
                lost_by_manager.push(talk.clone());
            }
        }
    }
    for talk in lost_by_manager {
        notify_talk_ended(game, index, &talk, today, Some(&team.name));
    }
}

fn talk_message(
    kind: &str,
    player: &Player,
    talk: &FreeAgentTalk,
    team_name: &str,
    today: NaiveDate,
    extra: &[(&str, String)],
) -> InboxMessage {
    let mut params = HashMap::new();
    params.insert("player".to_string(), player.full_name.clone());
    params.insert("team".to_string(), team_name.to_string());
    for (key, value) in extra {
        params.insert((*key).to_string(), value.clone());
    }
    InboxMessage::new(
        format!(
            "free_agent_talk_{kind}_{}_{}_{}",
            player.id,
            talk.round,
            format_date(today)
        ),
        String::new(),
        String::new(),
        String::new(),
        format_date(today),
    )
    .with_category(MessageCategory::Contract)
    .with_priority(MessagePriority::High)
    .with_sender_role("")
    .with_i18n(
        &format!("be.msg.freeAgentTalk.{kind}.subject"),
        &format!("be.msg.freeAgentTalk.{kind}.body"),
        params,
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

/// Tells the manager how one of the club's talks ended. Talks the game makes for other clubs say
/// nothing.
fn notify_talk_ended(
    game: &mut Game,
    index: usize,
    talk: &FreeAgentTalk,
    today: NaiveDate,
    winner_name: Option<&str>,
) {
    if !talk.made_by_manager {
        return;
    }
    let kind = match talk.status {
        FreeAgentTalkStatus::LostToRival => "lost",
        FreeAgentTalkStatus::Lapsed => "lapsed",
        _ => "rejected",
    };
    let club_name = game.team_name_or_id(&talk.club_id);
    let team_name = winner_name.map_or(club_name, str::to_string);
    let message = talk_message(kind, &game.players[index], talk, &team_name, today, &[]);
    game.messages.push(message);
}

fn notify_counter(game: &mut Game, index: usize, talk: &FreeAgentTalk, today: NaiveDate) {
    if !talk.made_by_manager {
        return;
    }
    let club_name = game.team_name_or_id(&talk.club_id);
    let extra = [
        ("wage", talk.counter_wage.unwrap_or(0).to_string()),
        ("years", talk.counter_years.unwrap_or(0).to_string()),
        ("days", COUNTER_STANDS_DAYS.to_string()),
    ];
    let message = talk_message(
        "counter",
        &game.players[index],
        talk,
        &club_name,
        today,
        &extra,
    );
    game.messages.push(message);
}
