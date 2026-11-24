//! What every result writer asks of the competition it just recorded a result in.
//!
//! There are two writers of a played fixture (the live and unwatched matches in
//! `turn::post_match`, the scoreline resolver in `catchup`). Both used to repeat the same two
//! calls; the new league-phase format needs a third behaviour, so the choice lives here, once.

use domain::league::{CompetitionFormat, League};

/// Settles whatever the result at `fixture_index` completes: the group table and bracket of a
/// group-and-knockout competition, the next round of a cup, or the table, the knockout rounds and
/// the stage of a league-phase competition.
pub(crate) fn after_result(league: &mut League, fixture_index: usize) {
    if league.rules.format == CompetitionFormat::LeaguePhaseAndKnockout {
        crate::league_phase::settle(league);
        return;
    }
    crate::group_stage::process_completed_fixture(league, fixture_index);
    crate::schedule::advance_knockout_competition_round(league);
}

/// Whether a match settled by scoreline alone needs a shootout: it is a knockout match whose tie
/// is level after this score (this match alone for a cup, the aggregate for a second leg).
pub(crate) fn shootout_required(
    league: &League,
    fixture_id: &str,
    home_goals: u8,
    away_goals: u8,
) -> bool {
    if league.rules.format == CompetitionFormat::LeaguePhaseAndKnockout {
        return crate::league_phase::knockout_leg(league, fixture_id)
            .is_some_and(|leg| leg.level_after(home_goals, away_goals));
    }
    home_goals == away_goals && league.is_knockout_fixture(fixture_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::league_phase::tests::{drawn, play};
    use crate::league_phase::{knockout_leg, profiles};
    use domain::league::{FixtureStatus, MatchResult};
    use domain::league_phase::{CompetitionStage, KnockoutStage};

    /// A competition at its play-offs, with the first play-off tie's fixture indices.
    fn at_the_playoffs() -> (League, usize, usize) {
        let mut league = drawn(profiles::champions_league_2026(), 40);
        for index in 0..league.fixtures.len() {
            play(&mut league, index, None);
        }
        let state = league.league_phase.as_ref().unwrap();
        let tie = state.ties_of(KnockoutStage::Playoffs).next().unwrap();
        let position = |id: &String| league.fixtures.iter().position(|f| &f.id == id).unwrap();
        let legs = (position(&tie.fixture_ids[0]), position(&tie.fixture_ids[1]));
        (league, legs.0, legs.1)
    }

    fn record(league: &mut League, index: usize, home: u8, away: u8) {
        let fixture = &mut league.fixtures[index];
        fixture.status = FixtureStatus::Completed;
        fixture.result = Some(MatchResult {
            home_goals: home,
            away_goals: away,
            ..Default::default()
        });
    }

    /// Given a league-phase competition whose every match is recorded but not yet settled
    /// When the result writers' one dispatch runs
    /// Then the stage machine moves it on (the play-offs are drawn), which the old two calls never did.
    #[test]
    fn the_dispatch_settles_a_league_phase_competition() {
        let mut league = drawn(profiles::champions_league_2026(), 41);
        for index in 0..league.fixtures.len() {
            record(&mut league, index, (index % 3) as u8, (index % 2) as u8);
        }

        after_result(&mut league, 0);

        assert_eq!(
            league.league_phase.as_ref().unwrap().stage,
            CompetitionStage::KnockoutPlayoffs
        );
    }

    /// Given a single-leg knockout cup whose first round is recorded
    /// When the dispatch runs
    /// Then the cup's own rule seeds the next round, exactly as the two calls it replaced did.
    #[test]
    fn the_dispatch_leaves_other_formats_to_their_own_rules() {
        let clubs: Vec<String> = (0..8).map(|i| format!("c{i}")).collect();
        let start = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2027, 1, 1, 0, 0, 0).unwrap();
        let mut cup = crate::schedule::generate_knockout_cup(
            "Cup",
            2027,
            &clubs,
            start,
            domain::league::CompetitionType::Cup,
            domain::league::CompetitionScope::Domestic,
        );
        for index in 0..cup.fixtures.len() {
            record(&mut cup, index, 2, 1);
        }

        after_result(&mut cup, 0);

        assert_eq!(cup.knockout_rounds.len(), 2);
        assert!(cup.league_phase.is_none());
    }

    /// Given the legs of a two-legged tie and a league-phase match, and a single-leg cup match
    /// When each is asked whether a given score needs a shootout
    /// Then the first leg never does, the second does only when the aggregate is level (not when
    ///      the leg alone is), the league-phase match never, and a cup match does when level.
    #[test]
    fn a_shootout_is_asked_for_on_the_aggregate_of_the_tie() {
        let (mut league, first, second) = at_the_playoffs();
        // The unseeded host won the first leg 3-1: the seeded side needs two to level.
        record(&mut league, first, 3, 1);
        let (first_id, second_id) = (
            league.fixtures[first].id.clone(),
            league.fixtures[second].id.clone(),
        );
        let league_match = league.league_phase.as_ref().unwrap().seeding.clone();
        assert!(!league_match.is_empty());

        assert!(
            !shootout_required(&league, &first_id, 1, 1),
            "a first leg is never decided on the spot"
        );
        assert!(
            shootout_required(&league, &second_id, 2, 0),
            "1-3 and 2-0 is level"
        );
        assert!(
            !shootout_required(&league, &second_id, 1, 1),
            "1-1 leaves the tie 2-4"
        );
        let a_league_match = league
            .fixtures
            .iter()
            .find(|f| {
                league
                    .league_phase
                    .as_ref()
                    .unwrap()
                    .tie_of_fixture(&f.id)
                    .is_none()
            })
            .unwrap()
            .id
            .clone();
        assert!(!shootout_required(&league, &a_league_match, 1, 1));

        let mut cup = League::new("cup".into(), "Cup".into(), 2027, &[]);
        cup.rules.format = CompetitionFormat::Knockout;
        cup.knockout_rounds = vec![domain::league::KnockoutRoundState {
            fixture_ids: vec!["tie".into()],
            ..Default::default()
        }];
        assert!(shootout_required(&cup, "tie", 1, 1));
        assert!(!shootout_required(&cup, "tie", 2, 1));
        assert!(!shootout_required(&cup, "other", 1, 1));
    }

    /// Given the same legs inside a game
    /// When the engine's one extra-time predicate is asked about each match
    /// Then only the second leg allows extra time, and the engine would be told the first leg's score.
    #[test]
    fn the_engine_is_told_what_each_leg_allows() {
        let (mut league, first, second) = at_the_playoffs();
        record(&mut league, first, 3, 1);
        let leg = knockout_leg(&league, &league.fixtures[second].id).unwrap();
        let league_match = league
            .fixtures
            .iter()
            .position(|f| {
                league
                    .league_phase
                    .as_ref()
                    .unwrap()
                    .tie_of_fixture(&f.id)
                    .is_none()
            })
            .unwrap();
        let manager = domain::manager::Manager::new(
            "m".into(),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            "ENG".into(),
        );
        let clock = crate::clock::GameClock::new(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2027, 2, 1, 12, 0, 0).unwrap(),
        );
        let mut game = Game::new(clock, manager, vec![], vec![], vec![], vec![]);
        game.league = Some(league);

        let allows = |index| crate::matchday::fixture_allows_extra_time(&game, index);

        assert_eq!(
            (allows(first), allows(second), allows(league_match)),
            (false, true, false)
        );
        // Asymmetric on purpose: the second leg's home side scored 1 in the first leg, the away side 3.
        assert_eq!(leg.first_leg_goals, (1, 3));
    }

    /// Given a second leg settled by scoreline alone (the dormant tier) and level on aggregate
    /// When it is resolved, over many seeds
    /// Then a shootout always decides it and the tie is resolved; a first leg never gets one.
    #[test]
    fn a_scoreline_resolved_second_leg_gets_its_shootout_and_the_tie_resolves() {
        let mut shootouts = 0;
        let (base, first, second) = at_the_playoffs();
        for seed in 0..400u64 {
            let mut league = base.clone();
            record(&mut league, first, 1, 1);
            league.league_phase.as_mut().unwrap().ties[0].resolution = None;
            let mut rng = crate::seed::rng_from_u64(seed);
            crate::catchup::resolve_fixture_with_strengths(&mut league, second, |_| 1.0, &mut rng);
            let result = league.fixtures[second].result.clone().unwrap();
            let tie = league
                .league_phase
                .as_ref()
                .unwrap()
                .ties_of(KnockoutStage::Playoffs)
                .next()
                .unwrap()
                .clone();
            if result.home_goals == result.away_goals {
                shootouts += 1;
                assert!(result.home_penalties.is_some(), "seed {seed}");
                assert!(
                    tie.resolution.is_some(),
                    "seed {seed}: the shootout resolved the tie"
                );
            } else {
                assert!(result.home_penalties.is_none(), "seed {seed}");
                assert!(tie.resolution.is_some(), "seed {seed}");
            }
        }
        assert!(
            shootouts > 0,
            "no level second leg in 400 seeds: the test saw nothing"
        );
    }
}
