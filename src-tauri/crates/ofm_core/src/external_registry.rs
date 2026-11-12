//! Recruiting a player from the external registry into the active world.
//!
//! The registry itself is built and queried by `fm24-adapter`; this module owns the one rule that
//! must not live in an adapter: how a source player becomes a real `Game` player. A promotion is
//! deliberately narrow.
//!
//! * The source person id is the identity. If the active game already has that id — on a roster or
//!   in the retired archive — the promotion is refused, so a player can never be two people.
//! * A player without a club, or whose source contract has expired, is a free agent and signs on the
//!   club's standard terms. A player still under contract at an external club is bought at the
//!   club's own request: the D7B1 asking price, raised to the deterministic floor the club puts on
//!   a player important to it. The one-click screen offers that request, so the player is expensive
//!   rather than impossible; an explicit lower offer can be countered or rejected before any money
//!   moves.
//! * Everything (identity, window, cash, wage policy) is checked before the first mutation, because
//!   `StateManager::update_game` cannot roll a closure back.
//! * The source attributes, positions, foot and value are copied unchanged. Only the contract is
//!   new, and it is written through the same ledger a free signing or a transfer uses.

use crate::contract_wage_policy::{BuyerWageFacts, renewal_wage_policy_error_message};
use crate::contracts::{
    ACCEPT_AT, ClubSquad, Incumbent, MAX_CONTRACT_YEARS, OfferTerms as AppealTerms, Verdict,
    appeal_of, club_standing, contract_entry, contract_record, counter_terms,
    expected_contract_years, expected_wage, facts_for, free_agent_signed_message, sign_free_agent,
    standard_contract_terms,
};
use crate::external_world::{
    ExternalOfferContext, ExternalOfferDecision, expected_ovr_for_reputation,
};
use crate::game::Game;
use crate::transfers::{
    ClubView, ERR_PLAYER_REFUSES_MOVE, FeeBlock, fee_block_after_pending, log_completed_transfer,
    pending_incoming_purchases, push_to_leave, senior_place_refusal, transfer_window_is_open,
};
use chrono::{Datelike, Months, NaiveDate};
use domain::contract_ledger::{ContractRecord, ContractSource};
use domain::free_agent_talk::FreeAgentTalkReason;
use domain::league::CompletedTransfer;
use domain::player::{
    Footedness, Player, PlayerAttributes, PlayerMovementEntry, PlayerMovementKind, Position,
};
use domain::team::Team;

pub const ERR_PLAYER_ALREADY_IN_WORLD: &str = "be.error.registry.playerAlreadyInWorld";
pub const ERR_PLAYER_NOT_AVAILABLE: &str = "be.error.registry.playerNotAvailable";
pub const ERR_PLAYER_INCOMPLETE: &str = "be.error.registry.playerIncomplete";
/// The external club refused the buyer's offer as too low. No money moves.
pub const ERR_OFFER_REJECTED: &str = "be.error.registry.offerRejected";
pub const ERR_WINDOW_CLOSED: &str = "be.error.transfers.transferWindowClosed";
pub const ERR_NO_TEAM_ASSIGNED: &str = "be.error.noTeamAssigned";
pub const ERR_MANAGED_TEAM_NOT_FOUND: &str = "be.error.managedTeamNotFound";
pub const ERR_CONTRACT_END: &str = "be.error.contracts.unableToCalculateContractEnd";
/// A negotiation attempt that did not end in agreement: a counter is not a signature, and the
/// caller must put the countered terms back on the table.
pub const ERR_NEGOTIATION_NOT_AGREED: &str = "be.error.registry.negotiationNotAgreed";
/// The manager's contract terms are outside what a contract may carry.
pub const ERR_INVALID_NEGOTIATION_TERMS: &str = "be.error.registry.invalidOffer";

/// Where a negotiation stands. The interface shows one of these and never has to infer it from
/// the numbers.
pub const NEGOTIATION_AGREED: &str = "agreed";
/// The selling club wants a higher fee; no deal exists until the buyer offers it.
pub const NEGOTIATION_CLUB_COUNTERED: &str = "club_countered";
/// The selling club refused the fee as an insult.
pub const NEGOTIATION_CLUB_REJECTED: &str = "club_rejected";
/// The player wants a different wage or length.
pub const NEGOTIATION_PLAYER_COUNTERED: &str = "player_countered";
/// The player will not move, whatever the money.
pub const NEGOTIATION_PLAYER_REFUSED: &str = "player_refused";
/// A permanent or financial rule blocks the move before either party answers.
pub const NEGOTIATION_BLOCKED: &str = "blocked";

/// What the manager puts on the table for one external player. Every field is optional: `None`
/// means the buyer's standard term for it, which keeps the one-click path a real offer rather than
/// a hidden shortcut. `#[serde(default)]` keeps an older client's payload valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalNegotiationRequest {
    /// The fee offered to the selling club. `None` offers the club's own request.
    #[serde(default)]
    pub fee: Option<u64>,
    /// The weekly wage offered to the player. `None` offers the buyer's standard terms.
    #[serde(default)]
    pub weekly_wage: Option<u32>,
    /// Contract length in years. `None` uses the standard length for his age.
    #[serde(default)]
    pub contract_years: Option<u32>,
    /// Paid to the player on completion, on top of the fee.
    #[serde(default)]
    pub signing_bonus: Option<u64>,
}

/// One complete answer to a negotiation attempt: both parties' decisions, the terms that would
/// end the negotiation if the manager accepts them, and every check the move still has to pass.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalNegotiationOutcome {
    pub player_id: String,
    pub free_agent: bool,
    /// One of the `NEGOTIATION_*` constants.
    pub status: String,
    /// The fee the buyer must pay to complete, or the club's counter when it countered.
    pub fee: u64,
    /// The club's own floor for this player, whatever was offered.
    pub minimum_fee: u64,
    /// `accepted`, `countered` or `rejected`, as the selling club answered.
    pub club_decision: String,
    /// The wage that would complete the move: the manager's, or the player's ask after a counter.
    pub weekly_wage: u32,
    /// The length that would complete the move.
    pub contract_years: u32,
    pub signing_bonus: u64,
    pub contract_end: String,
    /// `accepts`, `counters` or `refuses`, as the player answered.
    pub player_verdict: String,
    /// The player's explainable score for the terms; negative means he is not convinced.
    pub player_score: i32,
    /// The term that hurt his answer most, when he did not accept.
    pub main_obstacle: Option<FreeAgentTalkReason>,
    pub can_afford_fee: bool,
    pub wage_policy_allows: bool,
    pub can_afford_bonus: bool,
    pub window_open: bool,
    /// The translation key that blocks the move, if any.
    pub blocker: Option<String>,
    pub from_club: Option<String>,
}

/// The oldest age at which the registry still considers an external player available. It matches
/// the retirement projection the registry query applies, so a player who has disappeared from
/// search cannot be recruited through a stale id either.
pub const MAX_EXTERNAL_AGE: i32 = 37;

/// A club outside the active world. It is deliberately not a `Team`: the registry does not
/// simulate its competition, squad or finances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalClub {
    /// Stable id, `fm-c-<internal club id>`.
    pub id: String,
    pub name: String,
    /// The contract the source records at this club, if any.
    pub contract_end: Option<String>,
    pub weekly_wage: u32,
    /// The source club's standing on its own 0..=1000 scale. Used by the career projection to
    /// decide whether a player matters to the club and which clubs suit his level.
    pub reputation: i64,
}

/// One source player, before he enters the active world.
#[derive(Debug, Clone)]
pub struct ExternalPlayer {
    pub id: String,
    pub match_name: String,
    pub full_name: String,
    pub date_of_birth: String,
    /// OFM nation code mapped from the source nation name.
    pub nationality: String,
    pub position: Position,
    pub natural_position: Position,
    pub alternate_positions: Vec<Position>,
    pub footedness: Footedness,
    pub weak_foot: u8,
    pub attributes: PlayerAttributes,
    pub ovr: u8,
    pub potential: u8,
    pub market_value: u64,
    pub club: Option<ExternalClub>,
}

impl ExternalPlayer {
    /// The player as the active world stores him, before any contract exists. The source contract
    /// is recorded as an `InitialContract` entry so his wage expectation is his real wage rather
    /// than a number derived from his value, and so the club he came from survives in the ledger.
    pub fn to_player(&self, today: NaiveDate) -> Player {
        let mut player = Player::new(
            self.id.clone(),
            self.match_name.clone(),
            self.full_name.clone(),
            self.date_of_birth.clone(),
            self.nationality.clone(),
            self.position.clone(),
            self.attributes.clone(),
        );
        player.natural_position = self.natural_position.clone();
        player.alternate_positions = self.alternate_positions.clone();
        player.footedness = self.footedness;
        player.weak_foot = self.weak_foot;
        player.potential = self.potential;
        player.market_value = self.market_value;

        if let Some(club) = &self.club {
            let start = None;
            let end = club.contract_end.clone();
            let record = ContractRecord {
                start,
                end: end.clone(),
                weekly_wage: club.weekly_wage,
                source: ContractSource::Initial,
            };
            let entry = PlayerMovementEntry {
                to_team_id: Some(club.id.clone()),
                to_team_name: Some(club.name.clone()),
                contract: Some(record),
                ..PlayerMovementEntry::new(
                    today.format("%Y-%m-%d").to_string(),
                    PlayerMovementKind::InitialContract,
                )
            };
            // A source contract that ends before it starts is not a contract the ledger can hold;
            // the player still exists, and the promotion will give him a new one.
            if let Err(problem) = player.record_movement(entry) {
                log::debug!(
                    "external registry: source contract for {} was not recorded: {problem:?}",
                    player.id
                );
            }
        }
        player
    }

    /// Whether the source has him out of contract on `today`.
    pub fn is_free_agent(&self, today: NaiveDate) -> bool {
        match &self.club {
            None => true,
            Some(club) => club
                .contract_end
                .as_deref()
                .and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
                .is_some_and(|end| end <= today),
        }
    }

    pub fn age_on(&self, today: NaiveDate) -> Option<i32> {
        domain::age::age_on(today, &self.date_of_birth)
    }
}

/// The price an external club asks for a player, in euros.
///
/// Zero for a free agent or an expired contract. Otherwise the source value multiplied by the
/// contract left: 1.0x under six months, 1.2x under a year, 1.5x under two years, 1.75x beyond. An
/// unknown end is treated as two years plus, because the source cannot prove it is short. The
/// result is rounded to the nearest thousand and never below 1 000 for a positive value.
pub fn asking_price(market_value: u64, contract_end: Option<&str>, today: NaiveDate) -> u64 {
    if market_value == 0 {
        return 0;
    }
    let Some(end) = contract_end.and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
    else {
        return round_price(market_value.saturating_mul(175) / 100);
    };
    if end <= today {
        return 0;
    }
    let days = (end - today).num_days();
    let multiplier = if days <= 183 {
        100
    } else if days <= 365 {
        120
    } else if days <= 730 {
        150
    } else {
        175
    };
    round_price(market_value.saturating_mul(multiplier) / 100)
}

fn round_price(value: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    ((value / 1_000) * 1_000).max(1_000)
}

/// What a promotion would cost and whether it is allowed. The command layer returns this before
/// asking the player to confirm; [`recruit_external_player`] re-computes it and refuses on the same
/// terms, so a stale screen cannot slip a different deal through.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRecruitPreview {
    pub player_id: String,
    pub free_agent: bool,
    pub already_in_world: bool,
    pub fee: u64,
    /// The club's own floor for this player. `fee` is never below it for a contracted player.
    pub minimum_fee: u64,
    /// `accepted`, `countered` or `rejected`. The default one-click path always offers the club's
    /// request, so it is `accepted`; an explicit lower offer can produce the other two.
    pub club_decision: String,
    pub weekly_wage: u32,
    pub contract_years: u32,
    pub contract_end: String,
    pub can_afford_fee: bool,
    pub wage_policy_allows: bool,
    pub window_open: bool,
    /// The translation key that blocks the move, if any, in the order the recruit path reports it.
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRecruitOutcome {
    pub player_id: String,
    pub free_agent: bool,
    pub fee: u64,
    pub weekly_wage: u32,
    pub contract_end: String,
    pub from_club: Option<String>,
}

fn managed_team(game: &Game) -> Result<&Team, String> {
    let team_id = game
        .manager
        .team_id
        .as_deref()
        .ok_or(ERR_NO_TEAM_ASSIGNED.to_string())?;
    game.teams
        .iter()
        .find(|team| team.id == team_id)
        .ok_or(ERR_MANAGED_TEAM_NOT_FOUND.to_string())
}

fn duplicate_in_world(game: &Game, player_id: &str) -> bool {
    game.players.iter().any(|player| player.id == player_id)
        || game
            .retired_players
            .iter()
            .any(|player| player.id == player_id)
}

/// The standard contract a club would give this player, on its own board's terms.
fn standard_terms(game: &Game, player: &Player, team: &Team) -> Result<(u32, NaiveDate), String> {
    let today = game.clock.current_date.date_naive();
    standard_contract_terms(player, team, today, 0).ok_or(ERR_CONTRACT_END.to_string())
}

/// Whole years left on a source contract, on the career's clock. An unknown end counts as none,
/// which makes the club's valuation treat the player as expiring.
fn contract_years_left(contract_end: Option<&str>, today: NaiveDate) -> u8 {
    let Some(end) = contract_end.and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
    else {
        return 0;
    };
    if end <= today {
        return 0;
    }
    ((end - today).num_days() / 365).clamp(1, 5) as u8
}

pub fn preview_external_recruitment(
    game: &Game,
    external: &ExternalPlayer,
) -> Result<ExternalRecruitPreview, String> {
    preview_external_recruitment_with_offer(game, external, None)
}

/// What the preview shows for the fee: the club's floor, the fee actually payable, its named
/// decision and whether a lowball was refused.
struct FeeTerms {
    fee: u64,
    minimum_fee: u64,
    club_decision: &'static str,
    rejected: bool,
}

/// The pricing inputs of one preview. A plain value keeps the valuation helper to two parameters
/// instead of a long positional list whose order a caller could silently swap.
struct OfferRequest<'a> {
    external: &'a ExternalPlayer,
    today: NaiveDate,
    free_agent: bool,
    asking: u64,
    age: i32,
    buyer_reputation: i64,
    offer: Option<u64>,
}

/// Price one external player. `offer = None` is the one-click request; `Some(offer)` is an explicit
/// buyer's offer, which the club may accept, counter or reject. Pure: everything it reads is passed
/// in, including the career seed and season, so a repeated preview cannot re-roll the club.
fn fee_terms(game: &Game, request: &OfferRequest<'_>) -> FeeTerms {
    let OfferRequest {
        external,
        today,
        free_agent,
        asking,
        age,
        buyer_reputation,
        offer,
    } = *request;
    if free_agent {
        return FeeTerms {
            fee: 0,
            minimum_fee: 0,
            club_decision: "accepted",
            rejected: false,
        };
    }
    let context = ExternalOfferContext {
        seed: game.seed,
        season: today.year(),
        asking_price: asking,
        free_agent,
        player_ovr: external.ovr,
        player_potential: external.potential,
        player_age: age,
        contract_years_left: contract_years_left(
            external
                .club
                .as_ref()
                .and_then(|club| club.contract_end.as_deref()),
            today,
        ),
        seller_reputation: external
            .club
            .as_ref()
            .map(|club| club.reputation)
            .unwrap_or(0),
        buyer_reputation,
        player_id: external.id.clone(),
    };
    let minimum = context.minimum_fee();
    let terms = |fee: u64, decision: &'static str, rejected: bool| FeeTerms {
        fee,
        minimum_fee: minimum,
        club_decision: decision,
        rejected,
    };
    match offer {
        None => terms(asking.max(minimum), "accepted", false),
        Some(offer) => match context.decide(offer) {
            ExternalOfferDecision::Accepted { fee } => terms(fee, "accepted", false),
            ExternalOfferDecision::Countered { fee } => terms(fee, "countered", false),
            ExternalOfferDecision::Rejected { minimum } => terms(minimum, "rejected", true),
        },
    }
}

/// The promotion rule, optionally against an explicit buyer's offer.
///
/// `offer` is `None` for the one-click screen: the club's own request is the price, so an
/// important player is expensive rather than impossible. An explicit offer below 75% of that floor
/// is rejected outright, and no mutation happens; an offer in between is countered at the floor.
pub fn preview_external_recruitment_with_offer(
    game: &Game,
    external: &ExternalPlayer,
    offer: Option<u64>,
) -> Result<ExternalRecruitPreview, String> {
    let team = managed_team(game)?;
    let today = game.clock.current_date.date_naive();
    let player = external.to_player(today);
    let (weekly_wage, contract_end) = standard_terms(game, &player, team)?;
    let contract_years = expected_contract_years(&player, today).max(1);
    let free_agent = external.is_free_agent(today);
    let age = external.age_on(today).unwrap_or(i32::MAX);
    let asking = if free_agent {
        0
    } else {
        asking_price(
            external.market_value,
            external
                .club
                .as_ref()
                .and_then(|club| club.contract_end.as_deref()),
            today,
        )
    };
    let terms = fee_terms(
        game,
        &OfferRequest {
            external,
            today,
            free_agent,
            asking,
            age,
            buyer_reputation: i64::from(team.reputation),
            offer,
        },
    );

    let already_in_world = duplicate_in_world(game, &external.id);
    // Both limits are read against what the club has already promised: a deal agreed and waiting
    // to register holds its fee and its wage, and this signing completes at once, so it is the one
    // that could spend them a second time. See `transfers::pending_fee_commitments`.
    let fee_check = fee_block_after_pending(game, team, terms.fee);
    let wage_policy_allows = BuyerWageFacts::of_committed(game, &team.id)
        .purchase_verdict(team, &player, weekly_wage)
        .permits();
    let window_open = free_agent || transfer_window_is_open(game);

    let blocker = if already_in_world {
        Some(ERR_PLAYER_ALREADY_IN_WORLD.to_string())
    } else {
        identity_blocker(external, age, window_open)
            .or_else(|| terms.rejected.then(|| ERR_OFFER_REJECTED.to_string()))
            .or_else(|| squad_blocker(game, team, &external.id, free_agent))
            .or_else(|| finance_blocker(team, fee_check, wage_policy_allows))
    };

    Ok(ExternalRecruitPreview {
        player_id: external.id.clone(),
        free_agent,
        already_in_world,
        fee: terms.fee,
        minimum_fee: terms.minimum_fee,
        club_decision: terms.club_decision.to_string(),
        weekly_wage,
        contract_years,
        contract_end: contract_end.format("%Y-%m-%d").to_string(),
        can_afford_fee: fee_check.is_ok(),
        wage_policy_allows,
        window_open,
        blocker,
    })
}

/// Everything both parties decided, before the permanent and financial rules are folded in.
struct NegotiationFacts {
    today: NaiveDate,
    free_agent: bool,
    age: i32,
    terms: FeeTerms,
    response: PlayerResponse,
    final_wage: u32,
    final_years: u32,
    signing_bonus: u64,
    window_open: bool,
    can_afford_fee: bool,
    wage_policy_allows: bool,
    can_afford_bonus: bool,
    already_in_world: bool,
    bonus_blocker: Option<String>,
    squad_blocker: Option<String>,
    finance_blocker: Option<String>,
}

/// The whole negotiation, with no mutation: the selling club's answer on the fee, the player's
/// answer on the contract, and every permanent or financial rule that would stop the deal. The
/// same inputs always produce the same answer, so previewing twice cannot re-roll either party.
///
/// A counter is not an agreement. The manager accepts it by sending the countered terms back as a
/// new request; only [`finalize_external_transfer`] ever changes the game.
pub fn negotiate_external_transfer(
    game: &Game,
    external: &ExternalPlayer,
    request: ExternalNegotiationRequest,
) -> Result<ExternalNegotiationOutcome, String> {
    let facts = negotiation_facts(game, external, request)?;
    let (status, blocker) = judge_negotiation(external, &facts);
    outcome_from(external, facts, status, blocker)
}

/// Ask both parties and run every check. Pure, and split out so the public function reads as the
/// negotiation it is rather than as a list of calculations.
fn negotiation_facts(
    game: &Game,
    external: &ExternalPlayer,
    request: ExternalNegotiationRequest,
) -> Result<NegotiationFacts, String> {
    let team = managed_team(game)?;
    let today = game.clock.current_date.date_naive();
    let player = external.to_player(today);
    let (standard_wage, _) = standard_terms(game, &player, team)?;
    let standard_years = expected_contract_years(&player, today).max(1);
    let offered_wage = request
        .weekly_wage
        .filter(|wage| *wage > 0)
        .unwrap_or(standard_wage);
    let offered_years = request
        .contract_years
        .filter(|years| *years > 0)
        .unwrap_or(standard_years);
    let signing_bonus = request.signing_bonus.unwrap_or(0);
    validate_negotiation_terms(offered_wage, offered_years, signing_bonus)?;

    let free_agent = external.is_free_agent(today);
    let age = external.age_on(today).unwrap_or(i32::MAX);
    let asking = asking_price_of(external, today, free_agent);
    let terms = fee_terms(
        game,
        &OfferRequest {
            external,
            today,
            free_agent,
            asking,
            age,
            buyer_reputation: i64::from(team.reputation),
            offer: request.fee,
        },
    );

    let offer = AppealTerms {
        weekly_wage: offered_wage,
        contract_years: offered_years,
        signing_bonus,
        promise_stepping_stone: false,
    };
    let mut response = player_response(game, external, &player, team, offer, today);
    let club_accepted = terms.club_decision == "accepted";
    let (final_wage, final_years) = if club_accepted && response.verdict == Verdict::Counter {
        match response.counter {
            Some((wage, years)) => (wage, years),
            None => {
                response.verdict = Verdict::Refuse;
                (offered_wage, offered_years)
            }
        }
    } else {
        (offered_wage, offered_years)
    };

    let window_open = free_agent || transfer_window_is_open(game);
    let fee_check = fee_block_after_pending(game, team, terms.fee);
    let wage_policy_allows = BuyerWageFacts::of_committed(game, &team.id)
        .purchase_verdict(team, &player, final_wage)
        .permits();
    let bonus_check = signing_bonus_check(game, team, signing_bonus, today);
    let can_afford_bonus = bonus_check.is_ok();
    Ok(NegotiationFacts {
        today,
        free_agent,
        age,
        can_afford_fee: fee_check.is_ok(),
        finance_blocker: finance_blocker(team, fee_check, wage_policy_allows),
        squad_blocker: squad_blocker(game, team, &external.id, free_agent),
        bonus_blocker: bonus_check.err(),
        can_afford_bonus,
        wage_policy_allows,
        window_open,
        already_in_world: duplicate_in_world(game, &external.id),
        signing_bonus,
        final_wage,
        final_years,
        response,
        terms,
    })
}

/// The one status the screen shows, and the one key that stops the move, from the order the rules
/// already had: a club refusal, then a player refusal, then a permanent/financial block, then a
/// counter, then agreement.
fn judge_negotiation(
    external: &ExternalPlayer,
    facts: &NegotiationFacts,
) -> (String, Option<String>) {
    let club_accepted = facts.terms.club_decision == "accepted";
    let player_refused = club_accepted && facts.response.verdict == Verdict::Refuse;
    let mut blocker = if facts.already_in_world {
        Some(ERR_PLAYER_ALREADY_IN_WORLD.to_string())
    } else {
        identity_blocker(external, facts.age, facts.window_open)
    };
    if blocker.is_none() && facts.terms.rejected {
        blocker = Some(ERR_OFFER_REJECTED.to_string());
    }
    if blocker.is_none() {
        blocker.clone_from(&facts.squad_blocker);
    }
    if blocker.is_none() {
        blocker = facts
            .finance_blocker
            .clone()
            .or_else(|| facts.bonus_blocker.clone());
    }
    if blocker.is_none() && player_refused {
        blocker = Some(ERR_PLAYER_REFUSES_MOVE.to_string());
    }

    let status = if facts.terms.rejected {
        NEGOTIATION_CLUB_REJECTED
    } else if player_refused {
        NEGOTIATION_PLAYER_REFUSED
    } else if blocker.is_some() {
        NEGOTIATION_BLOCKED
    } else if facts.terms.club_decision == "countered" {
        NEGOTIATION_CLUB_COUNTERED
    } else {
        match facts.response.verdict {
            Verdict::Accept => NEGOTIATION_AGREED,
            Verdict::Counter => NEGOTIATION_PLAYER_COUNTERED,
            Verdict::Refuse => NEGOTIATION_PLAYER_REFUSED,
        }
    };
    (status.to_string(), blocker)
}

fn outcome_from(
    external: &ExternalPlayer,
    facts: NegotiationFacts,
    status: String,
    blocker: Option<String>,
) -> Result<ExternalNegotiationOutcome, String> {
    let contract_end = contract_end_after(facts.today, facts.final_years)?
        .format("%Y-%m-%d")
        .to_string();
    Ok(ExternalNegotiationOutcome {
        player_id: external.id.clone(),
        free_agent: facts.free_agent,
        status,
        fee: facts.terms.fee,
        minimum_fee: facts.terms.minimum_fee,
        club_decision: facts.terms.club_decision.to_string(),
        weekly_wage: facts.final_wage,
        contract_years: facts.final_years,
        signing_bonus: facts.signing_bonus,
        contract_end,
        player_verdict: verdict_name(facts.response.verdict).to_string(),
        player_score: facts.response.score,
        main_obstacle: facts.response.main_obstacle,
        can_afford_fee: facts.can_afford_fee,
        wage_policy_allows: facts.wage_policy_allows,
        can_afford_bonus: facts.can_afford_bonus,
        window_open: facts.window_open,
        blocker,
        from_club: external.club.as_ref().map(|club| club.name.clone()),
    })
}

/// Complete a negotiation that ended in agreement. Re-runs the negotiation and refuses anything
/// that is not `agreed`, so a counter, a refusal or a stale screen cannot sign a player.
pub fn finalize_external_transfer(
    game: &mut Game,
    external: &ExternalPlayer,
    request: ExternalNegotiationRequest,
) -> Result<ExternalRecruitOutcome, String> {
    let outcome = negotiate_external_transfer(game, external, request)?;
    if let Some(blocker) = &outcome.blocker {
        return Err(blocker.clone());
    }
    if outcome.status != NEGOTIATION_AGREED {
        return Err(ERR_NEGOTIATION_NOT_AGREED.to_string());
    }
    complete_external_transfer(game, external, &outcome)
}

fn validate_negotiation_terms(weekly_wage: u32, years: u32, bonus: u64) -> Result<(), String> {
    let weeks = 52 * u64::from(years.max(1));
    if weekly_wage == 0
        || years == 0
        || years > MAX_CONTRACT_YEARS
        || bonus > u64::from(weekly_wage) * weeks
    {
        return Err(ERR_INVALID_NEGOTIATION_TERMS.to_string());
    }
    Ok(())
}

fn contract_end_after(today: NaiveDate, years: u32) -> Result<NaiveDate, String> {
    today
        .checked_add_months(Months::new(years * 12))
        .ok_or_else(|| ERR_CONTRACT_END.to_string())
}

fn asking_price_of(external: &ExternalPlayer, today: NaiveDate, free_agent: bool) -> u64 {
    if free_agent {
        return 0;
    }
    asking_price(
        external.market_value,
        external
            .club
            .as_ref()
            .and_then(|club| club.contract_end.as_deref()),
        today,
    )
}

/// A selling club's side as the player sees it. The registry does not simulate a squad, so the
/// shape is derived from the club's reputation: enough seniors per group that "key player",
/// "starter" and "backup" mean something.
fn external_club_squad(club: &ExternalClub) -> ClubSquad {
    let level = expected_ovr_for_reputation(club.reputation).clamp(0, 100) as u8;
    let group = |offset: i32, count: i32| -> Vec<u8> {
        (0..count)
            .map(|index| (i32::from(level) + offset - index).clamp(0, 100) as u8)
            .collect()
    };
    ClubSquad::from_levels(&[group(3, 3), group(2, 6), group(2, 6), group(2, 4)])
}

struct PlayerResponse {
    verdict: Verdict,
    score: i32,
    main_obstacle: Option<FreeAgentTalkReason>,
    counter: Option<(u32, u32)>,
}

/// The player's answer, through P5's appeal score: the same one a free agent or a contracted
/// player weighs an offer by. An external player has no simulated club squad, so his incumbent
/// side is the shape [`external_club_squad`] derives from the selling club's reputation.
fn player_response(
    game: &Game,
    external: &ExternalPlayer,
    player: &Player,
    team: &Team,
    offer: AppealTerms,
    today: NaiveDate,
) -> PlayerResponse {
    let buyer_squad = ClubSquad::of(&game.players, &team.id);
    let mut facts = facts_for(player, team, &buyer_squad, &offer, today);
    if let Some(club) = &external.club {
        let reputation = club.reputation.clamp(0, 1000) as u32;
        let seller_squad = external_club_squad(club);
        facts.incumbent = Some(Incumbent {
            club_standing: club_standing(reputation, seller_squad.level()),
            place: seller_squad.place_among_others(player),
            push: push_to_leave(
                player,
                ClubView {
                    reputation,
                    in_debt: false,
                },
                today,
            ),
        });
    }
    let appeal = appeal_of(&facts, &offer);
    let verdict = appeal.verdict();
    PlayerResponse {
        verdict,
        score: appeal.score,
        main_obstacle: (appeal.score < ACCEPT_AT).then(|| appeal.main_obstacle()),
        counter: (verdict != Verdict::Accept)
            .then(|| counter_terms(&facts, &offer))
            .flatten(),
    }
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Accept => "accepts",
        Verdict::Counter => "counters",
        Verdict::Refuse => "refuses",
    }
}

fn signing_bonus_check(
    game: &Game,
    team: &Team,
    bonus: u64,
    today: NaiveDate,
) -> Result<(), String> {
    if bonus == 0 {
        return Ok(());
    }
    crate::transfers::ensure_bonus_affordable(team, bonus)?;
    let amount = i64::try_from(bonus).map_err(|_| "be.error.finance.amountOverflow".to_string())?;
    crate::finances::validate_posts(
        game,
        &[crate::finances::PostRequest::new(
            &team.id,
            -amount,
            crate::finances::CashKind::SigningBonus,
            today,
        )],
    )
}

fn pay_signing_bonus(
    game: &mut Game,
    team_id: &str,
    bonus: u64,
    today: NaiveDate,
) -> Result<(), String> {
    if bonus == 0 {
        return Ok(());
    }
    let amount = i64::try_from(bonus).map_err(|_| "be.error.finance.amountOverflow".to_string())?;
    crate::finances::post_all(
        game,
        &[crate::finances::PostRequest::new(
            team_id,
            -amount,
            crate::finances::CashKind::SigningBonus,
            today,
        )],
    )?;
    if let Some(club) = game.teams.iter_mut().find(|club| club.id == team_id) {
        club.transfer_budget -= amount;
    }
    Ok(())
}

/// Identity, record completeness, age and window: the refusals that do not depend on money. Order
/// matters — a stale screen must not hide a permanent refusal behind a price.
fn identity_blocker(external: &ExternalPlayer, age: i32, window_open: bool) -> Option<String> {
    if external.full_name.trim().is_empty()
        || external.nationality.trim().is_empty()
        || external.date_of_birth.trim().is_empty()
    {
        Some(ERR_PLAYER_INCOMPLETE.to_string())
    } else if !(14..=MAX_EXTERNAL_AGE).contains(&age) {
        Some(ERR_PLAYER_NOT_AVAILABLE.to_string())
    } else if !window_open {
        Some(ERR_WINDOW_CLOSED.to_string())
    } else {
        None
    }
}

/// The senior registration limit (`contracts::squad_capacity`), which a free agent signing meets
/// and a contracted purchase now meets too. A free agent is judged against the live list; a
/// purchase also counts the places promised to deals already agreed and waiting to register, the
/// same reservation P6 applies to an in-game purchase.
fn squad_blocker(game: &Game, team: &Team, external_id: &str, free_agent: bool) -> Option<String> {
    let promised = if free_agent {
        0
    } else {
        pending_incoming_purchases(game, &team.id, external_id)
    };
    senior_place_refusal(game, &team.id, promised)
}

/// Cash, transfer budget and wage policy. Runs only after the identity refusals.
fn finance_blocker(
    team: &Team,
    fee_check: Result<(), FeeBlock>,
    wage_policy_allows: bool,
) -> Option<String> {
    if let Err(block) = fee_check {
        Some(block.error_key().to_string())
    } else if !wage_policy_allows {
        Some(renewal_wage_policy_error_message(team))
    } else {
        None
    }
}

/// Promote `external` into the active game for the manager's club at the club's own request and
/// on the buyer's standard contract terms. This is the one-click path: it is a real negotiation
/// with no explicit counter, so it signs only when both parties accept.
pub fn recruit_external_player(
    game: &mut Game,
    external: &ExternalPlayer,
) -> Result<ExternalRecruitOutcome, String> {
    recruit_external_player_at_offer(game, external, None)
}

/// Promote `external` against an explicit fee, on standard contract terms. A rejected or countered
/// offer returns before the first mutation; a counter is a next step for the manager, not a
/// signature. Use [`finalize_external_transfer`] to accept a counter or to set the player's terms.
pub fn recruit_external_player_at_offer(
    game: &mut Game,
    external: &ExternalPlayer,
    offer: Option<u64>,
) -> Result<ExternalRecruitOutcome, String> {
    finalize_external_transfer(
        game,
        external,
        ExternalNegotiationRequest {
            fee: offer,
            ..ExternalNegotiationRequest::default()
        },
    )
}

/// Write an agreed negotiation into the world. Every check already passed in
/// [`negotiate_external_transfer`]; this is the only function on the external route that mutates.
fn complete_external_transfer(
    game: &mut Game,
    external: &ExternalPlayer,
    outcome: &ExternalNegotiationOutcome,
) -> Result<ExternalRecruitOutcome, String> {
    let today = game.clock.current_date.date_naive();
    let team = managed_team(game)?.clone();
    let mut player = external.to_player(today);
    crate::player_rating::refresh_player_derived(&mut player, today);
    if external.potential > 0 {
        player.potential = player.potential.max(external.potential).max(player.ovr);
    }
    game.players.push(player);
    let index = game.players.len() - 1;

    if outcome.free_agent {
        return complete_free_agent_transfer(game, external, outcome, &team, today, index);
    }
    complete_contracted_transfer(game, external, outcome, &team, today, index)
}

fn complete_free_agent_transfer(
    game: &mut Game,
    external: &ExternalPlayer,
    outcome: &ExternalNegotiationOutcome,
    team: &Team,
    today: NaiveDate,
    index: usize,
) -> Result<ExternalRecruitOutcome, String> {
    sign_free_agent(
        game,
        index,
        team,
        outcome.weekly_wage,
        outcome.contract_years,
        today,
    )?;
    pay_signing_bonus(game, &team.id, outcome.signing_bonus, today)?;
    let signed = &game.players[index];
    game.messages.push(free_agent_signed_message(
        &signed.id,
        &signed.full_name,
        &team.name,
        outcome.contract_years,
        &today.format("%Y-%m-%d").to_string(),
    ));
    Ok(ExternalRecruitOutcome {
        player_id: external.id.clone(),
        free_agent: true,
        fee: 0,
        weekly_wage: outcome.weekly_wage,
        contract_end: outcome.contract_end.clone(),
        from_club: None,
    })
}

fn complete_contracted_transfer(
    game: &mut Game,
    external: &ExternalPlayer,
    outcome: &ExternalNegotiationOutcome,
    team: &Team,
    today: NaiveDate,
    index: usize,
) -> Result<ExternalRecruitOutcome, String> {
    let fee = outcome.fee;
    let fee_i64 = i64::try_from(fee).map_err(|_| "be.error.finance.amountOverflow".to_string())?;
    crate::finances::post_all(
        game,
        &[crate::finances::PostRequest::new(
            &team.id,
            -fee_i64,
            crate::finances::CashKind::TransferFeeOut,
            today,
        )],
    )?;
    pay_signing_bonus(game, &team.id, outcome.signing_bonus, today)?;

    let from_club = external.club.clone().ok_or(ERR_PLAYER_NOT_AVAILABLE)?;
    let contract_end = NaiveDate::parse_from_str(&outcome.contract_end, "%Y-%m-%d")
        .map_err(|_| ERR_CONTRACT_END.to_string())?;
    let jersey = {
        let candidate = &game.players[index];
        crate::roster::resolve_jersey_for(game, candidate, team)
    };
    let player = &mut game.players[index];
    player.team_id = Some(team.id.clone());
    player.jersey_number = jersey;
    player.transfer_listed = false;
    player.loan_listed = false;
    player.transfer_offers.clear();
    player.loan_offers.clear();
    let movement = PlayerMovementEntry {
        from_team_id: Some(from_club.id.clone()),
        from_team_name: Some(from_club.name.clone()),
        fee: Some(fee),
        ..contract_entry(
            PlayerMovementKind::PermanentTransfer,
            today,
            team,
            contract_record(
                today,
                contract_end,
                outcome.weekly_wage,
                ContractSource::Transfer,
            ),
        )
    };
    player
        .record_movement(movement)
        .map_err(|_| ERR_CONTRACT_END.to_string())?;

    if let Some(club) = game.teams.iter_mut().find(|club| club.id == team.id) {
        club.transfer_budget -= fee_i64;
    }
    log_completed_transfer(
        game,
        CompletedTransfer {
            date: today.format("%Y-%m-%d").to_string(),
            from_team_id: from_club.id.clone(),
            to_team_id: team.id.clone(),
            player_id: external.id.clone(),
            fee,
        },
    );

    Ok(ExternalRecruitOutcome {
        player_id: external.id.clone(),
        free_agent: false,
        fee,
        weekly_wage: outcome.weekly_wage,
        contract_end: outcome.contract_end.clone(),
        from_club: Some(from_club.name),
    })
}

/// The wage a source player would be offered on his first day, for a preview screen that cannot
/// build the whole player. Kept here so the value shown and the value signed cannot drift.
pub fn expected_external_wage(game: &Game, external: &ExternalPlayer) -> Result<u32, String> {
    let team = managed_team(game)?;
    let today = game.clock.current_date.date_naive();
    let player = external.to_player(today);
    Ok(expected_wage(&player, team, today))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::game::Game;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{SquadRole, TransferOffer, TransferOfferStatus};
    use domain::season::TransferWindowStatus;
    use domain::team::Team;

    fn attrs() -> PlayerAttributes {
        PlayerAttributes {
            pace: 70,
            stamina: 70,
            strength: 70,
            agility: 70,
            passing: 70,
            shooting: 70,
            tackling: 60,
            dribbling: 70,
            defending: 55,
            positioning: 68,
            vision: 70,
            decisions: 70,
            composure: 68,
            aggression: 55,
            teamwork: 70,
            leadership: 60,
            handling: 20,
            reflexes: 20,
            aerial: 55,
        }
    }

    fn team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "Alpha FC".to_string(),
            "ALP".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Alpha Ground".to_string(),
            30_000,
        );
        team.reputation = 600;
        team.wage_budget = 500_000;
        team.finance = 50_000_000;
        team.transfer_budget = 20_000_000;
        team
    }

    fn game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Jane".to_string(),
            "Doe".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());
        let mut game = Game::new(clock, manager, vec![team()], vec![], vec![], vec![]);
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        game
    }

    fn external_free_agent(id: &str) -> ExternalPlayer {
        ExternalPlayer {
            id: id.to_string(),
            match_name: "J. Free".to_string(),
            full_name: "John Free".to_string(),
            date_of_birth: "2000-01-01".to_string(),
            nationality: "GB".to_string(),
            position: Position::Striker,
            natural_position: Position::Striker,
            alternate_positions: vec![Position::RightWinger],
            footedness: Footedness::Right,
            weak_foot: 3,
            attributes: attrs(),
            ovr: 70,
            potential: 80,
            market_value: 2_000_000,
            club: None,
        }
    }

    fn external_contracted(id: &str) -> ExternalPlayer {
        let mut player = external_free_agent(id);
        player.club = Some(ExternalClub {
            id: "fm-c-900".to_string(),
            name: "External United".to_string(),
            contract_end: Some("2028-06-30".to_string()),
            weekly_wage: 8_000,
            reputation: 600,
        });
        player
    }

    fn registered_senior(id: usize) -> Player {
        let mut player = Player::new(
            format!("senior-{id}"),
            format!("Senior {id}"),
            format!("S{id}"),
            "1995-01-01".to_string(),
            "England".to_string(),
            Position::Midfielder,
            attrs(),
        );
        player.team_id = Some("team-1".to_string());
        player.squad_role = SquadRole::Senior;
        player
    }

    fn fill_senior_list(game: &mut Game, seniors: usize) {
        game.players.extend((0..seniors).map(registered_senior));
    }

    /// A player another club has already agreed to sell to `team-1`, waiting to register: he holds
    /// a place the external market must not offer again.
    fn promised_incoming_purchase(game: &mut Game, id: &str) {
        let mut player = registered_senior(999);
        player.id = id.to_string();
        player.team_id = Some("elsewhere".to_string());
        player.transfer_offers.push(TransferOffer {
            id: format!("offer-{id}"),
            from_team_id: "team-1".to_string(),
            fee: 1_000_000,
            wage_offered: 5_000,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            status: TransferOfferStatus::PendingRegistration,
            date: "2026-08-01".to_string(),
            registration_date: Some("2026-08-01".to_string()),
            registration_failure_reason: None,
            closed_on: None,
            terms: Default::default(),
        });
        game.players.push(player);
    }

    /// Given a club with thirty registered seniors, when a contracted external player is
    /// negotiated, then the move is blocked by the same senior-list key a free agent meets and
    /// nothing is signed.
    #[test]
    fn a_contracted_purchase_meets_the_thirty_senior_limit() {
        let mut game = game();
        fill_senior_list(&mut game, 30);
        let players_before = game.players.len();
        let cash_before = game.teams[0].finance;
        let external = external_contracted("fm-p-52");

        let outcome =
            negotiate_external_transfer(&game, &external, ExternalNegotiationRequest::default())
                .unwrap();

        assert_eq!(outcome.status, NEGOTIATION_BLOCKED);
        assert!(
            outcome
                .blocker
                .as_deref()
                .is_some_and(|key| key.starts_with("be.error.freeAgents.squadFull")),
            "blocker was {:?}",
            outcome.blocker
        );

        let error = recruit_external_player(&mut game, &external).unwrap_err();
        assert!(error.starts_with("be.error.freeAgents.squadFull"));
        assert_eq!(game.players.len(), players_before);
        assert_eq!(game.teams[0].finance, cash_before);
    }

    /// Given a club with one senior place left and a purchase already agreed to fill it, when a
    /// contracted external player is negotiated, then the promised place is counted and the move
    /// is refused before anyone signs.
    #[test]
    fn a_contracted_purchase_counts_places_promised_to_agreed_deals() {
        let mut game = game();
        fill_senior_list(&mut game, 29);
        promised_incoming_purchase(&mut game, "agreed-arrival");
        let external = external_contracted("fm-p-53");

        let outcome =
            negotiate_external_transfer(&game, &external, ExternalNegotiationRequest::default())
                .unwrap();

        assert_eq!(outcome.status, NEGOTIATION_BLOCKED);
        assert!(
            outcome
                .blocker
                .as_deref()
                .is_some_and(|key| key.starts_with("be.error.freeAgents.squadFull")),
            "blocker was {:?}",
            outcome.blocker
        );
    }

    /// Given twenty-nine registered seniors and no promised place, when a contracted external
    /// player is negotiated, then the senior-list rule does not block the move.
    #[test]
    fn a_contracted_purchase_is_allowed_into_the_last_senior_place() {
        let mut game = game();
        fill_senior_list(&mut game, 29);
        let external = external_contracted("fm-p-54");

        let outcome =
            negotiate_external_transfer(&game, &external, ExternalNegotiationRequest::default())
                .unwrap();

        assert!(
            outcome.blocker.is_none(),
            "blocker was {:?}",
            outcome.blocker
        );
    }

    /// Given a free agent from the registry, when he is recruited, then he joins on a free-agent
    /// ledger entry, no fee is taken and the source identity is preserved.
    #[test]
    fn a_free_agent_joins_on_the_standard_contract_without_a_fee() {
        let mut game = game();
        let before = game.teams[0].finance;
        let outcome = recruit_external_player(&mut game, &external_free_agent("fm-p-42")).unwrap();

        assert!(outcome.free_agent);
        assert_eq!(outcome.fee, 0);
        assert_eq!(game.teams[0].finance, before);
        let player = game.players.iter().find(|p| p.id == "fm-p-42").unwrap();
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
        assert_eq!(player.natural_position, Position::Striker);
        assert_eq!(player.alternate_positions, vec![Position::RightWinger]);
        assert!(player.movement_history.iter().any(|entry| {
            entry.kind == PlayerMovementKind::FreeAgentSigning
                && entry.to_team_id.as_deref() == Some("team-1")
        }));
    }

    /// Given a player under contract outside the world, when he is bought, then the asking price
    /// leaves the buyer's cash and transfer envelope and the move names the external club.
    #[test]
    fn a_contracted_external_player_is_bought_at_the_asking_price() {
        let mut game = game();
        let cash_before = game.teams[0].finance;
        let budget_before = game.teams[0].transfer_budget;
        let outcome = recruit_external_player(&mut game, &external_contracted("fm-p-43")).unwrap();

        assert!(!outcome.free_agent);
        assert_eq!(
            outcome.fee,
            asking_price(
                2_000_000,
                Some("2028-06-30"),
                game.clock.current_date.date_naive()
            )
        );
        assert_eq!(game.teams[0].finance, cash_before - outcome.fee as i64);
        assert_eq!(
            game.teams[0].transfer_budget,
            budget_before - outcome.fee as i64
        );
        let player = game.players.iter().find(|p| p.id == "fm-p-43").unwrap();
        assert!(player.movement_history.iter().any(|entry| {
            entry.kind == PlayerMovementKind::PermanentTransfer
                && entry.from_team_id.as_deref() == Some("fm-c-900")
                && entry.to_team_id.as_deref() == Some("team-1")
        }));
    }

    /// Given a player already in the active world, when the same source record is recruited, then
    /// the move is refused and nothing is mutated.
    #[test]
    fn the_same_source_player_cannot_exist_twice() {
        let mut game = game();
        recruit_external_player(&mut game, &external_free_agent("fm-p-44")).unwrap();
        let players = game.players.len();
        let cash = game.teams[0].finance;

        let error =
            recruit_external_player(&mut game, &external_free_agent("fm-p-44")).unwrap_err();

        assert_eq!(error, ERR_PLAYER_ALREADY_IN_WORLD);
        assert_eq!(game.players.len(), players);
        assert_eq!(game.teams[0].finance, cash);
    }

    /// Given a contracted player and a closed window, when he is approached, then the refusal is the
    /// window key and no money moves. A free agent is not blocked by the window.
    #[test]
    fn a_contracted_player_needs_an_open_window_but_a_free_agent_does_not() {
        let mut game = game();
        game.season_context.transfer_window.status = TransferWindowStatus::Closed;
        let cash = game.teams[0].finance;

        let error =
            recruit_external_player(&mut game, &external_contracted("fm-p-45")).unwrap_err();
        assert_eq!(error, ERR_WINDOW_CLOSED);
        assert_eq!(game.teams[0].finance, cash);

        recruit_external_player(&mut game, &external_free_agent("fm-p-46")).unwrap();
        assert!(game.players.iter().any(|p| p.id == "fm-p-46"));
    }

    /// Given a club that cannot afford the asking price, when the move is previewed, then the fee
    /// keeps it blocked and a recruit attempt leaves the world untouched.
    #[test]
    fn an_unaffordable_fee_blocks_before_any_mutation() {
        let mut game = game();
        game.teams[0].transfer_budget = 100;
        let players = game.players.len();
        let player = external_contracted("fm-p-47");

        let preview = preview_external_recruitment(&game, &player).unwrap();
        assert!(!preview.can_afford_fee);
        assert_eq!(
            preview.blocker.as_deref(),
            Some("be.error.transfers.transferBudgetTooLow")
        );

        let error = recruit_external_player(&mut game, &player).unwrap_err();
        assert_eq!(error, "be.error.transfers.transferBudgetTooLow");
        assert_eq!(game.players.len(), players);
    }

    /// Given a source player older than the registry's availability limit, when he is recruited,
    /// then he is refused as unavailable.
    #[test]
    fn a_player_past_the_projected_retirement_age_is_refused() {
        let mut game = game();
        let mut player = external_free_agent("fm-p-48");
        player.date_of_birth = "1980-01-01".to_string();

        let error = recruit_external_player(&mut game, &player).unwrap_err();

        assert_eq!(error, ERR_PLAYER_NOT_AVAILABLE);
    }

    /// Given a source contract that has already expired, when the price is asked, then it is zero
    /// and the player is treated as a free agent.
    #[test]
    fn an_expired_contract_is_free() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        assert_eq!(asking_price(1_000_000, Some("2026-07-31"), today), 0);
        let mut player = external_contracted("fm-p-49");
        player.club.as_mut().unwrap().contract_end = Some("2026-07-31".to_string());
        assert!(player.is_free_agent(today));
    }

    /// Given a lowball explicit offer, when the move is previewed and attempted, then the club
    /// rejects it and nothing in the career moves.
    #[test]
    fn a_lowball_offer_is_rejected_before_any_mutation() {
        let mut game = game();
        let player = external_contracted("fm-p-50");
        let preview = preview_external_recruitment_with_offer(&game, &player, Some(1)).unwrap();
        assert_eq!(preview.club_decision, "rejected");
        assert_eq!(preview.blocker.as_deref(), Some(ERR_OFFER_REJECTED));
        assert!(preview.minimum_fee > 1);
        let cash = game.teams[0].finance;
        let players = game.players.len();

        let error = recruit_external_player_at_offer(&mut game, &player, Some(1)).unwrap_err();

        assert_eq!(error, ERR_OFFER_REJECTED);
        assert_eq!(game.teams[0].finance, cash);
        assert_eq!(game.players.len(), players);
    }

    /// Given an offer below the club's floor but above its rejection threshold, when it is made
    /// twice, then it is countered to the same fee both times; the counter is not a signature, and
    /// signing only succeeds once the manager puts the countered fee back on the table.
    #[test]
    fn a_counter_is_deterministic_and_payable() {
        let mut game = game();
        let player = external_contracted("fm-p-51");
        let requested = preview_external_recruitment(&game, &player).unwrap();
        let offer = requested.minimum_fee * 4 / 5;

        let first = preview_external_recruitment_with_offer(&game, &player, Some(offer)).unwrap();
        let second = preview_external_recruitment_with_offer(&game, &player, Some(offer)).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.club_decision, "countered");
        assert_eq!(first.fee, requested.minimum_fee);
        assert_eq!(first.blocker, None);

        let players = game.players.len();
        let cash = game.teams[0].finance;
        let error = recruit_external_player_at_offer(&mut game, &player, Some(offer)).unwrap_err();
        assert_eq!(error, ERR_NEGOTIATION_NOT_AGREED);
        assert_eq!(game.players.len(), players);
        assert_eq!(game.teams[0].finance, cash);

        let outcome =
            recruit_external_player_at_offer(&mut game, &player, Some(first.fee)).unwrap();
        assert_eq!(outcome.fee, requested.minimum_fee);
        assert_eq!(game.teams[0].finance, cash - outcome.fee as i64);
    }

    /// Given a low wage, when the player is asked, then he counters with a wage and length, no
    /// money moves, and only the countered terms sign him.
    #[test]
    fn a_low_wage_gets_a_counter_and_only_the_counter_signs() {
        let mut game = game();
        let player = external_contracted("fm-p-52");
        let standard =
            negotiate_external_transfer(&game, &player, ExternalNegotiationRequest::default())
                .unwrap();
        let low = ExternalNegotiationRequest {
            weekly_wage: Some(standard.weekly_wage * 5 / 10),
            ..ExternalNegotiationRequest::default()
        };

        let first = negotiate_external_transfer(&game, &player, low).unwrap();
        let second = negotiate_external_transfer(&game, &player, low).unwrap();
        assert_eq!(first, second, "a repeated preview re-rolled the player");
        assert_eq!(first.status, NEGOTIATION_PLAYER_COUNTERED);
        assert_eq!(first.player_verdict, "counters");
        assert!(first.weekly_wage > 500);
        assert!(first.main_obstacle.is_some());

        let players = game.players.len();
        let cash = game.teams[0].finance;
        let error = finalize_external_transfer(&mut game, &player, low).unwrap_err();
        assert_eq!(error, ERR_NEGOTIATION_NOT_AGREED);
        assert_eq!(game.players.len(), players);
        assert_eq!(game.teams[0].finance, cash);

        let accepted = ExternalNegotiationRequest {
            weekly_wage: Some(first.weekly_wage),
            contract_years: Some(first.contract_years),
            ..ExternalNegotiationRequest::default()
        };
        let outcome = finalize_external_transfer(&mut game, &player, accepted).unwrap();
        assert_eq!(outcome.weekly_wage, first.weekly_wage);
        assert_eq!(game.teams[0].finance, cash - outcome.fee as i64);
    }

    /// Given a club far below his level, when an elite player is approached, then he refuses
    /// whatever the fee and nothing in the world moves.
    #[test]
    fn a_player_refuses_a_club_far_below_his_level_without_mutation() {
        let mut game = game();
        game.teams[0].reputation = 20;
        let player = external_contracted("fm-p-53");

        let outcome =
            negotiate_external_transfer(&game, &player, ExternalNegotiationRequest::default())
                .unwrap();
        assert_eq!(outcome.status, NEGOTIATION_PLAYER_REFUSED);
        assert_eq!(outcome.blocker.as_deref(), Some(ERR_PLAYER_REFUSES_MOVE));
        assert_eq!(outcome.player_verdict, "refuses");

        let players = game.players.len();
        let cash = game.teams[0].finance;
        let error =
            finalize_external_transfer(&mut game, &player, ExternalNegotiationRequest::default())
                .unwrap_err();
        assert_eq!(error, ERR_PLAYER_REFUSES_MOVE);
        assert_eq!(game.players.len(), players);
        assert_eq!(game.teams[0].finance, cash);
    }

    /// Given a fee below the club's floor, when it is negotiated, then the club counter is a next
    /// step, not an agreement, and finalizing it changes nothing.
    #[test]
    fn a_club_counter_is_not_an_agreement() {
        let mut game = game();
        let player = external_contracted("fm-p-54");
        let request = ExternalNegotiationRequest {
            fee: Some(1),
            ..ExternalNegotiationRequest::default()
        };
        let outcome = negotiate_external_transfer(&game, &player, request).unwrap();
        assert_eq!(outcome.status, NEGOTIATION_CLUB_REJECTED);
        assert_eq!(outcome.blocker.as_deref(), Some(ERR_OFFER_REJECTED));

        let request = ExternalNegotiationRequest {
            fee: Some(outcome.minimum_fee * 4 / 5),
            ..ExternalNegotiationRequest::default()
        };
        let outcome = negotiate_external_transfer(&game, &player, request).unwrap();
        assert_eq!(outcome.status, NEGOTIATION_CLUB_COUNTERED);
        assert_eq!(outcome.fee, outcome.minimum_fee);
        let players = game.players.len();
        let error = finalize_external_transfer(&mut game, &player, request).unwrap_err();
        assert_eq!(error, ERR_NEGOTIATION_NOT_AGREED);
        assert_eq!(game.players.len(), players);
    }

    /// Given a signing bonus, when the move is agreed, then the bonus is paid from cash and the
    /// transfer envelope, and the contract is the negotiated one.
    #[test]
    fn an_agreed_signing_bonus_is_paid_and_budgeted() {
        let mut game = game();
        let player = external_free_agent("fm-p-55");
        let standard =
            negotiate_external_transfer(&game, &player, ExternalNegotiationRequest::default())
                .unwrap();
        let request = ExternalNegotiationRequest {
            weekly_wage: Some(standard.weekly_wage),
            contract_years: Some(2),
            signing_bonus: Some(100_000),
            ..ExternalNegotiationRequest::default()
        };
        let outcome = negotiate_external_transfer(&game, &player, request).unwrap();
        assert_eq!(outcome.status, NEGOTIATION_AGREED);
        assert!(outcome.can_afford_bonus);
        assert_eq!(outcome.contract_years, 2);

        let cash = game.teams[0].finance;
        let budget = game.teams[0].transfer_budget;
        let signed = finalize_external_transfer(&mut game, &player, request).unwrap();
        assert_eq!(signed.weekly_wage, standard.weekly_wage);
        assert_eq!(game.teams[0].finance, cash - 100_000);
        assert_eq!(game.teams[0].transfer_budget, budget - 100_000);
        assert!(game.players.iter().any(|stored| stored.id == "fm-p-55"));
    }

    /// Given terms a contract cannot carry, when they are negotiated, then the request is refused
    /// before either party answers.
    #[test]
    fn invalid_negotiation_terms_are_refused() {
        let game = game();
        let player = external_contracted("fm-p-56");
        let too_long = ExternalNegotiationRequest {
            contract_years: Some(6),
            ..ExternalNegotiationRequest::default()
        };
        assert_eq!(
            negotiate_external_transfer(&game, &player, too_long).unwrap_err(),
            ERR_INVALID_NEGOTIATION_TERMS
        );
        let bonus_over_wage = ExternalNegotiationRequest {
            weekly_wage: Some(1_000),
            contract_years: Some(1),
            signing_bonus: Some(100_000_000),
            ..ExternalNegotiationRequest::default()
        };
        assert_eq!(
            negotiate_external_transfer(&game, &player, bonus_over_wage).unwrap_err(),
            ERR_INVALID_NEGOTIATION_TERMS
        );
    }
}
