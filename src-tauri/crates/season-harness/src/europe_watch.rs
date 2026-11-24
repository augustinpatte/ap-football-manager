//! What a played European season must satisfy, watched day by day, and the proofs made at its end.
//!
//! Shared by the synthetic-world run (EUROPE4) and the real FM world run (INTEGRATION5), so both
//! hold the same rules: a European match is never left unplayed in the past, no club is in two
//! competitions, an edition holds the fixtures its stage promises, rest windows hold, and every
//! edition is archived once with its champion and renewed with a fresh draw.

use std::collections::BTreeMap;

use domain::league::FixtureStatus;
use domain::league_phase::{CompetitionStage, KnockoutStage};
use ofm_core::game::Game;

use crate::calendar::unplayed_clashes;
use crate::driver::{RunOptions, SeasonObserver, play_day};
use crate::europe::{clubs_in_two_european_competitions, european};
use crate::invariants::Invariants;

/// Prints the day each season is rolled over, and what the user's own competitions still had open.
pub struct Rollovers;

impl SeasonObserver for Rollovers {
    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let user = game.manager.team_id.clone().unwrap_or_default();
        let open: Vec<String> = game
            .competitions
            .iter()
            .filter(|c| c.participant_ids.contains(&user))
            .map(|c| {
                let left = c
                    .fixtures
                    .iter()
                    .filter(|f| f.status == FixtureStatus::Scheduled)
                    .count();
                format!("{}:{left}", c.id)
            })
            .collect();
        eprintln!(
            "rollover of season {season} on {} (open fixtures: {open:?})",
            game.clock.current_date.date_naive()
        );
    }
}

/// What the European competitions must satisfy at the end of every day.
#[derive(Default)]
pub struct EuropeWatch {
    pub faults: Vec<String>,
    pub days: u32,
    pub stages: BTreeMap<String, CompetitionStage>,
    /// `date competition stage season`, one line per change: the timeline a failure is read from.
    pub timeline: Vec<String>,
}

impl EuropeWatch {
    fn flag(&mut self, game: &Game, what: impl Into<String>) {
        if self.faults.len() < 40 {
            self.faults.push(format!(
                "{}: {}",
                game.clock.current_date.date_naive(),
                what.into()
            ));
        }
    }
}

impl SeasonObserver for EuropeWatch {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        self.days += 1;
        let today = game.clock.current_date.format("%Y-%m-%d").to_string();
        for competition in european(game) {
            if let Some(state) = &competition.league_phase
                && self.stages.insert(competition.id.clone(), state.stage) != Some(state.stage)
            {
                self.timeline.push(format!(
                    "{today} {} season {} -> {:?} ({} fixtures)",
                    competition.id,
                    competition.season,
                    state.stage,
                    competition.fixtures.len()
                ));
            }
        }
        for clash in unplayed_clashes(game).into_iter().take(3) {
            self.flag(game, format!("rest window broken: {clash}"));
        }
        for fault in clubs_in_two_european_competitions(game) {
            self.flag(game, fault);
        }
        for competition in european(game) {
            // A European match left unplayed in the past was skipped: whether or not the player
            // follows the competition, every match is played.
            let stranded = competition
                .fixtures
                .iter()
                .filter(|f| f.status == FixtureStatus::Scheduled && f.date < today)
                .count();
            if stranded > 0 {
                self.flag(
                    game,
                    format!("{} left {stranded} past matches unplayed", competition.id),
                );
            }
            let Some(state) = &competition.league_phase else {
                self.flag(
                    game,
                    format!("{} lost its league-phase state", competition.id),
                );
                continue;
            };
            let expected = match state.stage {
                CompetitionStage::Created => 0,
                _ => competition
                    .rules
                    .league_phase
                    .as_ref()
                    .map_or(0, |r| r.league_fixture_count()),
            };
            let knockout: usize = state.ties.iter().map(|t| t.fixture_ids.len()).sum();
            if state.stage != CompetitionStage::Created
                && competition.fixtures.len() != expected + knockout
            {
                self.flag(
                    game,
                    format!(
                        "{} holds {} fixtures, expected {}",
                        competition.id,
                        competition.fixtures.len(),
                        expected + knockout
                    ),
                );
            }
        }
    }
}

/// Where two strings first differ, with the text around it from each.
pub fn window_of_difference(left: &str, right: &str) -> String {
    let at = left
        .bytes()
        .zip(right.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    let around = |text: &str| {
        let from = at.saturating_sub(160);
        let to = (at + 160).min(text.len());
        String::from_utf8_lossy(&text.as_bytes()[from..to]).into_owned()
    };
    format!(
        "byte {at} of {} / {}\n  left : …{}…\n  right: …{}…",
        left.len(),
        right.len(),
        around(left),
        around(right)
    )
}

/// Whether every European competition is in the edition of `season` and the one before it is on
/// record in the archive: finished, archived and renewed.
pub fn all_european_editions_renewed(game: &Game, season: u32) -> bool {
    european(game).all(|competition| {
        competition.season == season
            && game
                .edition_archive
                .iter()
                .any(|edition| edition.is_edition_of(&competition.id, season - 1))
    })
}

/// Plays on, day by day, until every European edition is finished, archived and renewed.
pub fn finish_european_editions(
    game: &mut Game,
    watch: &mut (&mut EuropeWatch, &mut Invariants),
    first_day: u32,
    renewed_into: u32,
) {
    for day in first_day..first_day + 400 {
        if all_european_editions_renewed(game, renewed_into) {
            return;
        }
        play_day(game, RunOptions::seasons(1), 99, day, watch)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let state: Vec<String> = european(game)
        .map(|c| {
            let open = c
                .fixtures
                .iter()
                .filter(|f| f.status == FixtureStatus::Scheduled)
                .count();
            format!(
                "{} season {} {:?} open={open} draw_error={:?}",
                c.id,
                c.season,
                c.league_phase.as_ref().map(|s| s.stage),
                c.league_phase.as_ref().and_then(|s| s.draw_error.clone()),
            )
        })
        .collect();
    panic!(
        "the European editions were never renewed by {}: {state:#?}\n{:#?}",
        game.clock.current_date, watch.0.timeline
    );
}

pub fn assert_editions_archived(game: &Game, ids: &[&str], seasons: &[u32]) {
    for id in ids.iter().copied() {
        for season in seasons {
            let edition = game
                .edition_archive
                .iter()
                .find(|edition| edition.is_edition_of(id, *season))
                .unwrap_or_else(|| panic!("{id} season {season} was never archived"));
            let state = edition
                .league_phase
                .as_ref()
                .expect("the archive freezes the state");
            let counts: Vec<usize> = KnockoutStage::ALL
                .iter()
                .map(|stage| state.ties_of(*stage).count())
                .collect();
            assert_eq!(counts, vec![8, 8, 4, 2, 1], "{id} {season}");
            assert_eq!(
                state.champion_id.as_deref(),
                Some(edition.champion_id.as_str())
            );
            assert_eq!(edition.standings.len(), 36);
        }
        let archived: Vec<_> = game
            .edition_archive
            .iter()
            .filter(|edition| edition.competition_id == id)
            .collect();
        assert_eq!(
            archived.len(),
            seasons.len(),
            "{id}: one record per edition, never two"
        );
    }
}
