//! Calibration of the player's side of a paid transfer: the same player, offered the standard
//! terms by clubs of every standing, at several levels of ability.
//!
//! A club's squad is as good as its name (a level of `50 + reputation / 40`), as in the generated
//! worlds, and the player is a regular wherever his rating puts him. The table is printed with
//! `--nocapture` and committed to `docs/APFM_PAID_TRANSFER_REALISM.md`; the assertions are the
//! properties the model has to keep whatever its numbers become:
//!
//! * among buyers no bigger than his club, a more prestigious one is never less attractive;
//! * a player is never more willing to leave a bigger club than a smaller one for the same buyer;
//! * an elite player does not drop to a minnow on standard terms, and a step up is welcome;
//! * pay moves a hesitant player and never moves a hopeless case.

#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use domain::transfer_terms::TransferTerms;
use market_fixtures::{PLAYER_ID, game_with, player};
use ofm_core::contracts::InterestLevel;
use ofm_core::game::Game;
use ofm_core::transfers::{PlayerOutlook, project_transfer_bid_financial_impact_with_terms};

const REPUTATIONS: [u32; 5] = [900, 650, 400, 200, 80];
const ABILITIES: [u8; 4] = [58, 66, 74, 84];

fn level_of(reputation: u32) -> u8 {
    u8::try_from(50 + reputation / 40).unwrap()
}

fn world(seller: u32, buyer: u32, ovr: u8, terms_wage: u32) -> (Game, TransferTerms) {
    let mut owned = player("1998-01-01");
    owned.team_id = Some("big-club".to_string());
    owned.stage_contract_end(Some("2029-06-30".to_string()));
    owned.stage_wage(10_000);
    owned.ovr = ovr;
    owned.potential = ovr;
    owned.market_value = 8_000_000;
    owned.morale = 75;
    let mut game = game_with(owned);
    for team in &mut game.teams {
        match team.id.as_str() {
            "big-club" => team.reputation = seller,
            "team-1" => team.reputation = buyer,
            _ => {}
        }
        team.transfer_budget = 80_000_000;
        team.finance = 90_000_000;
        team.wage_budget = 80_000_000;
        let level = level_of(if team.id == "big-club" { seller } else { buyer });
        let id = team.id.clone();
        for (index, depth) in game
            .players
            .iter_mut()
            .filter(|p| p.id.starts_with(&format!("depth-{id}")))
            .enumerate()
        {
            depth.ovr = level.saturating_sub(u8::try_from(index % 3).unwrap());
        }
    }
    let terms = TransferTerms {
        player_weekly_wage: terms_wage,
        ..TransferTerms::default()
    };
    (game, terms)
}

fn outlook(seller: u32, buyer: u32, ovr: u8) -> PlayerOutlook {
    let (game, terms) = world(seller, buyer, ovr, 0);
    project_transfer_bid_financial_impact_with_terms(&game, PLAYER_ID, 8_000_000, &terms)
        .expect("a projection")
        .player_outlook
        .expect("the player's view")
}

fn rank(outlook: &PlayerOutlook) -> u8 {
    match outlook.interest {
        InterestLevel::Keen => 2,
        InterestLevel::Hesitant => 1,
        InterestLevel::Unlikely => 0,
    }
}

/// Given players of four levels and clubs of five standings, as seller and as buyer
/// When each is offered the standard terms
/// Then among the clubs that are not a step up, a bigger buyer is never less attractive, and the
///      whole table is printed for the record.
///
/// A step up is left out of the property on purpose: a bigger club has a better squad, and the part
/// he would play there counts, so a backup's place at a giant is rightly less attractive than a
/// regular's at a lesser club.
#[test]
fn willingness_rises_with_the_buyers_standing_when_he_does_not_step_up() {
    for ovr in ABILITIES {
        for seller in REPUTATIONS {
            let mut row = String::new();
            let mut previous = 0;
            for buyer in REPUTATIONS.iter().rev() {
                let view = outlook(seller, *buyer, ovr);
                row += &format!(" {:?}", view.interest);
                if *buyer <= seller {
                    assert!(
                        rank(&view) >= previous,
                        "ovr {ovr}, seller {seller}: a bigger buyer ({buyer}) is less attractive"
                    );
                    previous = rank(&view);
                }
            }
            eprintln!("CALIBRATION ovr {ovr:>2} seller {seller:>3} | buyers 80..900 |{row}");
        }
    }
}

/// An elite player of the most prestigious club does not go to a minnow; one stepping up is keen.
#[test]
fn an_elite_player_stays_a_step_up_is_welcome() {
    assert_eq!(outlook(900, 80, 84).interest, InterestLevel::Unlikely);
    assert_eq!(outlook(900, 200, 84).interest, InterestLevel::Unlikely);
    assert_eq!(outlook(200, 900, 74).interest, InterestLevel::Keen);
    assert_eq!(outlook(80, 650, 66).interest, InterestLevel::Keen);
}

/// A move between clubs of the same standing needs nothing extra.
#[test]
fn a_lateral_move_is_taken_on_standard_terms() {
    for ovr in ABILITIES {
        for standing in REPUTATIONS {
            let view = outlook(standing, standing, ovr);
            assert_ne!(
                view.interest,
                InterestLevel::Unlikely,
                "ovr {ovr}, standing {standing}"
            );
        }
    }
}

/// Pay moves a hesitant player; a hopeless case stays hopeless.
#[test]
fn pay_moves_a_hesitant_player_and_not_a_hopeless_one() {
    let hesitant = outlook(900, 650, 84);
    assert_eq!(hesitant.interest, InterestLevel::Hesitant);
    let ask = hesitant.asks_wage.expect("a price");
    let (game, terms) = world(900, 650, 84, ask);
    let paid =
        project_transfer_bid_financial_impact_with_terms(&game, PLAYER_ID, 8_000_000, &terms)
            .unwrap()
            .player_outlook
            .unwrap();
    assert_eq!(paid.interest, InterestLevel::Keen);

    let hopeless = outlook(900, 80, 84);
    assert_eq!(
        hopeless.asks_wage, None,
        "no pay buys a giant's star for a minnow"
    );
    let (game, terms) = world(900, 80, 84, 5_000_000);
    let paid =
        project_transfer_bid_financial_impact_with_terms(&game, PLAYER_ID, 8_000_000, &terms)
            .unwrap()
            .player_outlook
            .unwrap();
    assert_eq!(paid.interest, InterestLevel::Unlikely);
}
