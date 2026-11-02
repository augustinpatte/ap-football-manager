//! The FM24 free agents, in the world a career is played in.
//!
//! The pool is built by `apfm-import` from the save's unattached players and priced on the scale of
//! the save's own contracts. These tests read it where it matters: at the opening of a career, when
//! the player signs one, when the game is saved and read back, and across the season in which AI
//! clubs draw on it. The FM24 world is the player's own file (`APFM_WORLD_JSON`), never in the
//! repository: each test returns at once when it is not set.

use db::save_manager::SaveManager;
use domain::player::Player;
use domain::stats::StatsState;
use ofm_core::contracts::{
    FreeAgentOfferDecision, OfferTerms, process_free_agent_talks, submit_free_agent_offer,
};
use ofm_core::finances::calc_wages;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

const LENS: &str = "fm-c-501";

fn fm24_free_agents(game: &Game) -> Vec<&Player> {
    game.players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-") && p.team_id.is_none() && !p.retired)
        .collect()
}

/// Given the opening of a career in the FM24 world,
/// When the free agents that came from the save are counted,
/// Then there is a pool of thousands, each with no club and no contract, a value, and a wage asked
///      that is on the scale of what contracted players earn (not of OFM's `value / 200`).
#[test]
fn the_world_opens_with_a_pool_of_fm24_free_agents_priced_on_the_saves_scale() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let pool = fm24_free_agents(&game);
    assert!(
        (2_500..=3_500).contains(&pool.len()),
        "{} FM24 free agents",
        pool.len()
    );
    for p in &pool {
        assert!(p.contract_end().is_none(), "{} has a contract", p.id);
        assert!(p.market_value > 0, "{} has no value", p.id);
        assert!(p.wage() > 0, "{} asks nothing", p.id);
    }
    let median = |mut v: Vec<u32>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    let asked = median(pool.iter().map(|p| p.wage()).collect());
    let paid = median(
        game.players
            .iter()
            .filter(|p| p.id.starts_with("fm-p-") && p.team_id.is_some())
            .map(|p| p.wage())
            .collect(),
    );
    assert!(
        asked <= paid && asked * 10 >= paid,
        "free agents ask {asked} a week at the median, contracted players earn {paid}"
    );
}

/// Given an FM24 free agent,
/// When the player's club offers him exactly what he asks over three years,
/// Then he signs for that wage, the club's bill rises by it, and he is the club's.
#[test]
fn the_player_can_sign_an_fm24_free_agent_at_the_wage_he_asks() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let target = fm24_free_agents(&game)
        .into_iter()
        .filter(|p| p.ovr >= 55)
        .min_by_key(|p| (p.wage(), p.id.clone()))
        .map(|p| (p.id.clone(), p.wage()))
        .expect("an affordable free agent");
    let bill = calc_wages(&game, LENS);

    // His ask is a floor: he wants a little more than he last had. Offer 40 % over.
    let wage = target.1 + target.1 * 2 / 5;
    let outcome = submit_free_agent_offer(
        &mut game,
        &target.0,
        OfferTerms {
            weekly_wage: wage,
            contract_years: 3,
            signing_bonus: 0,
            promise_stepping_stone: false,
        },
    )
    .expect("the offer is taken");
    assert_eq!(outcome.decision, FreeAgentOfferDecision::Pending);
    for _ in 0..3 {
        game.clock.current_date += chrono::Duration::days(1);
        process_free_agent_talks(&mut game);
    }

    let signed = game.players.iter().find(|p| p.id == target.0).unwrap();
    assert_eq!(signed.team_id.as_deref(), Some(LENS));
    assert_eq!(signed.wage(), wage);
    assert_eq!(calc_wages(&game, LENS), bill + i64::from(wage));
}

/// Given the pool at the opening,
/// When the game is saved and read back,
/// Then every free agent is still there with his wage and value: the wage a free agent asks is
///      written to the save like any other.
#[test]
fn the_pool_survives_a_save_and_a_reload() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "pool")
        .expect("the save");
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&id)
        .unwrap();

    let before: std::collections::BTreeMap<_, _> = fm24_free_agents(&game)
        .into_iter()
        .map(|p| (p.id.clone(), (p.wage(), p.market_value, p.ovr)))
        .collect();
    let after: std::collections::BTreeMap<_, _> = fm24_free_agents(&reloaded)
        .into_iter()
        .map(|p| (p.id.clone(), (p.wage(), p.market_value, p.ovr)))
        .collect();
    assert_eq!(before.len(), after.len(), "free agents lost by the save");
    assert_eq!(before, after);
}

/// Given the opening of a career,
/// When a season is played through the production day and rollover,
/// Then AI clubs have signed FM24 free agents (the pool is drawn on), none of them above the
///      wage budget by more than the board's soft cap, and the harness invariants hold.
#[test]
fn ai_clubs_draw_on_the_pool_within_their_budgets() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    let before: std::collections::BTreeSet<String> = fm24_free_agents(&game)
        .into_iter()
        .map(|p| p.id.clone())
        .collect();
    let mut invariants = Invariants::new(&game);
    run_seasons(&mut game, RunOptions::seasons(1), &mut invariants).expect("a season plays");

    let signed: Vec<&Player> = game
        .players
        .iter()
        .filter(|p| before.contains(&p.id) && p.team_id.is_some())
        .collect();
    assert!(
        signed.len() >= 100,
        "only {} FM24 free agents found a club in a season",
        signed.len()
    );
    for team in game
        .teams
        .iter()
        .filter(|t| Some(t.id.as_str()) != game.manager.team_id.as_deref())
    {
        if signed
            .iter()
            .any(|p| p.team_id.as_deref() == Some(team.id.as_str()))
        {
            let bill = calc_wages(&game, &team.id);
            assert!(
                bill * 100 <= team.wage_budget * 200,
                "{} pays {bill} a week against a budget of {}",
                team.name,
                team.wage_budget
            );
        }
    }
    assert!(invariants.is_clean(), "{}", invariants.report());
}
