//! The generated calendar: the one question a rollover or a world build has to answer, and the
//! reload a save goes through between seasons.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use domain::league::{FixtureCompetition, FixtureStatus};
use ofm_core::calendar_booking::too_close;
use ofm_core::game::Game;

/// Pairs of a club's matches still to be played that sit closer than the minimum rest.
pub fn unplayed_clashes(game: &Game) -> Vec<String> {
    let today = game.clock.current_date.date_naive();
    let mut by_club: BTreeMap<&str, Vec<(NaiveDate, &str, &str)>> = BTreeMap::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if fixture.competition == FixtureCompetition::InternationalNation
                || fixture.status != FixtureStatus::Scheduled
            {
                continue;
            }
            let Ok(date) = NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d") else {
                continue;
            };
            if date < today {
                continue;
            }
            for club in [&fixture.home_team_id, &fixture.away_team_id] {
                by_club
                    .entry(club)
                    .or_default()
                    .push((date, &competition.id, &fixture.id));
            }
        }
    }
    let mut clashes = Vec::new();
    for (club, mut rows) in by_club {
        rows.sort();
        for pair in rows.windows(2) {
            if too_close(pair[0].0, pair[1].0) {
                clashes.push(format!(
                    "{club}: {} {} and {} {}",
                    pair[0].1, pair[0].0, pair[1].1, pair[1].0
                ));
            }
        }
    }
    clashes
}

/// The game as a save and a load return it.
pub fn reloaded(game: &Game) -> Game {
    let saved = serde_json::to_vec(game).expect("a game serialises");
    serde_json::from_slice(&saved).expect("a saved game loads")
}

/// What moved in one competition between the calendar its season was generated with and the one it
/// was played on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Displacement {
    pub competition: String,
    pub fixtures: usize,
    /// Fixtures played on another date than the one they were generated for.
    pub moved: usize,
    pub max_delay_days: i64,
    pub total_delay_days: i64,
    /// The last date the competition was generated to finish on, and the last it was played on.
    pub planned_end: Option<NaiveDate>,
    pub actual_end: Option<NaiveDate>,
}

impl Displacement {
    pub fn mean_delay_days(&self) -> f64 {
        if self.moved == 0 {
            0.0
        } else {
            self.total_delay_days as f64 / self.moved as f64
        }
    }
}

/// Remembers the date every fixture was *generated* for, so that a season's displacement is read
/// off its fixture ids when it ends: how many were moved, how far, and where the competition
/// finished against where it was planned to.
#[derive(Debug, Default, Clone)]
pub struct CalendarStudy {
    planned: BTreeMap<String, (String, NaiveDate)>,
}

impl CalendarStudy {
    /// Takes the dates of every fixture the game holds now. Call it before the first day is
    /// played: the calendar reconciliation moves fixtures from the first day on.
    pub fn new(game: &Game) -> Self {
        let mut planned = BTreeMap::new();
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                if let Ok(date) = NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d") {
                    planned.insert(fixture.id.clone(), (competition.id.clone(), date));
                }
            }
        }
        Self { planned }
    }

    /// Displacement per competition for the fixtures that existed when the study began.
    pub fn displacement(&self, game: &Game) -> Vec<Displacement> {
        let mut by_competition: BTreeMap<String, Displacement> = BTreeMap::new();
        for competition in &game.competitions {
            for fixture in &competition.fixtures {
                let Some((_, planned)) = self.planned.get(&fixture.id) else {
                    continue;
                };
                let Ok(actual) = NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d") else {
                    continue;
                };
                let entry = by_competition
                    .entry(competition.id.clone())
                    .or_insert_with(|| Displacement {
                        competition: competition.id.clone(),
                        fixtures: 0,
                        moved: 0,
                        max_delay_days: 0,
                        total_delay_days: 0,
                        planned_end: None,
                        actual_end: None,
                    });
                entry.fixtures += 1;
                let delay = (actual - *planned).num_days();
                if delay != 0 {
                    entry.moved += 1;
                    entry.max_delay_days = entry.max_delay_days.max(delay.abs());
                    entry.total_delay_days += delay.abs();
                }
                entry.planned_end = entry.planned_end.max(Some(*planned));
                entry.actual_end = entry.actual_end.max(Some(actual));
            }
        }
        by_competition.into_values().collect()
    }
}

/// How many matches `club` plays in each calendar month (`YYYY-MM`), across every competition.
pub fn matches_per_month(game: &Game, club: &str) -> BTreeMap<String, usize> {
    let mut months: BTreeMap<String, usize> = BTreeMap::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if fixture.competition == FixtureCompetition::InternationalNation
                || !(fixture.home_team_id == club || fixture.away_team_id == club)
            {
                continue;
            }
            if let Some(month) = fixture.date.get(..7) {
                *months.entry(month.to_string()).or_default() += 1;
            }
        }
    }
    months
}
