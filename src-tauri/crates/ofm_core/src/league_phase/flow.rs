//! The edition's stage machine.
//!
//! [`settle`] is the one function that moves a league-phase competition forward, and it is a pure
//! function of the competition's own state: the fixtures' recorded results and the persisted stage.
//! It can therefore be called after every result, twice in a row, or after a reload, and the
//! outcome is the same:
//!
//! * the table is rebuilt from the completed fixtures, not incremented;
//! * the league phase completes only when exactly the expected number of league-phase fixtures
//!   exist and every one is completed;
//! * a knockout round is created only when no tie of that round exists yet;
//! * a tie is resolved only from its own completed legs, and never defaults to the home side.

use super::bracket::{self, TieSpec};
use super::calendar::{self, CalendarError};
use super::table;
use crate::calendar_booking::MIN_GAP_DAYS;
use chrono::{Days, NaiveDate};
use domain::league::{
    CompetitionFormat, Fixture, FixtureCompetition, FixtureStatus, KnockoutRoundState, League,
};
use domain::league_phase::{
    CompetitionStage, KnockoutStage, KnockoutTie, LeaguePhaseRules, LeaguePhaseState, TieDecision,
    TieResolution,
};
use std::collections::HashMap;

/// What a fixture of a league-phase competition asks of the match engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnockoutLeg {
    /// Whether the match may go to extra time and a shootout: a second leg and the final.
    pub allows_extra_time: bool,
    /// Goals this fixture's home and away sides scored in the first leg (zero outside a second leg).
    pub first_leg_goals: (u8, u8),
    /// Whether the match is played at a neutral venue: the final. Its home side is only the one
    /// the bracket names (Article 19.05 "nominal home team for administrative purposes"), so it
    /// takes none of the home ground's advantages.
    pub neutral_venue: bool,
}

impl KnockoutLeg {
    /// Whether the tie is level after `home_goals`–`away_goals` in this match: the case that needs
    /// extra time, and then a shootout.
    pub fn level_after(&self, home_goals: u8, away_goals: u8) -> bool {
        self.allows_extra_time
            && u16::from(home_goals) + u16::from(self.first_leg_goals.0)
                == u16::from(away_goals) + u16::from(self.first_leg_goals.1)
    }
}

/// The knockout role of `fixture_id` in a league-phase competition; `None` for a league-phase
/// match, and for any competition of another format.
pub fn knockout_leg(league: &League, fixture_id: &str) -> Option<KnockoutLeg> {
    let state = league.league_phase.as_ref()?;
    let tie = state.tie_of_fixture(fixture_id)?;
    let position = tie.fixture_ids.iter().position(|id| id == fixture_id)?;
    if tie.fixture_ids.len() == 1 {
        return Some(KnockoutLeg {
            allows_extra_time: true,
            first_leg_goals: (0, 0),
            neutral_venue: tie.stage == KnockoutStage::Final,
        });
    }
    if position == 0 {
        return Some(KnockoutLeg {
            allows_extra_time: false,
            first_leg_goals: (0, 0),
            neutral_venue: false,
        });
    }
    // The first leg's home side is this leg's away side.
    let first_leg_goals = league
        .fixtures
        .iter()
        .find(|fixture| fixture.id == tie.fixture_ids[0])
        .and_then(|fixture| fixture.result.as_ref())
        .map_or((0, 0), |result| (result.away_goals, result.home_goals));
    Some(KnockoutLeg {
        allows_extra_time: true,
        first_leg_goals,
        neutral_venue: false,
    })
}

/// Moves the competition as far forward as its recorded results allow. A no-op for any other
/// format, for an undrawn edition and for a completed one.
pub fn settle(league: &mut League) {
    if league.rules.format != CompetitionFormat::LeaguePhaseAndKnockout {
        return;
    }
    let Some(rules) = league.rules.league_phase.clone() else {
        return;
    };
    let Some(mut state) = league.league_phase.take() else {
        return;
    };
    advance(league, &rules, &mut state);
    league.league_phase = Some(state);
}

fn advance(league: &mut League, rules: &LeaguePhaseRules, state: &mut LeaguePhaseState) {
    if matches!(
        state.stage,
        CompetitionStage::Created | CompetitionStage::Completed
    ) {
        return;
    }
    refresh_table(league, state);
    league.standings = state.table.iter().map(|row| row.standing()).collect();
    if state.stage.is_league_phase() {
        finish_league_phase_if_played(league, rules, state);
    }
    if state.stage == CompetitionStage::LeaguePhaseCompleted && state.ties.is_empty() {
        let Some(outcome) = state.outcome.clone() else {
            return;
        };
        let mut rng = knockout_rng(league, state, KnockoutStage::Playoffs);
        let specs = bracket::playoff_ties(&outcome, &state.associations, &mut rng);
        match create_round(league, rules, state, KnockoutStage::Playoffs, &specs) {
            Ok(()) => {
                state.draw_error = None;
                state.stage = CompetitionStage::KnockoutPlayoffs;
            }
            Err(error) => {
                log::error!(
                    "[league-phase] {} cannot date the play-offs: {error:?}",
                    league.id
                );
                state.draw_error = Some(error.key().to_string());
            }
        }
    }
    while let Some(stage) = state.stage.knockout_stage() {
        if !resolve_round(league, state, stage) {
            break;
        }
        let Some(next) = stage.next() else {
            let champion = state
                .ties_of(KnockoutStage::Final)
                .next()
                .and_then(|tie| tie.winner_id().map(str::to_string));
            state.champion_id = champion;
            state.stage = CompetitionStage::Completed;
            break;
        };
        if state.ties_of(next).next().is_none() {
            let Some(specs) = specs_for(league, state, stage) else {
                break;
            };
            if let Err(error) = create_round(league, rules, state, next, &specs) {
                log::error!(
                    "[league-phase] {} cannot date {next:?}: {error:?}",
                    league.id
                );
                state.draw_error = Some(error.key().to_string());
                break;
            }
        }
        state.draw_error = None;
        state.stage = next.competition_stage();
    }
}

fn specs_for(
    league: &League,
    state: &LeaguePhaseState,
    finished: KnockoutStage,
) -> Option<Vec<TieSpec>> {
    if finished == KnockoutStage::Playoffs {
        let outcome = state.outcome.as_ref()?;
        let playoffs: Vec<KnockoutTie> = state.ties_of(KnockoutStage::Playoffs).cloned().collect();
        let mut rng = knockout_rng(league, state, KnockoutStage::RoundOf16);
        return bracket::round_of_16_ties(outcome, &playoffs, &state.associations, &mut rng);
    }
    bracket::next_round_ties(finished, &state.ties)
}

fn knockout_rng(
    league: &League,
    state: &LeaguePhaseState,
    stage: KnockoutStage,
) -> rand_chacha::ChaCha12Rng {
    crate::seed::rng_for_seed(
        state.draw_seed,
        &format!("league-phase/knockout-draw/{}/{}", league.id, league.season),
        &format!("{stage:?}"),
    )
}

fn refresh_table(league: &League, state: &mut LeaguePhaseState) {
    let knockout = state.knockout_fixture_ids();
    let league_phase = league
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()));
    state.table = table::ranked_rows(league_phase, &state.seeding, &state.coefficients);
}

fn finish_league_phase_if_played(
    league: &League,
    rules: &LeaguePhaseRules,
    state: &mut LeaguePhaseState,
) {
    let knockout = state.knockout_fixture_ids();
    let fixtures: Vec<&Fixture> = league
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()))
        .collect();
    let played = fixtures
        .iter()
        .filter(|fixture| fixture.status == FixtureStatus::Completed && fixture.result.is_some())
        .count();
    if played > 0 && state.stage == CompetitionStage::LeaguePhaseScheduled {
        state.stage = CompetitionStage::LeaguePhaseInProgress;
    }
    if fixtures.len() != rules.league_fixture_count() || played != fixtures.len() {
        return;
    }
    state.outcome = Some(table::outcome_of(
        &state.table,
        usize::from(rules.direct_places),
        usize::from(rules.playoff_places),
    ));
    state.stage = CompetitionStage::LeaguePhaseCompleted;
}

/// Resolves every tie of `stage` whose legs are all played. Returns whether the whole round is
/// now resolved (and marks it completed).
fn resolve_round(league: &mut League, state: &mut LeaguePhaseState, stage: KnockoutStage) -> bool {
    let fixtures: HashMap<&str, &Fixture> = league
        .fixtures
        .iter()
        .map(|fixture| (fixture.id.as_str(), fixture))
        .collect();
    for tie in state.ties.iter_mut().filter(|tie| tie.stage == stage) {
        if tie.resolution.is_none() {
            tie.resolution = resolve_tie(tie, &fixtures);
        }
    }
    let done = state
        .ties
        .iter()
        .filter(|tie| tie.stage == stage)
        .all(|tie| tie.resolution.is_some());
    if done && let Some(round) = league.knockout_rounds.get_mut(stage.index()) {
        round.completed = true;
    }
    done
}

/// The outcome of a tie from its completed legs, or `None` while a leg is unplayed or the tie is
/// level with no shootout recorded (never resolved by default).
pub(super) fn resolve_tie(
    tie: &KnockoutTie,
    fixtures: &HashMap<&str, &Fixture>,
) -> Option<TieResolution> {
    let legs: Vec<&Fixture> = tie
        .fixture_ids
        .iter()
        .map(|id| fixtures.get(id.as_str()).copied())
        .collect::<Option<_>>()?;
    if legs.is_empty()
        || legs
            .iter()
            .any(|leg| leg.status != FixtureStatus::Completed || leg.result.is_none())
    {
        return None;
    }
    let (mut seeded_goals, mut unseeded_goals) = (0u16, 0u16);
    for leg in &legs {
        let result = leg.result.as_ref()?;
        let (seeded_scored, unseeded_scored) = if leg.home_team_id == tie.seeded_team_id {
            (result.home_goals, result.away_goals)
        } else {
            (result.away_goals, result.home_goals)
        };
        seeded_goals += u16::from(seeded_scored);
        unseeded_goals += u16::from(unseeded_scored);
    }
    let deciding = legs.last()?;
    let result = deciding.result.as_ref()?;
    let extra_time_played = result
        .report
        .as_ref()
        .is_some_and(|report| report.total_minutes > 90);
    let resolution = |winner: &str, penalties: Option<(u8, u8)>, decision| TieResolution {
        winner_id: winner.to_string(),
        seeded_goals,
        unseeded_goals,
        penalties,
        decision,
    };
    if seeded_goals != unseeded_goals {
        let winner = if seeded_goals > unseeded_goals {
            &tie.seeded_team_id
        } else {
            &tie.unseeded_team_id
        };
        let decision = if extra_time_played {
            TieDecision::ExtraTime
        } else {
            TieDecision::Aggregate
        };
        return Some(resolution(winner, None, decision));
    }
    // Level on aggregate: only a recorded, decisive shootout in the deciding match settles it.
    let (home_pens, away_pens) = (result.home_penalties?, result.away_penalties?);
    if home_pens == away_pens {
        return None;
    }
    let (seeded_pens, unseeded_pens) = if deciding.home_team_id == tie.seeded_team_id {
        (home_pens, away_pens)
    } else {
        (away_pens, home_pens)
    };
    let winner = if seeded_pens > unseeded_pens {
        &tie.seeded_team_id
    } else {
        &tie.unseeded_team_id
    };
    Some(resolution(
        winner,
        Some((seeded_pens, unseeded_pens)),
        TieDecision::Penalties,
    ))
}

fn parse_date(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

fn stage_name(stage: KnockoutStage) -> &'static str {
    match stage {
        KnockoutStage::Playoffs => "Knockout Play-offs",
        KnockoutStage::RoundOf16 => "Round of 16",
        KnockoutStage::QuarterFinals => "Quarterfinal",
        KnockoutStage::SemiFinals => "Semifinal",
        KnockoutStage::Final => "Final",
    }
}

/// The dates of a round's legs: the template's window, moved forward in whole weeks only when the
/// round before it ran late (a rest window after its last match).
fn round_dates(
    league: &League,
    rules: &LeaguePhaseRules,
    state: &LeaguePhaseState,
    stage: KnockoutStage,
) -> Result<(Vec<NaiveDate>, Vec<NaiveDate>), CalendarError> {
    let dates = calendar::edition_dates(
        &rules.calendar,
        state.calendar_year,
        rules.matches_per_club(),
    )?;
    let window = dates
        .round(stage)
        .ok_or(CalendarError::MissingWindow(stage))?;
    let previous_last = league
        .fixtures
        .iter()
        .filter_map(|fixture| parse_date(&fixture.date))
        .max();
    let rest = Days::new(MIN_GAP_DAYS as u64);
    let first_leg = match previous_last.and_then(|last| last.checked_add_days(rest)) {
        Some(earliest) => calendar::window_not_before(&window.first_leg, earliest),
        None => window.first_leg.clone(),
    };
    let second_leg = match first_leg
        .iter()
        .max()
        .and_then(|last| last.checked_add_days(rest))
    {
        Some(earliest) if stage.legs() == 2 => {
            calendar::window_not_before(&window.second_leg, earliest)
        }
        _ => Vec::new(),
    };
    Ok((first_leg, second_leg))
}

/// Creates the ties, fixtures and round record of `stage`. Dated from the edition's template,
/// moved forward in whole weeks only when the round before it ran late.
fn create_round(
    league: &mut League,
    rules: &LeaguePhaseRules,
    state: &mut LeaguePhaseState,
    stage: KnockoutStage,
    specs: &[TieSpec],
) -> Result<(), CalendarError> {
    let (first_leg, second_leg) = round_dates(league, rules, state, stage)?;
    let competition = fixture_competition(league);
    let matchday_base = rules.matches_per_club() + 2 * stage.index();
    let mut round_fixture_ids = Vec::new();
    for spec in specs {
        let tie_id = crate::seed::derived_id(&[
            "league-phase-tie",
            &league.id,
            &league.season.to_string(),
            &format!("{stage:?}"),
            &spec.slot.to_string(),
        ]);
        let slot = usize::from(spec.slot);
        let mut legs = Vec::new();
        if stage.legs() == 2 {
            legs.push((
                &spec.unseeded,
                &spec.seeded,
                first_leg[slot % first_leg.len()],
            ));
            legs.push((
                &spec.seeded,
                &spec.unseeded,
                second_leg[(slot + 1) % second_leg.len()],
            ));
        } else {
            legs.push((&spec.seeded, &spec.unseeded, first_leg[0]));
        }
        let mut fixture_ids = Vec::new();
        for (leg, (home, away, date)) in legs.into_iter().enumerate() {
            let date = date.format("%Y-%m-%d").to_string();
            let id = crate::seed::derived_id(&["league-phase-leg", &tie_id, &leg.to_string()]);
            fixture_ids.push(id.clone());
            league.fixtures.push(Fixture {
                id,
                competition_id: league.id.clone(),
                matchday: (matchday_base + leg + 1) as u32,
                date,
                home_team_id: home.clone(),
                away_team_id: away.clone(),
                competition: competition.clone(),
                status: FixtureStatus::Scheduled,
                result: None,
                postponement: None,
            });
        }
        round_fixture_ids.extend(fixture_ids.iter().cloned());
        state.ties.push(KnockoutTie {
            id: tie_id,
            stage,
            slot: spec.slot,
            path: spec.path,
            seeded_team_id: spec.seeded.clone(),
            unseeded_team_id: spec.unseeded.clone(),
            fixture_ids,
            resolution: None,
        });
    }
    league.knockout_rounds.push(KnockoutRoundState {
        id: format!("{}-round-{}", league.id, stage.index() + 1),
        name: stage_name(stage).to_string(),
        fixture_ids: round_fixture_ids,
        bye_team_ids: Vec::new(),
        completed: false,
    });
    Ok(())
}

pub(super) fn fixture_competition(league: &League) -> FixtureCompetition {
    use domain::league::CompetitionType;
    match league.kind {
        CompetitionType::InternationalClub => FixtureCompetition::InternationalClub,
        _ => FixtureCompetition::ContinentalClub,
    }
}

#[cfg(test)]
mod neutral_venue_tests {
    use super::*;
    use domain::league_phase::KnockoutTie;

    fn league_with_ties(stages: &[(KnockoutStage, &[&str])]) -> League {
        let mut league = League::new("eu".into(), "Eu".into(), 2026, &[]);
        let mut state = LeaguePhaseState::default();
        for (index, (stage, fixtures)) in stages.iter().enumerate() {
            state.ties.push(KnockoutTie {
                id: format!("tie-{index}"),
                stage: *stage,
                slot: index as u8,
                path: 1,
                seeded_team_id: "a".into(),
                unseeded_team_id: "b".into(),
                fixture_ids: fixtures.iter().map(|id| (*id).to_string()).collect(),
                resolution: None,
            });
        }
        league.league_phase = Some(state);
        league
    }

    /// Given a final (one match) and a two-legged semi-final
    /// When each match's role is read
    /// Then only the final is at a neutral venue; neither leg of the semi-final is.
    #[test]
    fn only_the_final_is_played_at_a_neutral_venue() {
        let league = league_with_ties(&[
            (KnockoutStage::SemiFinals, &["sf-1", "sf-2"]),
            (KnockoutStage::Final, &["final"]),
        ]);

        let neutral = |id: &str| knockout_leg(&league, id).map(|leg| leg.neutral_venue);

        assert_eq!(neutral("final"), Some(true));
        assert_eq!(neutral("sf-1"), Some(false));
        assert_eq!(neutral("sf-2"), Some(false));
        assert_eq!(neutral("a-league-phase-match"), None);
    }
}
