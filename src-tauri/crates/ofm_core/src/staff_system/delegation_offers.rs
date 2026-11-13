//! Delegated selling: what the assistant does about the offers other clubs make for the manager's
//! players.
//!
//! Every pending offer is judged once, from three things: what the club itself would ask for the
//! player (`suggested_incoming_fee`), what the side the club fields would lose if he went (Squad
//! Intelligence, net of the cheapest replacement a club can sign), and what the manager has said
//! (a protected player is not sold). The judgement is one of
//!
//! * **turn it down** — an insult, a protected player, a player the side cannot do without, or (under
//!   Total authority) an offer for a player the club would rather keep. Turning an offer down
//!   commits nothing and is routine: it is the assistant's whenever the duty is delegated;
//! * **accept it** — the player is surplus, or his sale funds more than he is worth to the side, and
//!   the price is fair;
//! * **ask for more** — the same player, an offer a little short of fair;
//! * **leave it** — to the manager, whose business it is.
//!
//! Accepting and countering are acted on only with Total authority ([`authorize_sale`]); otherwise
//! they become proposals the manager can read and answer, with what the sale costs the side and
//! what it brings. Every step goes through the same `respond_to_offer` and `counter_offer` the
//! manager uses, so the squad floor, the buyer's wage policy, cash and the registration window
//! apply unchanged: if a sale cannot be made legally it is not made, and the offer is left to the
//! manager with the reason in the history.
//!
//! How sharp the assistant is shows continuously: the share of the club's price he turns down grows
//! from 50 % (an assistant of 0) to 75 % (of 100), and the premium over that price he asks for a
//! first choice before he lets one go grows from none (30 and below) to 30 % (100). The same
//! assistant gives the same answer to the same offer.

use domain::delegation::{DecisionOutcome, Duty, ProposalAction, ShelveReason};
use domain::player::{Player, TransferOffer, TransferOfferStatus};

use super::delegation::{date, log, param, record, stamp};
use super::delegation_policy::{Authorization, authorize_sale};
use super::delegation_recruitment::Eye;
use crate::game::Game;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::{REINVESTMENT_RATIO, STARTER_LOSS, SaleCase};
use crate::transfers::{
    TransferNegotiationDecision, counter_offer, reinvestment_gain, respond_to_offer,
    round_transfer_fee, suggested_incoming_fee,
};

/// The share of the club's own price an offer must reach not to be an insult: 50 % for an
/// assistant of 0 rising to 75 % for one of 100.
fn derisory_share_pct(rating: f64) -> f64 {
    50.0 + 25.0 * (rating / 100.0).clamp(0.0, 1.0)
}

/// The share of the club's price an offer for a first choice must reach before the assistant lets
/// him go: no protection at 30 and below, 130 % at 100.
fn starter_premium_pct(rating: f64) -> f64 {
    100.0 + 30.0 * ((rating - 30.0) / 70.0).clamp(0.0, 1.0)
}

/// An offer this share of the club's price (or more) is an exceptional one.
const EXCEPTIONAL_SHARE_PCT: f64 = 125.0;

/// Counter-offers the assistant makes on one offer before he settles or leaves it.
const MAX_COUNTERS: u8 = 2;

/// After the counters are spent, an offer this share of the club's price is taken.
const SETTLE_SHARE_PCT: f64 = 90.0;

/// Days a sale the manager refused is not raised again.
const SHELVE_DECLINED_DAYS: u64 = 90;

/// Why an offer is turned down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    Protected,
    Derisory,
    KeepsStarter,
    /// Under Total authority only: not an insult, but not a sale the club wants.
    Kept,
}

impl Refusal {
    fn key(self) -> &'static str {
        match self {
            Self::Protected => "delegation.log.offerDeclinedProtected",
            Self::Derisory => "delegation.log.offerDeclined",
            Self::KeepsStarter => "delegation.log.offerKeepsStarter",
            Self::Kept => "delegation.log.offerKept",
        }
    }
}

/// Why a sale is sound, for the manager to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sound {
    /// The side does not need him.
    Surplus,
    /// The money mends more than he is worth to the side.
    Funds,
    /// A fair price for a player the side can spare.
    Fair,
}

impl Sound {
    fn name(self) -> &'static str {
        match self {
            Self::Surplus => "surplus",
            Self::Funds => "funds",
            Self::Fair => "fair",
        }
    }

    fn reason_key(self) -> &'static str {
        match self {
            Self::Surplus => "delegation.reason.saleSurplus",
            Self::Funds => "delegation.reason.saleFunds",
            Self::Fair => "delegation.reason.saleFair",
        }
    }

    /// The history's line for a sale made on this ground.
    fn accepted_key(self) -> &'static str {
        match self {
            Self::Surplus => "delegation.log.offerAcceptedSurplus",
            Self::Funds => "delegation.log.offerAcceptedFunds",
            Self::Fair => "delegation.log.offerAcceptedFair",
        }
    }
}

/// What the assistant makes of one offer.
#[derive(Debug, Clone, PartialEq)]
enum Verdict {
    Refuse(Refusal),
    Accept {
        why: Sound,
        loss_points: i32,
    },
    Counter {
        fee: u64,
        why: Sound,
        loss_points: i32,
    },
    Leave,
}

/// One pending offer, copied out so the game can be changed while it is acted on.
struct Pending {
    player_id: String,
    offer: TransferOffer,
}

fn pending_offers(game: &Game, team_id: &str) -> Vec<Pending> {
    let mut found = Vec::new();
    for player in game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team_id))
    {
        // A player the manager has listed himself is the manager's business whatever is offered;
        // one the assistant listed is the assistant's.
        if player.transfer_listed && !game.delegation.sales.listed.contains(&player.id) {
            continue;
        }
        for offer in player
            .transfer_offers
            .iter()
            .filter(|o| o.status == TransferOfferStatus::Pending)
        {
            found.push(Pending {
                player_id: player.id.clone(),
                offer: offer.clone(),
            });
        }
    }
    found
}

/// The club's squad as the assistant reads it, through his own foresight.
fn own_squad(game: &Game, team_id: &str, eye: Eye<'_>) -> Option<ClubSquad> {
    let appetite =
        Appetite::of(&crate::transfers::profile_of(game, team_id)).seen_by_assistant(eye.rating);
    ClubSquad::of_for_planning(game, team_id, appetite)
}

/// The share of the club's own price an offer is, in per cent.
fn share_of(fee: u64, fair: u64) -> f64 {
    fee as f64 * 100.0 / fair.max(1) as f64
}

/// Whether the player is one the manager has picked for his first eleven.
fn in_first_eleven(game: &Game, team_id: &str, player_id: &str) -> bool {
    game.teams
        .iter()
        .find(|t| t.id == team_id)
        .is_some_and(|t| t.starting_xi_ids.iter().any(|id| id == player_id))
}

fn judge(
    game: &Game,
    team_id: &str,
    eye: Eye<'_>,
    player: &Player,
    offer: &TransferOffer,
    counters: u8,
) -> Verdict {
    let fair = suggested_incoming_fee(date(game), player);
    let share = share_of(offer.fee, fair);
    if game.delegation.is_protected(&player.id) {
        return Verdict::Refuse(Refusal::Protected);
    }
    if share < derisory_share_pct(eye.rating) {
        return Verdict::Refuse(Refusal::Derisory);
    }
    let Some(squad) = own_squad(game, team_id, eye) else {
        return Verdict::Leave;
    };
    let case = squad.sale(&player.id);
    let loss_points =
        (case.net_loss * crate::squad_intelligence::model::SLOTS as f32).round() as i32;
    let funds = reinvestment_gain(&squad, &player.id) >= REINVESTMENT_RATIO * case.net_loss;
    let exceptional = share >= EXCEPTIONAL_SHARE_PCT && funds;
    // Nobody the side cannot replace, even cheaply, is sold at any price: a club that sells its only
    // reliable keeper has nobody to field.
    if case.critical {
        return Verdict::Refuse(Refusal::KeepsStarter);
    }
    let is_starter = case.net_loss >= STARTER_LOSS || in_first_eleven(game, team_id, &player.id);
    if is_starter && !exceptional && share < starter_premium_pct(eye.rating) {
        return Verdict::Refuse(Refusal::KeepsStarter);
    }
    let Some(why) = sound_sale(&case, funds, share, exceptional) else {
        return if game.delegation.limits.finance_bound {
            Verdict::Refuse(Refusal::Kept)
        } else {
            Verdict::Leave
        };
    };
    if share >= 100.0 {
        return Verdict::Accept { why, loss_points };
    }
    if counters < MAX_COUNTERS {
        return Verdict::Counter {
            fee: round_transfer_fee(fair),
            why,
            loss_points,
        };
    }
    if share >= SETTLE_SHARE_PCT {
        Verdict::Accept { why, loss_points }
    } else {
        Verdict::Leave
    }
}

/// Whether selling this player at this price is a sound thing for the club to do, and why: the
/// side does not need him; or the money mends more than he is worth to it; or he is one the side can
/// spare and the price is close enough to fair to settle on.
fn sound_sale(case: &SaleCase, funds: bool, share: f64, exceptional: bool) -> Option<Sound> {
    if case.surplus {
        return Some(Sound::Surplus);
    }
    if exceptional || (funds && share >= 100.0) {
        return Some(Sound::Funds);
    }
    (case.net_loss < STARTER_LOSS && share >= SETTLE_SHARE_PCT).then_some(Sound::Fair)
}

/// Judges every pending offer for the manager's players and acts on it within the authority the
/// manager has given.
pub(super) fn answer_offers(game: &mut Game, team_id: &str, eye: Eye<'_>) {
    let today = stamp(game);
    game.delegation.sales.forget_expired(&today);
    for pending in pending_offers(game, team_id) {
        let counters = {
            let record = game.delegation.sales.record_mut(&pending.offer.id, &today);
            if record.settled {
                continue;
            }
            record.counters
        };
        // The offer may have been answered, or the player moved, since the list was made.
        let Some(player) = game
            .players
            .iter()
            .find(|p| p.id == pending.player_id)
            .filter(|p| {
                p.transfer_offers
                    .iter()
                    .any(|o| o.id == pending.offer.id && o.status == TransferOfferStatus::Pending)
            })
            .cloned()
        else {
            continue;
        };
        let verdict = judge(game, team_id, eye, &player, &pending.offer, counters);
        act(game, &pending, &player, verdict, &today);
    }
}

fn act(game: &mut Game, pending: &Pending, player: &Player, verdict: Verdict, today: &str) {
    let offer = &pending.offer;
    match verdict {
        Verdict::Leave => {}
        Verdict::Refuse(refusal) => refuse(game, pending, player, refusal, today),
        Verdict::Accept { why, loss_points } => {
            match authorize_sale(&game.delegation, Duty::Offers) {
                Authorization::Auto => accept(game, pending, player, why, loss_points, today),
                Authorization::NeedsApproval(_) => propose_sale(
                    game,
                    ProposalAction::AcceptOffer {
                        player_id: player.id.clone(),
                        offer_id: offer.id.clone(),
                        fee: offer.fee,
                    },
                    "delegation.proposal.sellPlayer",
                    pending,
                    player,
                    (why, loss_points, offer.fee),
                    today,
                ),
                Authorization::NotDelegated => {}
            }
        }
        Verdict::Counter {
            fee,
            why,
            loss_points,
        } => match authorize_sale(&game.delegation, Duty::Offers) {
            Authorization::Auto => counter(game, pending, player, fee, today),
            Authorization::NeedsApproval(_) => propose_sale(
                game,
                ProposalAction::CounterOffer {
                    player_id: player.id.clone(),
                    offer_id: offer.id.clone(),
                    fee,
                },
                "delegation.proposal.counterOffer",
                pending,
                player,
                (why, loss_points, fee),
                today,
            ),
            Authorization::NotDelegated => {}
        },
    }
}

fn buyer_name(game: &Game, offer: &TransferOffer) -> String {
    game.team_name_or_id(&offer.from_team_id)
}

/// Turns the offer down through the answer the manager gives, and settles it.
fn refuse(game: &mut Game, pending: &Pending, player: &Player, refusal: Refusal, today: &str) {
    let club = buyer_name(game, &pending.offer);
    if respond_to_offer(game, &player.id, &pending.offer.id, false).is_ok() {
        game.delegation
            .sales
            .record_mut(&pending.offer.id, today)
            .settled = true;
        log(
            game,
            Duty::Offers,
            refusal.key(),
            vec![
                param("player", &player.full_name),
                param("club", club),
                param("fee", pending.offer.fee),
            ],
        );
    }
}

/// The sale is made through the manager's own answer. A sale that cannot be made legally (the squad
/// would be left short, the buyer cannot pay, the window is shut) is not made; the offer is left to
/// the manager, once, with the reason in the history.
fn accept(
    game: &mut Game,
    pending: &Pending,
    player: &Player,
    why: Sound,
    loss_points: i32,
    today: &str,
) {
    let club = buyer_name(game, &pending.offer);
    let result = respond_to_offer(game, &player.id, &pending.offer.id, true);
    game.delegation
        .sales
        .record_mut(&pending.offer.id, today)
        .settled = true;
    match result {
        Ok(()) => log(
            game,
            Duty::Offers,
            why.accepted_key(),
            vec![
                param("player", &player.full_name),
                param("club", club),
                param("fee", pending.offer.fee),
                param("why", why.name()),
                param("loss", loss_points),
                param("wage", player.wage()),
            ],
        ),
        Err(why_not) => record(
            game,
            Duty::Offers,
            "delegation.log.offerNotAccepted",
            vec![
                param("player", &player.full_name),
                param("club", club),
                param("fee", pending.offer.fee),
                param("error", why_not),
            ],
            DecisionOutcome::LeftToManager,
        ),
    }
}

/// Asks the buyer for more. The buyer answers at once, as to the manager; the offer is the next
/// morning's business again unless it was agreed or broken off.
fn counter(game: &mut Game, pending: &Pending, player: &Player, fee: u64, today: &str) {
    let club = buyer_name(game, &pending.offer);
    let result = counter_offer(game, &player.id, &pending.offer.id, fee);
    let record_entry = game.delegation.sales.record_mut(&pending.offer.id, today);
    record_entry.counters = record_entry.counters.saturating_add(1);
    record_entry.date = today.to_string();
    match result {
        Ok(outcome) => {
            let (answer, key) = match outcome.decision {
                TransferNegotiationDecision::Accepted => {
                    ("accepted", "delegation.log.offerCounteredAccepted")
                }
                TransferNegotiationDecision::CounterOffer => {
                    ("counter", "delegation.log.offerCounteredAgain")
                }
                TransferNegotiationDecision::Rejected
                | TransferNegotiationDecision::PlayerTalks => {
                    ("refused", "delegation.log.offerCounteredRefused")
                }
            };
            if outcome.decision != TransferNegotiationDecision::CounterOffer {
                game.delegation
                    .sales
                    .record_mut(&pending.offer.id, today)
                    .settled = true;
            }
            log(
                game,
                Duty::Offers,
                key,
                vec![
                    param("player", &player.full_name),
                    param("club", club),
                    param("offered", pending.offer.fee),
                    param("asked", fee),
                    param("answer", answer),
                ],
            );
        }
        Err(why_not) => {
            game.delegation
                .sales
                .record_mut(&pending.offer.id, today)
                .settled = true;
            record(
                game,
                Duty::Offers,
                "delegation.log.offerNotAccepted",
                vec![
                    param("player", &player.full_name),
                    param("club", club),
                    param("fee", pending.offer.fee),
                    param("error", why_not),
                ],
                DecisionOutcome::LeftToManager,
            );
        }
    }
}

/// A sale the assistant may not make by himself: put to the manager with what it costs the side and
/// what it brings. The offer is marked so that it is not proposed again each day; if the manager
/// declines, the player is set aside for a good while.
fn propose_sale(
    game: &mut Game,
    action: ProposalAction,
    title_key: &str,
    pending: &Pending,
    player: &Player,
    (why, loss_points, fee): (Sound, i32, u64),
    today: &str,
) {
    if game.delegation.sales.is_shelved(&player.id, today) {
        return;
    }
    let club = buyer_name(game, &pending.offer);
    let fair = suggested_incoming_fee(date(game), player);
    game.delegation.propose(
        Duty::Offers,
        today,
        action,
        title_key,
        vec![
            param("player", &player.full_name),
            param("player_id", &player.id),
            param("club", club),
            param("fee", fee),
            param("offered", pending.offer.fee),
            param("fair", fair),
            param("why", why.name()),
            param("loss", loss_points),
        ],
        why.reason_key(),
    );
    game.delegation
        .sales
        .record_mut(&pending.offer.id, today)
        .settled = true;
}

/// The manager said no to a sale (or to listing a player): the assistant leaves him alone for a good
/// while.
pub(super) fn on_declined(game: &mut Game, player_id: &str) {
    let until = date(game)
        .checked_add_days(chrono::Days::new(SHELVE_DECLINED_DAYS))
        .unwrap_or_else(|| date(game))
        .format("%Y-%m-%d")
        .to_string();
    game.delegation
        .sales
        .shelve(player_id, &until, ShelveReason::ManagerDeclined);
}

/// The manager approved a proposed sale: it is made at the fee he was shown, through his own
/// answer, unless the player has since been protected or the offer is no longer what it was.
pub(super) fn accept_approved(
    game: &mut Game,
    player_id: &str,
    offer_id: &str,
    fee: u64,
) -> Result<(), String> {
    if game.delegation.is_protected(player_id) {
        return Err("be.error.delegation.playerProtected".to_string());
    }
    let offered = game
        .players
        .iter()
        .find(|p| p.id == player_id)
        .and_then(|p| {
            p.transfer_offers
                .iter()
                .find(|o| o.id == offer_id && o.status == TransferOfferStatus::Pending)
        })
        .map(|o| o.fee)
        .ok_or("be.error.delegation.offerGone")?;
    if offered < fee {
        return Err("be.error.delegation.offerChanged".to_string());
    }
    respond_to_offer(game, player_id, offer_id, true)
}

/// The manager approved asking for more: the counter is made at the fee he was shown.
pub(super) fn counter_approved(
    game: &mut Game,
    player_id: &str,
    offer_id: &str,
    fee: u64,
) -> Result<(), String> {
    if game.delegation.is_protected(player_id) {
        return Err("be.error.delegation.playerProtected".to_string());
    }
    counter_offer(game, player_id, offer_id, fee).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::game;
    use super::super::delegation::run_after_day;
    use super::*;
    use domain::delegation::{DecisionOutcome, ProposalAction};
    use domain::player::TransferOffer;

    /// A midfielder the fixture's club has to spare: the sixteenth of its sixteen, behind an eleven
    /// and four others. (The fixture has no keeper, defender or forward, so each of the first
    /// eleven or so is missed.)
    const PLAYER: &str = "s15";

    fn offer(id: &str, fee: u64) -> TransferOffer {
        TransferOffer {
            id: id.to_string(),
            from_team_id: "team2".to_string(),
            fee,
            wage_offered: 5_000,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            status: TransferOfferStatus::Pending,
            date: "2026-08-01".to_string(),
            registration_date: None,
            registration_failure_reason: None,
            closed_on: None,
            terms: Default::default(),
        }
    }

    /// A club with a bench player worth four million, three offers for him (an insult, a poor one,
    /// a fair one), and an assistant of the given rating.
    fn with_offers(rating: u8) -> Game {
        let mut g = game(Some(rating), 20);
        g.teams[0].starting_xi_ids.retain(|id| id != PLAYER);
        let player = g.players.iter_mut().find(|p| p.id == PLAYER).unwrap();
        player.market_value = 4_000_000;
        player.transfer_offers = vec![
            offer("insult", 1_200_000),
            offer("poor", 2_600_000),
            offer("fair", 4_000_000),
        ];
        g
    }

    fn status(g: &Game, offer_id: &str) -> TransferOfferStatus {
        g.players
            .iter()
            .find(|p| p.id == PLAYER)
            .unwrap()
            .transfer_offers
            .iter()
            .find(|o| o.id == offer_id)
            .unwrap()
            .status
            .clone()
    }

    fn declined(g: &Game) -> usize {
        g.delegation
            .log
            .iter()
            .filter(|e| e.duty == Duty::Offers && e.title_key == "delegation.log.offerDeclined")
            .count()
    }

    /// Given offers for a player and nothing delegated
    /// When a day is played
    /// Then every offer is still waiting and nothing is written to the history.
    #[test]
    fn in_manual_mode_every_offer_waits_for_the_manager() {
        let mut g = with_offers(80);
        run_after_day(&mut g);
        for id in ["insult", "poor", "fair"] {
            assert_eq!(status(&g, id), TransferOfferStatus::Pending);
        }
        assert!(g.delegation.log.is_empty());
    }

    /// Given a capable assistant whose duty acts alone but who has no Total authority
    /// When a day is played
    /// Then the insult and the poor offer are turned down and logged, and the fair one is put to the
    /// manager as a proposal: selling is not the assistant's without Total authority.
    #[test]
    fn a_capable_assistant_without_total_authority_turns_down_insults_and_proposes_the_sale() {
        let mut g = with_offers(80);
        g.delegation.set(Duty::Offers, true, false);

        run_after_day(&mut g);

        assert_eq!(status(&g, "insult"), TransferOfferStatus::Rejected);
        assert_eq!(status(&g, "poor"), TransferOfferStatus::Rejected);
        assert_eq!(status(&g, "fair"), TransferOfferStatus::Pending);
        assert_eq!(declined(&g), 2);
        assert!(
            g.delegation
                .log
                .iter()
                .filter(|e| e.duty == Duty::Offers)
                .all(|e| e.outcome == DecisionOutcome::Executed)
        );
        assert!(
            g.delegation.proposals.iter().any(|p| matches!(
                &p.action,
                ProposalAction::AcceptOffer { offer_id, .. } if offer_id == "fair"
            )),
            "the sale is the manager's to approve"
        );
    }

    /// Given a poor assistant
    /// When the same day is played
    /// Then he recognises only the insult; the poor offer stays for the manager.
    #[test]
    fn a_poor_assistant_only_recognises_an_insult() {
        let mut g = with_offers(30);
        g.delegation.set(Duty::Offers, true, false);

        run_after_day(&mut g);

        assert_eq!(status(&g, "insult"), TransferOfferStatus::Rejected);
        assert_eq!(status(&g, "poor"), TransferOfferStatus::Pending);
    }

    /// Given a player the manager has put up for sale
    /// When a derisory offer arrives and the day is played
    /// Then it is left alone: a listed player's offers are the manager's to judge.
    #[test]
    fn a_listed_players_offers_are_left_to_the_manager() {
        let mut g = with_offers(80);
        g.delegation.set(Duty::Offers, true, false);
        g.players
            .iter_mut()
            .find(|p| p.id == PLAYER)
            .unwrap()
            .transfer_listed = true;

        run_after_day(&mut g);

        assert_eq!(status(&g, "insult"), TransferOfferStatus::Pending);
    }

    /// Given an offer already turned down
    /// When another day is played
    /// Then it is not answered or logged again.
    #[test]
    fn an_answered_offer_is_not_answered_twice() {
        let mut g = with_offers(80);
        g.delegation.set(Duty::Offers, true, false);
        run_after_day(&mut g);
        g.clock.current_date += chrono::Duration::days(1);
        run_after_day(&mut g);
        assert_eq!(declined(&g), 2);
    }

    /// A starter worth four million with a fair offer, a modest premium and a big premium.
    fn with_offers_for_a_starter(rating: u8) -> Game {
        let mut g = with_offers(rating);
        g.teams[0].starting_xi_ids.push(PLAYER.to_string());
        let player = g.players.iter_mut().find(|p| p.id == PLAYER).unwrap();
        player.transfer_offers = vec![
            offer("fair", 4_000_000),
            offer("modest premium", 4_800_000),
            offer("big premium", 6_400_000),
        ];
        g
    }

    /// Given a first-eleven player and a capable assistant
    /// When offers at his worth, a modest premium and a big premium arrive
    /// Then the first two are turned down as "he is in the first eleven" and the big one is left
    /// for the manager: the assistant keeps a starter unless the price is exceptional, and never
    /// sells him.
    #[test]
    fn a_starter_is_not_sold_for_a_fair_price_but_a_big_premium_is_the_managers_call() {
        let mut g = with_offers_for_a_starter(80);
        g.delegation.set(Duty::Offers, true, false);

        run_after_day(&mut g);

        assert_eq!(status(&g, "fair"), TransferOfferStatus::Rejected);
        assert_eq!(status(&g, "modest premium"), TransferOfferStatus::Rejected);
        assert_eq!(status(&g, "big premium"), TransferOfferStatus::Pending);
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.title_key == "delegation.log.offerKeepsStarter")
        );
    }

    /// Given the same starter and a poor assistant
    /// When the same offers arrive
    /// Then he does not protect the starter: he does not know who matters.
    #[test]
    fn a_poor_assistant_does_not_protect_a_starter() {
        let mut g = with_offers_for_a_starter(30);
        g.delegation.set(Duty::Offers, true, false);
        run_after_day(&mut g);
        assert_eq!(status(&g, "fair"), TransferOfferStatus::Pending);
    }

    /// Given a player on the bench
    /// When a fair offer arrives
    /// Then it is the manager's: only starters are kept.
    #[test]
    fn a_fair_offer_for_a_squad_player_is_left_to_the_manager() {
        let mut g = with_offers_for_a_starter(80);
        g.teams[0].starting_xi_ids.retain(|id| id != PLAYER);
        g.delegation.set(Duty::Offers, true, false);
        run_after_day(&mut g);
        assert_eq!(status(&g, "fair"), TransferOfferStatus::Pending);
    }
}
