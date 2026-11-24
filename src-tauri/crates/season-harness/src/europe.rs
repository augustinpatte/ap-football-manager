//! A synthetic European season for the league-phase engine: three 36-club competitions (a
//! Champions-League-shaped, a Europa-League-shaped and a Conference-League-shaped one) played
//! alongside the generated world's domestic leagues and cups, by the game's own day loop.
//!
//! The clubs are the generated world's, the associations are its nations, and the three fields
//! are disjoint. Nothing here is a real club, association or coefficient. The fields are built
//! from the world's reputations, which is what the engine's own rollover does until the world
//! carries real access lists (INTEGRATION5).

use std::collections::{BTreeMap, BTreeSet};

use domain::league::{CompetitionScope, CompetitionType, League};
use domain::league_phase::{CompetitionStage, LeaguePhaseRules};
use ofm_core::game::Game;
use ofm_core::league_phase::{self, profiles};

use crate::worlds::{Scope, WorldSpec};

/// Twelve European nations: three with a second tier, nine with one. Twelve associations of at
/// least nine clubs give three disjoint fields of three clubs per association each. No southern-
/// hemisphere nation: its league runs on another calendar, and a European competition piled onto
/// it would hold the whole world's rollover back for months (found by this harness).
pub const EUROPE_NATIONS: &[&str] = &[
    "ENG", "ES", "FR", "PT", "NL", "BE", "SCO", "HR", "SE", "IE", "WAL", "NIR",
];

/// `(id, name, priority)` of the three competitions, highest tier first.
pub const EUROPEAN_COMPETITIONS: [(&str, &str, u32); 3] = [
    ("europe-cl", "Synthetic Champions Cup", 1),
    ("europe-el", "Synthetic Europa Cup", 2),
    ("europe-ecl", "Synthetic Conference Cup", 3),
];

pub fn europe_spec(seed: u64) -> WorldSpec {
    WorldSpec {
        seed,
        nations: EUROPE_NATIONS,
        user_nation: "ENG",
        start_year: 2033,
        scope: Scope::Followed,
        package: None,
    }
}

fn rules_of(id: &str) -> LeaguePhaseRules {
    match id {
        "europe-cl" => profiles::champions_league_2026(),
        "europe-el" => profiles::europa_league_2026(),
        _ => profiles::conference_league_2026(),
    }
}

/// The world with its own continental competition replaced by the three league-phase ones, each
/// drawn, dated and settled into the calendar. The manager's club is in the first.
pub fn europe_world(seed: u64) -> Result<Game, String> {
    let mut game = europe_spec(seed).build()?;
    game.competitions
        .retain(|competition| competition.kind != CompetitionType::ContinentalClub);
    let remaining: BTreeSet<String> = game.competitions.iter().map(|c| c.id.clone()).collect();
    game.active_competition_ids
        .retain(|id| remaining.contains(id));

    // Three disjoint fields: each nation's strongest nine clubs, three to a competition.
    let mut by_nation: BTreeMap<String, Vec<&domain::team::Team>> = BTreeMap::new();
    for team in &game.teams {
        by_nation
            .entry(team.football_nation.clone())
            .or_default()
            .push(team);
    }
    let mut fields: [Vec<String>; 3] = Default::default();
    for teams in by_nation.values_mut() {
        teams.sort_by(|a, b| {
            b.reputation
                .cmp(&a.reputation)
                .then_with(|| a.id.cmp(&b.id))
        });
        for (tier, field) in fields.iter_mut().enumerate() {
            field.extend(
                teams
                    .iter()
                    .skip(tier * 3)
                    .take(3)
                    .map(|team| team.id.clone()),
            );
        }
    }
    let start = game.clock.current_date;
    let season = u32::try_from(
        start
            .format("%Y")
            .to_string()
            .parse::<i32>()
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let competitions: Vec<League> = EUROPEAN_COMPETITIONS
        .iter()
        .zip(fields)
        .map(|((id, name, priority), field)| {
            let mut league = League::new((*id).into(), (*name).into(), season, &field);
            league.kind = CompetitionType::ContinentalClub;
            league.scope = CompetitionScope::Continental;
            league.priority = *priority;
            league.season_start_month = 8;
            league.season_start_day = 1;
            league.rules.format = domain::league::CompetitionFormat::LeaguePhaseAndKnockout;
            league.rules.league_phase = Some(rules_of(id));
            league.league_phase = Some(Default::default());
            league
        })
        .collect();
    for mut league in competitions {
        league_phase::renew_from_teams(&mut league, &game.teams, game.seed, season, start)
            .map_err(|error| format!("{} cannot be drawn: {}", league.id, error.key))?;
        game.competitions.push(league);
    }
    ofm_core::end_of_season::refresh_user_competition_scope(&mut game);
    settle_calendar(&mut game);
    Ok(game)
}

/// Settles every competition's calendar as the day loop does at a day's end.
fn settle_calendar(game: &mut Game) {
    let today = game.clock.current_date.date_naive();
    let blackout =
        ofm_core::national_team::international_blackout_around(&game.competitions, today);
    ofm_core::calendar_reconcile::settle_calendar(&mut game.competitions, &blackout, today, |_| {
        false
    });
    game.sync_legacy_league();
}

pub fn european(game: &Game) -> impl Iterator<Item = &League> {
    game.competitions
        .iter()
        .filter(|competition| league_phase::rules_of(competition).is_some())
}

/// Clubs that are in more than one of the three competitions' current editions.
pub fn clubs_in_two_european_competitions(game: &Game) -> Vec<String> {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    let mut faults = Vec::new();
    for competition in european(game) {
        for club in &competition.participant_ids {
            if let Some(other) = seen.insert(club, &competition.id) {
                faults.push(format!("{club} is in {other} and {}", competition.id));
            }
        }
    }
    faults
}

/// Whether every European competition has finished its edition.
pub fn all_european_editions_completed(game: &Game) -> bool {
    european(game).all(|competition| {
        competition
            .league_phase
            .as_ref()
            .is_some_and(|state| state.stage == CompetitionStage::Completed)
    })
}
