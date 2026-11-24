//! INTEGRATION5: the real FM 2026/27 world with its three European competitions, played by the
//! game's own day loop and rollover.
//!
//! The world is the owner's own file (`APFM_WORLD_JSON`, built by `apfm-import` from their
//! `world.db` into a scratch directory), never in the repository, so each test returns at once
//! without it. A test that returns for that reason has verified nothing about the real source: the
//! audit report says which of these ran.
//! Run with `APFM_WORLD_JSON=<file> cargo test -p season-harness --test integration5_europe`.

use domain::league_phase::CompetitionStage;
use season_harness::calendar::unplayed_clashes;
use season_harness::driver::{RunOptions, run_seasons};
use season_harness::europe::{clubs_in_two_european_competitions, european};
use season_harness::europe_watch::{
    EuropeWatch, Rollovers, assert_editions_archived, finish_european_editions,
};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;

const PSG: &str = "fm-c-498";
const EUROPEAN_IDS: [&str; 3] = ["eu-ucl", "eu-uel", "eu-uecl"];

/// Given the real FM 2026/27 world and a new Paris Saint-Germain career in June 2026
/// When the career is opened
/// Then the three European competitions exist with 36 distinct clubs each, no club in two of them,
///      are drawn and dated, PSG is in the Champions League with its eight league-phase matches,
///      and the competition is one the manager follows.
#[test]
fn a_new_psg_career_opens_with_the_champions_league_drawn() {
    let Some(path) = world_path_from_env() else {
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
    let Some(path) = world_path_from_env() else {
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
}
