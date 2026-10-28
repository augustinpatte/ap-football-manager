//! The competitions of the FM24 world over several seasons: the six national cups, the authored
//! promotion rules and the reserve sides, through the production day and rollover.
//!
//! Like `fm24_career`, these need the player's own world file (`APFM_WORLD_JSON`) and return at once
//! without it. Run: `APFM_WORLD_JSON=<file> cargo test --release -p season-harness --test fm24_competitions`.

use std::collections::BTreeMap;

use db::save_manager::SaveManager;
use domain::league::{CompetitionFormat, CompetitionType, FixtureStatus};
use domain::stats::StatsState;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, play_day, roll_season_over};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

const PSG: &str = "fm-c-498";
const REAL_MADRID: &str = "fm-c-1113";

/// Every competition as JSON, keyed by id: what a reload has to give back.
fn competitions_by_id(game: &Game) -> BTreeMap<String, serde_json::Value> {
    game.competitions
        .iter()
        .map(|competition| {
            (
                competition.id.clone(),
                serde_json::to_value(competition).expect("a competition serialises"),
            )
        })
        .collect()
}

/// The history keyed by edition: a reload gives it back sorted, not in the order it was written.
fn archive_by_edition(game: &Game) -> BTreeMap<(String, u32), serde_json::Value> {
    game.edition_archive
        .iter()
        .map(|edition| {
            (
                (edition.competition_id.clone(), edition.season),
                serde_json::to_value(edition).expect("an edition serialises"),
            )
        })
        .collect()
}

fn national_cups(game: &Game) -> Vec<&domain::league::League> {
    game.competitions
        .iter()
        .filter(|c| c.kind == CompetitionType::Cup && c.rules.format == CompetitionFormat::Knockout)
        .collect()
}

/// Given a career with the six national cups running,
/// When it is saved and read back every month of two seasons,
/// Then every competition (brackets, dates, results, authored rules, withdrawn sides) comes back as
///      it was saved, and every cup has a champion each season.
#[test]
fn the_cups_survive_a_save_and_a_reload_at_every_point_of_two_seasons() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    assert_eq!(national_cups(&game).len(), 6, "the six national cups");
    let saves = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(saves.path()).unwrap();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "career")
        .expect("the opening save");
    let mut invariants = Invariants::new(&game);
    let options = RunOptions::seasons(1);

    for season in 1..=2 {
        let mut day = 0;
        while !ofm_core::end_of_season::is_season_complete(&game) {
            day += 1;
            assert!(day < 460, "season {season} did not complete");
            play_day(&mut game, options, season, day, &mut invariants).expect("a day plays");
            if day % 30 == 0 {
                manager
                    .save_game_with_stats(&game, &StatsState::default(), &id)
                    .expect("the save");
                let reloaded = SaveManager::init(saves.path())
                    .unwrap()
                    .load_game(&id)
                    .expect("the reload");
                assert!(
                    competitions_by_id(&reloaded) == competitions_by_id(&game),
                    "season {season} day {day}: a competition changed through the save"
                );
            }
        }
        for cup in national_cups(&game) {
            assert!(
                cup.fixtures
                    .iter()
                    .all(|f| f.status == FixtureStatus::Completed),
                "{} is not finished at the rollover",
                cup.name
            );
        }
        roll_season_over(&mut game, options, season, &mut invariants).expect("the rollover runs");
        manager
            .save_game_with_stats(&game, &StatsState::default(), &id)
            .expect("the save after the rollover");
        let reloaded = SaveManager::init(saves.path())
            .unwrap()
            .load_game(&id)
            .expect("the reload after the rollover");
        assert!(
            competitions_by_id(&reloaded) == competitions_by_id(&game),
            "season {season}: the renewed competitions changed through the save"
        );
        assert!(
            archive_by_edition(&reloaded) == archive_by_edition(&game),
            "season {season}: the history changed through the save"
        );
    }

    // Twelve leagues and six cups archived each season; the cups have a champion, the leagues
    // that moved clubs remember what they moved.
    let cup_ids: Vec<&str> = national_cups(&game).iter().map(|c| c.id.as_str()).collect();
    for cup in &cup_ids {
        let editions = game
            .edition_archive
            .iter()
            .filter(|e| e.competition_id == *cup)
            .count();
        assert_eq!(editions, 2, "{cup}: one edition per season in the history");
    }
    assert!(
        game.edition_archive
            .iter()
            .filter(|e| e.movements.is_some())
            .count()
            >= 2 * 7,
        "every authored boundary records what it moved"
    );
    assert!(invariants.is_clean(), "{}", invariants.report());
}

/// Given a Real Madrid career, with its reserve side Castilla in LaLiga 2,
/// When five seasons are played,
/// Then no reserve side ever plays level with or above its parent, the Copa del Rey never holds a
///      reserve side, and the harness invariants hold.
#[test]
fn reserve_sides_stay_below_their_parents_for_five_seasons() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, REAL_MADRID, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let options = RunOptions::seasons(1);
    // The list is repeated on every competition of the country.
    let reserves: Vec<(String, String)> = game
        .competitions
        .iter()
        .flat_map(|c| c.rules.reserve_sides.iter())
        .map(|side| (side.team_id.clone(), side.parent_id.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        reserves.len(),
        3,
        "Castilla, Real Sociedad B and Celta Fortuna"
    );

    for season in 1..=5 {
        let mut day = 0;
        while !ofm_core::end_of_season::is_season_complete(&game) {
            day += 1;
            assert!(day < 460, "season {season} did not complete");
            play_day(&mut game, options, season, day, &mut invariants).expect("a day plays");
        }
        roll_season_over(&mut game, options, season, &mut invariants).expect("the rollover runs");
        let division_of = |club: &str| {
            game.competitions
                .iter()
                .filter(|c| c.kind == CompetitionType::League)
                .find(|c| c.participant_ids.iter().any(|p| p == club))
                .map(|c| c.priority)
        };
        for (reserve, parent) in &reserves {
            if let (Some(reserve_tier), Some(parent_tier)) =
                (division_of(reserve), division_of(parent))
            {
                assert!(
                    reserve_tier > parent_tier,
                    "season {season}: {reserve} is not below {parent}"
                );
            }
        }
        for cup in national_cups(&game) {
            if cup.rules.cup.exclude_reserve_sides {
                for (reserve, _) in &reserves {
                    assert!(
                        !cup.participant_ids.contains(reserve),
                        "season {season}: {} holds the reserve side {reserve}",
                        cup.name
                    );
                }
            }
        }
    }
    assert!(invariants.is_clean(), "{}", invariants.report());
}
