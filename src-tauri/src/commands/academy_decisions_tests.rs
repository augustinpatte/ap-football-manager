//! The manager's academy decisions, through the commands the screen calls: what the review advises
//! is what the commands allow, a loan or a registration limit never lets a youngster bypass the
//! rules, and an individual training focus reaches the review.

use chrono::{TimeZone, Utc};
use domain::manager::Manager;
use domain::player::{ActiveLoan, Player, PlayerAttributes, Position, SquadRole};
use domain::team::{Team, TrainingFocus};
use ofm_core::academy_advice::AdviceState;
use ofm_core::academy_review::academy_review;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::state::StateManager;

use super::squad::{set_player_squad_role_internal, set_player_training_focus_internal};
use super::transfers::toggle_loan_list_internal;

fn attributes() -> PlayerAttributes {
    serde_json::from_value(serde_json::json!({
        "pace": 50, "stamina": 50, "strength": 50, "agility": 50, "passing": 50,
        "shooting": 50, "tackling": 50, "dribbling": 50, "defending": 50,
        "positioning": 50, "vision": 50, "decisions": 50, "composure": 50,
        "aggression": 50, "teamwork": 50, "leadership": 50,
        "handling": 50, "reflexes": 50, "aerial": 50
    }))
    .expect("attributes")
}

fn team(id: &str) -> Team {
    let mut team = Team::new(
        id.to_string(),
        id.to_string(),
        id.to_uppercase(),
        "England".to_string(),
        "London".to_string(),
        format!("{id} Ground"),
        25_000,
    );
    if id == "team-1" {
        team.manager_id = Some("manager-1".to_string());
    }
    team
}

fn person(id: &str, date_of_birth: &str, role: SquadRole, club: &str) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        date_of_birth.to_string(),
        "England".to_string(),
        Position::Midfielder,
        attributes(),
    );
    player.team_id = Some(club.to_string());
    player.squad_role = role;
    player.ovr = 50;
    player.potential = 68;
    player
}

/// The manager's club with `seniors` registered seniors and these players besides.
fn game(seniors: usize, others: Vec<Player>) -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
    let mut manager = Manager::new(
        "manager-1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team-1".to_string());
    let mut players: Vec<Player> = (0..seniors)
        .map(|n| {
            person(
                &format!("senior-{n}"),
                "1998-01-01",
                SquadRole::Senior,
                "team-1",
            )
        })
        .collect();
    players.extend(others);
    Game::new(
        clock,
        manager,
        vec![team("team-1"), team("team-2")],
        players,
        vec![],
        vec![],
    )
}

fn loan_out(player: &mut Player) {
    player.team_id = Some("team-2".to_string());
    player.active_loan = Some(ActiveLoan {
        parent_team_id: "team-1".to_string(),
        loan_team_id: "team-2".to_string(),
        loan_team_name: None,
        start_date: "2026-07-01".to_string(),
        end_date: "2027-06-30".to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
        terms_source: domain::player::LoanTermsSource::Agreed,
    });
}

fn row_of(state: &StateManager, id: &str) -> ofm_core::academy_review::AcademyRow {
    // Through the seam the screen and the MCP tool both call.
    let review = super::scouting::get_academy_review_internal(state)
        .expect("a game")
        .expect("the manager has a club");
    let rows: Vec<_> = review
        .rows
        .into_iter()
        .filter(|r| r.player_id == id)
        .collect();
    assert_eq!(rows.len(), 1, "{id} must be listed exactly once");
    rows.into_iter().next().expect("one row")
}

/// Given an academy youngster out on loan, whose contract stays with the club
/// When the manager tries to promote and to loan-list him, and the review is read
/// Then both commands refuse, he stays a Youth with his parent contract and appears in the review
///      once, with promotion and loan listing blocked for the loan.
#[test]
fn a_youngster_out_on_loan_cannot_be_promoted_or_listed_and_the_review_says_so() {
    let mut lent = person("lent", "2008-01-01", SquadRole::Youth, "team-1");
    loan_out(&mut lent);
    let state = StateManager::new();
    state.set_game(game(20, vec![lent]));

    assert_eq!(
        set_player_squad_role_internal(&state, "lent", "Senior").unwrap_err(),
        "be.error.playerNotInSquad"
    );
    assert_eq!(
        toggle_loan_list_internal(&state, "lent").unwrap_err(),
        "be.error.transfers.playerNotOwnedByUser"
    );

    let stored = state.get_game(|g| g.clone()).expect("a game");
    let lent = stored
        .players
        .iter()
        .find(|p| p.id == "lent")
        .expect("he stays");
    assert_eq!(lent.squad_role, SquadRole::Youth);
    assert_eq!(lent.contract_club_id(), Some("team-1"));
    assert!(!lent.loan_listed);
    let row = row_of(&state, "lent");
    assert!(row.on_loan);
    assert_eq!(row.advice.promote.state, AdviceState::Blocked);
    assert_eq!(row.advice.promote.reasons, vec!["promote.onLoan"]);
    assert_eq!(row.advice.loan.state, AdviceState::Blocked);
}

/// Given a club at its senior limit and a youngster the review would call ready
/// When the review is read and the promotion attempted
/// Then the review says blocked for the place and the command refuses; with a place free the review
///      stops saying blocked and the same command succeeds, so advice and rule never disagree.
#[test]
fn the_reviews_promotion_advice_matches_what_the_command_allows() {
    let state = StateManager::new();
    state.set_game(game(
        30,
        vec![person("kid", "2008-01-01", SquadRole::Youth, "team-1")],
    ));

    let full = row_of(&state, "kid");
    assert_eq!(full.advice.promote.state, AdviceState::Blocked);
    assert_eq!(full.advice.promote.reasons, vec!["promote.noPlace"]);
    assert!(full.promotion_blocked.is_some());
    assert!(set_player_squad_role_internal(&state, "kid", "Senior").is_err());
    assert_eq!(
        row_of(&state, "kid").advice.promote.state,
        AdviceState::Blocked
    );

    state
        .update_game(|game| game.players.retain(|p| p.id != "senior-0"))
        .expect("a game");

    assert_ne!(
        row_of(&state, "kid").advice.promote.state,
        AdviceState::Blocked
    );
    set_player_squad_role_internal(&state, "kid", "Senior").expect("a place is free");
    let stored = state.get_game(|g| g.clone()).expect("a game");
    assert_eq!(stored.players.iter().filter(|p| p.id == "kid").count(), 1);
    assert!(
        state
            .get_game(academy_review)
            .expect("a game")
            .expect("a club")
            .rows
            .iter()
            .all(|r| r.player_id != "kid"),
        "a promoted player leaves the academy list"
    );
}

/// Given an academy youngster
/// When the manager sets an individual training focus through the training command, and then clears it
/// Then the review shows the focus and then none, and his true ceiling is untouched throughout.
#[test]
fn an_individual_training_focus_reaches_the_review_and_leaves_the_ceiling_alone() {
    let state = StateManager::new();
    state.set_game(game(
        20,
        vec![person("kid", "2008-01-01", SquadRole::Youth, "team-1")],
    ));

    set_player_training_focus_internal(&state, "kid", Some("Technical")).expect("a focus");
    assert_eq!(
        row_of(&state, "kid").training_focus,
        Some(TrainingFocus::Technical)
    );
    set_player_training_focus_internal(&state, "kid", None).expect("cleared");
    assert_eq!(row_of(&state, "kid").training_focus, None);

    let stored = state.get_game(|g| g.clone()).expect("a game");
    let kid = stored.players.iter().find(|p| p.id == "kid").expect("kid");
    assert_eq!((kid.ovr, kid.potential), (50, 68));
}

/// Given a youngster who has grown past the academy's age limit at the manager's club (the AI
///       clubs graduate theirs automatically; the manager's is the manager's to move)
/// When the review is read, he is promoted, and the manager then tries to send him back
/// Then the review advises promotion because of his age and has no seasons left for him, the
///      promotion succeeds once, and the academy refuses him again.
#[test]
fn an_overage_academy_player_is_advised_to_move_on_and_cannot_return() {
    let state = StateManager::new();
    state.set_game(game(
        20,
        vec![person("grown", "2003-01-01", SquadRole::Youth, "team-1")],
    ));

    let row = row_of(&state, "grown");
    assert_eq!(row.seasons_left_in_academy, 0);
    assert_eq!(row.advice.promote.state, AdviceState::Recommended);
    assert_eq!(row.advice.promote.reasons, vec!["promote.pastAcademyAge"]);
    assert_eq!(row.advice.keep.reasons, vec!["keep.pastAcademyAge"]);

    set_player_squad_role_internal(&state, "grown", "Senior").expect("a place is free");
    let error = set_player_squad_role_internal(&state, "grown", "Youth").expect_err("overage");
    assert_eq!(error, "be.error.youthAcademyOverage");
    let stored = state.get_game(|g| g.clone()).expect("a game");
    assert_eq!(stored.players.iter().filter(|p| p.id == "grown").count(), 1);
}

/// Given a youngster at the club who is not out on loan
/// When the manager lists him for loan and takes him off the list again
/// Then the commands succeed and the review shows the listing each time: the control for the
///      loaned-out refusal above, so that refusal is about the loan and not about the fixture.
#[test]
fn a_youngster_who_is_not_out_on_loan_can_be_listed_and_unlisted() {
    let state = StateManager::new();
    state.set_game(game(
        20,
        vec![person("kid", "2008-01-01", SquadRole::Youth, "team-1")],
    ));

    toggle_loan_list_internal(&state, "kid").expect("listed");
    assert!(row_of(&state, "kid").loan_listed);
    toggle_loan_list_internal(&state, "kid").expect("unlisted");
    assert!(!row_of(&state, "kid").loan_listed);
}
