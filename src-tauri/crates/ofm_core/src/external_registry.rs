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
    contract_entry, contract_record, expected_contract_years, expected_wage,
    free_agent_signed_message, sign_free_agent, squad_capacity, standard_contract_terms,
};
use crate::external_world::{ExternalOfferContext, ExternalOfferDecision};
use crate::game::Game;
use crate::transfers::{
    FeeBlock, fee_block_after_pending, log_completed_transfer, transfer_window_is_open,
};
use chrono::{Datelike, NaiveDate};
use domain::contract_ledger::{ContractRecord, ContractSource};
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
struct OfferTerms {
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
fn offer_terms(game: &Game, request: &OfferRequest<'_>) -> OfferTerms {
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
        return OfferTerms {
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
    let terms = |fee: u64, decision: &'static str, rejected: bool| OfferTerms {
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
    let terms = offer_terms(
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
            .or_else(|| squad_blocker(game, team, free_agent))
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

/// The senior registration limit, which every free agent signing meets (`contracts::squad_capacity`).
/// A purchase from another club is judged by the transfer rules, which have no such limit.
fn squad_blocker(game: &Game, team: &Team, free_agent: bool) -> Option<String> {
    let capacity = squad_capacity(game, &team.id);
    (free_agent && capacity.blocks_signing).then(|| capacity.full_error())
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

/// Promote `external` into the active game for the manager's club at the club's own request.
pub fn recruit_external_player(
    game: &mut Game,
    external: &ExternalPlayer,
) -> Result<ExternalRecruitOutcome, String> {
    recruit_external_player_at_offer(game, external, None)
}

/// Promote `external` against an explicit offer. A rejected offer returns
/// [`ERR_OFFER_REJECTED`] before the first mutation; a counter is accepted at the club's floor.
pub fn recruit_external_player_at_offer(
    game: &mut Game,
    external: &ExternalPlayer,
    offer: Option<u64>,
) -> Result<ExternalRecruitOutcome, String> {
    let preview = preview_external_recruitment_with_offer(game, external, offer)?;
    if let Some(blocker) = preview.blocker.clone() {
        return Err(blocker);
    }
    let today = game.clock.current_date.date_naive();
    let team = managed_team(game)?.clone();
    let mut player = external.to_player(today);
    crate::player_rating::refresh_player_derived(&mut player, today);
    if external.potential > 0 {
        player.potential = player.potential.max(external.potential).max(player.ovr);
    }
    game.players.push(player);
    let index = game.players.len() - 1;

    if preview.free_agent {
        sign_free_agent(
            game,
            index,
            &team,
            preview.weekly_wage,
            preview.contract_years,
            today,
        )?;
        let signed = &game.players[index];
        game.messages.push(free_agent_signed_message(
            &signed.id,
            &signed.full_name,
            &team.name,
            preview.contract_years,
            &today.format("%Y-%m-%d").to_string(),
        ));
        return Ok(ExternalRecruitOutcome {
            player_id: external.id.clone(),
            free_agent: true,
            fee: 0,
            weekly_wage: preview.weekly_wage,
            contract_end: preview.contract_end,
            from_club: None,
        });
    }

    let fee = preview.fee;
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

    let from_club = external.club.clone().ok_or(ERR_PLAYER_NOT_AVAILABLE)?;
    let contract_end = NaiveDate::parse_from_str(&preview.contract_end, "%Y-%m-%d")
        .map_err(|_| ERR_CONTRACT_END.to_string())?;
    let jersey = {
        let candidate = &game.players[index];
        crate::roster::resolve_jersey_for(game, candidate, &team)
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
            &team,
            contract_record(
                today,
                contract_end,
                preview.weekly_wage,
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
        weekly_wage: preview.weekly_wage,
        contract_end: preview.contract_end,
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
        team.reputation = 50;
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
    /// twice, then it is countered to the same fee both times and signing at that counter succeeds.
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

        let cash = game.teams[0].finance;
        let outcome = recruit_external_player_at_offer(&mut game, &player, Some(offer)).unwrap();
        assert_eq!(outcome.fee, requested.minimum_fee);
        assert_eq!(game.teams[0].finance, cash - outcome.fee as i64);
    }
}
