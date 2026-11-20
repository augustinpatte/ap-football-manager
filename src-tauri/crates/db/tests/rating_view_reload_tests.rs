//! What the manager may read of a rating, saved to a real `.db` file and read back by a fresh
//! `SaveManager`: the club's reports are part of the save, so the reading must be the same after a
//! reload, and an old report must stay old and stay as written when the player has changed.

#[path = "../../ofm_core/tests/market_fixtures/mod.rs"]
#[expect(
    dead_code,
    reason = "a shared fixture: this file uses only part of it; owner=#589"
)]
mod market_fixtures;

use db::save_manager::SaveManager;
use domain::league::{Fixture, FixtureCompetition, FixtureStatus, League, StandingEntry};
use market_fixtures::{PLAYER_ID, game_with, rated_player};
use ofm_core::game::Game;
use ofm_core::rating_view::{PublicStanding, RatingView, rating_view};
use ofm_core::scouting_dossier::{Confidence, Range, ScoutReport};

/// The manager's own player, a stranger nobody scouted, and a stranger a scout reported on before
/// this season began, who has since grown from 61 to 78.
fn career() -> Game {
    let mut reported = rated_player("1999-01-01", 78);
    reported.team_id = Some("big-club".to_string());
    let mut game = game_with(reported);
    let mut unknown = rated_player("2000-01-01", 74);
    unknown.id = "unknown".to_string();
    unknown.team_id = Some("big-club".to_string());
    let mut own = rated_player("1997-01-01", 69);
    own.id = "own".to_string();
    own.team_id = Some("team-1".to_string());
    game.players.push(unknown);
    game.players.push(own);
    // The strangers' club is not in the manager's league. The season's start is derived from the
    // league's fixtures, on this side of a reload and on
    // the other: a report is "old" against that date.
    game.league = Some(League {
        id: "league".to_string(),
        name: "League".to_string(),
        season: 2026,
        fixtures: vec![Fixture {
            id: "fx1".to_string(),
            matchday: 1,
            date: "2026-07-01".to_string(),
            home_team_id: "team-1".to_string(),
            away_team_id: "small-club".to_string(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            ..Default::default()
        }],
        standings: vec![
            StandingEntry::new("team-1".to_string()),
            StandingEntry::new("small-club".to_string()),
        ],
        ..Default::default()
    });
    ofm_core::season_context::refresh_game_context(&mut game);
    assert_eq!(
        game.season_context.season_start.as_deref(),
        Some("2026-07-01")
    );
    game.scouting_dossier.reports.push(ScoutReport {
        player_id: PLAYER_ID.to_string(),
        scout_id: "scout".to_string(),
        scout_name: "Sam Scout".to_string(),
        scouted_on: "2026-03-10".to_string(),
        judging_ability: 70,
        judging_potential: 70,
        reports: 1,
        ability: Range { low: 57, high: 65 },
        potential: None,
        age: 27,
        strengths: vec![],
        weaknesses: vec![],
    });
    game
}

fn reading(game: &Game, id: &str) -> RatingView {
    rating_view(game, game.players.iter().find(|p| p.id == id).unwrap())
}

/// Given a career with an own player, an unscouted stranger and a stranger reported on last season
/// When it is saved to disk and loaded by a fresh save manager
/// Then each reads as before: exact, a standing, and the old range still marked old, although the
///      reported player is now rated 78, and the saved file holds the true ratings the simulation needs
#[test]
fn the_readings_survive_a_save_and_a_reload_and_an_old_report_stays_old() {
    let game = career();
    let before: Vec<RatingView> = [PLAYER_ID, "unknown", "own"]
        .iter()
        .map(|id| reading(&game, id))
        .collect();
    assert_eq!(
        before,
        vec![
            RatingView::Estimate {
                low: 57,
                high: 65,
                confidence: Confidence::Moderate,
                current: false,
                scouted_on: "2026-03-10".to_string(),
            },
            RatingView::Standing {
                standing: PublicStanding::Good
            },
            RatingView::Exact { ovr: 69 },
        ]
    );

    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    let id = SaveManager::init(&saves)
        .unwrap()
        .create_save(&game, "Ratings")
        .unwrap();
    let reloaded = SaveManager::init(&saves).unwrap().load_game(&id).unwrap();

    let after: Vec<RatingView> = [PLAYER_ID, "unknown", "own"]
        .iter()
        .map(|id| reading(&reloaded, id))
        .collect();
    assert_eq!(before, after);
    // The simulation's values are untouched by the projection: the save still holds the truth.
    assert_eq!(
        reloaded
            .players
            .iter()
            .find(|p| p.id == "unknown")
            .unwrap()
            .ovr,
        74
    );
}
