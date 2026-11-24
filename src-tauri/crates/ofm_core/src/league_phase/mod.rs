//! UEFA-style league phase followed by two-legged knockout rounds.
//!
//! One competition container ([`domain::league::League`]) carries it; this module owns what
//! decides an outcome. See the audit document for the regulation each part implements.
//!
//! * [`draw`]: the constrained draw (pots, opponents per pot, associations), seeded and verified.
//! * [`matchdays`]: the draw on matchdays, with exact home/away balance and a tidy sequence.
//! * [`calendar`] and [`profiles`]: dated, versioned templates and the 2026/27 profiles.
//! * [`table`]: the Article 18 ranking, rebuilt from the fixtures.
//! * [`bracket`]: the knockout paths. [`flow`]: the stage machine that moves an edition along.

pub(crate) mod bracket;
pub(crate) mod calendar;
pub mod coefficient;
pub mod draw;
pub(crate) mod exclusivity;
pub(crate) mod flow;
pub mod matchdays;
pub(crate) mod pots;
pub(crate) mod prize;
pub mod profiles;
pub(crate) mod rollover;
pub(crate) mod table;
pub(crate) mod verify;

use chrono::{DateTime, Datelike, Utc};
use domain::league::{CompetitionFormat, Fixture, FixtureStatus, League};
use domain::league_phase::{CompetitionStage, LeaguePhaseRow, LeaguePhaseRules, LeaguePhaseState};
use std::collections::BTreeMap;

pub use flow::{KnockoutLeg, knockout_leg, settle};

/// The clubs of one edition, as the world supplies them.
#[derive(Debug, Clone, Default)]
pub struct Field {
    /// Strongest first. The pots are chunks of this order, so it is where the coefficient
    /// ranking enters: the world, not this module, decides who is seeded where.
    pub seeding: Vec<String>,
    /// Association of each club, by explicit identity.
    pub associations: BTreeMap<String, String>,
    /// Club coefficients when the world has them; empty otherwise (never invented).
    pub coefficients: BTreeMap<String, u32>,
    /// Mixed into the draw, so two careers from one world draw differently.
    pub salt: u64,
}

/// Why an edition could not be built, as a stable translation key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError {
    pub key: String,
}

fn fail(league: &mut League, key: &str) -> BuildError {
    league.fixtures.clear();
    league.knockout_rounds.clear();
    let state = league
        .league_phase
        .get_or_insert_with(LeaguePhaseState::default);
    state.stage = CompetitionStage::Created;
    state.draw_error = Some(key.to_string());
    BuildError {
        key: key.to_string(),
    }
}

/// Builds (or rebuilds) an edition in place: draw, matchdays, dates, fixtures, empty table.
/// Anything the competition held from a previous edition (fixtures, rounds, ties, results) is
/// replaced. On failure the competition holds no fixtures, is in stage `Created` and says why in
/// `draw_error`: a failed draw is never a calendar.
pub fn regenerate_for_season(
    league: &mut League,
    field: &Field,
    season: u32,
    start: DateTime<Utc>,
) -> Result<(), BuildError> {
    league.season = season;
    crate::calendar_identity::record_regenerated_calendar(league, start);
    league.standings.clear();
    league.groups.clear();
    league.fixtures.clear();
    league.knockout_rounds.clear();
    league.league_phase = None;

    let Some(rules) = league.rules.league_phase.clone() else {
        return Err(fail(league, "be.error.leaguePhase.rulesMissing"));
    };
    if let Err(key) = rules.validate() {
        return Err(fail(league, key));
    }
    if field.seeding.len() != rules.field_size() {
        return Err(fail(league, "be.error.leaguePhase.draw.fieldShape"));
    }
    // Rounds follow each other as the regulations date them, a rest window apart at least; the
    // generic cup gap (two weeks) would push every round past its published window.
    league.rules.knockout_round_gap_days = crate::calendar_booking::MIN_GAP_DAYS as u32;
    // The pots are the ranking, unless the ranking leaves no possible draw: then the smallest
    // trade across a pot boundary (recorded, never silent) makes one possible.
    let (seeding, pot_adjustments) = pots::relax(&field.seeding, &field.associations, &rules);
    league.participant_ids = seeding.clone();
    let pots: Vec<Vec<String>> = seeding
        .chunks(rules.pot_size())
        .map(<[String]>::to_vec)
        .collect();
    let tag = format!("league-phase/draw/{}/{season}", league.id);
    let graph = match draw::draw(&draw::DrawInput {
        pots: &pots,
        associations: &field.associations,
        rules: &rules,
        seed: field.salt,
        tag: &tag,
    }) {
        Ok(graph) => graph,
        Err(error) => return Err(fail(league, error.key())),
    };
    let plan = match matchdays::schedule(
        &graph,
        seeding.len(),
        rules.matches_per_club(),
        field.salt,
        &tag,
    ) {
        Ok(plan) => plan,
        Err(_) => return Err(fail(league, "be.error.leaguePhase.schedule.noColouring")),
    };
    let calendar_year = start.year();
    let dates =
        match calendar::edition_dates(&rules.calendar, calendar_year, rules.matches_per_club()) {
            Ok(dates) => dates,
            Err(error) => return Err(fail(league, error.key())),
        };
    league.fixtures = league_fixtures(league, &plan, &seeding, &dates, season);
    league.league_phase = Some(LeaguePhaseState {
        stage: CompetitionStage::LeaguePhaseScheduled,
        calendar_year,
        draw_seed: field.salt,
        seeding: seeding.clone(),
        pots,
        pot_adjustments,
        associations: field.associations.clone(),
        coefficients: field.coefficients.clone(),
        table: initial_table(&seeding, &field.coefficients),
        ..Default::default()
    });
    Ok(())
}

/// The field of an edition from the world's clubs: seeded by reputation (strongest first, the id
/// breaking ties) because the world carries no club coefficients, with each club's association
/// read from its football nation. A club the world does not know has no association, and the draw
/// refuses it rather than guessing.
pub fn field_from_teams(teams: &[domain::team::Team], participants: &[String], salt: u64) -> Field {
    let by_id: BTreeMap<&str, &domain::team::Team> =
        teams.iter().map(|team| (team.id.as_str(), team)).collect();
    let reputation = |id: &String| by_id.get(id.as_str()).map_or(0, |team| team.reputation);
    let mut seeding = participants.to_vec();
    seeding.sort_by(|a, b| reputation(b).cmp(&reputation(a)).then_with(|| a.cmp(b)));
    let associations = seeding
        .iter()
        .filter_map(|id| {
            by_id
                .get(id.as_str())
                .map(|team| (id.clone(), team.football_nation.clone()))
        })
        .collect();
    Field {
        seeding,
        associations,
        coefficients: BTreeMap::new(),
        salt,
    }
}

/// Renews an edition from the world's clubs; see [`regenerate_for_season`].
pub fn renew_from_teams(
    league: &mut League,
    teams: &[domain::team::Team],
    salt: u64,
    season: u32,
    start: DateTime<Utc>,
) -> Result<(), BuildError> {
    let field = field_from_teams(teams, &league.participant_ids.clone(), salt);
    regenerate_for_season(league, &field, season, start)
}

fn initial_table(seeding: &[String], coefficients: &BTreeMap<String, u32>) -> Vec<LeaguePhaseRow> {
    table::ranked_rows(std::iter::empty::<&Fixture>(), seeding, coefficients)
}

/// The league-phase fixtures: each matchday's matches dealt across that matchday's dates (every
/// club plays once a matchday, so any split is legal), ordered by a stable hash so the split does
/// not favour the strongest clubs.
fn league_fixtures(
    league: &League,
    plan: &matchdays::MatchdayPlan,
    seeding: &[String],
    dates: &calendar::EditionDates,
    season: u32,
) -> Vec<Fixture> {
    let competition = flow::fixture_competition(league);
    let mut fixtures = Vec::new();
    for (matchday, days) in dates.league.iter().enumerate() {
        let mut matches: Vec<(String, &matchdays::ScheduledMatch)> = plan
            .matches
            .iter()
            .filter(|scheduled| scheduled.matchday == matchday)
            .map(|scheduled| {
                let id = crate::seed::derived_id(&[
                    "league-phase-fixture",
                    &league.id,
                    &season.to_string(),
                    &matchday.to_string(),
                    &seeding[scheduled.home],
                    &seeding[scheduled.away],
                ]);
                (id, scheduled)
            })
            .collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        for (index, (id, scheduled)) in matches.into_iter().enumerate() {
            fixtures.push(Fixture {
                id,
                competition_id: league.id.clone(),
                matchday: matchday as u32 + 1,
                date: days[index % days.len()].format("%Y-%m-%d").to_string(),
                home_team_id: seeding[scheduled.home].clone(),
                away_team_id: seeding[scheduled.away].clone(),
                competition: competition.clone(),
                status: FixtureStatus::Scheduled,
                result: None,
                postponement: None,
            });
        }
    }
    fixtures
}

/// Draws every league-phase edition a freshly built game holds undrawn, and returns how many it
/// drew.
///
/// An authored competition is built undrawn, because a draw needs each club's association and only
/// the whole world knows them. A world built from a package therefore has its European competitions
/// as empty shells until this runs; a game that starts in the middle of a season rebuilds them
/// through `world::foundations` instead. The draw is the game's own (seeded by the game's seed and
/// the competition's id), never UEFA's. A refused draw is recorded on the competition and retried
/// by the daily renewal, so it is never fatal here.
pub fn draw_pending_editions(game: &mut crate::game::Game) -> usize {
    let season = crate::world::preseason_league_year(&game.clock);
    let game_start = game.clock.start_date;
    let seed = game.seed;
    let mut drawn = 0;
    for index in 0..game.competitions.len() {
        let competition = &game.competitions[index];
        let undrawn = rules_of(competition).is_some()
            && competition.fixtures.is_empty()
            && competition
                .league_phase
                .as_ref()
                .is_none_or(|state| state.stage == CompetitionStage::Created);
        if !undrawn {
            continue;
        }
        let (start, _) = crate::generator::start_date_at_game_open(
            game_start,
            competition.season_start_month,
            competition.season_start_day,
        );
        let id = competition.id.clone();
        match renew_from_teams(
            &mut game.competitions[index],
            &game.teams,
            seed,
            season,
            start,
        ) {
            Ok(()) => drawn += 1,
            Err(error) => log::error!("[league-phase] {id} cannot be drawn: {}", error.key),
        }
    }
    drawn
}

/// Whether an edition is over and may be renewed: it has a champion, or it never started (no
/// fixtures: not drawn yet, or its draw was refused). An edition in the middle of any stage is not
/// finished, whatever its fixtures say.
pub fn edition_is_finished(league: &League) -> bool {
    match &league.league_phase {
        Some(state) => {
            (state.stage == CompetitionStage::Completed && state.champion_id.is_some())
                || (state.stage == CompetitionStage::Created && league.fixtures.is_empty())
        }
        None => league.fixtures.is_empty(),
    }
}

/// Whether the edition has results worth archiving before it is renewed: it was played.
pub fn edition_was_played(league: &League) -> bool {
    !league.fixtures.is_empty()
}

/// The rules of a competition, when it has the league-phase format.
pub fn rules_of(league: &League) -> Option<&LeaguePhaseRules> {
    (league.rules.format == CompetitionFormat::LeaguePhaseAndKnockout)
        .then_some(league.rules.league_phase.as_ref())
        .flatten()
}

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
pub(crate) mod tests;
