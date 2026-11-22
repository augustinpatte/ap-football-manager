//! DATA2 regression tests on the player's own `world.db` (`APFM_WORLD_DB`).
//!
//! Skipped (they pass) without that variable: the data never lives in the repository. Players are
//! named by their source person id, never by name, because two Mbappés and two Tielemans exist.
//! The expectations are bounds with an explanation, not fixed prices: the valuation is a model of
//! the save's own values and these tests state what it must never do to them.

use fm24_adapter::{Config, pipeline, valuation};
use std::path::PathBuf;
use std::sync::OnceLock;

const RASHFORD: i64 = 12275;
const MBAPPE_KYLIAN: i64 = 40368;
const MAINOO: i64 = 55635;
const TIELEMANS_YOURI: i64 = 6606;
const GODTS: i64 = 53728;
const MOSCARDO: i64 = 63609;

/// One build of the real world, shared by every test of this file (it takes a few seconds).
fn run() -> Option<&'static pipeline::Run> {
    static RUN: OnceLock<Option<pipeline::Run>> = OnceLock::new();
    RUN.get_or_init(|| {
        let source = PathBuf::from(std::env::var_os("APFM_WORLD_DB")?);
        let scratch = std::env::temp_dir().join(format!("apfm-data2-named-{}", std::process::id()));
        let built = pipeline::run(&source, &scratch.join("package"), &Config::default_config());
        let _ = std::fs::remove_dir_all(&scratch);
        Some(built.expect("the real world builds"))
    })
    .as_ref()
}

fn plan(run: &pipeline::Run, person: i64) -> &pipeline::PlayerPlan {
    run.plans
        .iter()
        .find(|p| p.person_id == person)
        .unwrap_or_else(|| panic!("person {person} is placed in the world"))
}

fn value(run: &pipeline::Run, person: i64) -> u64 {
    plan(run, person).def.value.expect("a value")
}

/// Given the real save, where Marcus Rashford (CA 161, 28) is stored above Kylian Mbappé (CA 191, 27),
/// When the world is built,
/// Then the stronger player is valued above the weaker, and the weaker is the one the model moved.
#[test]
fn a_much_stronger_player_is_not_valued_below_a_weaker_one_of_the_same_context() {
    let Some(run) = run() else { return };

    let (rashford, mbappe) = (plan(run, RASHFORD), plan(run, MBAPPE_KYLIAN));

    assert!(
        rashford.value_in_source_eur > mbappe.value_in_source_eur,
        "the premise: the save itself stores the inversion"
    );
    assert!(value(run, MBAPPE_KYLIAN) > value(run, RASHFORD));
    assert_eq!(rashford.value_source, "save (ramenée à l'estimation)");
    assert!(value(run, RASHFORD) < rashford.value_in_source_eur.unwrap());
}

/// Given a player whose stored value is far above comparable players' (Tielemans, 29, 60 M EUR) and a
/// prospect stored far above his ability's worth (Godts, CA 106 / PA 161),
/// When valued,
/// Then each is reduced, but never below the model's own estimate: the source is not discarded.
#[test]
fn inflated_values_are_reduced_towards_the_estimate_and_not_past_it() {
    let Some(run) = run() else { return };

    for person in [TIELEMANS_YOURI, GODTS] {
        let p = plan(run, person);
        let (final_value, source, estimate) = (
            value(run, person),
            p.value_in_source_eur.unwrap(),
            p.value_estimate_eur.unwrap(),
        );
        assert!(final_value < source, "person {person}");
        assert!(final_value > estimate, "person {person}");
    }
}

/// Given players whose stored value agrees with comparable players (Mainoo, Moscardo),
/// When valued,
/// Then the stored value is kept to the euro.
#[test]
fn a_stored_value_that_agrees_with_comparable_players_is_kept_exactly() {
    let Some(run) = run() else { return };

    for person in [MAINOO, MOSCARDO] {
        let p = plan(run, person);
        assert_eq!(p.def.value, p.value_in_source_eur, "person {person}");
        assert_eq!(p.value_source, "save", "person {person}");
    }
}

/// Given the whole real save,
/// When every imported player is valued,
/// Then clearly-ordered comparable pairs are inverted at least five times less often than in the
/// stored values, and under one pair in a hundred; and nobody is worth more than the richest club's
/// transfer budget.
#[test]
fn comparable_players_are_not_systematically_inverted() {
    let Some(run) = run() else { return };
    let cfg = &run.cfg;
    let on = chrono::NaiveDate::parse_from_str(&cfg.start.date, "%Y-%m-%d").expect("start date");
    let rows: std::collections::BTreeMap<i64, &fm24_adapter::source::PlayerRow> =
        run.players.iter().map(|r| (r.person_id, r)).collect();
    let comparables = |use_source: bool| -> Vec<valuation::Comparable> {
        run.plans
            .iter()
            .filter_map(|p| {
                let row = rows.get(&p.person_id)?;
                let inputs = valuation::Inputs::from_row(
                    row,
                    p.natural_code,
                    p.def.contract_end.as_deref(),
                    on,
                    cfg,
                );
                // Only players the save has a value for: the same population before and after.
                let stored = p.value_in_source_eur?;
                let value_eur = if use_source { stored } else { p.def.value? };
                Some(valuation::Comparable {
                    id: p.def.id.clone(),
                    inputs,
                    value_eur,
                })
            })
            .collect()
    };

    let (pairs, raw) = valuation::strict_inversions(&mut comparables(true));
    let (same_pairs, after) = valuation::strict_inversions(&mut comparables(false));

    assert_eq!(pairs, same_pairs);
    assert!(
        pairs > 50_000,
        "a meaningful number of comparable pairs: {pairs}"
    );
    assert!(
        raw * 100 > pairs,
        "the save itself inverts >1% of them: {raw}/{pairs}"
    );
    assert!(after * 5 < raw, "{after} inversions after, {raw} before");
    assert!(after * 100 < pairs, "{after} of {pairs}");
    let cap = run
        .valuation
        .cap_eur
        .expect("the save carries transfer budgets");
    assert!(
        run.plans
            .iter()
            .all(|p| p.def.value.is_some_and(|v| v <= cap))
    );
}

/// Given the real save, where Mika Godts is registered with Paris Saint-Germain's under-21 side with
/// a youth squad status and an ability (CA 106) below PSG's first-team floor,
/// When the world is built,
/// Then he is in the academy: the importer follows the save, and every signal in the save agrees
/// (this is the cause of the owner's observation, not an importer defect).
#[test]
fn godts_is_in_the_academy_because_every_signal_in_the_save_says_so() {
    let Some(run) = run() else { return };

    let row = run
        .players
        .iter()
        .find(|r| r.person_id == GODTS)
        .expect("his source row");
    let p = plan(run, GODTS);

    assert_eq!(row.registered_slot, Some(1));
    assert!(row.registered_is_youth);
    assert_eq!(row.squad_status_code, Some(11));
    assert_eq!(p.placement.role, domain::player::SquadRole::Youth);
    assert!(!p.placement.senior_on_status);
    let first_team_cas: Vec<i64> = run
        .players
        .iter()
        .filter(|r| r.registered_club_id == row.registered_club_id && r.registered_slot == Some(0))
        .map(|r| r.ca)
        .collect();
    assert!(
        first_team_cas.iter().all(|ca| *ca > row.ca),
        "below every first-teamer of his club"
    );
}

/// Given every club of the real save,
/// When the world is built,
/// Then every blocking integrity check passes: viable squads, one effective club per player, no
/// over-age academy player, no loaned player in his owner's lineup, nobody dropped or duplicated.
#[test]
fn every_blocking_integrity_check_passes_for_all_clubs() {
    let Some(run) = run() else { return };

    let failed: Vec<String> = fm24_adapter::checks::run_checks(run)
        .into_iter()
        .filter(|c| c.blocking && !c.ok)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();

    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(run.clubs.len(), 240);
}

/// Given the real save, which carries no career records,
/// When a career is started with generated history (two seasons here: the property does not depend on
/// depth, and twelve, the new-game default, take two minutes of league simulation),
/// Then no imported player has an invented season, club, appearance or award, and each carries the
/// "earlier career unavailable" provenance; generated fillers are not affected.
#[test]
fn imported_players_get_no_invented_history_when_a_career_starts() {
    let Some(run) = run() else { return };
    use domain::player::EarlierCareer;

    let manager = domain::manager::Manager::new(
        "m".to_string(),
        "A".to_string(),
        "B".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    let clock = ofm_core::clock::GameClock::new(
        ofm_core::world::start_date_for_year(2026).expect("a start date"),
    );
    let started = std::time::Instant::now();
    let (game, _) = ofm_core::world::build::build_game_from_imported_world(
        clock,
        manager,
        ofm_core::world::build::PastHistory {
            start_year: 2026,
            depth_years: 2,
        },
        run.world.clone(),
        ofm_core::generator::ReputationBasis::Stored,
    );
    eprintln!(
        "career start with 2 years of history: {:.1}s",
        started.elapsed().as_secs_f64()
    );

    let imported: Vec<_> = game
        .players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-"))
        .collect();
    assert!(imported.len() > 10_000);
    assert!(
        imported.iter().all(|p| p.career.is_empty()),
        "no invented entry for a real player"
    );
    assert!(
        imported
            .iter()
            .all(|p| p.provenance.earlier_career == EarlierCareer::Unavailable)
    );
    for person in [
        RASHFORD,
        MBAPPE_KYLIAN,
        MAINOO,
        TIELEMANS_YOURI,
        GODTS,
        MOSCARDO,
    ] {
        let p = game
            .players
            .iter()
            .find(|p| p.id == format!("fm-p-{person}"))
            .expect("in the game");
        assert!(p.career.is_empty(), "person {person}");
    }
    let fillers: Vec<_> = game
        .players
        .iter()
        .filter(|p| !p.id.starts_with("fm-p-"))
        .collect();
    assert!(
        fillers
            .iter()
            .all(|p| p.provenance.earlier_career == EarlierCareer::Generated)
    );
}

/// Given the real save, where 81 players are owned by a club outside the 240 and play for one inside,
/// When the world is built,
/// Then each keeps the owner's name as the save states it, plays for the borrower, and has no loan
/// that could return him to a club the game does not contain.
#[test]
fn an_owner_outside_the_world_is_kept_as_identity_and_never_as_a_loan_return() {
    let Some(run) = run() else { return };
    use domain::player::EarlierCareer;

    let outside: Vec<_> = run
        .plans
        .iter()
        .filter(|p| p.placement.kind == fm24_adapter::placement::Kind::LoanedInFromOutside)
        .collect();

    assert!(outside.len() > 50, "{}", outside.len());
    for plan in outside {
        let player = run
            .world
            .players
            .iter()
            .find(|p| p.id == plan.def.id)
            .expect("in the world");
        let owner = player
            .provenance
            .outside_owner
            .as_ref()
            .expect("owner identity kept");
        assert!(!owner.club_name.is_empty());
        assert!(
            !run.world.teams.iter().any(|t| t.id == owner.club_id),
            "never a team of the world"
        );
        assert!(
            player.active_loan.is_none(),
            "no return to a club that does not exist"
        );
        assert_eq!(player.provenance.earlier_career, EarlierCareer::Unavailable);
    }
}

/// Given the real save, where Gabriel Moscardo is owned by PSG and registered with Espanyol, both
/// clubs of the world, and the save records no loan terms,
/// When the world is built,
/// Then the loan names owner and borrower, and its terms are marked as estimated, not agreed.
#[test]
fn moscardos_loan_names_both_clubs_and_marks_its_terms_as_estimated() {
    let Some(run) = run() else { return };
    use domain::player::LoanTermsSource;

    let player = run
        .world
        .players
        .iter()
        .find(|p| p.id == format!("fm-p-{MOSCARDO}"))
        .expect("in the world");
    let loan = player.active_loan.as_ref().expect("on loan");

    assert_eq!(loan.parent_team_id, "fm-c-498");
    assert_eq!(loan.loan_team_id, "fm-c-1107");
    assert_eq!(player.team_id.as_deref(), Some("fm-c-1107"));
    assert_eq!(loan.terms_source, LoanTermsSource::Estimated);
    assert!(
        run.loans_applied.iter().all(|id| {
            run.world
                .players
                .iter()
                .find(|p| &p.id == id)
                .and_then(|p| p.active_loan.as_ref())
                .is_some_and(|l| l.terms_source == LoanTermsSource::Estimated)
        }),
        "every loan the importer made is marked as estimated"
    );
}
