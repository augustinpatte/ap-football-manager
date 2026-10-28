//! The squads of the FM24 world, as the save registers them (roster audit).
//!
//! The adapter places each player by the team the save registers him with, not by his contract
//! club: a reserve or youth side is the academy, a loan carries its owner. These tests read that
//! where a career lives: at its opening for Paris Saint-Germain, through the squad projection the
//! squad screen uses, and across a save and a load. The identifiers are person ids of world.db,
//! checked there. The FM24 world is the player's own file (`APFM_WORLD_JSON`), never in the
//! repository: each test returns at once when it is not set.

use db::save_manager::SaveManager;
use domain::player::SquadRole;
use domain::stats::StatsState;
use ofm_core::game::Game;
use ofm_core::slices::squad::query_squad;
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const PSG: &str = "fm-c-498";

fn career() -> Option<Game> {
    let path = world_path_from_env()?;
    Some(build_fm24_career(&path, PSG, 7).expect("the career builds"))
}

fn player<'a>(game: &'a Game, id: &str) -> &'a domain::player::Player {
    game.players.iter().find(|p| p.id == id).expect(id)
}

/// Given a PSG career,
/// When the squad screen's projection is read,
/// Then Joao Neves (person 47802, first team) is a senior, Kais Anelka (77083, under-18 side,
///      born 2008-06-24) is in the academy, and the players the save lends out are not listed.
#[test]
fn the_squad_screen_separates_first_team_academy_and_loans() {
    let Some(game) = career() else {
        return;
    };
    let squad = query_squad(&game, PSG);
    let row = |id: &str| squad.iter().find(|s| s.player.id == id);
    assert_eq!(
        row("fm-p-47802").unwrap().player.squad_role,
        SquadRole::Senior
    );
    assert_eq!(
        row("fm-p-77083").unwrap().player.squad_role,
        SquadRole::Youth
    );
    assert_eq!(
        row("fm-p-77083").unwrap().player.date_of_birth,
        "2008-06-24"
    );
    let seniors = squad
        .iter()
        .filter(|s| s.player.squad_role == SquadRole::Senior)
        .count();
    let academy = squad.len() - seniors;
    assert_eq!((seniors, academy), (24, 45));
    for lent in [
        "fm-p-63609",
        "fm-p-62608",
        "fm-p-60572",
        "fm-p-71487",
        "fm-p-74489",
    ] {
        assert!(row(lent).is_none(), "{lent} is on loan, not in the squad");
    }
}

/// Given the five PSG players the save registers with other clubs,
/// When the career opens,
/// Then each is on loan with PSG as owner, keeps PSG's contract, and the loan to a club outside the
///      world (Nacional, Pafos) carries that club's name.
#[test]
fn a_player_registered_elsewhere_is_on_loan_from_his_owner() {
    let Some(game) = career() else {
        return;
    };
    for (id, team, name) in [
        ("fm-p-63609", "fm-c-1107", None),
        ("fm-p-62608", "fm-c-1600", Some("Clube Desportivo Nacional")),
        ("fm-p-60572", "fm-c-16401", Some("Pafos FC")),
    ] {
        let p = player(&game, id);
        let loan = p.active_loan.as_ref().expect(id);
        assert_eq!(p.team_id.as_deref(), Some(team), "{id}");
        assert_eq!(loan.parent_team_id, PSG, "{id}");
        assert_eq!(loan.loan_team_name.as_deref(), name, "{id}");
        assert_eq!(p.contract_club_id(), Some(PSG), "{id}");
        assert!(p.contract_end().is_some(), "{id} keeps his contract");
    }
}

/// Given a PSG career saved and loaded,
/// When the loans and roles are compared,
/// Then nothing changed: the new loan field survives the save.
#[test]
fn loans_and_roles_survive_a_save() {
    let Some(game) = career() else {
        return;
    };
    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "rosters")
        .expect("the save");
    let reloaded = SaveManager::init(saves.path())
        .unwrap()
        .load_game(&id)
        .expect("the load");
    let summary = |g: &Game| {
        g.players
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    p.team_id.clone(),
                    p.squad_role,
                    p.active_loan.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let (mut a, mut b) = (summary(&game), summary(&reloaded));
    a.sort_by(|x, y| x.0.cmp(&y.0));
    b.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(a, b);
    assert!(a.iter().filter(|p| p.3.is_some()).count() > 1_000);
}
