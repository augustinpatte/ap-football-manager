//! Routine inbox decisions: the offers other clubs make for the manager's players.
//!
//! The assistant does the part of the inbox that needs no judgement of the club's future: an offer
//! far below what the club itself would ask for the player is turned down, through the same
//! answer the manager gives. Everything else stays in the manager's inbox. The assistant never
//! accepts, counters or lists a player: selling is the manager's decision (a test pins it), and a
//! player the manager has put up for sale is the manager's business whatever is offered.
//!
//! How far below counts as far depends on the assistant. A good one knows what a player is worth
//! and turns down anything under three quarters of it; a poor one only recognises an insult, under
//! half. The same assistant gives the same answer to the same offer.
//!
//! The second routine answer is for the first eleven: a capable assistant turns down an offer for
//! a starter that is not clearly above his worth. A big premium goes to the manager, who may well
//! want it. A poor assistant does not know who matters and does not protect anyone.

use domain::delegation::Duty;
use domain::player::TransferOfferStatus;

use super::delegation::{log, param};
use crate::game::Game;
use crate::transfers::suggested_incoming_fee;

/// An offer below this share of the club's own fair price is turned down by a capable assistant.
const DERISORY_SHARE_PCT: u64 = 75;
/// The share a poor assistant (below [`CAPABLE_RATING`]) recognises as an insult.
const INSULT_SHARE_PCT: u64 = 50;
/// The assistant rating from which he knows what a player is worth.
const CAPABLE_RATING: f64 = 45.0;
/// An offer for a starter has to reach this share of the club's fair price before the manager is
/// bothered with it.
const STARTER_PREMIUM_PCT: u64 = 130;

/// Turns down the offers for the manager's players that are far below the player's worth.
pub(super) fn answer_derisory_offers(game: &mut Game, team_id: &str, assistant_rating: f64) {
    let today = game.clock.current_date.date_naive();
    let share = if assistant_rating >= CAPABLE_RATING {
        DERISORY_SHARE_PCT
    } else {
        INSULT_SHARE_PCT
    };
    let capable = assistant_rating >= CAPABLE_RATING;
    let first_eleven: Vec<&str> = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.starting_xi_ids.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let mut turned_down: Vec<(String, String, String, u64, &'static str)> = Vec::new();
    for player in game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team_id) && !p.transfer_listed)
    {
        let fair = suggested_incoming_fee(today, player);
        let is_starter = capable && first_eleven.contains(&player.id.as_str());
        for offer in player
            .transfer_offers
            .iter()
            .filter(|o| o.status == TransferOfferStatus::Pending)
        {
            let scaled = offer.fee.saturating_mul(100);
            let reason = if scaled < fair.saturating_mul(share) {
                "delegation.log.offerDeclined"
            } else if is_starter && scaled < fair.saturating_mul(STARTER_PREMIUM_PCT) {
                "delegation.log.offerKeepsStarter"
            } else {
                continue;
            };
            turned_down.push((
                player.id.clone(),
                offer.id.clone(),
                offer.from_team_id.clone(),
                offer.fee,
                reason,
            ));
        }
    }
    for (player_id, offer_id, from_team_id, fee, reason) in turned_down {
        let name = game
            .players
            .iter()
            .find(|p| p.id == player_id)
            .map(|p| p.full_name.clone())
            .unwrap_or_default();
        let club = game.team_name_or_id(&from_team_id);
        if crate::transfers::respond_to_offer(game, &player_id, &offer_id, false).is_ok() {
            log(
                game,
                Duty::Offers,
                reason,
                vec![
                    param("player", name),
                    param("club", club),
                    param("fee", fee),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::game;
    use super::super::delegation::run_after_day;
    use super::*;
    use domain::delegation::{DecisionOutcome, DelegationMode};
    use domain::player::TransferOffer;

    const PLAYER: &str = "s5";

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

    /// Given a capable assistant under full delegation
    /// When a day is played
    /// Then the insult and the poor offer are turned down and logged, and the fair one is left to
    /// the manager: the assistant never accepts.
    #[test]
    fn a_capable_assistant_turns_down_derisory_offers_and_never_accepts() {
        let mut g = with_offers(80);
        g.delegation.apply_mode(DelegationMode::Full);

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
