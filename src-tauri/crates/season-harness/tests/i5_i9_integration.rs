//! I5 (tactical intelligence) and I9 (staff, delegation, training) working on one career.
//!
//! The two branches were built apart and share no file. What they share is the club: I9's
//! delegation writes the user's club plan, I5's pre-match response answers the opponent. These
//! tests hold the seam between them:
//!
//! * I5's pre-match response is a match-local adjustment of the AI's side. It is never saved to a
//!   club's plan, and it never touches the manager's own dugout (delegated or not).
//! * A Total-delegated career, with a named owner of training, saved and reloaded mid-season,
//!   follows the career that was never saved, including the club's formation and tactical plan, and
//!   stays inside the squad limits every day.

use db::save_manager::SaveManager;
use domain::delegation::{DelegationMode, Duty};
use domain::league::{CompetitionType, Fixture, FixtureStatus, League};
use domain::player::SquadRole;
use domain::staff::StaffPost;
use domain::stats::StatsState;
use engine::Side;
use ofm_core::game::Game;
use ofm_core::live_match_manager::{MatchMode, create_live_match, play_unwatched_fixture};
use ofm_core::staff_system::{delegation_owner, market, profile};
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::worlds::WorldSpec;

const SENIOR_LIMIT: usize = 30;
const SENIOR_FLOOR: usize = 15;

struct Quiet;
impl SeasonObserver for Quiet {}

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn seniors(game: &Game, team_id: &str) -> usize {
    game.players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team_id) && p.squad_role == SquadRole::Senior && !p.retired
        })
        .count()
}

/// One fixture today between `home` and `away`, in the legacy competition slot, as the studies do.
fn with_fixture(game: &Game, home: &str, away: &str) -> Game {
    let mut game = game.clone();
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.league = Some(League {
        id: "i5-i9".to_string(),
        name: "I5 I9".to_string(),
        kind: CompetitionType::League,
        fixtures: vec![Fixture {
            id: "i5-i9-fixture".to_string(),
            competition_id: "i5-i9".to_string(),
            matchday: 1,
            date: today,
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            status: FixtureStatus::Scheduled,
            ..Fixture::default()
        }],
        ..League::default()
    });
    game
}

fn world_after_six_weeks() -> Game {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    for day in 1..=42 {
        play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
    }
    game
}

/// The two dugouts' dials as a live match starts, for a fixture between `home` and `away`.
fn dials_at_kick_off(
    game: &Game,
    home: &str,
    away: &str,
) -> (engine::TacticsConfig, engine::TacticsConfig) {
    let game = with_fixture(game, home, away);
    let session = create_live_match(&game, 0, MatchMode::Instant, false).expect("prepared");
    (
        session.match_state.tactics(Side::Home).clone(),
        session.match_state.tactics(Side::Away).clone(),
    )
}

/// Given a world whose AI clubs have settled their plans, and the manager's club in a fixture
/// When the match is prepared once with the manager at his own club and once with the manager
///      elsewhere (so both sides are AI)
/// Then the manager's side is answered by no opponent when human (its dials never vary with the
///      opponent), and when AI it is answered for at least one opponent in the league: I5's
///      response never touches the human dugout, and the comparison is not vacuous.
#[test]
fn the_pre_match_response_never_touches_the_managers_dugout() {
    let game = world_after_six_weeks();
    let user = user_team(&game);
    let others: Vec<String> = game
        .teams
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| *id != user)
        .collect();
    let bystander = others.last().expect("a club to stand in").clone();

    let mut manager_side_answered_when_ai = 0;
    for opponent in others.iter().filter(|id| **id != bystander) {
        // Manager's own club: home.
        let (human_home, _) = dials_at_kick_off(&game, &user, opponent);
        // The same fixture with the manager at a club that is in neither side.
        let mut stand_in = game.clone();
        stand_in.manager.team_id = Some(bystander.clone());
        let (ai_home, _) = dials_at_kick_off(&stand_in, &user, opponent);

        if human_home != ai_home {
            manager_side_answered_when_ai += 1;
        }
    }

    assert!(
        manager_side_answered_when_ai > 0,
        "no opponent in the league ever drew an answer from the user's club: the comparison is vacuous"
    );
    // The manager's side is the same plan every time it is human, whatever the opponent does
    // to the stored plan: nothing was turned for him. (Its dials are the saved plan, not a reply.)
    let first = dials_at_kick_off(&game, &user, &others[0]).0;
    for opponent in others.iter().filter(|id| **id != bystander) {
        assert_eq!(
            dials_at_kick_off(&game, &user, opponent).0,
            first,
            "the manager's dugout changed with the opponent: a response was applied to a human side"
        );
    }
}

/// Given two AI clubs that meet
/// When the match is played to the end through the unwatched path
/// Then neither club's saved formation, play style or tactical plan differs from before, and the
///      match is deterministic: the same fixture played from the same game gives the same report.
#[test]
fn a_played_match_leaves_both_clubs_saved_plans_as_they_were() {
    let game = world_after_six_weeks();
    let user = user_team(&game);
    let ids: Vec<String> = game
        .teams
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| *id != user)
        .collect();
    // A pair of clubs whose plans differ, so a response has something to answer.
    let (home, away) = ids
        .iter()
        .flat_map(|h| ids.iter().map(move |a| (h, a)))
        .find(|(h, a)| {
            h != a && {
                let tactics = |id: &String| {
                    game.teams
                        .iter()
                        .find(|t| &t.id == id)
                        .map(|t| t.tactics_phase.clone())
                };
                tactics(h) != tactics(a)
            }
        })
        .expect("two AI clubs have different plans");

    let snapshot = |game: &Game| {
        [home, away].map(|id| {
            let team = game.teams.iter().find(|t| &t.id == id).expect("club");
            (
                team.formation.clone(),
                team.play_style.clone(),
                team.tactics_phase.clone(),
            )
        })
    };
    let before = snapshot(&game);

    let mut first = with_fixture(&game, home, away);
    let mut second = with_fixture(&game, home, away);
    let report_a = play_unwatched_fixture(&mut first, 0).expect("the match plays");
    let report_b = play_unwatched_fixture(&mut second, 0).expect("the match plays");

    assert_eq!(
        snapshot(&first),
        before,
        "playing the match rewrote a saved plan"
    );
    assert_eq!(
        (report_a.report.home_goals, report_a.report.away_goals),
        (report_b.report.home_goals, report_b.report.away_goals),
        "the same fixture from the same game played differently"
    );
}

/// Given a Total-delegated career with a named coach in charge of training
/// When it is saved to SQLite mid-season and reloaded, and the same days are then played from the
///      reloaded game and from the game that was never saved
/// Then the club's formation, tactical plan and delegation state are identical in both, the named
///      owner is still in charge, the delegated tactics duty really wrote a plan, and on no day is
///      the squad above 30 or below 15 seniors.
#[test]
fn a_total_delegated_career_reloads_into_the_same_formation_and_tactics() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let team = user_team(&game);
    if !profile::team_staff(&game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager)
    {
        let reputation = game
            .teams
            .iter()
            .find(|t| t.id == team)
            .map_or(0, |t| t.reputation);
        let (id, wage) = game
            .staff
            .iter()
            .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
            .max_by_key(|s| (s.overall(), s.id.clone()))
            .map(|s| (s.id.clone(), market::asking_wage(s, reputation)))
            .expect("a free assistant exists");
        market::hire(&mut game, &team, &id, wage, 3).expect("hired");
    }
    let coach = game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
        .find(|s| {
            matches!(
                s.post(),
                StaffPost::FirstTeamCoach | StaffPost::FitnessCoach
            )
        })
        .map(|s| {
            s.contract_end = Some("2099-01-01".to_string());
            s.id.clone()
        })
        .expect("the club has a coach");
    game.delegation.apply_mode(DelegationMode::Full);
    delegation_owner::assign(&mut game, &team, Duty::Training, Some(&coach)).expect("owner");

    let options = RunOptions::seasons(1);
    let within_limits = |game: &Game, day: u32| {
        let n = seniors(game, &team);
        assert!(
            n <= SENIOR_LIMIT,
            "day {day}: {n} seniors is above the limit of 30"
        );
        assert!(
            n >= SENIOR_FLOOR,
            "day {day}: {n} seniors is below the floor of 15"
        );
    };

    for day in 1..=60 {
        play_day(&mut game, options, 1, day, &mut Quiet).unwrap_or_else(|e| panic!("{e}"));
        within_limits(&game, day);
    }
    assert!(
        game.delegation.applied(Duty::Tactics).is_some(),
        "the delegated tactics duty never wrote a plan, so the comparison below is vacuous"
    );
    assert!(
        game.delegation.applied(Duty::Training).is_some(),
        "training never planned"
    );

    let dir = tempfile::tempdir().unwrap();
    let mut saves = SaveManager::init(dir.path()).unwrap();
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "i5-i9")
        .unwrap();
    saves
        .save_game_with_stats(&game, &StatsState::default(), &save_id)
        .unwrap();
    let mut reloaded = SaveManager::init(dir.path())
        .unwrap()
        .load_game(&save_id)
        .unwrap();
    assert_eq!(reloaded.delegation, game.delegation);
    assert_eq!(
        reloaded.delegation.owner(Duty::Training),
        Some(coach.as_str())
    );

    let club = |game: &Game| {
        let t = game.teams.iter().find(|t| t.id == team).expect("club");
        (
            t.formation.clone(),
            t.play_style.clone(),
            t.tactics_phase.clone(),
            t.starting_xi_ids.clone(),
        )
    };
    assert_eq!(
        club(&reloaded),
        club(&game),
        "the round trip changed the club's plan"
    );

    for day in 61..=100 {
        play_day(&mut game, options, 1, day, &mut Quiet).unwrap_or_else(|e| panic!("{e}"));
        play_day(&mut reloaded, options, 1, day, &mut Quiet).unwrap_or_else(|e| panic!("{e}"));
        within_limits(&game, day);
        within_limits(&reloaded, day);
    }
    assert_eq!(
        club(&reloaded),
        club(&game),
        "the reloaded club plays another plan"
    );
    assert_eq!(
        reloaded.delegation, game.delegation,
        "delegation state diverged after reload"
    );
}
