//! A procedurally generated world must pass the same integrity audit an imported one does.
//! Fictional is not the same as inconsistent: two youngsters who are the same "person", or a
//! player called "Simon Simon", are generator defects, not flavour.

use chrono::NaiveDate;
use ofm_core::generator::{self, definitions::DefinitionSources};
use ofm_core::world_integrity::{IntegrityLimits, audit_players};

/// Several seeds: the defects below are collisions, so one seed can miss them.
const SEEDS: [u64; 5] = [20260820, 1, 7, 42, 9001];

fn world(seed: u64) -> (Vec<domain::team::Team>, Vec<domain::player::Player>) {
    let (teams, players, _staff) =
        generator::generate_world_seeded(seed, &DefinitionSources::embedded_only());
    (teams, players)
}

fn limits() -> IntegrityLimits {
    IntegrityLimits::new(NaiveDate::from_ymd_opt(2026, 6, 1).expect("valid date"))
}

/// Given worlds generated from several seeds, when audited, then no two players share a name,
/// birth date and nationality.
#[test]
fn generated_worlds_contain_no_duplicate_identity() {
    for seed in SEEDS {
        let (teams, players) = world(seed);
        let report = audit_players(&teams, &players, &limits());
        let clashes: Vec<String> = report
            .findings
            .iter()
            .filter(|finding| finding.code() == "duplicate_identity")
            .map(ToString::to_string)
            .collect();
        assert!(clashes.is_empty(), "seed {seed}: {clashes:?}");
    }
}

/// Given worlds generated from several seeds, when player names are read, then none repeats its
/// own word ("Simon Simon", "Scott Scott").
#[test]
fn generated_names_do_not_repeat_the_same_word_twice() {
    for seed in SEEDS {
        let (_, players) = world(seed);
        let repeated: Vec<&str> = players
            .iter()
            .filter(|player| {
                let words: Vec<&str> = player.full_name.split_whitespace().collect();
                words.len() == 2 && words[0].eq_ignore_ascii_case(words[1])
            })
            .map(|player| player.full_name.as_str())
            .collect();
        assert!(repeated.is_empty(), "seed {seed}: {repeated:?}");
    }
}

/// Given a generated world, when audited, then it has no integrity error at all (ratings, club
/// membership, squad floor, birth dates).
#[test]
fn a_generated_world_passes_the_integrity_audit() {
    for seed in SEEDS {
        let (teams, players) = world(seed);
        let report = audit_players(&teams, &players, &limits());
        let errors: Vec<String> = report.errors().map(ToString::to_string).collect();
        assert!(errors.is_empty(), "seed {seed}: {errors:?}");
    }
}

/// Given the same seed twice, when the world is generated, then it is identical, so the
/// de-duplication cannot make generation non-deterministic.
#[test]
fn generation_stays_deterministic_for_a_seed() {
    let (_, first) = world(SEEDS[0]);
    let (_, second) = world(SEEDS[0]);
    let key = |players: &[domain::player::Player]| -> Vec<(String, String, String)> {
        players
            .iter()
            .map(|p| (p.id.clone(), p.full_name.clone(), p.date_of_birth.clone()))
            .collect()
    };
    assert_eq!(key(&first), key(&second));
}

/// The reproducible case behind `docs/APFM_DATA_PROVENANCE.md`: "Brussels FC" exists only because
/// the generator joins a Belgian city to the pattern "{} FC". Its players are generated too.
///
/// Given the standard generated world of seed 20260820, when Brussels FC is looked up, then it is
/// a Belgian club with a full roster whose every player carries a generator id (a v4-shaped UUID),
/// never an imported `fm-p-` id, and which passes the same audit as every other club.
#[test]
fn brussels_fc_is_a_generated_club_with_a_full_generated_roster() {
    let (teams, players) = world(20260820);
    let brussels = teams
        .iter()
        .find(|team| team.name == "Brussels FC")
        .expect("the standard world generates a Brussels FC");
    assert_eq!(brussels.country, "BE");

    let roster: Vec<_> = players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(brussels.id.as_str()))
        .collect();
    assert_eq!(roster.len(), 23);
    for player in &roster {
        assert!(
            uuid::Uuid::parse_str(&player.id).is_ok() && !player.id.starts_with("fm-p-"),
            "{} is not a generator id",
            player.id
        );
    }
    let report = audit_players(&teams, &players, &limits());
    assert!(
        report.errors().next().is_none(),
        "{:?}",
        report.errors().collect::<Vec<_>>()
    );
}
