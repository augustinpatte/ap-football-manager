//! Whole-edition tests: a synthetic 36-club competition played from the draw to the champion.

use super::*;
use chrono::TimeZone;
use domain::league::{CompetitionScope, CompetitionType, MatchResult};
use domain::league_phase::{KnockoutStage, KnockoutTie, TieDecision};

pub(crate) fn clubs() -> Vec<String> {
    (0..36).map(|i| format!("club-{i:02}")).collect()
}

/// 36 clubs over 18 associations (two each), strongest first.
pub(crate) fn field(salt: u64) -> Field {
    let seeding = clubs();
    let associations = seeding
        .iter()
        .enumerate()
        .map(|(i, club)| (club.clone(), format!("assoc-{}", i % 18)))
        .collect();
    Field {
        seeding,
        associations,
        coefficients: BTreeMap::new(),
        salt,
    }
}

pub(crate) fn competition(rules: LeaguePhaseRules) -> League {
    let mut league = League::new("cl".into(), "Champions".into(), 2026, &[]);
    league.kind = CompetitionType::ContinentalClub;
    league.scope = CompetitionScope::Continental;
    league.rules.format = CompetitionFormat::LeaguePhaseAndKnockout;
    league.rules.league_phase = Some(rules);
    league
}

fn start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap()
}

pub(crate) fn drawn(rules: LeaguePhaseRules, salt: u64) -> League {
    let mut league = competition(rules);
    regenerate_for_season(&mut league, &field(salt), 2026, start())
        .expect("a feasible field draws");
    league
}

/// A deterministic scoreline from the fixture's id.
fn scoreline(id: &str) -> (u8, u8) {
    let hash = id
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    ((hash % 4) as u8, ((hash / 7) % 3) as u8)
}

/// Plays one fixture as the result writers do: records the result, then settles. Level knockout
/// ties that need a decision get a shootout (the away side wins it on odd ids).
pub(crate) fn play(league: &mut League, index: usize, score: Option<(u8, u8)>) {
    let id = league.fixtures[index].id.clone();
    let (home_goals, away_goals) = score.unwrap_or_else(|| scoreline(&id));
    let level =
        knockout_leg(league, &id).is_some_and(|leg| leg.level_after(home_goals, away_goals));
    let shootout = level.then_some(if id.len().is_multiple_of(2) {
        (5, 4)
    } else {
        (4, 5)
    });
    let fixture = &mut league.fixtures[index];
    fixture.status = FixtureStatus::Completed;
    fixture.result = Some(MatchResult {
        home_goals,
        away_goals,
        home_penalties: shootout.map(|(home, _)| home),
        away_penalties: shootout.map(|(_, away)| away),
        ..Default::default()
    });
    settle(league);
}

/// Plays every fixture the competition has, round after round, until none is left.
pub(crate) fn play_everything(league: &mut League) {
    for _ in 0..400 {
        let Some(index) = league
            .fixtures
            .iter()
            .position(|fixture| fixture.status == FixtureStatus::Scheduled)
        else {
            return;
        };
        play(league, index, None);
    }
    panic!("the competition never ran out of fixtures");
}

fn state(league: &League) -> &LeaguePhaseState {
    league
        .league_phase
        .as_ref()
        .expect("a league-phase competition has state")
}

/// Given a feasible field
/// When each shipped profile is built
/// Then it has the regulation fixture count, 18 matches a matchday, stable unique ids, and
///      every club plays exactly its eight (or six) matches, half at home.
#[test]
fn a_built_edition_has_the_regulation_fixtures() {
    for (rules, fixtures, per_club) in [
        (profiles::champions_league_2026(), 144, 8),
        (profiles::europa_league_2026(), 144, 8),
        (profiles::conference_league_2026(), 108, 6),
    ] {
        let league = drawn(rules, 5);

        assert_eq!(league.fixtures.len(), fixtures);
        assert_eq!(state(&league).stage, CompetitionStage::LeaguePhaseScheduled);
        let mut ids: Vec<&str> = league.fixtures.iter().map(|f| f.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), fixtures, "ids are unique");
        for matchday in 1..=per_club as u32 {
            let count = league
                .fixtures
                .iter()
                .filter(|f| f.matchday == matchday)
                .count();
            assert_eq!(count, 18, "matchday {matchday}");
        }
        for club in clubs() {
            let home = league
                .fixtures
                .iter()
                .filter(|f| f.home_team_id == club)
                .count();
            let away = league
                .fixtures
                .iter()
                .filter(|f| f.away_team_id == club)
                .count();
            assert_eq!((home, away), (per_club / 2, per_club / 2), "{club}");
        }
    }
}

/// Given the same field and salt
/// When an edition is built twice
/// Then the fixtures, dates and state are identical.
#[test]
fn building_an_edition_is_deterministic() {
    let first = drawn(profiles::champions_league_2026(), 9);
    let again = drawn(profiles::champions_league_2026(), 9);
    let other = drawn(profiles::champions_league_2026(), 10);

    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
    assert_ne!(
        serde_json::to_string(&first.fixtures).unwrap(),
        serde_json::to_string(&other.fixtures).unwrap()
    );
}

/// Given a league phase that is not fully played
/// When results arrive one at a time
/// Then the edition is in progress, no outcome exists and no knockout tie is drawn, however
///      many results have arrived, until the very last one.
#[test]
fn the_league_phase_is_not_complete_while_a_fixture_is_open() {
    let mut league = drawn(profiles::champions_league_2026(), 3);
    let total = league.fixtures.len();

    for index in 0..total - 1 {
        play(&mut league, index, None);
    }

    assert_eq!(
        state(&league).stage,
        CompetitionStage::LeaguePhaseInProgress
    );
    assert!(state(&league).outcome.is_none());
    assert!(state(&league).ties.is_empty());

    play(&mut league, total - 1, None);

    assert_eq!(state(&league).stage, CompetitionStage::KnockoutPlayoffs);
    let outcome = state(&league).outcome.clone().expect("the table is final");
    assert_eq!(
        (
            outcome.direct.len(),
            outcome.playoff.len(),
            outcome.eliminated.len()
        ),
        (8, 16, 12)
    );
}

/// Given a finished league phase
/// When the competition is settled again and again
/// Then nothing changes: the play-offs are created exactly once.
#[test]
fn settling_twice_creates_nothing_twice() {
    let mut league = drawn(profiles::champions_league_2026(), 4);
    for index in 0..league.fixtures.len() {
        play(&mut league, index, None);
    }
    let once = serde_json::to_string(&league).unwrap();

    settle(&mut league);
    settle(&mut league);

    assert_eq!(serde_json::to_string(&league).unwrap(), once);
    assert_eq!(state(&league).ties.len(), 8);
    assert_eq!(league.fixtures.len(), 144 + 16);
    assert_eq!(league.knockout_rounds.len(), 1);
}

/// Given an edition played to the end
/// When it finishes
/// Then every round has its tie count (8, 8, 4, 2, 1), a champion exists, it is the final's
///      winner, every qualifier came through exactly one route, and the edition is finished.
#[test]
fn a_full_edition_reaches_one_champion() {
    for (rules, salt) in [
        (profiles::champions_league_2026(), 1),
        (profiles::europa_league_2026(), 2),
        (profiles::conference_league_2026(), 3),
    ] {
        let mut league = drawn(rules, salt);

        play_everything(&mut league);

        let state = state(&league);
        assert_eq!(state.stage, CompetitionStage::Completed);
        let counts: Vec<usize> = KnockoutStage::ALL
            .iter()
            .map(|s| state.ties_of(*s).count())
            .collect();
        assert_eq!(counts, vec![8, 8, 4, 2, 1]);
        let the_final = state.ties_of(KnockoutStage::Final).next().unwrap();
        assert_eq!(state.champion_id.as_deref(), the_final.winner_id());
        assert!(edition_is_finished(&league));
        let outcome = state.outcome.as_ref().unwrap();
        let mut routes: Vec<&String> = outcome
            .direct
            .iter()
            .chain(&outcome.playoff)
            .chain(&outcome.eliminated)
            .collect();
        routes.sort();
        routes.dedup();
        assert_eq!(routes.len(), 36, "no club takes two routes");
        assert_eq!(league.knockout_rounds.len(), 5);
        assert!(league.knockout_rounds.iter().all(|round| round.completed));
        // 144 (or 108) league matches, 16 + 16 + 8 + 4 + 1 knockout matches.
        let expected = if rules_of(&league).unwrap().pot_count == 4 {
            144
        } else {
            108
        } + 45;
        assert_eq!(league.fixtures.len(), expected);
    }
}

/// A play-off tie of a freshly built knockout, by slot.
fn playoff(league: &League, slot: u8) -> KnockoutTie {
    state(league)
        .ties_of(KnockoutStage::Playoffs)
        .find(|tie| tie.slot == slot)
        .unwrap()
        .clone()
}

fn fixture_index(league: &League, id: &str) -> usize {
    league
        .fixtures
        .iter()
        .position(|fixture| fixture.id == id)
        .unwrap()
}

fn at_the_playoffs(salt: u64) -> League {
    let mut league = drawn(profiles::champions_league_2026(), salt);
    for index in 0..league.fixtures.len() {
        play(&mut league, index, None);
    }
    league
}

/// Given a tie level on aggregate where the side with fewer away goals was ahead on the old rule
/// When the second leg's shootout is recorded
/// Then the shootout decides, not away goals, and the winner is justified by the recorded tie.
#[test]
fn level_on_aggregate_goes_to_penalties_without_an_away_goals_rule() {
    let mut league = at_the_playoffs(6);
    let tie = playoff(&league, 0);
    let (first, second) = (
        fixture_index(&league, &tie.fixture_ids[0]),
        fixture_index(&league, &tie.fixture_ids[1]),
    );

    // First leg: the unseeded host wins 2-1. Second leg: the seeded host wins 1-0. 2-2 overall;
    // the seeded side scored one away goal and the unseeded none, so away goals would pick it.
    play(&mut league, first, Some((2, 1)));
    assert!(
        playoff(&league, 0).resolution.is_none(),
        "one leg decides nothing"
    );
    // The shootout goes to the *unseeded* side, the one an away-goals rule would eliminate.
    let fixture = &mut league.fixtures[second];
    fixture.status = FixtureStatus::Completed;
    fixture.result = Some(MatchResult {
        home_goals: 1,
        away_goals: 0,
        home_penalties: Some(3),
        away_penalties: Some(5),
        ..Default::default()
    });
    settle(&mut league);

    let resolution = playoff(&league, 0)
        .resolution
        .expect("a shootout decided it");
    assert_eq!((resolution.seeded_goals, resolution.unseeded_goals), (2, 2));
    assert_eq!(resolution.decision, TieDecision::Penalties);
    assert_eq!(resolution.penalties, Some((3, 5)));
    assert_eq!(
        resolution.winner_id, tie.unseeded_team_id,
        "the shootout, not away goals, decides"
    );
}

/// Given a second leg that ends level on aggregate with no shootout recorded
/// When the tie is settled
/// Then it stays unresolved: nothing defaults to the home side, and the round does not advance.
#[test]
fn a_level_tie_without_a_shootout_is_never_resolved_by_default() {
    let mut league = at_the_playoffs(7);
    let tie = playoff(&league, 1);
    let first = fixture_index(&league, &tie.fixture_ids[0]);
    let second = fixture_index(&league, &tie.fixture_ids[1]);
    play(&mut league, first, Some((1, 1)));
    // The result writer recorded a level second leg and forgot the shootout.
    let fixture = &mut league.fixtures[second];
    fixture.status = FixtureStatus::Completed;
    fixture.result = Some(MatchResult::default());
    settle(&mut league);

    assert!(playoff(&league, 1).resolution.is_none());
    assert_eq!(state(&league).stage, CompetitionStage::KnockoutPlayoffs);
    assert!(
        state(&league)
            .ties_of(KnockoutStage::RoundOf16)
            .next()
            .is_none()
    );
}

/// Given a tie won in extra time of the second leg
/// When it is resolved
/// Then the decision is recorded as extra time, with the aggregate.
#[test]
fn extra_time_in_the_second_leg_is_recorded() {
    let mut league = at_the_playoffs(8);
    let tie = playoff(&league, 2);
    let first = fixture_index(&league, &tie.fixture_ids[0]);
    let second = fixture_index(&league, &tie.fixture_ids[1]);
    play(&mut league, first, Some((0, 0)));
    // 1-0 to the seeded host after 120 minutes.
    let report = domain::league::CompactMatchReport {
        total_minutes: 120,
        home_stats: domain::league::CompactTeamMatchStats {
            possession_pct: 50,
            shots: 0,
            shots_on_target: 0,
            fouls: 0,
            corners: 0,
            yellow_cards: 0,
            red_cards: 0,
        },
        away_stats: domain::league::CompactTeamMatchStats {
            possession_pct: 50,
            shots: 0,
            shots_on_target: 0,
            fouls: 0,
            corners: 0,
            yellow_cards: 0,
            red_cards: 0,
        },
        events: Vec::new(),
    };
    let fixture = &mut league.fixtures[second];
    fixture.status = FixtureStatus::Completed;
    fixture.result = Some(MatchResult {
        home_goals: 1,
        report: Some(report),
        ..Default::default()
    });
    settle(&mut league);

    let resolution = playoff(&league, 2).resolution.unwrap();
    assert_eq!(resolution.decision, TieDecision::ExtraTime);
    assert_eq!(resolution.winner_id, tie.seeded_team_id);
}

/// Given a tie whose first leg is played
/// When the engine is asked what each leg allows
/// Then the first leg never goes to extra time, the second does with the first leg's score
///      carried to it, and a league-phase match is not a knockout leg at all.
#[test]
fn each_leg_tells_the_engine_what_it_allows() {
    let mut league = at_the_playoffs(2);
    let tie = playoff(&league, 0);
    let first = fixture_index(&league, &tie.fixture_ids[0]);
    play(&mut league, first, Some((3, 1)));

    let first_leg = knockout_leg(&league, &tie.fixture_ids[0]).unwrap();
    let second_leg = knockout_leg(&league, &tie.fixture_ids[1]).unwrap();

    assert!(!first_leg.allows_extra_time);
    // The second leg's home side is the first leg's away side: it scored 1, the other 3.
    assert_eq!(second_leg.first_leg_goals, (1, 3));
    assert!(second_leg.allows_extra_time);
    assert!(second_leg.level_after(2, 0), "0-0... 1+2 = 3+0");
    assert!(!second_leg.level_after(1, 0));
    assert!(knockout_leg(&league, &league.fixtures[0].id).is_none());
}

/// Given an edition whose round of 16 exists but whose persisted stage was rolled back
/// When it is settled again
/// Then no second round of 16 is drawn, and the stage moves forward again.
#[test]
fn a_stale_stage_cannot_draw_a_round_twice() {
    let mut league = at_the_playoffs(31);
    while state(&league).stage == CompetitionStage::KnockoutPlayoffs {
        let index = league
            .fixtures
            .iter()
            .position(|f| f.status == FixtureStatus::Scheduled)
            .unwrap();
        play(&mut league, index, None);
    }
    assert_eq!(state(&league).stage, CompetitionStage::RoundOf16);
    let fixtures = league.fixtures.len();
    league.league_phase.as_mut().unwrap().stage = CompetitionStage::KnockoutPlayoffs;

    settle(&mut league);

    assert_eq!(state(&league).ties_of(KnockoutStage::RoundOf16).count(), 8);
    assert_eq!(league.fixtures.len(), fixtures);
    assert_eq!(league.knockout_rounds.len(), 2);
    assert_eq!(state(&league).stage, CompetitionStage::RoundOf16);
}

/// Given an unfeasible field
/// When the edition is built
/// Then it is an error with a key, the competition holds no fixtures and no false calendar.
#[test]
fn a_failed_draw_is_an_error_not_a_calendar() {
    let mut league = competition(profiles::champions_league_2026());
    let mut bad = field(1);
    for (i, club) in bad.seeding.iter().enumerate() {
        bad.associations
            .insert(club.clone(), format!("assoc-{}", i % 3));
    }

    let error = regenerate_for_season(&mut league, &bad, 2026, start()).unwrap_err();

    assert_eq!(error.key, "be.error.leaguePhase.draw.infeasible");
    assert!(league.fixtures.is_empty());
    assert_eq!(state(&league).stage, CompetitionStage::Created);
    assert_eq!(
        state(&league).draw_error.as_deref(),
        Some(error.key.as_str())
    );
    // Never started, so renewable: the daily renewal retries the draw instead of leaving the
    // competition dead for the career.
    assert!(edition_is_finished(&league));
}

/// Given a field of the wrong size, or a competition without rules
/// When the edition is built
/// Then each is refused with its own key.
#[test]
fn a_malformed_edition_is_refused() {
    let mut league = competition(profiles::champions_league_2026());
    let mut short = field(1);
    short.seeding.pop();
    assert_eq!(
        regenerate_for_season(&mut league, &short, 2026, start())
            .unwrap_err()
            .key,
        "be.error.leaguePhase.draw.fieldShape"
    );

    league.rules.league_phase = None;
    assert_eq!(
        regenerate_for_season(&mut league, &field(1), 2026, start())
            .unwrap_err()
            .key,
        "be.error.leaguePhase.rulesMissing"
    );
}

/// Given a saved competition mid-edition
/// When it is serialised, reloaded and played on
/// Then it finishes with the same champion as one never reloaded.
#[test]
fn an_edition_survives_a_save_and_reload_between_rounds() {
    let mut straight = drawn(profiles::champions_league_2026(), 12);
    play_everything(&mut straight);

    let mut reloaded = drawn(profiles::champions_league_2026(), 12);
    for _ in 0..400 {
        let Some(index) = reloaded
            .fixtures
            .iter()
            .position(|f| f.status == FixtureStatus::Scheduled)
        else {
            break;
        };
        play(&mut reloaded, index, None);
        // Save and reload after every single result.
        reloaded = serde_json::from_str(&serde_json::to_string(&reloaded).unwrap()).unwrap();
    }

    assert_eq!(
        serde_json::to_string(&reloaded).unwrap(),
        serde_json::to_string(&straight).unwrap()
    );
}

/// Given a competition of another format
/// When it is settled
/// Then nothing happens.
#[test]
fn other_formats_are_untouched() {
    let mut cup = League::new("cup".into(), "Cup".into(), 2026, &[]);
    cup.rules.format = CompetitionFormat::Knockout;
    let before = serde_json::to_string(&cup).unwrap();

    settle(&mut cup);

    assert_eq!(serde_json::to_string(&cup).unwrap(), before);
}

/// Given participants of three clubs with and without coefficients, and equal reputations
/// When the field is built with coefficients
/// Then a higher coefficient is seeded first, reputation then the id break ties, a club without a
///      coefficient comes after those that have one, and the coefficients are carried on the field
///      only for the clubs that have one.
#[test]
fn coefficients_seed_before_reputation_and_a_club_without_one_comes_last() {
    let team = |id: &str, reputation: u32| {
        let mut team = domain::team::Team::new(
            id.into(),
            id.into(),
            id.into(),
            "ENG".into(),
            "C".into(),
            "G".into(),
            1,
        );
        team.reputation = reputation;
        team.football_nation = "ENG".into();
        team
    };
    let teams = vec![
        team("a", 900),
        team("b", 500),
        team("c", 500),
        team("d", 100),
    ];
    let participants: Vec<String> = ["d", "c", "b", "a"].map(String::from).to_vec();
    let coefficients: BTreeMap<String, u32> = [
        ("d".to_string(), 9_000),
        ("b".to_string(), 4_000),
        ("c".to_string(), 4_000),
    ]
    .into();

    let field = field_from_teams_with(&teams, &participants, &coefficients, 1);

    // d has the highest coefficient; b and c tie on it and on reputation, so the id decides; a has
    // none and is last, whatever its reputation.
    assert_eq!(
        field.seeding,
        ["d", "b", "c", "a"].map(String::from).to_vec()
    );
    assert_eq!(field.coefficients.len(), 3);
    assert!(!field.coefficients.contains_key("a"));
    // With no coefficients the order is the reputation order, exactly as before.
    let plain = field_from_teams(&teams, &participants, 1);
    assert_eq!(
        plain.seeding,
        ["a", "b", "c", "d"].map(String::from).to_vec()
    );
    assert!(plain.coefficients.is_empty());
}

/// Given a ranking that puts five clubs of one association in the top pot (no draw exists for it)
/// When the edition is built
/// Then it is built, one trade across the pot boundary is recorded in the state, the seeding the
///      state keeps is the traded one, and the draw is verified: a refused field would have been a
///      competition that never starts.
#[test]
fn a_crowded_pot_is_traded_and_the_trade_is_recorded_in_the_edition() {
    let mut crowded = field(3);
    // The five strongest clubs all belong to one association, the others to distinct ones.
    for (index, club) in crowded.seeding.clone().iter().enumerate() {
        let association = if index < 5 {
            "crowded".to_string()
        } else {
            format!("other-{index}")
        };
        crowded.associations.insert(club.clone(), association);
    }
    let mut league = competition(profiles::champions_league_2026());
    let start = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 1, 0, 0, 0).unwrap();

    regenerate_for_season(&mut league, &crowded, 2026, start).expect("the crowded pot is repaired");

    let state = league.league_phase.as_ref().unwrap();
    assert_eq!(
        state.pot_adjustments.len(),
        1,
        "{:?}",
        state.pot_adjustments
    );
    let trade = &state.pot_adjustments[0];
    assert_eq!((trade.from_pot, trade.to_pot), (0, 1));
    let pot_of = |club: &str| {
        state
            .pots
            .iter()
            .position(|pot| pot.iter().any(|c| c == club))
    };
    assert_eq!(
        pot_of(&trade.moved_club_id),
        Some(1),
        "the crowded club moved down"
    );
    assert_eq!(
        pot_of(&trade.exchanged_club_id),
        Some(0),
        "the other club moved up"
    );
    assert_eq!(
        state.seeding.iter().position(|c| *c == trade.moved_club_id),
        Some(9)
    );
    assert_eq!(league.fixtures.len(), 144);
}
