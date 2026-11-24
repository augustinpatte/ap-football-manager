//! The strict proof that a league-phase edition is over, for the archive and the rollover.
//!
//! Nothing here trusts the persisted state: the table and every tie are recomputed from the
//! recorded fixtures and must equal what the state says, and each round's field must be exactly
//! what the round before sent on. A stale or hand-edited state cannot be archived.

use super::flow::resolve_tie;
use super::table;
use crate::competition_completion::{CompletionFailure, CompletionProof};
use chrono::NaiveDate;
use domain::league::{Fixture, FixtureStatus, League};
use domain::league_phase::{KnockoutStage, KnockoutTie, LeaguePhaseRules, LeaguePhaseState};
use std::collections::{BTreeSet, HashMap};

fn date_of(fixture: &Fixture) -> Result<NaiveDate, CompletionFailure> {
    crate::schedule::date_str_to_utc(&fixture.date)
        .map(|date| date.date_naive())
        .ok_or(CompletionFailure::InvalidFixtureData)
}

/// `Ok` with the date it ended and its champion when every part of the edition proves out.
pub fn verify_completed(league: &League) -> Result<CompletionProof, CompletionFailure> {
    let rules = super::rules_of(league).ok_or(CompletionFailure::InvalidTableShape)?;
    let state = league
        .league_phase
        .as_ref()
        .ok_or(CompletionFailure::InvalidTableShape)?;
    if league
        .fixtures
        .iter()
        .any(|fixture| fixture.status != FixtureStatus::Completed || fixture.result.is_none())
        || state.stage != domain::league_phase::CompetitionStage::Completed
    {
        return Err(CompletionFailure::UnfinishedFixtures);
    }
    let participants: BTreeSet<&str> = league.participant_ids.iter().map(String::as_str).collect();
    if participants.len() != rules.field_size() || state.seeding.len() != rules.field_size() {
        return Err(CompletionFailure::InvalidFixtureData);
    }
    let mut ids = BTreeSet::new();
    let mut completed_on = None;
    for fixture in &league.fixtures {
        let date = date_of(fixture)?;
        if fixture.id.is_empty()
            || !ids.insert(fixture.id.as_str())
            || fixture.competition_id != league.id
            || fixture.home_team_id == fixture.away_team_id
            || !participants.contains(fixture.home_team_id.as_str())
            || !participants.contains(fixture.away_team_id.as_str())
        {
            return Err(CompletionFailure::InvalidFixtureData);
        }
        completed_on = Some(completed_on.map_or(date, |last: NaiveDate| last.max(date)));
    }
    verify_league_phase(league, rules, state)?;
    let champion = verify_knockout(league, state)?;
    Ok(CompletionProof {
        completed_on: completed_on.ok_or(CompletionFailure::UnfinishedFixtures)?,
        champion_id: Some(champion),
    })
}

fn verify_league_phase(
    league: &League,
    rules: &LeaguePhaseRules,
    state: &LeaguePhaseState,
) -> Result<(), CompletionFailure> {
    let fail = CompletionFailure::InvalidTableShape;
    let knockout = state.knockout_fixture_ids();
    let played: Vec<&Fixture> = league
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()))
        .collect();
    if played.len() != rules.league_fixture_count() {
        return Err(fail);
    }
    let mut pairs = BTreeSet::new();
    let mut per_club: HashMap<&str, (usize, usize)> = HashMap::new();
    for fixture in &played {
        let (a, b) = (fixture.home_team_id.as_str(), fixture.away_team_id.as_str());
        if !pairs.insert((a.min(b), a.max(b))) {
            return Err(fail);
        }
        per_club.entry(a).or_default().0 += 1;
        per_club.entry(b).or_default().1 += 1;
    }
    let each = rules.matches_per_club() / 2;
    if per_club.len() != rules.field_size()
        || per_club
            .values()
            .any(|(home, away)| *home != each || *away != each)
    {
        return Err(fail);
    }
    let rows = table::ranked_rows(played.iter().copied(), &state.seeding, &state.coefficients);
    let outcome = state.outcome.as_ref().ok_or(fail)?;
    let recomputed = table::outcome_of(
        &rows,
        usize::from(rules.direct_places),
        usize::from(rules.playoff_places),
    );
    if rows != state.table || recomputed != *outcome {
        return Err(fail);
    }
    Ok(())
}

fn ids(clubs: impl IntoIterator<Item = impl AsRef<str>>) -> BTreeSet<String> {
    clubs
        .into_iter()
        .map(|club| club.as_ref().to_string())
        .collect()
}

fn entrants(ties: &[&KnockoutTie]) -> BTreeSet<String> {
    ids(ties
        .iter()
        .flat_map(|tie| [&tie.seeded_team_id, &tie.unseeded_team_id]))
}

fn winners(ties: &[&KnockoutTie]) -> BTreeSet<String> {
    ids(ties.iter().filter_map(|tie| tie.winner_id()))
}

/// Checks every round and returns the champion.
fn verify_knockout(league: &League, state: &LeaguePhaseState) -> Result<String, CompletionFailure> {
    let fail = CompletionFailure::InvalidBracket;
    let outcome = state.outcome.as_ref().ok_or(fail)?;
    let by_id: HashMap<&str, &Fixture> = league
        .fixtures
        .iter()
        .map(|fixture| (fixture.id.as_str(), fixture))
        .collect();
    let mut expected: BTreeSet<String> = ids(&outcome.playoff);
    let mut direct: BTreeSet<String> = ids(&outcome.direct);
    let mut seen_fixtures = BTreeSet::new();
    let mut champion = None;
    for (index, stage) in KnockoutStage::ALL.iter().enumerate() {
        let ties: Vec<&KnockoutTie> = state.ties_of(*stage).collect();
        let slots: BTreeSet<u8> = ties.iter().map(|tie| tie.slot).collect();
        if ties.len() != stage.tie_count() || slots.len() != ties.len() {
            return Err(fail);
        }
        // Who plays this round: the play-offs' field, then the previous winners (with the
        // direct qualifiers joining at the round of 16).
        if *stage == KnockoutStage::RoundOf16 {
            expected.extend(std::mem::take(&mut direct));
        }
        if entrants(&ties) != expected {
            return Err(fail);
        }
        for tie in &ties {
            let legs: Vec<&Fixture> = tie
                .fixture_ids
                .iter()
                .map(|id| by_id.get(id.as_str()).copied())
                .collect::<Option<_>>()
                .ok_or(fail)?;
            let shape_ok = legs.len() == stage.legs()
                && match legs.as_slice() {
                    [one] => {
                        one.home_team_id == tie.seeded_team_id
                            && one.away_team_id == tie.unseeded_team_id
                    }
                    [first, second] => {
                        first.home_team_id == tie.unseeded_team_id
                            && first.away_team_id == tie.seeded_team_id
                            && second.home_team_id == tie.seeded_team_id
                            && second.away_team_id == tie.unseeded_team_id
                    }
                    _ => false,
                };
            if !shape_ok
                || tie
                    .fixture_ids
                    .iter()
                    .any(|id| !seen_fixtures.insert(id.clone()))
                || resolve_tie(tie, &by_id).as_ref() != tie.resolution.as_ref()
                || tie.resolution.is_none()
            {
                return Err(fail);
            }
        }
        let round = league.knockout_rounds.get(index).ok_or(fail)?;
        let round_ids: BTreeSet<&String> = round.fixture_ids.iter().collect();
        let tie_ids: BTreeSet<&String> = ties.iter().flat_map(|tie| &tie.fixture_ids).collect();
        if !round.completed || round_ids != tie_ids {
            return Err(fail);
        }
        expected = winners(&ties);
        if *stage == KnockoutStage::Final {
            champion = expected
                .iter()
                .next()
                .cloned()
                .filter(|_| expected.len() == 1);
        }
    }
    let knockout_fixtures = seen_fixtures.len();
    let league_phase_fixtures = league.fixtures.len() - knockout_fixtures;
    let champion = champion.ok_or(fail)?;
    if state.champion_id.as_deref() != Some(champion.as_str()) || league_phase_fixtures == 0 {
        return Err(fail);
    }
    Ok(champion)
}
