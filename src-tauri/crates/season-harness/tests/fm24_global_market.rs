//! The world-wide market of the FM24 world (mission 8).
//!
//! In a career scoped to the player's league only that league's 18 clubs used to buy: clubs abroad
//! sold but never bought, lent but never borrowed. Now every AI club shops on its own weekday, its
//! own country first. These tests play the world through the production day and rollover and read
//! what the market did where it matters: who bought, whether a player is ever owned twice, what
//! a save keeps of a market in motion, and whether the wage bills still drift.
//!
//! The FM24 world is the player's own file (`APFM_WORLD_JSON`), never in the repository: each test
//! returns at once when it is not set.

use std::collections::{BTreeMap, HashMap, HashSet};

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use ofm_core::finances::calc_wages;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const LENS: &str = "fm-c-501";

/// Watches nothing.
struct Quiet;

impl SeasonObserver for Quiet {}

/// Counts, day by day, the permanent moves between two AI clubs and the loans they start, by the
/// buyer's place in the career's scope and by country.
#[derive(Default)]
struct Market {
    team_of: HashMap<String, Option<String>>,
    in_scope: HashSet<String>,
    country: HashMap<String, String>,
    transfers: u32,
    by_buyer_out_of_scope: u32,
    cross_country: u32,
    double_owned: u32,
}

impl Market {
    fn opening(game: &Game) -> Self {
        let mut market = Self {
            in_scope: game.active_team_ids().expect("a scoped career"),
            country: game
                .teams
                .iter()
                .map(|t| (t.id.clone(), t.country.clone()))
                .collect(),
            ..Self::default()
        };
        market.remember(game);
        market
    }

    fn remember(&mut self, game: &Game) {
        self.team_of = game
            .players
            .iter()
            .map(|p| (p.id.clone(), p.team_id.clone()))
            .collect();
    }
}

impl SeasonObserver for Market {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let user = game.manager.team_id.clone().unwrap_or_default();
        for player in &game.players {
            let Some(Some(before)) = self.team_of.get(&player.id) else {
                continue;
            };
            let Some(after) = player.team_id.as_deref() else {
                continue;
            };
            if before == after || player.active_loan.is_some() || before == &user || after == user {
                continue;
            }
            self.transfers += 1;
            if !self.in_scope.contains(after) {
                self.by_buyer_out_of_scope += 1;
            }
            if self.country.get(before) != self.country.get(after) {
                self.cross_country += 1;
            }
        }
        if game.clock.current_date.format("%d").to_string() == "01" {
            let mut seen = HashSet::new();
            self.double_owned += game
                .players
                .iter()
                .filter(|p| !seen.insert(p.id.as_str()))
                .count() as u32;
        }
        self.remember(game);
    }
}

/// Given a career scoped to Ligue 1's league,
/// When a season is played through the production day and rollover,
/// Then clubs outside that scope have bought players, some from another country, most from their
///      own, and no player is ever listed twice.
#[test]
fn clubs_outside_the_players_league_buy_players() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let mut market = Market::opening(&game);
    assert!(
        market.in_scope.len() < game.teams.len() / 4,
        "the career is scoped to a single league"
    );
    run_seasons(&mut game, RunOptions::seasons(1), &mut market).expect("a season plays");

    assert!(
        market.transfers >= 60,
        "{} transfers between AI clubs in a season",
        market.transfers
    );
    assert!(
        market.by_buyer_out_of_scope * 2 >= market.transfers,
        "{} of {} transfers were bought by a club outside the scope",
        market.by_buyer_out_of_scope,
        market.transfers
    );
    assert!(
        market.cross_country > 0 && market.cross_country * 2 < market.transfers,
        "{} of {} transfers crossed a border: expected some, but not most",
        market.cross_country,
        market.transfers
    );
    assert_eq!(market.double_owned, 0, "a player was owned twice");
}

/// Given a market in motion (players on loan, transfers made, a season played),
/// When the game is saved and read back,
/// Then every player has the same club, loan, wage and contract end, and every club the same
///      wage budget and cash.
#[test]
fn a_market_in_motion_survives_a_save_and_a_reload() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    // A season, then the first weeks of the next: loans last until a season's end, and the
    // summer window is when they are made.
    let options = RunOptions::seasons(1);
    run_seasons(&mut game, options, &mut Quiet).expect("a season plays");
    for day in 1..=60 {
        play_day(&mut game, options, 2, day, &mut Quiet).expect("a day plays");
    }
    assert!(
        game.players.iter().any(|p| p.active_loan.is_some()),
        "a loan is in progress"
    );

    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "market")
        .expect("the save");
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&id)
        .unwrap();

    let players = |g: &Game| {
        g.players
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    (
                        p.team_id.clone(),
                        p.active_loan.as_ref().map(|l| l.loan_team_id.clone()),
                        p.wage(),
                        p.contract_end().map(str::to_string),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(players(&game), players(&reloaded));
    let clubs = |g: &Game| {
        g.teams
            .iter()
            .map(|t| (t.id.clone(), (t.wage_budget, t.finance, t.transfer_budget)))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(clubs(&game), clubs(&reloaded));
    assert_eq!(calc_wages(&game, LENS), calc_wages(&reloaded, LENS));
}

/// Given five seasons of a career,
/// When the clubs' wage bills are read against their budgets,
/// Then no club is far over its budget: the drift (12 clubs above 150 % by the fifth season before
///      the budgets were reviewed and renewals were held to what the club can carry) is gone, and
///      the clubs still keep their players (renewals go on).
#[test]
fn wage_bills_no_longer_drift_away_from_the_budgets() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    run_seasons(&mut game, RunOptions::seasons(5), &mut Quiet).expect("five seasons play");

    let user = game.manager.team_id.clone().unwrap_or_default();
    let over_150 = game
        .teams
        .iter()
        .filter(|t| t.id != user && t.wage_budget > 0)
        .filter(|t| calc_wages(&game, &t.id) * 100 > t.wage_budget * 150)
        .count();
    assert!(over_150 <= 2, "{over_150} clubs above 150 % of their wage budget");
    let thin = game
        .teams
        .iter()
        .filter(|t| t.id != user)
        .filter(|t| {
            game.players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(t.id.as_str()))
                .count()
                < 18
        })
        .count();
    assert_eq!(thin, 0, "clubs below 18 players: they keep their squads");
}
