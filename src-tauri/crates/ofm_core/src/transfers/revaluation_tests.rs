//! What a seasonal revaluation must leave alone: an estimate of what a player is worth is not a
//! contract, an offer already made, a club's cash or a completed deal.

use super::execution::execute_transfer_with_terms;
use super::tests::{incoming_transfer, make_game};
use crate::aging::apply_seasonal_aging;
use crate::game::Game;
use crate::revaluation::{anchor_value, revalue_after_season};
use crate::value_scale::sample_imported_scale;
use chrono::NaiveDate;
use domain::player::{ActiveLoan, LoanTermsSource, PlayerMovementKind};

fn season_end() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 5, 31).unwrap()
}

/// A game on a calibrated scale whose one player, a 34-year-old forward, was valued a year ago and
/// has since declined: his revaluation at this season's end is a large, certain drop.
fn game_with_a_declining_veteran() -> Game {
    let mut game = make_game();
    game.value_scale = sample_imported_scale();
    let player = &mut game.players[0];
    player.date_of_birth = "1992-01-01".to_string();
    player.market_value = 20_000_000;
    anchor_value(
        player,
        &sample_imported_scale(),
        NaiveDate::from_ymd_opt(2025, 5, 31).unwrap(),
    );
    player.ovr = 82;
    player.potential = 82;
    if let Some(basis) = player.value_basis.as_mut() {
        basis.ovr = 90;
        basis.potential = 90;
    }
    game
}

/// Given a veteran with a pending offer at a fixed fee, a wage and a contract,
/// When the season's end revalues him,
/// Then his estimated value falls and everything agreed stays exactly as it was: the pending
/// offer's fee, wage and status, and his wage and contract. (Revaluation takes a player, not a
/// club, so it cannot reach cash or budgets; the five-season studies check those at world level.)
#[test]
fn a_revaluation_leaves_offers_and_contracts_alone() {
    let mut game = game_with_a_declining_veteran();
    let offer_id = incoming_transfer(&mut game);
    let wage = game.players[0].wage();
    let contract_end = game.players[0].contract_end().map(str::to_string);
    let offers: Vec<_> = game.players[0]
        .transfer_offers
        .iter()
        .map(|o| (o.id.clone(), o.fee, o.wage_offered, o.status.clone()))
        .collect();
    let value_before = game.players[0].market_value;

    revalue_after_season(
        &mut game.players[0],
        &game.value_scale.clone(),
        season_end(),
        2026,
    );

    assert!(
        game.players[0].market_value < value_before / 2,
        "the veteran's value should have fallen sharply: {}",
        game.players[0].market_value
    );
    let kept: Vec<_> = game.players[0]
        .transfer_offers
        .iter()
        .map(|o| (o.id.clone(), o.fee, o.wage_offered, o.status.clone()))
        .collect();
    assert_eq!(kept, offers);
    assert_eq!(game.players[0].transfer_offers[0].id, offer_id);
    assert_eq!(game.players[0].transfer_offers[0].fee, 600_000);
    assert_eq!(game.players[0].wage(), wage);
    assert_eq!(
        game.players[0].contract_end().map(str::to_string),
        contract_end
    );
}

/// Given a revalued player sold to another club at an agreed fee,
/// When the deal completes,
/// Then he arrives with his value and his basis untouched (a transfer is not a revaluation), the
/// seller is paid the agreed fee rather than his estimated value, and the next season's end
/// carries his value on from the basis he brought (his new club inherits no fresh anchor).
#[test]
fn a_transfer_moves_the_player_with_his_value_and_basis_and_pays_the_agreed_fee() {
    let mut game = game_with_a_declining_veteran();
    revalue_after_season(
        &mut game.players[0],
        &game.value_scale.clone(),
        season_end(),
        2026,
    );
    let (value, basis) = (game.players[0].market_value, game.players[0].value_basis);
    let seller_cash = game.teams[0].finance;
    let buyer_cash = game.teams[1].finance;
    let id = game.players[0].id.clone();

    // Forwards enough behind him to keep the seller above the squad floor.
    for n in 0..16 {
        let mut cover = game.players[0].clone();
        cover.id = format!("cover-{n}");
        cover.transfer_offers.clear();
        game.players.push(cover);
    }
    let terms = domain::transfer_terms::TransferTerms::default();
    let outcome = execute_transfer_with_terms(&mut game, &id, "team2", "team1", 600_000, &terms);
    if let Err(error) = &outcome {
        // The squad floor is not what this test is about; skip loudly rather than pass vacuously.
        panic!("the transfer was refused ({error}): this fixture no longer reaches the money path");
    }

    let moved = game.players.iter().find(|p| p.id == id).unwrap();
    assert_eq!(moved.team_id.as_deref(), Some("team2"));
    assert_eq!((moved.market_value, moved.value_basis), (value, basis));
    assert_eq!(game.teams[0].finance, seller_cash + 600_000);
    assert_eq!(game.teams[1].finance, buyer_cash - 600_000);
    assert!(
        moved
            .movement_history
            .iter()
            .any(|entry| entry.kind == PlayerMovementKind::PermanentTransfer)
    );
}

/// Given a veteran on loan across a season's end,
/// When the season is revalued while he is away and his loan then ends,
/// Then he was revalued once, and coming home neither resets his value nor revalues him again.
#[test]
fn a_loan_return_neither_resets_nor_revalues_the_player() {
    let mut game = game_with_a_declining_veteran();
    game.players[0].team_id = Some("team2".to_string());
    game.players[0].active_loan = Some(ActiveLoan {
        parent_team_id: "team1".to_string(),
        loan_team_id: "team2".to_string(),
        loan_team_name: None,
        start_date: "2025-08-01".to_string(),
        end_date: "2026-06-30".to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
        terms_source: LoanTermsSource::Estimated,
    });
    apply_seasonal_aging(&mut game, season_end(), 2026);
    let away = &game.players[0];
    // Carried from the basis taken a year before (90 rated at 33) to wherever the season left him.
    let expected = game.value_scale.carry(
        20_000_000,
        crate::value_scale::Standing {
            ovr: 90,
            potential: 90,
            age: 33,
        },
        crate::value_scale::Standing {
            ovr: away.ovr,
            potential: away.potential.max(away.ovr),
            age: 34,
        },
    );
    assert_eq!(away.market_value, expected, "revalued while on loan");
    assert!(away.market_value < 20_000_000);
    let (value, basis) = (away.market_value, away.value_basis);
    assert_eq!(basis.map(|b| b.revalued_year), Some(2026));

    game.clock.current_date = NaiveDate::from_ymd_opt(2026, 6, 30)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    super::loans::process_loan_returns(&mut game);

    let home = &game.players[0];
    assert_eq!(home.team_id.as_deref(), Some("team1"), "he came home");
    assert!(home.active_loan.is_none());
    assert_eq!((home.market_value, home.value_basis), (value, basis));
}
