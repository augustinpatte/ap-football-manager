//! A formation is one fact in three places: the club's row, the match that is prepared from it and
//! the match that is played. This asserts they stay one fact across a save and a reload.
//!
//! The AI clubs' shapes are the ones their managers chose over the first weeks of a season (the
//! formation review writes `Team::formation`); the user's is changed by hand to one no club plays
//! at the start. After a SQLite round trip every club has the shape it had, the match prepared for
//! the same fixture names the same eleven in the same formation for both engines' inputs, and the
//! match played from the reloaded game is the match played from the original.

use db::save_manager::SaveManager;
use domain::league::{CompetitionType, Fixture, FixtureStatus, League};
use domain::stats::StatsState;
use ofm_core::game::Game;
use ofm_core::live_match_manager::{
    MatchMode, adapt_spatial_fixture, create_live_match, play_unwatched_fixture,
};
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::worlds::WorldSpec;

struct Quiet;
impl SeasonObserver for Quiet {}

/// Schedules one fixture today between `home` and `away` in the legacy slot, as a study does.
fn with_fixture(game: &Game, home: &str, away: &str) -> Game {
    let mut game = game.clone();
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.league = Some(League {
        id: "persistence".to_string(),
        name: "Persistence".to_string(),
        kind: CompetitionType::League,
        fixtures: vec![Fixture {
            id: "persistence-fixture".to_string(),
            competition_id: "persistence".to_string(),
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

/// Given a world played for six weeks, so its AI clubs have chosen their shapes, and the user's
///   formation changed by hand
/// When it is saved to SQLite and loaded by a fresh manager
/// Then every club has the formation it had, the match prepared for a fixture between two clubs
///   is the same in both engines' inputs before and after, and the match played from the reloaded
///   game is the match played from the original.
#[test]
fn formations_survive_a_save_and_reach_the_match_that_is_prepared_and_played() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    for day in 1..=42 {
        play_day(&mut game, RunOptions::seasons(1), 1, day, &mut Quiet).expect("the day plays");
    }
    let user = game.manager.team_id.clone().expect("the user has a club");
    game.teams
        .iter_mut()
        .find(|team| team.id == user)
        .expect("the user's club exists")
        .formation = "5-3-2".to_string();
    let shapes: std::collections::BTreeSet<&str> = game
        .teams
        .iter()
        .map(|team| team.formation.as_str())
        .collect();
    assert!(
        shapes.len() >= 3,
        "the AI clubs chose different shapes, or the test proves nothing: {shapes:?}"
    );

    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut saves = SaveManager::init(dir.path()).expect("saves initialise");
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "formations")
        .expect("the game saves");
    let reloaded = SaveManager::init(dir.path())
        .expect("a fresh manager")
        .load_game(&save_id)
        .expect("the game loads");

    for team in &game.teams {
        let after = reloaded
            .teams
            .iter()
            .find(|t| t.id == team.id)
            .expect("every club survives");
        assert_eq!(after.formation, team.formation, "{}", team.name);
    }

    // Two AI clubs of different shapes, and the user's club, each at home in turn.
    let ai: Vec<&str> = game
        .teams
        .iter()
        .filter(|team| team.id != user)
        .map(|team| team.id.as_str())
        .collect();
    let first = ai[0];
    let other = ai
        .iter()
        .find(|id| {
            game.teams
                .iter()
                .find(|t| &t.id == *id)
                .map(|t| &t.formation)
                != game
                    .teams
                    .iter()
                    .find(|t| t.id == first)
                    .map(|t| &t.formation)
        })
        .copied()
        .expect("two AI clubs play different shapes");
    for (home, away) in [(first, other), (user.as_str(), first)] {
        let before = with_fixture(&game, home, away);
        let after = with_fixture(&reloaded, home, away);

        let prepared = |game: &Game| {
            let session = create_live_match(game, 0, MatchMode::Instant, false)
                .expect("the match is prepared");
            let snapshot = session.snapshot();
            (
                snapshot.home_team.formation.clone(),
                snapshot
                    .home_team
                    .players
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>(),
                snapshot.away_team.formation.clone(),
                snapshot
                    .away_team
                    .players
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let (home_shape, ..) = prepared(&before);
        assert_eq!(
            home_shape,
            before
                .teams
                .iter()
                .find(|t| t.id == home)
                .map(|t| t.formation.clone())
                .expect("the home club exists"),
            "the match is prepared in the club's formation"
        );
        assert_eq!(
            prepared(&before),
            prepared(&after),
            "{home} v {away}: prepared"
        );

        let spatial = |game: &Game| {
            let adapted = adapt_spatial_fixture(game, 0).expect("M12 can adapt the match");
            (
                adapted.input.home.formation.clone(),
                adapted.input.away.formation.clone(),
                adapted
                    .input
                    .home
                    .lineup
                    .iter()
                    .map(|slot| slot.player.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let (m12_home, m12_away, _) = spatial(&before);
        assert_eq!(
            m12_home,
            prepared(&before).0,
            "both engines are given the home formation"
        );
        assert_eq!(
            m12_away,
            prepared(&before).2,
            "both engines are given the away formation"
        );
        assert_eq!(
            spatial(&before),
            spatial(&after),
            "{home} v {away}: M12 input"
        );

        let mut played_before = before.clone();
        let mut played_after = after.clone();
        let score = |game: &mut Game| {
            let report = play_unwatched_fixture(game, 0)
                .expect("the match is played")
                .report;
            (
                report.home_goals,
                report.away_goals,
                report.home_stats.shots,
                report.away_stats.shots,
                report.events.len(),
            )
        };
        assert_eq!(
            score(&mut played_before),
            score(&mut played_after),
            "{home} v {away}: played"
        );
    }
}
