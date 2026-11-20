//! What the players list may tell the manager about a rating: the row, the sort and the filter
//! all read the club's knowledge of the player, never the true `ovr`.

use chrono::{TimeZone, Utc};
use domain::league::League;
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position};
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::rating_view::{PublicStanding, RatingView};
use ofm_core::scouting_dossier::{Confidence, Range, ScoutReport};
use ofm_core::slices::players::{
    PlayerScope, PlayerSortKey, PlayerStatusFilter, PlayersPageQuery, ScoutingFilter, query_page,
};

fn attrs(value: u8) -> PlayerAttributes {
    serde_json::from_value(serde_json::json!({
        "pace": value, "stamina": value, "strength": value, "agility": value,
        "passing": value, "shooting": value, "tackling": value, "dribbling": value,
        "defending": value, "positioning": value, "vision": value, "decisions": value,
        "composure": value, "aggression": value, "teamwork": value, "leadership": value,
        "handling": value, "reflexes": value, "aerial": value
    }))
    .unwrap()
}

fn team(id: &str) -> Team {
    Team::new(
        id.to_string(),
        id.to_string(),
        id.chars().take(3).collect(),
        "GB".to_string(),
        "City".to_string(),
        "Ground".to_string(),
        20_000,
    )
}

fn player(id: &str, club: Option<&str>, ovr: u8) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "2000-01-01".to_string(),
        "GB".to_string(),
        Position::Midfielder,
        attrs(ovr),
    );
    player.team_id = club.map(str::to_string);
    player.ovr = ovr;
    player
}

/// The manager coaches "mine"; "rival" shares his division; "faraway" does not.
fn game(players: Vec<Player>) -> Game {
    let mut manager = Manager::new(
        "m".to_string(),
        "M".to_string(),
        "G".to_string(),
        "1980-01-01".to_string(),
        "GB".to_string(),
    );
    manager.team_id = Some("mine".to_string());
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 9, 15, 12, 0, 0).unwrap());
    let teams = vec![team("mine"), team("rival"), team("faraway")];
    let mut game = Game::new(clock, manager, teams, players, vec![], vec![]);
    let table = |id: &str, clubs: &[&str]| {
        let clubs: Vec<String> = clubs.iter().map(|c| c.to_string()).collect();
        League::new(id.to_string(), id.to_string(), 2026, &clubs)
    };
    game.competitions = vec![
        table("division-one", &["mine", "rival"]),
        table("division-two", &["faraway"]),
    ];
    game.season_context.season_start = Some("2026-08-01".to_string());
    game
}

fn query(sort_key: PlayerSortKey, sort_asc: bool) -> PlayersPageQuery {
    PlayersPageQuery {
        search: None,
        position: None,
        positions: vec![],
        age_min: None,
        age_max: None,
        ovr_min: None,
        ovr_max: None,
        value_min: None,
        value_max: None,
        in_budget: false,
        scope: PlayerScope::All,
        team_id: None,
        scouting: ScoutingFilter::Any,
        potential_min: None,
        contract_expiring: false,
        status: PlayerStatusFilter::All,
        sort_key,
        sort_asc,
        page: 1,
        page_size: 50,
    }
}

fn order(game: &Game, query: &PlayersPageQuery) -> Vec<String> {
    query_page(game, query)
        .items
        .into_iter()
        .map(|item| item.id)
        .collect()
}

fn report(player_id: &str, scouted_on: &str, ability: Range) -> ScoutReport {
    ScoutReport {
        player_id: player_id.to_string(),
        scout_id: "s".to_string(),
        scout_name: "S".to_string(),
        scouted_on: scouted_on.to_string(),
        judging_ability: 70,
        judging_potential: 70,
        reports: 1,
        ability,
        potential: None,
        age: 26,
        strengths: vec![],
        weaknesses: vec![],
    }
}

fn rating_of(game: &Game, id: &str) -> RatingView {
    query_page(game, &query(PlayerSortKey::Name, true))
        .items
        .into_iter()
        .find(|item| item.id == id)
        .unwrap()
        .rating
}

/// Given an own player, a division rival, an unscouted player elsewhere and a reported one
/// When the list is projected
/// Then each row carries the rating the manager may read, and the unscouted row carries no number
#[test]
fn each_row_carries_the_rating_the_manager_may_read() {
    let mut game = game(vec![
        player("own", Some("mine"), 71),
        player("rival-player", Some("rival"), 66),
        player("stranger", Some("faraway"), 74),
        player("reported", Some("faraway"), 69),
    ]);
    game.scouting_dossier.reports.push(report(
        "reported",
        "2026-09-01",
        Range { low: 65, high: 72 },
    ));

    assert_eq!(rating_of(&game, "own"), RatingView::Exact { ovr: 71 });
    assert_eq!(
        rating_of(&game, "rival-player"),
        RatingView::Exact { ovr: 66 }
    );
    assert_eq!(
        rating_of(&game, "stranger"),
        RatingView::Standing {
            standing: PublicStanding::Good
        }
    );
    assert_eq!(
        rating_of(&game, "reported"),
        RatingView::Estimate {
            low: 65,
            high: 72,
            confidence: Confidence::Moderate,
            current: true,
            scouted_on: "2026-09-01".to_string(),
        }
    );
}

/// Given unscouted players whose true ratings are 71, 74 and 76, all one standing
/// When the list is sorted by rating, best first and worst first
/// Then they come out in the same (name) order both ways: the sort cannot rank them
#[test]
fn sorting_unscouted_players_in_one_standing_reveals_no_ranking() {
    let game = game(vec![
        player("c", Some("faraway"), 71),
        player("a", Some("faraway"), 76),
        player("b", Some("faraway"), 74),
    ]);

    let best_first = order(&game, &query(PlayerSortKey::Ovr, false));
    let worst_first = order(&game, &query(PlayerSortKey::Ovr, true));

    assert_eq!(best_first, vec!["a", "b", "c"]);
    assert_eq!(worst_first, best_first);
}

/// Given unscouted players in different standings
/// When the list is sorted by rating
/// Then the standings still order the list, so the market stays usable
#[test]
fn sorting_by_rating_still_orders_the_standings() {
    let game = game(vec![
        player("average", Some("faraway"), 64),
        player("elite", Some("faraway"), 85),
        player("good", Some("faraway"), 72),
    ]);

    assert_eq!(
        order(&game, &query(PlayerSortKey::Ovr, false)),
        vec!["elite", "good", "average"]
    );
    assert_eq!(
        order(&game, &query(PlayerSortKey::Ovr, true)),
        vec!["average", "good", "elite"]
    );
}

/// Given exact and reported players mixed with an unscouted one
/// When sorted by rating
/// Then exact sorts by its number, a range by its middle, a standing by the middle of its span
#[test]
fn exact_ranges_and_standings_sort_by_what_the_manager_reads() {
    let mut game = game(vec![
        player("own", Some("mine"), 70),
        player("reported", Some("faraway"), 90),
        player("stranger", Some("faraway"), 90),
    ]);
    // The scout read him at 60 to 64 although he is 90: the screen must follow the scout.
    game.scouting_dossier.reports.push(report(
        "reported",
        "2026-09-01",
        Range { low: 60, high: 64 },
    ));

    assert_eq!(
        order(&game, &query(PlayerSortKey::Ovr, false)),
        vec!["stranger", "own", "reported"]
    );
}

/// Given an unscouted player of 71 and another of 75 in the same standing
/// When the list is filtered at 73 or more, and at 72 or less
/// Then both pass both filters: the bound cannot be used to find the true rating
#[test]
fn a_rating_filter_cannot_probe_an_unscouted_players_true_rating() {
    let game = game(vec![
        player("low", Some("faraway"), 71),
        player("high", Some("faraway"), 75),
    ]);

    let mut at_least_73 = query(PlayerSortKey::Name, true);
    at_least_73.ovr_min = Some(73);
    let mut at_most_72 = query(PlayerSortKey::Name, true);
    at_most_72.ovr_max = Some(72);

    assert_eq!(order(&game, &at_least_73), vec!["high", "low"]);
    assert_eq!(order(&game, &at_most_72), vec!["high", "low"]);
}

/// Given an unscouted player, a division rival and an own player
/// When the list is filtered by rating bounds that exclude them as exact numbers
/// Then exactly known players are filtered exactly, and the standing's span still excludes a
///      standing wholly outside the bounds
#[test]
fn a_rating_filter_is_exact_for_exact_players_and_excludes_whole_standings() {
    let game = game(vec![
        player("own", Some("mine"), 71),
        player("rival-player", Some("rival"), 66),
        player("elite", Some("faraway"), 88),
    ]);

    let mut query = query(PlayerSortKey::Name, true);
    query.ovr_min = Some(70);
    query.ovr_max = Some(75);

    assert_eq!(order(&game, &query), vec!["own"]);
}
