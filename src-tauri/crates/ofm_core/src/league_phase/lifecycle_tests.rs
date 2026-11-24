//! The edition's life around the competition itself: the strict completion proof, the archive,
//! the rollover guard and the daily renewal.

use super::tests::{competition, drawn, field, play_everything};
use super::*;
use crate::competition_completion::{CompletionFailure, verify_completed_edition};
use crate::edition_archive::{ArchiveOutcome, archive_completed_edition};
use crate::game::Game;
use chrono::Utc;
use domain::league_phase::{KnockoutStage, TieResolution};
use domain::team::Team;

fn finished(salt: u64) -> League {
    let mut league = drawn(profiles::champions_league_2026(), salt);
    play_everything(&mut league);
    league
}

/// Given a played-out edition
/// When it is proved complete and archived
/// Then the proof names the final's winner, the archive keeps the table in Article 18 order, the
///      frozen state and the champion, and archiving again records nothing more.
#[test]
fn a_finished_edition_is_proved_and_archived_once() {
    let league = finished(21);
    let champion = league.league_phase.as_ref().unwrap().champion_id.clone();

    let proof = verify_completed_edition(&league, None).expect("a finished edition proves out");
    let mut archive = Vec::new();
    let first = archive_completed_edition(&mut archive, &league);
    let again = archive_completed_edition(&mut archive, &league);

    assert_eq!(proof.champion_id, champion);
    assert_eq!(
        (first, again),
        (
            Ok(ArchiveOutcome::Archived),
            Ok(ArchiveOutcome::AlreadyArchived)
        )
    );
    let [record] = archive.as_slice() else {
        panic!("one record, got {}", archive.len());
    };
    assert_eq!(Some(&record.champion_id), champion.as_ref());
    assert_eq!(record.standings.len(), 36);
    let ranking = &league
        .league_phase
        .as_ref()
        .unwrap()
        .outcome
        .as_ref()
        .unwrap()
        .ranking;
    let archived_order: Vec<&String> = record.standings.iter().map(|row| &row.team_id).collect();
    assert_eq!(archived_order, ranking.iter().collect::<Vec<_>>());
    assert!(record.league_phase.is_some());
}

/// Given an edition at every unfinished point
/// When it is proved or archived
/// Then it is refused as unfinished and nothing is archived: no edition is archived early.
#[test]
fn an_unfinished_edition_is_never_archived() {
    let mut mid_league_phase = drawn(profiles::champions_league_2026(), 22);
    for index in 0..50 {
        super::tests::play(&mut mid_league_phase, index, None);
    }
    let mut at_the_final = drawn(profiles::champions_league_2026(), 23);
    while at_the_final.league_phase.as_ref().unwrap().stage != CompetitionStage::Final {
        let index = at_the_final
            .fixtures
            .iter()
            .position(|f| f.status == FixtureStatus::Scheduled)
            .unwrap();
        super::tests::play(&mut at_the_final, index, None);
    }
    assert_eq!(
        at_the_final
            .fixtures
            .iter()
            .filter(|f| f.status == FixtureStatus::Scheduled)
            .count(),
        1,
        "only the final is left to play"
    );

    for league in [&mid_league_phase, &at_the_final] {
        let mut archive = Vec::new();
        assert_eq!(
            archive_completed_edition(&mut archive, league),
            Err(CompletionFailure::UnfinishedFixtures)
        );
        assert!(archive.is_empty());
        assert!(!edition_is_finished(league));
    }
}

/// Given a finished edition whose persisted state was altered
/// When it is proved
/// Then each alteration is caught, because the table and every tie are recomputed from the fixtures.
#[test]
fn a_tampered_state_cannot_be_archived() {
    let genuine = finished(24);
    let swap_winner = |league: &mut League| {
        let state = league.league_phase.as_mut().unwrap();
        let tie = state
            .ties
            .iter_mut()
            .find(|t| t.stage == KnockoutStage::RoundOf16)
            .unwrap();
        let other = if tie.winner_id() == Some(tie.seeded_team_id.as_str()) {
            tie.unseeded_team_id.clone()
        } else {
            tie.seeded_team_id.clone()
        };
        let resolution: &mut TieResolution = tie.resolution.as_mut().unwrap();
        resolution.winner_id = other;
    };
    let wrong_champion = |league: &mut League| {
        league.league_phase.as_mut().unwrap().champion_id = Some("nobody".into());
    };
    let stale_table = |league: &mut League| {
        league.league_phase.as_mut().unwrap().table[0].points += 3;
    };
    let outsider_in_the_round_of_16 = |league: &mut League| {
        let state = league.league_phase.as_mut().unwrap();
        let tie = state
            .ties
            .iter_mut()
            .find(|t| t.stage == KnockoutStage::RoundOf16)
            .unwrap();
        tie.unseeded_team_id = "outsider".into();
    };
    let dropped_fixture = |league: &mut League| {
        league.fixtures.remove(3);
    };

    for (tamper, expected) in [
        (
            &swap_winner as &dyn Fn(&mut League),
            CompletionFailure::InvalidBracket,
        ),
        (&wrong_champion, CompletionFailure::InvalidBracket),
        (
            &outsider_in_the_round_of_16,
            CompletionFailure::InvalidBracket,
        ),
        (&stale_table, CompletionFailure::InvalidTableShape),
        (&dropped_fixture, CompletionFailure::InvalidTableShape),
    ] {
        let mut league = genuine.clone();
        tamper(&mut league);
        assert_eq!(verify_completed_edition(&league, None), Err(expected));
    }
    assert!(verify_completed_edition(&genuine, None).is_ok());
}

/// Given a competition mid-edition, one finished, and one with nothing in it
/// When the rollover asks which to regenerate
/// Then only the finished one and the empty one: an unfinished European edition is kept.
#[test]
fn the_rollover_regenerates_only_a_finished_or_empty_edition() {
    let mid = drawn(profiles::champions_league_2026(), 25);
    let done = finished(25);
    let empty = competition(profiles::champions_league_2026());

    assert!(!crate::end_of_season::is_regenerated_at_rollover(&mid));
    assert!(crate::end_of_season::is_regenerated_at_rollover(&done));
    assert!(crate::end_of_season::is_regenerated_at_rollover(&empty));
}

fn world_with(league: League) -> Game {
    let teams: Vec<Team> = field(1)
        .seeding
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut team = Team::new(
                id.clone(),
                id.clone(),
                id.clone(),
                "ENG".into(),
                "City".into(),
                "Ground".into(),
                10_000,
            );
            team.football_nation = format!("assoc-{}", i % 18);
            team.reputation = 900 - i as u32;
            team
        })
        .collect();
    let manager = domain::manager::Manager::new(
        "m".into(),
        "A".into(),
        "B".into(),
        "1980-01-01".into(),
        "ENG".into(),
    );
    let clock = crate::clock::GameClock::new(
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2027, 6, 10, 12, 0, 0).unwrap(),
    );
    let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
    game.seed = 77;
    game.competitions = vec![league];
    game
}

/// Given a finished edition whose next edition is four weeks from starting
/// When the day ends, twice
/// Then it is archived and renewed once: the next edition is drawn, scheduled and empty of
///      results, the archive holds the old champion, and nothing happens the second day.
#[test]
fn a_finished_edition_is_archived_and_renewed_once_when_the_next_is_near() {
    let mut game = world_with(finished(26));
    let champion = game.competitions[0]
        .league_phase
        .as_ref()
        .unwrap()
        .champion_id
        .clone();

    let first = crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05");
    let second = crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-06");

    assert_eq!((first, second), (1, 0));
    let next = &game.competitions[0];
    assert_eq!(next.season, 2027);
    assert_eq!(next.fixtures.len(), 144);
    assert!(
        next.fixtures
            .iter()
            .all(|f| f.status == FixtureStatus::Scheduled)
    );
    let state = next.league_phase.as_ref().unwrap();
    assert_eq!(state.stage, CompetitionStage::LeaguePhaseScheduled);
    assert!(state.ties.is_empty() && state.champion_id.is_none());
    assert_eq!(game.edition_archive.len(), 1);
    assert_eq!(
        Some(&game.edition_archive[0].champion_id),
        champion.as_ref()
    );
    // The next edition's first matchday is a Tuesday in September, shifted by whole weeks.
    let first_date = next.fixtures.iter().map(|f| f.date.as_str()).min().unwrap();
    assert!(first_date.starts_with("2027-09-"), "{first_date}");
}

/// Given an edition still being played
/// When the daily renewal runs
/// Then it is left alone: no archive, no renewal.
#[test]
fn an_unfinished_edition_is_not_renewed() {
    let mut game = world_with(drawn(profiles::champions_league_2026(), 27));

    assert_eq!(
        crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05"),
        0
    );
    assert!(game.edition_archive.is_empty());
    assert_eq!(game.competitions[0].season, 2026);
}

/// Given an edition finished a year ago whose next field was set aside by a rollover
/// When it renews
/// Then the next edition is drawn from that field, not from the old participants.
#[test]
fn the_renewal_draws_from_the_field_the_rollover_set_aside() {
    let mut league = finished(28);
    let mut new_field = league.participant_ids.clone();
    new_field.reverse();
    league.league_phase.as_mut().unwrap().next_entrants = new_field.clone();
    let mut game = world_with(league);

    crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05");

    let mut got = game.competitions[0].participant_ids.clone();
    let mut want = new_field;
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

/// Given a finished edition and a world that cannot supply a feasible field
/// When it renews
/// Then the edition is archived, the next one is an explicit failure (no fixtures, a stage of
///      Created and a key), never a calendar that looks fine.
#[test]
fn a_renewal_that_cannot_draw_says_so() {
    let mut game = world_with(finished(29));
    for team in &mut game.teams {
        team.football_nation = "ENG".into();
    }

    crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05");

    let next = &game.competitions[0];
    assert!(next.fixtures.is_empty());
    let state = next.league_phase.as_ref().unwrap();
    assert_eq!(state.stage, CompetitionStage::Created);
    assert_eq!(
        state.draw_error.as_deref(),
        Some("be.error.leaguePhase.draw.infeasible")
    );
    assert_eq!(game.edition_archive.len(), 1);
}

/// Given an edition whose draw was refused because the world's clubs were not fit to draw
/// When the world has since been fixed and the next edition is near
/// Then the daily renewal draws it (nothing is archived: nothing was played), instead of leaving
///      the competition dead for the rest of the career.
#[test]
fn a_refused_draw_is_retried_once_the_world_can_draw() {
    let mut refused = competition(profiles::champions_league_2026());
    let mut bad_field = field(1);
    for club in bad_field.seeding.clone() {
        bad_field
            .associations
            .insert(club, "one-association".into());
    }
    let start = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 1, 0, 0, 0).unwrap();
    regenerate_for_season(&mut refused, &bad_field, 2026, start).unwrap_err();
    assert_eq!(
        refused.league_phase.as_ref().unwrap().stage,
        CompetitionStage::Created
    );
    assert!(
        edition_is_finished(&refused),
        "a never-started edition is renewable"
    );
    let mut game = world_with(refused);

    let renewed = crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05");

    assert_eq!(renewed, 1);
    let next = &game.competitions[0];
    assert_eq!(next.fixtures.len(), 144);
    let state = next.league_phase.as_ref().unwrap();
    assert_eq!(
        (state.stage, state.draw_error.clone()),
        (CompetitionStage::LeaguePhaseScheduled, None)
    );
    assert!(
        game.edition_archive.is_empty(),
        "nothing was played, so nothing is archived"
    );
}

/// Given an edition that was played but never made it into the archive
/// When the rollover renews it
/// Then it is kept as it is (renewing it would lose it); once it is on record it is renewed.
#[test]
fn the_rollover_never_renews_a_played_edition_that_is_not_on_record() {
    let game = world_with(finished(32));
    let mut kept = game.competitions[0].clone();
    let before = serde_json::to_string(&kept).unwrap();
    let start = chrono::TimeZone::with_ymd_and_hms(&Utc, 2027, 8, 1, 0, 0, 0).unwrap();

    super::rollover::renew(&mut kept, &game.teams, &[], 5, (2027, start));
    assert_eq!(
        serde_json::to_string(&kept).unwrap(),
        before,
        "kept untouched"
    );

    let mut archive = Vec::new();
    archive_completed_edition(&mut archive, &kept).unwrap();
    super::rollover::renew(&mut kept, &game.teams, &archive, 5, (2027, start));
    assert_eq!(kept.season, 2027);
    assert_eq!(kept.fixtures.len(), 144);
}

/// Given a field of the wrong size offered at the rollover
/// When it is offered
/// Then it is refused and the competition keeps its clubs; a right-sized one is set aside while
///      the edition is still being played, and adopted when the rollover renews it.
#[test]
fn the_rollover_offers_a_field_only_when_it_is_the_right_size() {
    let mut playing = drawn(profiles::champions_league_2026(), 33);
    let mut short = playing.participant_ids.clone();
    short.pop();
    let mut full = playing.participant_ids.clone();
    full.reverse();

    super::rollover::offer_field(&mut playing, &short, false);
    assert!(
        playing
            .league_phase
            .as_ref()
            .unwrap()
            .next_entrants
            .is_empty()
    );

    super::rollover::offer_field(&mut playing, &full, false);
    assert_eq!(playing.league_phase.as_ref().unwrap().next_entrants, full);

    let mut done = finished(33);
    super::rollover::offer_field(&mut done, &full, true);
    assert_eq!(done.participant_ids, full);
}

/// Given a manager whose club is drawn into a European competition they were not following
/// When the daily renewal draws the new edition
/// Then the competition joins the competitions the manager follows, so their own matches are played
///      on the engine (and can be managed) instead of being settled by scoreline all season.
#[test]
fn a_renewed_edition_joins_the_scope_of_the_manager_whose_club_is_in_it() {
    let mut game = world_with(finished(34));
    let club = game.competitions[0].participant_ids[0].clone();
    game.manager.hire(club);
    game.active_competition_ids = vec!["domestic-league".to_string()];

    let renewed = crate::turn::cup_renewal::renew_finished_league_phases(&mut game, "2027-07-05");

    assert_eq!(renewed, 1);
    assert!(
        game.active_competition_ids
            .contains(&game.competitions[0].id),
        "the manager's competitions are {:?}",
        game.active_competition_ids
    );
}
