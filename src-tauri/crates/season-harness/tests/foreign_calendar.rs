//! The competitions of a world that is not the manager's own, over several seasons: each edition
//! renews on its own authored calendar, each season is archived exactly once, and no country sits
//! idle for most of a year waiting for the manager's rollover.
//!
//! The gate world puts the manager in England and the world in Portugal and Argentina. Portugal is
//! an ordinary two-division country that finishes in November. Argentina is a split season: its
//! Apertura opens in February and its Clausura in July, and both halves share the same clubs. The
//! daily hook is what keeps those dates; the rollover alone used to start them at its own anchor.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use domain::league::{CompetitionFormat, CompetitionScope, CompetitionType, League};
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, play_day, roll_season_over, run_seasons};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;
use season_harness::worlds::WorldSpec;

const SEED: u64 = 1;
const SEASONS: u32 = 3;

/// The domestic league tables: the competitions a country's ladder is made of.
fn ladder_tables(game: &Game) -> Vec<&League> {
    game.competitions
        .iter()
        .filter(|competition| {
            competition.scope == CompetitionScope::Domestic
                && competition.kind == CompetitionType::League
                && competition.rules.format == CompetitionFormat::LeagueTable
        })
        .collect()
}

fn opener(competition: &League) -> Option<NaiveDate> {
    competition
        .fixtures
        .iter()
        .filter_map(|fixture| NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").ok())
        .min()
}

/// How many months apart two calendar months are, the short way round.
fn month_distance(left: u32, right: u32) -> u32 {
    let difference = left.abs_diff(right);
    difference.min(12 - difference)
}

/// A calendar finding, named by the competition and edition it is about.
fn calendar_findings(game: &Game) -> Vec<String> {
    let mut findings = Vec::new();
    for competition in ladder_tables(game) {
        let mut editions: Vec<_> = game
            .edition_archive
            .iter()
            .filter(|edition| edition.competition_id == competition.id)
            .collect();
        editions.sort_by_key(|edition| edition.season);

        let seasons: Vec<u32> = editions.iter().map(|edition| edition.season).collect();
        for pair in seasons.windows(2) {
            if pair[1] != pair[0] + 1 {
                findings.push(format!(
                    "{}: archived seasons skip from {} to {} (an edition was lost or renewed twice)",
                    competition.id, pair[0], pair[1]
                ));
            }
        }
        if let Some(duplicate) = seasons.windows(2).find(|pair| pair[0] == pair[1]) {
            findings.push(format!(
                "{}: season {} is archived twice",
                competition.id, duplicate[0]
            ));
        }

        for edition in &editions {
            if let Some(start) = opener_from_fixtures(&edition.fixtures) {
                if competition.season_start_month != 0
                    && month_distance(start.month(), competition.season_start_month as u32) > 1
                {
                    findings.push(format!(
                        "{}: season {} opens on {} instead of its authored month {}",
                        competition.id, edition.season, start, competition.season_start_month
                    ));
                }
                if start.year() < edition.season as i32 {
                    findings.push(format!(
                        "{}: season {} opened in {} before its own season",
                        competition.id, edition.season, start
                    ));
                }
            } else {
                findings.push(format!(
                    "{}: season {} is archived with no fixture to date it",
                    competition.id, edition.season
                ));
            }
        }

        // A foreign league that waits for the manager's rollover sat finished for most of a year.
        // These calendars leave at most a four-month break between an edition and its next opener.
        for pair in editions.windows(2) {
            let Some(previous_end) = row_completed_on(&pair[0].completed_on) else {
                continue;
            };
            let Some(next_start) = opener_from_fixtures(&pair[1].fixtures) else {
                continue;
            };
            let idle = (next_start - previous_end).num_days();
            if idle > 150 {
                findings.push(format!(
                    "{}: season {} ended {} and season {} only opens {} ({idle} idle days)",
                    competition.id, pair[0].season, previous_end, pair[1].season, next_start
                ));
            }
        }
    }
    findings
}

fn opener_from_fixtures(fixtures: &[domain::league::Fixture]) -> Option<NaiveDate> {
    fixtures
        .iter()
        .filter_map(|fixture| NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").ok())
        .min()
}

/// Fixture-identity findings: every fixture id belongs to one competition, no id appears twice in
/// the world, a fixture names its own competition, and no club plays itself.
fn fixture_identity_findings(game: &Game) -> Vec<String> {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    let mut findings = Vec::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if let Some(owner) = seen.insert(fixture.id.as_str(), competition.id.as_str()) {
                findings.push(format!(
                    "fixture id {} is reused by {} and {}",
                    fixture.id, owner, competition.id
                ));
            }
            if !fixture.competition_id.is_empty() && fixture.competition_id != competition.id {
                findings.push(format!(
                    "fixture {} of {} names competition {}",
                    fixture.id, competition.id, fixture.competition_id
                ));
            }
            if fixture.home_team_id == fixture.away_team_id {
                findings.push(format!(
                    "fixture {} in {} has {} playing itself",
                    fixture.id, competition.id, fixture.home_team_id
                ));
            }
        }
    }
    findings
}

fn row_completed_on(completed_on: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(completed_on, "%Y-%m-%d").ok()
}

/// Given the gate world, When three seasons are played with the production day and rollover paths,
/// Then every domestic table renews on its own calendar, every edition is archived once, and no
///      competition sits finished for most of a year.
#[test]
fn foreign_leagues_keep_their_authored_calendar_and_archive_every_edition_once() {
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
    let findings = calendar_findings(&game);
    assert!(findings.is_empty(), "{}", findings.join("\n"));
    let identities = fixture_identity_findings(&game);
    assert!(identities.is_empty(), "{}", identities.join("\n"));

    // The defect this replaced: Argentina's Apertura finished in October and only started again in
    // April/May, at the rollover anchor. It now opens on 1 February, every season.
    let apertura = game
        .competitions
        .iter()
        .find(|competition| competition.id == "ar-d1-apertura")
        .expect("the gate world has the Argentinian Apertura");
    let mut openers: Vec<_> = game
        .edition_archive
        .iter()
        .filter(|edition| edition.competition_id == "ar-d1-apertura")
        .filter_map(|edition| opener_from_fixtures(&edition.fixtures))
        .collect();
    openers.sort();
    assert_eq!(openers.len(), SEASONS as usize);
    assert!(
        openers
            .iter()
            .all(|start| (start.month(), start.day()) == (2, 1)),
        "the Apertura opener drifted: {openers:?}"
    );
    assert_eq!(
        opener(apertura)
            .map(|start| (start.month(), start.day()))
            .unwrap_or_default(),
        (2, 1)
    );
}

/// Given a career whose seasons are rolled over one at a time, When it is saved to SQLite and read
/// back at every rollover, Then the reloaded world is identical and the calendar findings stay
///      empty: the hook's renewals are saved state, not a session-only view.
#[test]
fn a_reloaded_career_keeps_the_renewed_calendar() {
    let mut career = Career::begin();
    let mut invariants = Invariants::new(&career.game);
    let mut lifecycle = Lifecycle::new(&career.game);
    let options = RunOptions::seasons(1);

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
        }
        roll_season_over(
            &mut career.game,
            options,
            season,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        career.save_and_reload(season);
    }

    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    let findings = calendar_findings(&career.game);
    assert!(findings.is_empty(), "{}", findings.join("\n"));
    let identities = fixture_identity_findings(&career.game);
    assert!(identities.is_empty(), "{}", identities.join("\n"));
}

/// The world as a reload must reproduce it, in order. The only line left out is the squad-floor
/// top-up log, a session diagnostic that is never saved (`Game::squad_floor_top_ups`).
fn fingerprint(game: &Game) -> String {
    season_harness::fingerprint::fingerprint(game)
        .lines()
        .filter(|line| !line.starts_with("top_up "))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Career {
    game: Game,
    manager: db::save_manager::SaveManager,
    save_id: String,
    _dir: tempfile::TempDir,
}

impl Career {
    fn begin() -> Self {
        let game = WorldSpec::gate(SEED).build().expect("the world builds");
        let dir = tempfile::tempdir().expect("a temporary save directory");
        let mut manager = db::save_manager::SaveManager::init(dir.path()).expect("save manager");
        let save_id = manager
            .create_save_with_stats(
                &game,
                &domain::stats::StatsState::default(),
                "foreign-calendar",
            )
            .expect("the opening save");
        Self {
            game,
            manager,
            save_id,
            _dir: dir,
        }
    }

    fn save_and_reload(&mut self, season: u32) {
        let before = fingerprint(&self.game);
        self.manager
            .save_game_with_stats(
                &self.game,
                &domain::stats::StatsState::default(),
                &self.save_id,
            )
            .unwrap_or_else(|error| panic!("season {season}: the save failed: {error}"));
        let loaded = db::save_manager::SaveManager::init(self._dir.path())
            .expect("a fresh save manager")
            .load_game(&self.save_id)
            .unwrap_or_else(|error| panic!("season {season}: the load failed: {error}"));
        let after = fingerprint(&loaded);
        assert_eq!(
            before,
            after,
            "season {season}: the reloaded career differs: {}",
            season_harness::fingerprint::first_difference(&before, &after)
        );
        self.game = loaded;
    }
}

/// Given the player's own imported FM24 world, with the manager at PSG, When five seasons are
/// played, Then every foreign domestic table renews on its authored month, every edition is
/// archived once, and no league sits finished waiting for the French rollover.
///
/// The world file is not in the repository: run with `APFM_WORLD_JSON=<m7world file>`; without it
/// the study reports itself as skipped and returns. `m7world` is the import that carries the six
/// national cups; the older `m4world` file has leagues only.
#[test]
#[ignore = "imported-world study (needs APFM_WORLD_JSON)"]
fn imported_world_keeps_foreign_calendars_for_five_seasons() {
    let Some(path) = season_harness::fm24::world_path_from_env() else {
        eprintln!("skipped: APFM_WORLD_JSON is not set to a world file");
        return;
    };
    let mut game =
        season_harness::fm24::build_fm24_career(&path, "fm-c-498", 7).expect("the career builds");
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);
    let options = RunOptions::seasons(1);
    let started = std::time::Instant::now();

    for season in 1..=5 {
        let began = std::time::Instant::now();
        run_seasons(&mut game, options, &mut (&mut invariants, &mut lifecycle))
            .unwrap_or_else(|error| panic!("season {season}: {error}"));
        let findings = calendar_findings(&game);
        assert!(
            findings.is_empty(),
            "season {season}: {}",
            findings.join("\n")
        );
        let identities = fixture_identity_findings(&game);
        assert!(
            identities.is_empty(),
            "season {season}: {}",
            identities.join("\n")
        );
        eprintln!("  imported season {season}: {:.1?}", began.elapsed());
    }

    eprintln!(
        "imported world: {} clubs, {} competitions, {} archived editions in {:.1?}",
        game.teams.len(),
        game.competitions.len(),
        game.edition_archive.len(),
        started.elapsed()
    );
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    // The authored calendar of the import is what each foreign table must keep.
    let foreign: Vec<_> = ladder_tables(&game)
        .into_iter()
        .filter(|competition| competition.country_id.as_deref() != Some("FR"))
        .collect();
    assert!(
        foreign.len() >= 8,
        "the import has several foreign divisions: {}",
        foreign.len()
    );
    for competition in foreign {
        let mut editions: Vec<_> = game
            .edition_archive
            .iter()
            .filter(|edition| edition.competition_id == competition.id)
            .collect();
        editions.sort_by_key(|edition| edition.season);
        assert!(
            editions.len() >= 2,
            "{} renewed at least once",
            competition.id
        );
    }
}

/// Thirty seasons of the gate world: the long-run proof that no country drifts, no edition is
/// archived twice, and the daily hook does not accumulate cost. Measurement only (`#[ignore]`d).
#[test]
#[ignore = "thirty-season stress study"]
fn thirty_seasons_of_foreign_calendars() {
    let mut game = WorldSpec::gate(SEED).build().expect("the world builds");
    let mut invariants = Invariants::new(&game);
    let mut lifecycle = Lifecycle::new(&game);
    let options = RunOptions::seasons(1);
    let started = std::time::Instant::now();
    let mut per_season = BTreeMap::new();

    for season in 1..=30 {
        let began = std::time::Instant::now();
        run_seasons(&mut game, options, &mut (&mut invariants, &mut lifecycle))
            .unwrap_or_else(|error| panic!("season {season}: {error}"));
        per_season.insert(season, began.elapsed());
        let findings = calendar_findings(&game);
        assert!(
            findings.is_empty(),
            "season {season}: {}",
            findings.join("\n")
        );
        let identities = fixture_identity_findings(&game);
        assert!(
            identities.is_empty(),
            "season {season}: {}",
            identities.join("\n")
        );
    }

    eprintln!("thirty gate seasons in {:.1?}", started.elapsed());
    for (season, elapsed) in &per_season {
        if *season <= 3 || season % 5 == 0 {
            eprintln!("  season {season:>2}: {elapsed:.1?}");
        }
    }
    eprintln!(
        "archived editions: {} across {} competitions",
        game.edition_archive.len(),
        game.competitions.len()
    );
    assert!(invariants.is_clean(), "{}", invariants.report());
    assert!(lifecycle.is_clean(), "{}", lifecycle.report());
    let findings = calendar_findings(&game);
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}
