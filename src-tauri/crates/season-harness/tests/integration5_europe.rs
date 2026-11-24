//! INTEGRATION5: the real FM 2026/27 world with its three European competitions, played by the
//! game's own day loop and rollover.
//!
//! The world is the owner's own file (`APFM_WORLD_JSON`, built by `apfm-import` from their
//! `world.db` into a scratch directory), never in the repository, so each test returns at once
//! without it. A test that returns for that reason has verified nothing about the real source: the
//! audit report says which of these ran.
//! Run with `APFM_WORLD_JSON=<file> cargo test -p season-harness --test integration5_europe`.

use std::collections::BTreeMap;

use domain::league::CompetitionType;
use domain::league_phase::CompetitionStage;
use ofm_core::game::Game;
use season_harness::calendar::{
    CalendarStudy, Displacement, matches_per_month, reloaded, unplayed_clashes,
};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, run_seasons};
use season_harness::europe::{
    all_european_editions_completed, clubs_in_two_european_competitions, european,
};
use season_harness::europe_watch::{
    EuropeWatch, Rollovers, assert_editions_archived, finish_european_editions,
    window_of_difference,
};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;

const PSG: &str = "fm-c-498";

/// The world file, or a visible SKIPPED line: a test that returns here has verified nothing about
/// the real source, and its green must not be read as coverage.
fn world_or_skip() -> Option<std::path::PathBuf> {
    let path = world_path_from_env();
    if path.is_none() {
        eprintln!(
            "SKIPPED: APFM_WORLD_JSON is not set, so nothing was checked against the real world"
        );
    }
    path
}
const EUROPEAN_IDS: [&str; 3] = ["eu-ucl", "eu-uel", "eu-uecl"];

/// Given the real FM 2026/27 world and a new Paris Saint-Germain career in June 2026
/// When the career is opened
/// Then the three European competitions exist with 36 distinct clubs each, no club in two of them,
///      are drawn and dated, PSG is in the Champions League with its eight league-phase matches,
///      and the competition is one the manager follows.
#[test]
fn a_new_psg_career_opens_with_the_champions_league_drawn() {
    let Some(path) = world_or_skip() else {
        return;
    };
    let game = build_fm24_career(&path, PSG, 7).expect("the career builds");

    let found: Vec<&str> = european(&game).map(|c| c.id.as_str()).collect();
    for id in EUROPEAN_IDS {
        assert!(found.contains(&id), "{id} is missing from {found:?}");
    }
    for competition in european(&game) {
        assert_eq!(competition.participant_ids.len(), 36, "{}", competition.id);
        let state = competition.league_phase.as_ref().unwrap();
        assert_eq!(
            state.stage,
            CompetitionStage::LeaguePhaseScheduled,
            "{} must be drawn at career start",
            competition.id
        );
        assert_eq!(state.draw_error, None);
    }
    assert!(clubs_in_two_european_competitions(&game).is_empty());
    let champions_league = game.competitions.iter().find(|c| c.id == "eu-ucl").unwrap();
    assert!(champions_league.participant_ids.iter().any(|id| id == PSG));
    let psg_matches = champions_league
        .fixtures
        .iter()
        .filter(|f| f.home_team_id == PSG || f.away_team_id == PSG)
        .count();
    assert_eq!(psg_matches, 8, "eight league-phase matches");
    assert!(game.active_competition_ids.iter().any(|id| id == "eu-ucl"));
    assert!(
        unplayed_clashes(&game).is_empty(),
        "{:?}",
        unplayed_clashes(&game)
    );
}

/// Given a PSG career in the real world
/// When the whole first season is played through the production day loop and rollover, and play
///      goes on until the three European editions are finished, archived and renewed
/// Then every harness invariant holds, no European match is left unplayed in the past, each edition
///      is archived once with a champion, and the next edition is drawn.
#[test]
fn the_first_european_season_is_played_to_its_finals_and_archived() {
    let Some(path) = world_or_skip() else {
        return;
    };
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);
    let mut watch = EuropeWatch::default();

    run_seasons(
        &mut game,
        RunOptions::seasons(1),
        &mut (
            &mut Rollovers,
            (&mut watch, (&mut invariants, &mut lifecycle)),
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let days = watch.days;
    finish_european_editions(
        &mut game,
        &mut (&mut watch, &mut invariants),
        days + 1,
        2027,
    );

    eprintln!("timeline:\n{:#?}", watch.timeline);
    assert!(watch.faults.is_empty(), "{:#?}", watch.faults);
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    assert_editions_archived(&game, &EUROPEAN_IDS, &[2026]);
    assert_prize_money_posted_once(&game, 2026);
    assert_next_fields_follow_the_season(&game, 2026);
}

/// What a season's calendar looked like against the one it was generated with, and how busy the
/// managed club's months were: read when the season is complete, before the rollover replaces it.
#[derive(Default)]
struct CalendarReport {
    study: Option<CalendarStudy>,
    displacement: Vec<Displacement>,
    months: BTreeMap<String, usize>,
    club: String,
}

impl SeasonObserver for CalendarReport {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        if let Some(study) = &self.study {
            self.displacement = study.displacement(game);
        }
        self.months = matches_per_month(game, &self.club);
    }
}

impl CalendarReport {
    fn print(&self) {
        eprintln!(
            "calendar displacement (fixtures moved / total, max delay, mean delay, planned end -> played end):"
        );
        for d in &self.displacement {
            eprintln!(
                "  {:<22} {:>4}/{:<4} max {:>3}d mean {:>5.1}d  {:?} -> {:?}",
                d.competition,
                d.moved,
                d.fixtures,
                d.max_delay_days,
                d.mean_delay_days(),
                d.planned_end,
                d.actual_end
            );
        }
        eprintln!("matches per month of {}: {:?}", self.club, self.months);
    }
}

/// The 2027/28 fields follow from the 2026/27 season: the Premier League's top four are in the
/// Champions League, the Champions League's and the Europa League's champions are there, the
/// Conference League's champion is at least in the Europa League, every field is full and no club
/// holds two places. Read from the archive, not from the qualification code.
fn assert_next_fields_follow_the_season(game: &Game, season: u32) {
    let archived = |id: &str| {
        game.edition_archive
            .iter()
            .find(|edition| edition.is_edition_of(id, season))
            .unwrap_or_else(|| panic!("{id} {season} is not archived"))
    };
    let field = |id: &str| -> Vec<String> {
        game.competitions
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} is missing"))
            .participant_ids
            .clone()
    };
    let (ucl, uel, uecl) = (field("eu-ucl"), field("eu-uel"), field("eu-uecl"));
    for (id, clubs) in [("eu-ucl", &ucl), ("eu-uel", &uel), ("eu-uecl", &uecl)] {
        assert_eq!(clubs.len(), 36, "{id}");
    }
    let mut seen = std::collections::BTreeSet::new();
    for club in ucl.iter().chain(&uel).chain(&uecl) {
        assert!(seen.insert(club), "{club} holds two European places");
    }
    let premier_league = game
        .edition_archive
        .iter()
        .find(|edition| edition.is_edition_of("fm-k-8", season))
        .expect("the Premier League season is archived");
    for entry in premier_league.standings.iter().take(4) {
        assert!(
            ucl.contains(&entry.team_id),
            "{} finished in the Premier League's top four and is not in the Champions League",
            entry.team_id
        );
    }
    assert!(
        ucl.contains(&archived("eu-ucl").champion_id),
        "the CL champion keeps its place"
    );
    assert!(
        ucl.contains(&archived("eu-uel").champion_id),
        "the EL champion moves up"
    );
    let conference_champion = &archived("eu-uecl").champion_id;
    assert!(
        uel.contains(conference_champion) || ucl.contains(conference_champion),
        "the ECL champion moves up"
    );
}

/// How many payments an archived edition must have posted, counted from its fixtures and ties alone
/// (not from the prize module): one participation payment per club, one per decisive league-phase
/// match (to the winner), two per drawn one, one per club in each knockout tie, and the champion's.
fn expected_prize_payments(edition: &domain::edition_archive::CompletedEdition) -> usize {
    let state = edition
        .league_phase
        .as_ref()
        .expect("a league-phase edition");
    let knockout = state.knockout_fixture_ids();
    let league_phase: usize = edition
        .fixtures
        .iter()
        .filter(|f| !knockout.contains(f.id.as_str()))
        .filter(|f| f.status == domain::league::FixtureStatus::Completed)
        .filter_map(|f| f.result.as_ref())
        .map(|result| {
            if result.home_goals == result.away_goals {
                2
            } else {
                1
            }
        })
        .sum();
    edition.participant_ids.len() + league_phase + 2 * state.ties.len() + 1
}

/// Prize money posted once: every payment key of an archived edition is unique and their number is
/// exactly what the results earned, so a retry, a reload or the rollover paid nothing twice, and no
/// club was paid for a match it did not play or a round it did not reach.
fn assert_prize_money_posted_once(game: &Game, season: u32) {
    for id in EUROPEAN_IDS {
        let edition = game
            .edition_archive
            .iter()
            .find(|edition| edition.is_edition_of(id, season))
            .unwrap_or_else(|| panic!("{id} {season} is not archived"));
        let posted = &edition.league_phase.as_ref().unwrap().payments_posted;
        let unique: std::collections::BTreeSet<&String> = posted.iter().collect();
        assert_eq!(
            unique.len(),
            posted.len(),
            "{id} {season}: a payment key was posted twice"
        );
        assert_eq!(
            posted.len(),
            expected_prize_payments(edition),
            "{id} {season}: payments posted differ from what the results earned"
        );
    }
}

/// Given a PSG career in the real world
/// When the first season is played
/// Then the calendar displacement of every domestic league and cup is measured against the dates
///      they were generated with, and nothing is moved so far that a competition cannot finish: no
///      domestic league finishes more than a month after it was planned to.
#[test]
fn european_dates_displace_domestic_fixtures_by_a_measured_amount() {
    let Some(path) = world_or_skip() else {
        return;
    };
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let mut report = CalendarReport {
        study: Some(CalendarStudy::new(&game)),
        club: PSG.to_string(),
        ..Default::default()
    };
    let mut invariants = Invariants::new(&game);
    run_seasons(
        &mut game,
        RunOptions::seasons(1),
        &mut (&mut report, &mut invariants),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    report.print();

    for d in report.displacement.iter().filter(|d| {
        game.competitions
            .iter()
            .any(|c| c.id == d.competition && c.kind == CompetitionType::League)
    }) {
        let late = match (d.planned_end, d.actual_end) {
            (Some(planned), Some(actual)) => (actual - planned).num_days(),
            _ => 0,
        };
        assert!(
            late <= 31,
            "{} finished {late} days after the day it was planned to ({:?} -> {:?})",
            d.competition,
            d.planned_end,
            d.actual_end
        );
    }
}

/// Which of the three European competitions a club is in at career start (`None`: no European
/// football), checked on the opened career.
fn european_competitions_of(game: &Game, club: &str) -> Vec<String> {
    european(game)
        .filter(|c| c.participant_ids.iter().any(|id| id == club))
        .map(|c| c.id.clone())
        .collect()
}

/// Given a manager of a Champions League club, a Europa League club, a Conference League club and
///       a club with no European football (each in the real world)
/// When each career is opened and a season is played
/// Then each club is in exactly its own competition (none for the last), follows only that
///      competition, plays its matches there, and no extra European competition appears.
#[test]
#[ignore = "minutes: four full careers; run by hand for the audit"]
fn each_kind_of_club_plays_exactly_its_own_european_season() {
    let Some(path) = world_or_skip() else {
        return;
    };
    // PSG (CL), Marseille (EL), Monaco (ECL) and a Ligue 1 club in none of the three fields.
    let probe = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let in_europe: std::collections::BTreeSet<&str> = european(&probe)
        .flat_map(|c| c.participant_ids.iter().map(String::as_str))
        .collect();
    let ligue_1 = probe
        .competitions
        .iter()
        .find(|c| c.id == "fm-k-12")
        .expect("Ligue 1");
    let none = ligue_1
        .participant_ids
        .iter()
        .find(|id| !in_europe.contains(id.as_str()))
        .expect("a Ligue 1 club without European football")
        .clone();
    let cases: [(&str, Option<&str>); 4] = [
        (PSG, Some("eu-ucl")),
        ("fm-c-496", Some("eu-uel")),
        ("fm-c-466", Some("eu-uecl")),
        (none.as_str(), None),
    ];
    for (club, expected) in cases {
        let mut game = build_fm24_career(&path, club, 7).expect("the career builds");
        let own = european_competitions_of(&game, club);
        assert_eq!(
            own,
            expected.map(String::from).into_iter().collect::<Vec<_>>(),
            "{club}"
        );
        let followed: Vec<&str> = european(&game)
            .filter(|c| game.active_competition_ids.contains(&c.id))
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(
            followed,
            expected.into_iter().collect::<Vec<_>>(),
            "{club} follows"
        );

        let mut invariants = Invariants::new(&game);
        let mut watch = EuropeWatch::default();
        run_seasons(
            &mut game,
            RunOptions::seasons(1),
            &mut (&mut watch, &mut invariants),
        )
        .unwrap_or_else(|error| panic!("{club}: {error}"));
        assert!(watch.faults.is_empty(), "{club}: {:#?}", watch.faults);
        assert!(invariants.is_clean(), "{club}: {}", invariants.report());
        // The club's own matches in Europe were all played: 8 or 6 league-phase matches at least.
        if let Some(id) = expected {
            let played = game
                .competitions
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .fixtures
                .iter()
                .filter(|f| {
                    (f.home_team_id == club || f.away_team_id == club)
                        && f.status == domain::league::FixtureStatus::Completed
                })
                .count();
            assert!(played >= 6, "{club} played only {played} matches in {id}");
        }
        eprintln!("{club}: ok ({:?})", expected);
    }
}

/// Given a PSG career in the real world
/// When three seasons are played, the game saved and reloaded each time any European competition
///      moves to a new stage (between the legs of a tie, before a final, before and after each
///      rollover included), and play goes on until the third edition is archived
/// Then every competition is byte-identical after each reload, every edition is archived once with
///      its champion and renewed with a fresh draw, no club is ever in two competitions, and every
///      harness rule holds.
#[test]
#[ignore = "about ten minutes; run by hand for the audit"]
fn three_european_seasons_survive_a_reload_at_every_stage() {
    let Some(path) = world_or_skip() else {
        return;
    };
    let seed = std::env::var("I5_SEED")
        .ok()
        .and_then(|seed| seed.parse::<u64>().ok())
        .unwrap_or(7);
    let mut game = build_fm24_career(&path, PSG, seed).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut watch = EuropeWatch::default();
    let mut last: BTreeMap<String, CompetitionStage> = BTreeMap::new();
    let mut reloads = 0;
    let mut day = 0;
    let mut season = 1;
    let rollovers_wanted = 3;
    let mut rolled = 0;
    while rolled < rollovers_wanted || !all_european_editions_completed(&game) {
        day += 1;
        assert!(day < 1_500, "the run did not end: {:#?}", watch.timeline);
        if ofm_core::end_of_season::is_season_complete(&game) {
            season_harness::driver::roll_season_over(
                &mut game,
                RunOptions::seasons(1),
                season,
                &mut (&mut watch, &mut invariants),
            )
            .unwrap_or_else(|error| panic!("{error}"));
            rolled += 1;
            season += 1;
            // After the rollover: the reload that proves renewal is saved correctly.
            game = reloaded(&game);
            reloads += 1;
            continue;
        }
        play_day(
            &mut game,
            RunOptions::seasons(1),
            season,
            day,
            &mut (&mut watch, &mut invariants),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let moved = european(&game).any(|c| {
            c.league_phase
                .as_ref()
                .is_some_and(|state| last.insert(c.id.clone(), state.stage) != Some(state.stage))
        });
        if moved {
            let before: Vec<String> = european(&game)
                .map(|c| serde_json::to_string(c).unwrap())
                .collect();
            game = reloaded(&game);
            let after: Vec<String> = european(&game)
                .map(|c| serde_json::to_string(c).unwrap())
                .collect();
            for (a, b) in before.iter().zip(&after) {
                assert!(
                    a == b,
                    "a reload changed a competition: {}",
                    window_of_difference(a, b)
                );
            }
            reloads += 1;
        }
    }
    eprintln!(
        "seed {seed}: {reloads} reloads, {rolled} rollovers, clock {}",
        game.clock.current_date.date_naive()
    );
    assert!(watch.faults.is_empty(), "{:#?}", watch.faults);
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(reloads >= 30, "only {reloads} reloads");
    // Editions 2026 and 2027 are archived (2028 may be playing): one record each, never two.
    assert_editions_archived(&game, &EUROPEAN_IDS, &[2026, 2027]);
    assert_prize_money_posted_once(&game, 2026);
    assert_prize_money_posted_once(&game, 2027);
}

struct Quiet;

impl SeasonObserver for Quiet {}

/// Given a PSG career played to the end of the league phase (the first knockout draw is made)
/// When it is written to a SQLite save and loaded from a fresh manager
/// Then every European competition (draw, pots, table, ties, posted prizes, the continental-only
///      clubs' teams and players) is identical to the one saved, and both copies then play the same
///      thirty days to the same fingerprint.
#[test]
fn a_mid_season_european_career_survives_the_sqlite_save() {
    use db::save_manager::SaveManager;
    use domain::stats::StatsState;
    use season_harness::fingerprint::{fingerprint, first_difference};

    let Some(path) = world_or_skip() else {
        return;
    };
    let mut original = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let options = RunOptions::seasons(1);
    for day in 1..=260 {
        play_day(&mut original, options, 1, day, &mut Quiet).expect("a day plays");
    }
    assert!(
        european(&original).any(|c| {
            c.league_phase
                .as_ref()
                .is_some_and(|s| s.stage != CompetitionStage::LeaguePhaseScheduled)
        }),
        "the run reached the knockout phase"
    );

    let dir = tempfile::tempdir().expect("a directory");
    let mut manager = SaveManager::init(dir.path()).expect("a save manager");
    let id = manager
        .create_save_with_stats(&original, &StatsState::default(), "i5")
        .expect("the save");
    let mut twin = SaveManager::init(dir.path())
        .expect("a fresh manager")
        .load_game(&id)
        .expect("the load");

    // The save lists competitions in its own order (by id): compare each competition with its twin.
    for a in european(&original) {
        let b = european(&twin)
            .find(|candidate| candidate.id == a.id)
            .unwrap_or_else(|| panic!("{} is missing after the reload", a.id));
        let (left, right) = (
            serde_json::to_string(a).unwrap(),
            serde_json::to_string(b).unwrap(),
        );
        assert!(
            left == right,
            "{} differs after the SQLite round trip: {}",
            a.id,
            window_of_difference(&left, &right)
        );
    }
    assert_eq!(original.teams.len(), twin.teams.len());
    assert_eq!(original.players.len(), twin.players.len());
    for day in 261..=290 {
        play_day(&mut original, options, 1, day, &mut Quiet).expect("plays");
        play_day(&mut twin, options, 1, day, &mut Quiet).expect("plays");
    }
    // Line by line and sorted: the save lists competitions by id, so only a difference in content
    // (not in the order competitions are listed in) is a difference in play.
    let sorted = |text: String| {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines.sort();
        lines.join("\n")
    };
    let (left, right) = (sorted(fingerprint(&original)), sorted(fingerprint(&twin)));
    assert!(
        left == right,
        "the reloaded career plays differently: {:?}",
        first_difference(&left, &right)
    );
}

/// Given a PSG career whose days are played on the manager's own workflow (the live engine with
///       benches and both managers, finished with the day's tail: `matchday::play_user_matchday`)
/// When the first European league-phase matches fall due
/// Then PSG's Champions League matches are played that way too, each with the engine's report
///      attached (the same report the 2D view and the post-match screen read), once each, with the
///      competition's table following the results and no home advantage given to a neutral venue.
#[test]
fn psg_european_matches_play_through_the_managers_live_matchday() {
    let Some(path) = world_or_skip() else {
        return;
    };
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let options = RunOptions {
        day_path: season_harness::driver::DayPath::UserMatchLive,
        ..RunOptions::seasons(1)
    };
    let mut live_days = 0;
    for day in 1..=246 {
        if play_day(&mut game, options, 1, day, &mut Quiet).expect("a day plays") {
            live_days += 1;
        }
    }
    let champions_league = game.competitions.iter().find(|c| c.id == "eu-ucl").unwrap();
    let psg_played: Vec<_> = champions_league
        .fixtures
        .iter()
        .filter(|f| f.home_team_id == PSG || f.away_team_id == PSG)
        .filter(|f| f.status == domain::league::FixtureStatus::Completed)
        .collect();
    assert_eq!(
        psg_played.len(),
        8,
        "the eight league-phase matches were played by the end of January"
    );
    assert!(live_days >= 8, "{live_days} live days");
    for fixture in &psg_played {
        let result = fixture.result.as_ref().expect("a result");
        assert!(
            result.report.is_some(),
            "{} {}: the engine's report is attached",
            fixture.date,
            fixture.id
        );
    }
    // The table is the sum of the results, for PSG too: it played eight.
    let state = champions_league.league_phase.as_ref().unwrap();
    let psg_row = state.table.iter().find(|row| row.team_id == PSG).unwrap();
    assert_eq!(psg_row.played, 8);
    assert!(state.stage != CompetitionStage::LeaguePhaseScheduled);
}

/// Prints what a career costs in the world named by `APFM_WORLD_JSON`: building the game, one full
/// season, saving to SQLite (size and time) and loading it back. Run it on the European world and
/// on the `--no-european` control world and compare; it asserts nothing about the numbers.
#[test]
#[ignore = "measurement: run by hand on both worlds with /usr/bin/time -l"]
fn what_a_career_costs() {
    use db::save_manager::SaveManager;
    use domain::stats::StatsState;
    use std::time::Instant;

    let Some(path) = world_or_skip() else {
        return;
    };
    let started = Instant::now();
    let mut game = build_fm24_career(&path, PSG, 7).expect("the career builds");
    let build = started.elapsed();
    let (teams, players) = (game.teams.len(), game.players.len());

    let started = Instant::now();
    let mut quiet = Quiet;
    run_seasons(&mut game, RunOptions::seasons(1), &mut quiet).expect("a season plays");
    let season = started.elapsed();

    let dir = tempfile::tempdir().expect("a directory");
    let mut manager = SaveManager::init(dir.path()).expect("a save manager");
    let started = Instant::now();
    let id = manager
        .create_save_with_stats(&game, &StatsState::default(), "perf")
        .expect("the save");
    let save = started.elapsed();
    let bytes: u64 = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                std::fs::read_dir(path)
                    .map(|inner| inner.filter_map(Result::ok).map(|e| e.path()).collect())
                    .unwrap_or_default()
            } else {
                vec![path]
            }
        })
        .filter_map(|file| std::fs::metadata(file).ok())
        .map(|meta| meta.len())
        .sum();
    let started = Instant::now();
    let _loaded = SaveManager::init(dir.path())
        .expect("a fresh manager")
        .load_game(&id)
        .expect("the load");
    let load = started.elapsed();
    eprintln!(
        "PERF world={} teams={teams} players={players} build={build:.1?} season={season:.1?} save={save:.1?} load={load:.1?} save_bytes={bytes}",
        path.display()
    );
}
