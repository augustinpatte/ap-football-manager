//! The season lifecycle, end to end and across more than two seasons: a career is started, every
//! fixture of every competition is played, promotion and relegation run, the next season begins,
//! and it is repeated — with the career saved to SQLite and read back at every critical stage.
//!
//! Budget: four seasons of the 80-club gate world plus ten SQLite round trips. Measured numbers
//! are in `docs/APFM_RUN4_SEASONS.md`.

use db::save_manager::SaveManager;
use domain::stats::StatsState;
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, play_day, roll_season_over, run_seasons};
use season_harness::fingerprint::{fingerprint as raw_fingerprint, first_difference};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::{self, Lifecycle, Snapshot};
use season_harness::worlds::WorldSpec;

/// The world as a reload must reproduce it, in order. The only line left out is the squad-floor
/// top-up log, a session diagnostic that is never saved (`Game::squad_floor_top_ups`).
fn fingerprint(game: &Game) -> String {
    raw_fingerprint(game)
        .lines()
        .filter(|line| !line.starts_with("top_up "))
        .collect::<Vec<_>>()
        .join("\n")
}

const SEASONS: u32 = 4;
const SEED: u64 = 1;

/// Given a generated world with the player in charge of a club,
/// When four consecutive seasons are played the way the game plays them,
/// Then every invariant of the harness and every lifecycle rule holds at every day and every
///      rollover, and the player's club was still in a competition at the end.
#[test]
fn four_consecutive_seasons_hold_every_lifecycle_rule() {
    let mut game = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);

    run_seasons(
        &mut game,
        RunOptions::seasons(SEASONS),
        &mut (&mut invariants, &mut lifecycle),
    )
    .unwrap_or_else(|error| panic!("{error}"));

    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    let user = game
        .manager
        .team_id
        .clone()
        .expect("the manager has a club");
    for season in 1..=SEASONS {
        // The first season of the 2033 world is labelled 2033.
        let ended = 2032 + season;
        let champions = game
            .edition_archive
            .iter()
            .filter(|edition| edition.season == ended)
            .count();
        assert!(champions > 0, "no edition of season {ended} was archived");
    }
    let records = game
        .teams
        .iter()
        .find(|team| team.id == user)
        .map(|team| team.history.len())
        .unwrap_or(0);
    assert!(
        records >= SEASONS as usize,
        "the player's club kept {records} season records over {SEASONS} seasons"
    );
}

/// Given other seeds, and the player's own match played through the live path,
/// When two seasons are played,
/// Then the lifecycle rules hold there too: a result applied by the live path and again by the
///      batch, or a fixture the live day stranded, would break `standings-match-results` or
///      `fixture-ids-unique` here and nowhere in the batch run of seed 1.
#[test]
fn the_lifecycle_rules_hold_for_other_seeds_and_the_live_path() {
    use season_harness::driver::DayPath;
    for (seed, day_path) in [
        (2, DayPath::Batch),
        (3, DayPath::Batch),
        (4, DayPath::UserMatchLive),
    ] {
        let mut game = WorldSpec::gate(seed).build().expect("the world builds");
        let mut invariants = Invariants::new(&game);
        let mut lifecycle = Lifecycle::new(&game);
        let options = RunOptions {
            day_path,
            ..RunOptions::seasons(2)
        };

        run_seasons(&mut game, options, &mut (&mut invariants, &mut lifecycle))
            .unwrap_or_else(|error| panic!("seed {seed}: {error}"));

        assert!(
            invariants.is_clean(),
            "seed {seed}: {}",
            invariants.report()
        );
        assert!(lifecycle.is_clean(), "seed {seed}: {}", lifecycle.report());
    }
}

/// The stages a career is saved at: the points where a half-written season would show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Opening,
    EarlySeason,
    MidSeason,
    SeasonComplete,
    JustRolledOver,
}

struct Career {
    game: Game,
    manager: SaveManager,
    save_id: String,
    _dir: tempfile::TempDir,
    reloads: Vec<String>,
}

impl Career {
    fn begin() -> Self {
        let game = WorldSpec::gate(SEED).build().expect("the world builds");
        let dir = tempfile::tempdir().unwrap();
        let mut manager = SaveManager::init(dir.path()).unwrap();
        let save_id = manager
            .create_save_with_stats(&game, &StatsState::default(), "lifecycle")
            .expect("the opening save");
        Self {
            game,
            manager,
            save_id,
            _dir: dir,
            reloads: Vec::new(),
        }
    }

    /// Saves over the career's file, loads it back through a fresh manager, and requires the
    /// loaded world to be the one that was saved. The loaded world replaces the live one, so the
    /// career goes on from what a player's reload would give them.
    fn save_and_reload(&mut self, season: u32, stage: Stage) {
        let before = fingerprint(&self.game);
        self.manager
            .save_game_with_stats(&self.game, &StatsState::default(), &self.save_id)
            .unwrap_or_else(|error| panic!("season {season} {stage:?}: the save failed: {error}"));
        let loaded = SaveManager::init(self._dir.path())
            .unwrap()
            .load_game(&self.save_id)
            .unwrap_or_else(|error| panic!("season {season} {stage:?}: the load failed: {error}"));
        let after = fingerprint(&loaded);
        assert!(
            before == after,
            "season {season} {stage:?}: the reloaded career differs from the saved one: {}",
            first_difference(&before, &after)
        );
        self.reloads.push(format!("season {season} {stage:?}"));
        self.game = loaded;
    }
}

/// Given a career saved to SQLite and read back at four stages of each of four seasons (day 15,
///       day 100, the last matchday, and the instant after the rollover) plus the opening,
/// When play continues from each reloaded world,
/// Then every reload reproduces the saved world exactly, every invariant and lifecycle rule holds
///      on the reloaded world and as play goes on, and all four seasons complete.
#[test]
fn a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons() {
    let mut career = Career::begin();
    let options = RunOptions::seasons(1);
    let mut invariants = Invariants::new(&career.game);
    let mut lifecycle = Lifecycle::new(&career.game);

    career.save_and_reload(0, Stage::Opening);
    for season in 1..=SEASONS {
        let mut day = 0;
        while !is_season_complete(&career.game) {
            day += 1;
            assert!(day < 460, "season {season} did not complete");
            play_day(
                &mut career.game,
                options,
                season,
                day,
                &mut (&mut invariants, &mut lifecycle),
            )
            .unwrap_or_else(|error| panic!("{error}"));
            match day {
                15 => career.save_and_reload(season, Stage::EarlySeason),
                100 => career.save_and_reload(season, Stage::MidSeason),
                _ => {}
            }
        }
        career.save_and_reload(season, Stage::SeasonComplete);
        roll_season_over(
            &mut career.game,
            options,
            season,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        career.save_and_reload(season, Stage::JustRolledOver);
    }

    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    assert_eq!(career.reloads.len(), 1 + 4 * SEASONS as usize);
}

/// Given the same career at the same point,
/// When one copy is saved and reloaded through SQLite and both are played on for thirty days,
/// Then the two worlds stay identical: a reload changes nothing a day's dice decide.
#[test]
fn a_reloaded_career_plays_on_exactly_like_the_one_never_saved() {
    let mut original = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut quiet = Invariants::new(&original);
    let options = RunOptions::seasons(1);
    for day in 1..=60 {
        play_day(&mut original, options, 1, day, &mut quiet).unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(dir.path()).unwrap();
    let id = manager
        .create_save_with_stats(&original, &StatsState::default(), "twin")
        .unwrap();
    let mut twin = SaveManager::init(dir.path())
        .unwrap()
        .load_game(&id)
        .unwrap();

    for day in 61..=90 {
        play_day(&mut original, options, 1, day, &mut quiet).unwrap();
        play_day(&mut twin, options, 1, day, &mut quiet).unwrap();
    }
    let (left, right) = (fingerprint(&original), fingerprint(&twin));
    assert!(
        left == right,
        "the reloaded career diverged: {}",
        first_difference(&left, &right)
    );
}

/// Given a career saved and reloaded the instant after a rollover, when the first day's matches
///       have yet to be played,
/// When the reloaded copy and the copy that was never saved are both played for forty days,
/// Then they are the same world: the check of the loaded calendar (which moves matches inside
///      international windows) happens on the first day and lands where the live sweep puts them.
///      Saved just after a rollover is the state with the most matches waiting to move.
#[test]
fn a_career_reloaded_just_after_a_rollover_plays_on_like_the_one_never_saved() {
    let mut original = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut quiet = Invariants::new(&original);
    run_seasons(&mut original, RunOptions::seasons(1), &mut quiet).expect("a season plays");

    let dir = tempfile::tempdir().unwrap();
    let mut manager = SaveManager::init(dir.path()).unwrap();
    let id = manager
        .create_save_with_stats(&original, &StatsState::default(), "twin")
        .unwrap();
    let mut twin = SaveManager::init(dir.path())
        .unwrap()
        .load_game(&id)
        .unwrap();
    let (left, right) = (fingerprint(&original), fingerprint(&twin));
    assert!(
        left == right,
        "the load itself changed the world: {}",
        first_difference(&left, &right)
    );

    let options = RunOptions::seasons(1);
    for day in 1..=40 {
        play_day(&mut original, options, 2, day, &mut quiet).unwrap();
        play_day(&mut twin, options, 2, day, &mut quiet).unwrap();
    }
    let (left, right) = (fingerprint(&original), fingerprint(&twin));
    assert!(
        left == right,
        "the reloaded career diverged: {}",
        first_difference(&left, &right)
    );
}

// ── The rules must be able to fail ──────────────────────────────────────────

fn thirty_days_in() -> Game {
    let mut game = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut quiet = Invariants::new(&game);
    for day in 1..=30 {
        play_day(&mut game, RunOptions::seasons(1), 1, day, &mut quiet).expect("a day plays");
    }
    game
}

fn one_season_in() -> Game {
    let mut game = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut quiet = Invariants::new(&game);
    run_seasons(&mut game, RunOptions::seasons(1), &mut quiet).expect("a season plays");
    game
}

fn first_table(game: &Game) -> usize {
    game.competitions
        .iter()
        .position(|competition| {
            competition.rules.format == domain::league::CompetitionFormat::LeagueTable
                && !competition.fixtures.is_empty()
        })
        .expect("the world has a league table")
}

/// Given a world holding one season,
/// When a fixture is duplicated, a table row altered, a postponement set behind its date, a
///      history record repeated, an edition archived twice and a milestone message repeated,
/// Then each rule reports its own fault, and none of them reports the intact world.
#[test]
fn each_lifecycle_rule_fails_on_the_fault_it_names() {
    let intact = one_season_in();
    let midseason = thirty_days_in();
    assert_eq!(lifecycle::fixture_ids_unique(&intact), Vec::<String>::new());
    assert_eq!(
        lifecycle::league_pairings_not_duplicated(&intact),
        Vec::<String>::new()
    );
    assert_eq!(
        lifecycle::standings_match_results(&intact),
        Vec::<String>::new()
    );
    assert_eq!(
        lifecycle::standings_match_results(&midseason),
        Vec::<String>::new()
    );
    assert_eq!(
        lifecycle::postponements_move_forward(&intact),
        Vec::<String>::new()
    );

    assert_eq!(
        lifecycle::cup_champions_won_their_final(&intact),
        Vec::<String>::new()
    );
    let table = first_table(&intact);

    let mut wrong_champion = intact.clone();
    let edition = wrong_champion
        .edition_archive
        .iter_mut()
        .find(|edition| {
            edition
                .knockout_rounds
                .last()
                .is_some_and(|round| round.fixture_ids.len() == 1)
        })
        .expect("a cup edition with a single-match final was archived");
    edition.champion_id = "nobody".to_string();
    assert!(!lifecycle::cup_champions_won_their_final(&wrong_champion).is_empty());

    let mut duplicated = intact.clone();
    let copy = duplicated.competitions[table].fixtures[0].clone();
    duplicated.competitions[table].fixtures.push(copy);
    assert!(!lifecycle::fixture_ids_unique(&duplicated).is_empty());
    assert!(!lifecycle::league_pairings_not_duplicated(&duplicated).is_empty());

    let mut altered = midseason.clone();
    let mid_table = first_table(&midseason);
    let played = altered.competitions[mid_table]
        .standings
        .iter_mut()
        .find(|row| row.played > 0)
        .expect("a club has played");
    played.points += 3;
    assert!(!lifecycle::standings_match_results(&altered).is_empty());

    let mut postponed = intact.clone();
    let fixture = &mut postponed.competitions[table].fixtures[0];
    fixture.postponement = Some(domain::league::Postponement {
        original_date: "2999-01-01".to_string(),
        reason: domain::league::RescheduleReason::LeagueClash,
    });
    assert!(!lifecycle::postponements_move_forward(&postponed).is_empty());

    let snapshot = Snapshot::capture(&intact);
    let summary = ofm_core::end_of_season::EndOfSeasonSummary {
        season: 2034,
        ..Default::default()
    };

    let mut repeated = intact.clone();
    let record = repeated.teams[0].history[0].clone();
    repeated.teams[0].history.push(record);
    assert!(!lifecycle::history_records_follow_seasons(&snapshot, &repeated, &summary).is_empty());

    let mut archived_twice = intact.clone();
    let edition = archived_twice.edition_archive[0].clone();
    archived_twice.edition_archive.push(edition);
    assert!(
        !lifecycle::edition_archive_keeps_the_champion(&snapshot, &archived_twice, &summary)
            .is_empty()
    );

    assert_eq!(
        lifecycle::ai_contracts_outlive_the_rollover(&snapshot, &intact, &summary),
        Vec::<String>::new()
    );
    // Ten years on, every contract of every AI club has lapsed.
    let mut lapsed = intact.clone();
    lapsed.clock.current_date += chrono::Duration::days(3650);
    assert!(!lifecycle::ai_contracts_outlive_the_rollover(&snapshot, &lapsed, &summary).is_empty());

    let mut mailed_twice = intact.clone();
    let message = mailed_twice.messages[0].clone();
    mailed_twice.messages.push(message);
    assert!(!lifecycle::milestone_mail_sent_once(&snapshot, &mailed_twice, &summary).is_empty());
}
