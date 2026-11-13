//! The external market (D7B2) joined to a live P5 career, end to end.
//!
//! Nothing here is a stub of the other side: a small source `world.db` goes through the production
//! `build_registry`, each career gets its cache from `ensure_career_cache`, a search and a detail
//! read run the same `search_internal` / `detail_internal` the commands run, the signing goes
//! through `recruit_internal` into a generated P5 world (the gate world, with its calendar, squads
//! and saved eleven), and the save is written and read back through the production persistence.
//! The source is synthetic and invented; no football data is read or written.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use chrono::{Datelike, Duration};
use db::game_database::GameDatabase;
use db::game_persistence::{GamePersistenceReader, GamePersistenceWriter};
use domain::league::FixtureStatus;
use fm24_adapter::registry::{build_registry, RegistryAvailability};
use fm24_adapter::source::ATTRIBUTE_COLUMNS;
use ofm_core::live_match_manager::{kick_off_live_match, MatchMode};
use rusqlite::Connection;
use season_harness::worlds::WorldSpec;
use tempfile::TempDir;

use super::*;

const NOT_ALREADY_THERE: &str = "be.error.registry.playerAlreadyInWorld";

/// The invented people of the source: `(person id, club id or none, ability, birth year offset)`.
/// Club 900 and 901 are outside the game's world, club 1 is the source's own perimeter.
struct Person {
    id: i64,
    club: Option<i64>,
    ability: i64,
    age: i32,
    position: &'static str,
}

const PEOPLE: [Person; 9] = [
    Person {
        id: 101,
        club: None,
        ability: 62,
        age: 24,
        position: "MC",
    },
    Person {
        id: 102,
        club: Some(900),
        ability: 66,
        age: 27,
        position: "MC",
    },
    Person {
        id: 103,
        club: Some(901),
        ability: 64,
        age: 25,
        position: "DC",
    },
    // A player far above the user's club, so the next match picks him on merit.
    Person {
        id: 104,
        club: Some(900),
        ability: 95,
        age: 26,
        position: "MC",
    },
    Person {
        id: 105,
        club: None,
        ability: 58,
        age: 22,
        position: "AMC",
    },
    Person {
        id: 106,
        club: Some(900),
        ability: 63,
        age: 28,
        position: "MC",
    },
    Person {
        id: 107,
        club: None,
        ability: 60,
        age: 29,
        position: "MC",
    },
    // Every season of a real market loses players to retirement, and the market replaces them.
    // He retires on the first advance, so the fixture has the turnover the intake is sized from.
    Person {
        id: 108,
        club: Some(900),
        ability: 52,
        age: 38,
        position: "DC",
    },
    // The source's own perimeter: the registry never offers him.
    Person {
        id: 1,
        club: Some(1),
        ability: 70,
        age: 25,
        position: "STC",
    },
];

fn write_source(path: &Path, start_season: i32) {
    let conn = Connection::open(path).unwrap();
    let columns = ATTRIBUTE_COLUMNS
        .iter()
        .map(|column| format!("{column} INTEGER"))
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute_batch(&format!(
        "CREATE TABLE import_run(id INTEGER PRIMARY KEY, start_season INTEGER, game_version TEXT,
            source_sha256 TEXT, importer_version TEXT, imported_at TEXT);
         INSERT INTO import_run VALUES (1, {start_season}, 'synthetic', 'synthetic-source-hash', '1', '2026-01-01');
         CREATE TABLE nation(id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO nation VALUES (1, 'England');
         CREATE TABLE club(id INTEGER PRIMARY KEY, fm_uid TEXT, name TEXT, short_name TEXT,
            nation_id INTEGER, reputation INTEGER);
         INSERT INTO club VALUES (1, '1', 'Alpha FC', 'ALP', 1, 60),
                                 (900, '900', 'External United', 'EXT', 1, 55),
                                 (901, '901', 'Faraway Town', 'FAR', 1, 35);
         CREATE TABLE team(id INTEGER PRIMARY KEY, club_id INTEGER, slot INTEGER, is_youth INTEGER);
         INSERT INTO team VALUES (10, 1, 0, 0), (9000, 900, 0, 0), (9010, 901, 0, 0);
         CREATE TABLE person(id INTEGER PRIMARY KEY, fm_uid INTEGER, fm_uid_status TEXT,
            first_name TEXT, last_name TEXT, common_name TEXT, birth_date TEXT, nation_id INTEGER,
            height_cm INTEGER);
         CREATE TABLE player(person_id INTEGER PRIMARY KEY, ca INTEGER, pa INTEGER,
            rep_current INTEGER, rep_home INTEGER, rep_world INTEGER, foot_left INTEGER,
            foot_right INTEGER, value_gbp INTEGER, value_source TEXT, registered_team_id INTEGER);
         CREATE TABLE player_attributes(person_id INTEGER PRIMARY KEY, {columns});
         CREATE TABLE player_position(person_id INTEGER, position TEXT, rating INTEGER);
         CREATE TABLE contract(id INTEGER PRIMARY KEY, person_id INTEGER, club_id INTEGER,
            team_id INTEGER, is_current INTEGER, wage_gbp_week INTEGER, wage_original INTEGER,
            wage_source TEXT, start_date TEXT, end_date TEXT, contract_type_code INTEGER,
            squad_status_code INTEGER, role_code TEXT);
         CREATE TABLE competition(id INTEGER PRIMARY KEY, kind TEXT, playable INTEGER, simulated INTEGER);
         INSERT INTO competition VALUES (1, 'league', 1, 0);
         CREATE TABLE competition_season(id INTEGER PRIMARY KEY, competition_id INTEGER, season INTEGER);
         INSERT INTO competition_season VALUES (1, 1, {start_season});
         CREATE TABLE competition_season_team(competition_season_id INTEGER, team_id INTEGER);
         INSERT INTO competition_season_team VALUES (1, 10);"
    ))
    .unwrap();
    for (n, person) in PEOPLE.iter().enumerate() {
        let birth = start_season - person.age;
        let attributes = vec![person.ability.to_string(); ATTRIBUTE_COLUMNS.len()].join(", ");
        let registered = match person.club {
            Some(1) => "10",
            _ => "NULL",
        };
        let contract = match person.club {
            Some(club) => format!(
                "INSERT INTO contract VALUES ({n}, {id}, {club}, {team}, 1, 9000, 9000, 'save',
                    '{start}-07-01', '{end}-06-30', 1, 1, NULL);",
                id = person.id,
                team = if club == 1 { 10 } else { club * 10 },
                start = start_season - 1,
                end = start_season + 3,
            ),
            None => String::new(),
        };
        conn.execute_batch(&format!(
            "INSERT INTO person VALUES ({id}, {id}, 'unique', 'Invented', 'Player{id}', NULL,
                '{birth}-03-01', 1, 180);
             INSERT INTO player VALUES ({id}, {ca}, {ca}, 50, 40, 30, 20, 80, 2000000, 'save', {registered});
             INSERT INTO player_attributes VALUES ({id}, {attributes});
             INSERT INTO player_position VALUES ({id}, '{position}', 20);
             {contract}",
            id = person.id,
            ca = person.ability * 2,
            position = person.position,
        ))
        .unwrap();
    }
}

/// One built world, cloned per test: generating it is the slow part and no test mutates the original.
fn generated_world() -> &'static Game {
    static WORLD: OnceLock<Game> = OnceLock::new();
    WORLD.get_or_init(|| {
        let mut game = WorldSpec::gate(31).build().expect("the gate world builds");
        game.seed = 4_242;
        game
    })
}

/// A career: the shared source registry in its own directory, and a P5 game with money to spend.
struct Career {
    dir: TempDir,
    base: PathBuf,
}

impl Career {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let start = generated_world().clock.current_date.year();
        let source = dir.path().join("world.db");
        write_source(&source, start);
        let base = dir.path().join("registry.db");
        build_registry(&source, &base).expect("the registry builds");
        Self { dir, base }
    }

    /// A copy of the generated world as one career's game: its own seed, rich, the window open.
    fn game(&self, seed: u64) -> Game {
        let mut game = generated_world().clone();
        game.seed = seed;
        let team_id = game
            .manager
            .team_id
            .clone()
            .expect("the manager has a club");
        // Cash moves through the ledger like everywhere else (the finance-journal guard).
        let today = game.clock.current_date.date_naive();
        ofm_core::finances::post_all(
            &mut game,
            &[ofm_core::finances::PostRequest::new(
                &team_id,
                900_000_000,
                domain::finance::CashKind::BoardSupport,
                today,
            )],
        )
        .expect("the board funds the club");
        let team = game
            .teams
            .iter_mut()
            .find(|team| team.id == team_id)
            .unwrap();
        team.transfer_budget = 900_000_000;
        team.wage_budget = 900_000_000;
        game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Open;
        game
    }

    fn search(&self, game: &Game, availability: RegistryAvailability) -> Vec<String> {
        let context = registry_context(game);
        let request = RegistrySearchRequest {
            availability,
            page_size: 100,
            ..RegistrySearchRequest::default()
        };
        search_internal(&self.base, &context, request)
            .expect("the search runs")
            .items
            .into_iter()
            .map(|item| item.id)
            .collect()
    }

    /// The commands' own path: detail from this career's cache, then the promotion.
    fn recruit(&self, state: &StateManager, id: &str) -> Result<ExternalRecruitOutcome, String> {
        let context = state.get_game(registry_context).expect("a game");
        let detail = detail_internal(&self.base, &context, id)?
            .ok_or_else(|| "be.error.registry.playerNotAvailable".to_string())?;
        let external = external_from_detail(detail)?;
        recruit_internal(state, &external).map(|(_, outcome)| outcome)
    }

    fn source_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.base).unwrap()
    }

    fn cache_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("career-") && name.ends_with(".db"))
            })
            .collect();
        files.sort();
        files
    }
}

fn state_with(game: Game) -> StateManager {
    let state = StateManager::new();
    state.set_game(game);
    state
}

fn user_club(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn count_of(game: &Game, id: &str) -> usize {
    game.players.iter().filter(|player| player.id == id).count()
}

/// Given a career with a free agent and a contracted player in its market
/// When the manager signs both through the commands' own path
/// Then each is promoted exactly once under his stable `fm-p-*` id, sits in the club's squad, and
///      is gone from this career's search; a second attempt is refused
#[test]
fn a_registry_player_is_promoted_once_and_leaves_the_market() {
    let career = Career::new();
    let state = state_with(career.game(1));
    let before = state
        .get_game(|game| career.search(game, RegistryAvailability::All))
        .unwrap();
    assert!(before.contains(&"fm-p-101".to_string()) && before.contains(&"fm-p-102".to_string()));
    assert!(
        !before.contains(&"fm-p-1".to_string()),
        "the source's own perimeter is never offered"
    );

    let free = career
        .recruit(&state, "fm-p-101")
        .expect("a free agent signs");
    let bought = career
        .recruit(&state, "fm-p-102")
        .expect("a contracted player is bought");

    assert!(free.free_agent && !bought.free_agent && bought.fee > 0);
    let game = state.get_game(Game::clone).unwrap();
    let club = user_club(&game);
    for id in ["fm-p-101", "fm-p-102"] {
        assert_eq!(count_of(&game, id), 1, "{id} is promoted exactly once");
        let player = game.players.iter().find(|player| player.id == id).unwrap();
        assert_eq!(player.team_id.as_deref(), Some(club.as_str()));
    }
    let after = career.search(&game, RegistryAvailability::All);
    assert!(!after.contains(&"fm-p-101".to_string()) && !after.contains(&"fm-p-102".to_string()));
    assert_eq!(
        career.recruit(&state, "fm-p-101").err().as_deref(),
        Some(NOT_ALREADY_THERE)
    );
}

/// Given a career two seasons into its life
/// When the manager signs a Newgen from the market
/// Then he is promoted once under his own `fm-n-*` id and the market never offers him again
#[test]
fn a_newgen_is_promoted_once_and_leaves_the_market() {
    let career = Career::new();
    let mut game = career.game(2);
    game.clock.current_date += Duration::days(2 * 365);
    let state = state_with(game);
    let market = state
        .get_game(|game| career.search(game, RegistryAvailability::All))
        .unwrap();
    let newgen = market
        .iter()
        .find(|id| id.starts_with("fm-n-"))
        .expect("two seasons of intake put Newgens on the market")
        .clone();

    career.recruit(&state, &newgen).expect("a Newgen signs");

    let game = state.get_game(Game::clone).unwrap();
    assert_eq!(count_of(&game, &newgen), 1);
    assert!(!career
        .search(&game, RegistryAvailability::All)
        .contains(&newgen));
    assert_eq!(
        career.recruit(&state, &newgen).err().as_deref(),
        Some(NOT_ALREADY_THERE)
    );
}

/// Given a career that has signed an `fm-p-*` player and, two seasons on, a Newgen
/// When the game is saved and loaded back
/// Then both are still in the squad on the same contract, neither is offered again, and neither
///      can be recruited a second time from the same registry
#[test]
fn a_recruit_survives_a_save_and_reload() {
    let career = Career::new();
    let state = state_with(career.game(3));
    career.recruit(&state, "fm-p-103").expect("signs");
    state.get_game(|_| ()).expect("a game");
    let mut game = state.get_game(Game::clone).unwrap();
    game.clock.current_date += Duration::days(2 * 365);
    let state = state_with(game);
    let market = state
        .get_game(|game| career.search(game, RegistryAvailability::All))
        .unwrap();
    let newgen = market
        .iter()
        .find(|id| id.starts_with("fm-n-"))
        .unwrap()
        .clone();
    career.recruit(&state, &newgen).expect("signs");
    let game = state.get_game(Game::clone).unwrap();

    let db = GameDatabase::open_in_memory().unwrap();
    GamePersistenceWriter::write_game(&db, &game, "save-1", "Career").unwrap();
    let loaded = GamePersistenceReader::read_game(&db).unwrap();

    for id in ["fm-p-103", newgen.as_str()] {
        let kept = loaded
            .players
            .iter()
            .find(|player| player.id == id)
            .expect("kept");
        let was = game.players.iter().find(|player| player.id == id).unwrap();
        assert_eq!(kept.team_id, was.team_id);
        assert_eq!(kept.wage(), was.wage());
        assert_eq!(kept.contract_end(), was.contract_end());
        assert_eq!(kept.full_name, was.full_name);
        assert_eq!(count_of(&loaded, id), 1);
        assert!(!career
            .search(&loaded, RegistryAvailability::All)
            .contains(&id.to_string()));
    }
    assert_eq!(
        loaded.seed, game.seed,
        "the seed that derives the market is saved"
    );
    let reloaded = state_with(loaded);
    assert_eq!(
        career.recruit(&reloaded, "fm-p-103").err().as_deref(),
        Some(NOT_ALREADY_THERE)
    );
    assert_eq!(
        career.recruit(&reloaded, &newgen).err().as_deref(),
        Some(NOT_ALREADY_THERE)
    );
}

/// Given two careers of different seeds on one source registry, and one fork of a career
/// When one career signs a player
/// Then the other still sees and can sign him, the fork too, the shared source is byte-identical
///      throughout, and each seed has its own cache file
#[test]
fn careers_are_independent_and_the_source_is_never_written() {
    let career = Career::new();
    let source_before = career.source_bytes();
    let a = state_with(career.game(11));
    let b = state_with(career.game(12));
    let fork = state_with(career.game(11));

    career.recruit(&a, "fm-p-104").expect("career A signs him");

    for (name, state) in [("another seed", &b), ("a fork of A", &fork)] {
        let market = state
            .get_game(|game| career.search(game, RegistryAvailability::All))
            .unwrap();
        assert!(
            market.contains(&"fm-p-104".to_string()),
            "{name} still offers him"
        );
    }
    career
        .recruit(&b, "fm-p-104")
        .expect("career B signs him too");
    assert_eq!(
        career.source_bytes(),
        source_before,
        "the shared source is read-only"
    );
    assert_eq!(
        career.cache_files().len(),
        2,
        "one cache per seed, shared by a fork"
    );
}

/// Given a career whose clock has moved forward and then back, as when an older save is loaded
/// When the market is searched at the older date again
/// Then the cache is rebuilt to that season and shows exactly the market it showed then; deleting
///      the cache changes nothing; concurrent readers see one market
#[test]
fn an_older_save_rebuilds_the_cache_and_the_market_is_reproducible() {
    let career = Career::new();
    let early = career.game(21);
    let mut late = early.clone();
    late.clock.current_date += Duration::days(3 * 365);
    let at_start = career.search(&early, RegistryAvailability::All);

    let at_later = career.search(&late, RegistryAvailability::All);
    assert_ne!(at_start, at_later, "three seasons change the market");
    let cache_dir = career.base.parent().unwrap();
    let rebuilt = fm24_adapter::career::ensure_career_cache(
        &career.base,
        cache_dir,
        early.seed,
        early.clock.current_date.date_naive(),
    )
    .unwrap();
    assert!(
        rebuilt.rebuilt,
        "going back in time cannot un-retire anyone, so the cache is rebuilt"
    );
    assert_eq!(career.search(&early, RegistryAvailability::All), at_start);

    for file in career.cache_files() {
        std::fs::remove_file(file).unwrap();
    }
    assert_eq!(
        career.search(&late, RegistryAvailability::All),
        at_later,
        "deleting the cache loses nothing"
    );

    let readings: Vec<BTreeSet<String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| career.search(&late, RegistryAvailability::All)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap().into_iter().collect())
            .collect()
    });
    assert!(
        readings.windows(2).all(|pair| pair[0] == pair[1]),
        "concurrent readers disagree"
    );
}

/// Given a source player the game already holds (the imported world's `fm-p-*` player) and one it
///       retired, both still eligible in the registry's own books
/// When the market is searched and the manager tries to sign them
/// Then neither is offered and both are refused, so no identity can exist twice
#[test]
fn a_player_the_game_already_has_is_never_offered() {
    let career = Career::new();
    let mut game = career.game(31);
    let template = game.players[0].clone();
    let mut held = template.clone();
    held.id = "fm-p-106".to_string();
    game.players.push(held);
    let mut retired = template;
    retired.id = "fm-p-107".to_string();
    let mut archive =
        domain::retired_player::RetiredPlayer::from_player(&retired, "2030-06-30", 2030);
    archive.id = "fm-p-107".to_string();
    game.retired_players.push(archive);
    let state = state_with(game);

    let market = state
        .get_game(|game| career.search(game, RegistryAvailability::All))
        .unwrap();

    assert!(!market.contains(&"fm-p-106".to_string()) && !market.contains(&"fm-p-107".to_string()));
    for id in ["fm-p-106", "fm-p-107"] {
        assert_eq!(
            career.recruit(&state, id).err().as_deref(),
            Some(NOT_ALREADY_THERE),
            "{id}"
        );
    }
}

/// Given the generated gate world (what a Random World career is made of)
/// When its player ids are compared with the registry's namespaces
/// Then no generated player can collide with a registry identity
#[test]
fn a_generated_world_never_uses_the_registry_namespaces() {
    let clashes = generated_world()
        .players
        .iter()
        .filter(|player| player.id.starts_with("fm-p-") || player.id.starts_with("fm-n-"))
        .count();
    assert_eq!(clashes, 0);
}

/// Given an elite player signed from the registry by a club whose saved eleven is empty, as a new
///       career's is
/// When the user's next league fixture is prepared and kicked off
/// Then the kick-off picks him on merit: he is on the five-zone team sheet and in the M12 input
#[test]
fn a_recruit_is_selectable_in_the_next_match() {
    let career = Career::new();
    let state = state_with(career.game(41));
    career.recruit(&state, "fm-p-104").expect("signs");
    let mut game = state.get_game(Game::clone).unwrap();
    let club = user_club(&game);
    assert!(
        game.teams
            .iter()
            .find(|team| team.id == club)
            .unwrap()
            .starting_xi_ids
            .is_empty(),
        "the fixture: a new career has no saved eleven, so the kick-off chooses"
    );
    let fixture = game
        .league
        .as_ref()
        .unwrap()
        .fixtures
        .iter()
        .position(|fixture| {
            fixture.status == FixtureStatus::Scheduled
                && (fixture.home_team_id == club || fixture.away_team_id == club)
        })
        .expect("the user has a league fixture to play");

    let adapted = ofm_core::live_match_manager::adapt_spatial_fixture(&game, fixture)
        .expect("M12 can adapt it");
    let session = kick_off_live_match(&mut game, fixture, MatchMode::Instant, false)
        .expect("the match kicks off");

    let snapshot = session.match_state.snapshot();
    let starters: Vec<&str> = snapshot
        .home_team
        .players
        .iter()
        .chain(&snapshot.away_team.players)
        .map(|player| player.id.as_str())
        .collect();
    assert!(
        starters.contains(&"fm-p-104"),
        "the five-zone team sheet fields him"
    );
    assert!(
        serde_json::to_string(&adapted.input)
            .unwrap()
            .contains("fm-p-104"),
        "the M12 input carries him"
    );
}
